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
  if (typeof engine.start_threads !== 'function') return { threads: 1, why: 'built without threads' }
  if (!self.crossOriginIsolated) {
    return { threads: 1, why: 'the page is not cross-origin isolated' }
  }
  // Four, not every core. Measured on a selective scenario, deals characterized
  // in six seconds: 1 thread 5.0M, 2 8.9M, 3 11.6M, 4 13.8M, 5 10.7M, 6 8.5M,
  // 8 6.6M, 12 3.3M. It peaks at four and then falls below one thread — asking
  // for twelve is worse than asking for none.
  const wanted = Math.max(1, Math.min(navigator.hardwareConcurrency || 1, 4))
  try {
    await engine.start_threads(wanted)
    return { threads: wanted, why: null }
  } catch (e) {
    return { threads: 1, why: e?.message || String(e) }
  }
}

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
  const { id, script, options } = event.data || {}
  try {
    // Loaded once per worker, and a worker outlives any single run — so the
    // thread pool is started once too, not per run.
    if (!ready) ready = bringUp()
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
    if (options.source === 'library') {
      const { zrd, info } = await dealsFromLibrary(options, (status) =>
        self.postMessage({ id, type: 'library', status }),
      )
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
        onProgress,
      )
    }
    self.postMessage({ id, type: 'done', raw, threads: pool.threads, library: libraryInfo })
  } catch (e) {
    // `Error` does not survive structured cloning with its message intact in
    // every browser, so send the text.
    self.postMessage({ id, type: 'error', message: e?.message || String(e) })
  }
}
