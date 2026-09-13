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

import init, * as engine from '@/wasm/dealer3_wasm.js'
import { runEnvelope } from './envelope.js'
import { createSyncFetcher, forgetOldLibraryCache } from '@/lib/library.js'

let ready = null

/// Bring up the engine, and its thread pool if this build has one.
///
/// A threaded build spawns workers of its own that share the wasm's memory, so
/// it needs `SharedArrayBuffer` — which exists only on a page served with COOP
/// and COEP. Both are set in `public/_headers`; a dev server or a host that
/// drops them leaves `crossOriginIsolated` false, and then the pool cannot
/// start.
///
/// Failing to start one is not an error. The engine falls back to this thread
/// and deals exactly the same deals, only slower — which is what makes it safe
/// to try and carry on.
async function bringUp() {
  await init()
  if (typeof engine.start_threads !== 'function') {
    return { threads: 1, why: 'this build has no thread pool', supported: false }
  }
  if (!self.crossOriginIsolated) {
    return { threads: 1, why: 'the page is not cross-origin isolated', supported: true }
  }
  // Every core the browser admits to, up to MAX_THREADS. Every run deals on
  // all of it; `wasm/build.sh` carries the measurements.
  const wanted = Math.max(1, Math.min(navigator.hardwareConcurrency || 1, MAX_THREADS))
  try {
    await engine.start_threads(wanted)
    return { threads: wanted, why: null, supported: true }
  } catch (e) {
    return { threads: 1, why: e?.message || String(e), supported: true }
  }
}

/// The most workers to ask for, however many cores the browser reports.
///
/// Scaling was measured out to twelve and is flattening well before it: on a
/// twelve-core M4 Pro a real scenario gains 6.1x at eight threads and 6.9x at
/// twelve, and a double-dummy run 2.90x and 2.93x. Past twelve each worker is a
/// thread and a stack for a share of the work that has stopped shrinking, so a
/// thirty-two core machine asking for thirty-two would be paying for the ones
/// that are not helping. `wasm/build.sh` carries the full table.
const MAX_THREADS = 12

// --- The solved-deal library ----------------------------------------------
//
// The engine reads the library itself, a slice at a time as the run asks for
// deals (#21), and calls back here for each piece it needs. The callback is
// synchronous because the run is: a deal is asked for in the middle of one call
// into the wasm, with nowhere to await. A synchronous request is allowed in a
// worker, and blocks only this one — which is blocked in the run already.
//
// Nothing is cached here. The pieces are served `immutable`, so the browser's
// own cache answers a repeat and evicts what goes unused. See `library.js`.

// Versions of the page before #21 kept every piece in the Cache API for good.
// Cleared once per worker, and never waited for.
forgetOldLibraryCache()

self.onmessage = async (event) => {
  const { id, type, script, options } = event.data || {}
  try {
    // Loaded once per worker, and a worker outlives any single run — so the
    // thread pool is started once too, not per run.
    if (!ready) ready = bringUp()

    // "How many threads did you get?", asked before any run. The page shows the
    // answer, because a console line is only seen by someone who already
    // suspects — and the failure this guards against is one nobody suspects:
    // the run works, it is simply four times slower than it should be.
    if (type === 'pool') {
      const pool = await ready
      self.postMessage({
        id,
        type: 'pool',
        threads: pool.threads,
        why: pool.why,
        supported: pool.supported,
      })
      return
    }

    const pool = await ready
    // Once per worker, not per run: a page that quietly fell back to one thread
    // looks exactly like a slow scenario, which is how the first threaded build
    // shipped serial without anyone noticing.
    if (!pool.reported) {
      pool.reported = true
      console.info(
        pool.threads > 1
          ? `dealer3: dealing on ${pool.threads} threads`
          : `dealer3: dealing on one thread (${pool.why})`,
      )
    }

    const onProgress = (message) => {
      // Passed through as the engine wrote it; the page decides what to show.
      self.postMessage({ id, type: 'progress', message })
    }

    // Two deal sources, one run. The engine takes an envelope describing the
    // run; for the library it also takes a way to fetch pieces, and reads them
    // as it goes. The filter, the statistics, the levelling and the output are
    // the same either way.
    let raw
    // Time spent waiting on the library. It is inside the run now — the engine
    // fetches as it reads — so the engine's own clock includes it; this is kept
    // apart so a slow network is not blamed on the engine, or credited to it.
    let librarySeconds = 0
    if (options.source === 'library') {
      const manifestUrl = engine.rpdd_manifest_url()
      const fetcher = createSyncFetcher({
        manifestUrl,
        onFetch: (status) => self.postMessage({ id, type: 'library', status }),
      })
      try {
        raw = engine.run_library_json(
          runEnvelope(script, options),
          manifestUrl,
          fetcher.fetch,
          onProgress,
        )
      } finally {
        librarySeconds = fetcher.seconds()
      }
    } else {
      raw = engine.run_json(runEnvelope(script, options), undefined, onProgress)
    }
    self.postMessage({ id, type: 'done', raw, threads: pool.threads, librarySeconds })
  } catch (e) {
    // `Error` does not survive structured cloning with its message intact in
    // every browser, so send the text.
    self.postMessage({ id, type: 'error', message: e?.message || String(e) })
  }
}
