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
import {
  createLibraryFetcher,
  dealsToRequest,
  learnLibrarySize,
  supplyLibraryPieces,
} from '@/lib/library.js'

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
// The fetching lives HERE, in the worker, and not on the main thread, because
// the wasm `Library` cannot leave the wasm instance that made it. There are two
// instances — the page's, for the editor's instant calls, and this one, for
// generating — and a `Library` is a handle into linear memory, not something
// `postMessage` can clone. The main thread could fetch the bytes and send them
// across, but only this side can say which URLs are wanted, because that answer
// comes from `needs()`. Splitting the loop over the two would put a round trip
// between every ask and its answer to no purpose.
//
// So the worker fetches, and the caching is arranged to survive the worker:
// pieces go into the browser's Cache API, which outlives a `terminate()` — how
// Cancel works — and a reload. The in-memory map on top of it is per worker and
// merely saves the cache read. See `library.js`.
//
// The `Library` itself is kept between runs, so a second run in the same region
// of the library fetches nothing at all.

let library = null
let fetchPiece = null
let piecesHeld = 0

/// Pieces to hold before releasing them all. Each is 640 KiB of tables inside
/// the wasm's memory; the fetched bytes are still cached, so releasing costs a
/// re-supply and not a download.
const MAX_HELD_PIECES = 8

/// The library, and the fetcher that feeds it, made once per worker.
function libraryHandle() {
  if (!library) {
    library = new engine.Library(engine.rpdd_manifest_url())
    fetchPiece = createLibraryFetcher({ manifestUrl: library.manifest_url })
  }
  return library
}

/// The deals this run should read, as `.zrd` bytes for `generate_from_deals`.
///
/// The order matters and is forced: the manifest says how big the library is,
/// the size is what the seed is reduced against, and only then is there an
/// index to say which pieces to fetch.
async function dealsFromLibrary(options, report) {
  const lib = libraryHandle()
  const totalDeals = await learnLibrarySize(lib, fetchPiece, report)

  // The engine's mapping, never one of ours. A JavaScript hash would send the
  // page to a different deal from the one `dealer -s N --input-deals` reads,
  // and nothing anywhere would say so — see `record_for_seed`.
  const firstDeal = engine.record_for_seed(options.seed, totalDeals)
  const requested = dealsToRequest({
    produce: options.produce,
    maxGenerate: options.maxGenerate,
    totalDeals,
  })

  const { fetched } = await supplyLibraryPieces(lib, firstDeal, requested, {
    fetchPiece,
    onProgress: report,
  })
  piecesHeld += fetched

  const zrd = lib.zrd(firstDeal, requested)
  if (piecesHeld > MAX_HELD_PIECES) {
    lib.forget_chunks()
    piecesHeld = 0
  }
  return { zrd, info: { firstDeal, totalDeals, requested, fetched } }
}

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

    // Two deal sources, one run. `generate_from_deals` is the same engine over
    // deals it was handed instead of deals it shuffled — the filter, the
    // statistics, the levelling and the output are all the ones above.
    let raw
    let libraryInfo = null
    // Getting the deals is part of the run, so it is part of the time. The
    // engine only times what it does itself, and for a library run most of the
    // work happens before it is called: fetching the pieces, and rebuilding
    // the deals from their indexes. Reporting the engine's figure alone showed
    // a number that omitted the larger half.
    let librarySeconds = 0
    if (options.source === 'library') {
      const startedLibrary = performance.now()
      const { zrd, info } = await dealsFromLibrary(options, (status) =>
        self.postMessage({ id, type: 'library', status }),
      )
      librarySeconds = (performance.now() - startedLibrary) / 1000
      libraryInfo = info
      raw = engine.generate_from_deals(
        script,
        zrd,
        options.seed,
        options.produce,
        options.maxGenerate,
        options.format,
        options.autoLevel,
        options.roundRobin,
        options.params || [],
        options.measureSeconds,
        onProgress,
      )
    } else {
      raw = engine.generate(
        script,
        options.seed,
        options.produce,
        options.maxGenerate,
        options.format,
        options.autoLevel,
        options.roundRobin,
        options.params || [],
        options.measureSeconds,
        onProgress,
      )
    }
    self.postMessage({
      id,
      type: 'done',
      raw,
      threads: pool.threads,
      library: libraryInfo,
      librarySeconds,
    })
  } catch (e) {
    // `Error` does not survive structured cloning with its message intact in
    // every browser, so send the text.
    self.postMessage({ id, type: 'error', message: e?.message || String(e) })
  }
}
