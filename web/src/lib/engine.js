// The dealer3 engine, compiled to WebAssembly.
//
// One module owns loading it, so the rest of the app can await `ready()` and
// then call synchronously. Everything runs in the browser: no script, no deal
// and no keystroke is sent anywhere.

import init, {
  generate as wasmGenerate,
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
 * Throws with the engine's message on a parse or evaluation error.
 */
export function generate(script, { seed = 1, produce = 20, maxGenerate = 500000, format = 'oneline' } = {}) {
  assertReady('generate')
  const raw = JSON.parse(wasmGenerate(script, seed, produce, maxGenerate, format))
  return {
    deals: raw.deals,
    generated: raw.generated,
    produced: raw.produced,
    seconds: raw.seconds,
    hitLimit: raw.hit_limit,
    averages: raw.averages,
    frequencies: raw.frequencies,
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
