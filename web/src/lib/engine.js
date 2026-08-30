// The dealer3 engine, compiled to WebAssembly.
//
// One module owns loading it, so the rest of the app can await `ready()` and
// then call synchronously. Everything runs in the browser: no script, no deal
// and no keystroke is sent anywhere.

import init, {
  check_script as wasmCheck,
  language_info as wasmLanguageInfo,
  version as wasmVersion,
} from '@/wasm/dealer3_wasm.js'

let loading = null
let loaded = false

/** Load the engine once. Safe to call repeatedly; later calls await the first. */
export function ready() {
  if (!loading) loading = init().then((v) => { loaded = true; return v })
  return loading
}

/** Whether the wasm module has finished initialising. */
export function isReady() {
  return loaded
}

// --- The worker that does the generating ---------------------------------
//
// One worker, kept between runs so the wasm is loaded once. Cancelling
// terminates it — the only thing that stops code already inside the wasm,
// since a flag would need the blocked thread to come back and read it — and
// the next run makes a new one.

let worker = null
let nextRunId = 1

function ensureWorker() {
  if (!worker) {
    worker = new Worker(new URL('./engine.worker.js', import.meta.url), { type: 'module' })
  }
  return worker
}

/** Stop the run in flight, if any. Its promise rejects with `cancelled`. */
export function cancelGenerate() {
  if (!worker) return false
  worker.terminate()
  worker = null
  return true
}

/// Hand one run to the worker, routing its progress messages back.
function runInWorker(script, options) {
  const w = ensureWorker()
  const id = nextRunId++

  return new Promise((resolve, reject) => {
    const finish = (fn, value) => {
      w.removeEventListener('message', onMessage)
      w.removeEventListener('error', onError)
      options.signal?.removeEventListener?.('abort', onAbort)
      fn(value)
    }

    const onMessage = (event) => {
      const data = event.data || {}
      // A message from a run that was cancelled and replaced.
      if (data.id !== id) return
      if (data.type === 'progress') {
        if (options.onProgress) {
          try {
            options.onProgress(JSON.parse(data.message))
          } catch {
            // A malformed report is not worth failing the run over.
          }
        }
        return
      }
      if (data.type === 'done') finish(resolve, data.raw)
      else finish(reject, new Error(data.message || 'the engine failed'))
    }

    // A worker that dies outright — out of memory, or a wasm trap — reports
    // here rather than as a message, and without this the promise never
    // settles and the page stays "Running…" for ever.
    const onError = (event) => {
      worker = null
      finish(reject, new Error(event.message || 'the engine stopped unexpectedly'))
    }

    const onAbort = () => {
      cancelGenerate()
      const error = new Error('cancelled')
      error.cancelled = true
      finish(reject, error)
    }

    w.addEventListener('message', onMessage)
    w.addEventListener('error', onError)
    options.signal?.addEventListener?.('abort', onAbort, { once: true })

    w.postMessage({
      id,
      script,
      options: {
        seed: options.seed,
        produce: options.produce,
        maxGenerate: options.maxGenerate,
        format: options.format,
        autoLevel: options.autoLevel,
        roundRobin: options.roundRobin,
      },
    })
  })
}

/**
 * Calling into the module before `ready()` resolves fails deep inside the
 * generated bindings with "Cannot read properties of undefined (reading
 * '__wbindgen_free')", which says nothing about the actual mistake. This has
 * caught two components already, so name it.
 */
function assertReady(fn) {
  if (!loaded) {
    throw new Error(
      `dealer3 engine used before it finished loading (${fn}). ` +
        'Await ready() first, or gate the component on it.',
    )
  }
}

/**
 * Run a script.
 *
 * `maxGenerate` bounds the work: a browser tab has no Ctrl-C, so a very
 * selective filter must not be able to hang it. The result's `hitLimit` says
 * whether that bound was reached, which is not the same as "no more matches
 * exist" and should be shown differently.
 *
 * With `autoLevel`, the engine measures the script's `HandType_*` variables,
 * works out a keep rate for each and runs the levelled copy — both passes
 * inside the engine, so the page and the command line agree on what a levelling
 * is. The deals come back interleaved, and `leveling` carries the numbers.
 *
 * Throws with the engine's message on a parse or evaluation error.
 */
export async function generate(
  script,
  {
    seed = 1,
    produce = 20,
    maxGenerate = 1000000,
    format = 'oneline',
    autoLevel = false,
    /// Divide `produce` among the script's `HandType_` variables — one of each
    /// per round — instead of taking deals as they come.
    roundRobin = false,
    /// Called with `{ phase, produced, generated, target }` as the run goes.
    onProgress = null,
    /// Resolves — or rejects — if the caller abandons the run.
    signal = null,
  } = {},
) {
  const raw = JSON.parse(await runInWorker(script, {
    seed,
    produce,
    maxGenerate,
    format,
    autoLevel,
    roundRobin,
    onProgress,
    signal,
  }))
  return {
    deals: raw.deals,
    generated: raw.generated,
    produced: raw.produced,
    seconds: raw.seconds,
    hitLimit: raw.hit_limit,
    averages: raw.averages,
    frequencies: raw.frequencies,
    // Whatever the script's `printes` statements wrote. The CLI sends this to
    // the terminal interleaved with the deals; here it comes back as one block
    // for the page to show above them.
    printes: raw.printes,
    // The hand type each deal matched, parallel to `deals`.
    dealTypes: raw.deal_types,
    // Every `HandType_*` the script declares, with its share of this run. When
    // the run was levelled, `natural` is what the measuring pass saw and
    // `delivered` what the keeps produced; in a round robin, `planned` is the
    // even split asked for and `wanted` the count each type was owed;
    // otherwise they all agree.
    handTypes: raw.hand_types,
    // Present only when the run was levelled: the scenario that actually ran,
    // and what it cost. Numbers only — the page draws the bars. `rarest_error`
    // is the relative standard error on the rate every keep divides by, which
    // is the one figure that says whether a levelling is worth trusting.
    leveling: raw.leveling,
    // Present only when the run was dealt round robin: how many complete
    // rounds, how many deals were left over, and whether the rounds were even
    // or weighted by `HandType_X_Share`.
    roundRobin: raw.round_robin,
    // `deals` is capped by the engine; `produced` counts every match. A script
    // gathering statistics over 50,000 deals returns statistics for all of them
    // and only the first few hundred deals.
    dealsTruncated: raw.produced > raw.deals.length,
  }
}

/**
 * Validate without generating. Returns `{ ok, error, line, column }`.
 *
 * Deliberately does not throw: this runs on every keystroke, and the line and
 * column come from the parser itself, so editor markers agree with the engine
 * rather than approximating it.
 */
export function checkScript(script) {
  assertReady('checkScript')
  return JSON.parse(wasmCheck(script))
}

/** The language's vocabulary, for highlighting and completion. */
export function languageInfo() {
  assertReady('languageInfo')
  return JSON.parse(wasmLanguageInfo())
}

export function version() {
  return wasmVersion()
}
