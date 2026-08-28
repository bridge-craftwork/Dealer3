// The engine, off the main thread.
//
// Generation is one synchronous call into the wasm that can run for many
// seconds. On the main thread that blocks everything: the Run button never
// paints its disabled state, a second click queues up behind the first and
// starts another run the moment the tab thaws, and nothing can report how far
// along it is. None of that is fixable from the outside — a `requestAnimationFrame`
// yield only lets the browser *reach* the blocking call sooner.
//
// So generation runs here instead. The main thread stays responsive, progress
// arrives as messages, and Cancel is a `terminate()` — which is the one form of
// cancellation that works against code already inside the wasm, since a flag
// would need the blocked thread to come back and read it.
//
// Only `generate` moved. `check_script` and `language_info` are called
// synchronously while the editor is being set up and are far too fast to be
// worth an await, so they stay on the main thread's own instance.

import init, { generate as wasmGenerate } from '@/wasm/dealer3_wasm.js'

let ready = null

self.onmessage = async (event) => {
  const { id, script, options } = event.data || {}
  try {
    // Loaded once per worker, and a worker outlives any single run.
    if (!ready) ready = init()
    await ready

    const onProgress = (message) => {
      // Passed through as the engine wrote it; the page decides what to show.
      self.postMessage({ id, type: 'progress', message })
    }

    const raw = wasmGenerate(
      script,
      options.seed,
      options.produce,
      options.maxGenerate,
      options.format,
      options.autoLevel,
      onProgress,
    )
    self.postMessage({ id, type: 'done', raw })
  } catch (e) {
    // `Error` does not survive structured cloning with its message intact in
    // every browser, so send the text.
    self.postMessage({ id, type: 'error', message: e?.message || String(e) })
  }
}
