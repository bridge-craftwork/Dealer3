// The dealer3 engine, compiled to WebAssembly.
//
// One module owns loading it, so the rest of the app can await `ready()` and
// then call synchronously. Everything runs in the browser: no script, no deal
// and no keystroke is sent anywhere.

import init, {
  check_script as wasmCheck,
  script_uses_double_dummy as wasmUsesDoubleDummy,
  script_params as wasmScriptParams,
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
      // Fetching a piece of the solved-deal library: a network wait, where
      // generating is immediate. Reported from the first byte rather than held
      // back like the progress bars, because there is nothing else to see.
      if (data.type === 'library') {
        options.onLibrary?.(data.status)
        return
      }
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
      if (data.type === 'done') finish(resolve, data)
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
        // Where the deals come from: 'random' shuffles from the seed, 'library'
        // draws from the published solved-deal library, where the seed picks
        // the starting position instead. The worker does the fetching — see
        // engine.worker.js for why it cannot be done out here.
        source: options.source,
        // A plain copy: the caller's array is a Vue ref's, and a reactive
        // proxy cannot be structured-cloned — postMessage fails outright with
        // "[object Object] could not be cloned", which says nothing about
        // where it came from.
        params: Array.from(options.params || []),
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
    /// What to put where `$0`-`$9` stand, in `--param`'s own `N=TEXT` spelling.
    /// A parameter left out here falls back to the script's own `# param`
    /// default, and fails the run if it has none.
    params = [],
    /// Where the deals come from: `'random'` shuffles them from the seed, as
    /// this page always has; `'library'` draws them from the published
    /// solved-deal library, where the seed picks a starting position instead
    /// and every deal arrives with its double-dummy table.
    source = 'random',
    /// Called with `{ phase, produced, generated, target }` as the run goes.
    onProgress = null,
    /// Called with `{ stage, done, total, url }` while pieces of the library
    /// are being fetched. Only a library run reports this, and it reports it
    /// before any deal has been looked at.
    onLibrary = null,
    /// Resolves — or rejects — if the caller abandons the run.
    signal = null,
  } = {},
) {
  const message = await runInWorker(script, {
    seed,
    produce,
    maxGenerate,
    format,
    autoLevel,
    roundRobin,
    source,
    params,
    onProgress,
    onLibrary,
    signal,
  })
  const raw = JSON.parse(message.raw)
  return {
    deals: raw.deals,
    generated: raw.generated,
    produced: raw.produced,
    // The engine's own time plus what it took to get the deals to it. For a
    // library run the fetch and the rebuilding from indexes are most of the
    // work, and they happen before the engine is called — so its figure alone
    // reports a fraction of the wait and calls it the total.
    seconds: raw.seconds + (message.librarySeconds || 0),
    // Kept apart as well, for anyone who wants to know which half was which.
    engineSeconds: raw.seconds,
    librarySeconds: message.librarySeconds || 0,
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
    // What the run read, when it read rather than dealt: how many deals
    // arrived, how many came with double-dummy tables, what could not be read.
    // A page has no stderr, and this is the only thing that tells a run over a
    // short library from a run over all of it — neither `produced` nor
    // `hitLimit` can, since a run that exhausts its deals has not hit its
    // budget. Absent for a run that shuffled.
    input: raw.input || null,
    // Where in the library this run started, how big the library is and how
    // many deals were asked for. From the worker rather than the engine: it is
    // what the page asked for, against which `input.read` is worth reading.
    library: message.library || null,
    // Whether the format asked for renders deals at all: false only under
    // `none`, which keeps the statistics and collects no hands. It travels with
    // the result rather than being read off the format control, so changing
    // that control without running again cannot make the page describe what is
    // on screen as something else. An empty `deals` cannot say it — a run that
    // matched nothing has one too.
    dealsRendered: raw.renders_deals,
    // `deals` is capped by the engine; `produced` counts every match. A script
    // gathering statistics over 50,000 deals returns statistics for all of them
    // and only the first few hundred deals. Under `none` there is no
    // truncation to report: nothing was going to be shown in the first place.
    dealsTruncated: raw.renders_deals && raw.produced > raw.deals.length,
  }
}

/**
 * Validate without generating. Returns `{ ok, error, line, column }`.
 *
 * Deliberately does not throw: this runs on every keystroke, and the line and
 * column come from the parser itself, so editor markers agree with the engine
 * rather than approximating it.
 */
export function checkScript(script, params = []) {
  assertReady('checkScript')
  return JSON.parse(wasmCheck(script, params))
}

/**
 * Whether anything in the script can reach the double-dummy solver: `true`,
 * `false`, or `undefined` when the script does not parse, which is not the same
 * as no.
 *
 * The page asks before offering pre-solved deals, since a script that never
 * calls `tricks()`, `dds()` or `par()` gains nothing from deals that arrive
 * with the answers. Answered by the engine off the parsed program rather than
 * by searching the text here: `t = tricks(north, notrump)` mentions one and
 * `x = t` does not, and a comment mentioning `par` is not a call.
 */
export function usesDoubleDummy(script, params = []) {
  assertReady('usesDoubleDummy')
  return wasmUsesDoubleDummy(script, params)
}

/**
 * What a script says about its own `$0`-`$9`: `{ ok, error, params }`, where
 * each entry is `{ index, default, description, declaredOn, usedOn }`.
 *
 * This is what lets the page ask for the parameters a script wants. The `$n`
 * occurrences on their own give it nowhere to put a label and no sensible
 * starting value, which is why a parameterised scenario used to fail here with
 * an error naming a line the reader could not act on.
 */
export function scriptParams(script) {
  assertReady('scriptParams')
  const raw = JSON.parse(wasmScriptParams(script))
  return {
    ok: raw.ok,
    error: raw.error,
    params: raw.params.map((p) => ({
      index: p.index,
      default: p.default,
      description: p.description,
      declaredOn: p.declared_on,
      usedOn: p.used_on,
    })),
  }
}

/** The language's vocabulary, for highlighting and completion. */
export function languageInfo() {
  assertReady('languageInfo')
  return JSON.parse(wasmLanguageInfo())
}

export function version() {
  return wasmVersion()
}
