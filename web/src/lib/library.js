// The solved-deal library, from the page's side.
//
// Richard Pavlicek solved 10,485,760 deals over nearly two years of computer
// time. A deal that arrives already solved makes `tricks()`, `dds()` and
// `par()` lookups rather than searches — about 23ms a deal saved, which is the
// difference between double-dummy being usable in a tab and not.
//
// Only the tables are published, as 640 KiB pieces; the deals are a pure
// function of their index and the engine recreates them. The engine also
// decides which pieces a run needs, and when: it reads the library a slice at a
// time as the run asks for deals (#21), so a run that finds its matches in the
// first piece fetches one, and a run that needs the whole library reads all of
// it while holding a piece at a time. What is left here is the page's side:
//
//   * fetching a URL when the engine asks, synchronously, because it asks in
//     the middle of a run that has nowhere to await;
//   * clearing out the copy of the library earlier versions of this page kept;
//   * and what to say about what a run read.
//
// ## Why there is no cache here
//
// The pieces are served `immutable` with a year's lifetime, so the browser's
// own HTTP cache answers a repeat — and that cache is bounded, and evicts what
// has not been used. Before #21 the page also wrote every piece into the Cache
// API, which is the site's own storage and is not evicted that way. With a run
// capped at one piece that stayed small; a run reading the whole library would
// have left 100 MB there for good.

/// Where versions of this page before #21 kept every piece they fetched.
export const OLD_CACHE_NAME = 'dealer3-library-v1'

/**
 * Delete the copy of the library earlier versions of this page kept.
 *
 * Best effort, and never a reason to fail anything: a browser with no Cache API,
 * or one that refuses it, has nothing there to delete.
 *
 * @param {CacheStorage} [cacheStorage] for tests; defaults to `caches`
 * @returns {Promise<boolean>} whether anything was deleted
 */
export async function forgetOldLibraryCache(cacheStorage = globalThis.caches) {
  if (!cacheStorage || typeof cacheStorage.delete !== 'function') return false
  try {
    return await cacheStorage.delete(OLD_CACHE_NAME)
  } catch {
    return false
  }
}

/**
 * A synchronous fetch for the engine to call while a run reads the library.
 *
 * Synchronous because the engine asks for a piece in the middle of a run, which
 * is one call into the wasm with nowhere to await. A synchronous request is
 * allowed in a worker — which is where runs happen — and blocks only that
 * worker, which is blocked in the run anyway. Cancel still works, because Cancel
 * terminates the worker.
 *
 * @param {object} [options]
 * @param {string} [options.manifestUrl] so the status can tell the index from a piece
 * @param {(status: object) => void} [options.onFetch] told `{ stage, url, pieces }`
 *   before each request and `{ stage: 'read', pieces }` after a piece arrives
 * @param {Function} [options.XhrImpl] for tests; defaults to `XMLHttpRequest`
 * @param {() => number} [options.now] for tests; defaults to `performance.now`
 * @returns {{ fetch: (url: string) => Uint8Array, seconds: () => number, pieces: () => number }}
 */
export function createSyncFetcher({ manifestUrl = '', onFetch, XhrImpl, now } = {}) {
  const Xhr = XhrImpl || globalThis.XMLHttpRequest
  const clock = now || (() => globalThis.performance.now())
  let spent = 0
  let pieces = 0

  function fetch(url) {
    const isManifest = url === manifestUrl
    if (!isManifest) pieces += 1
    onFetch?.(isManifest ? { stage: 'manifest', url } : { stage: 'piece', url, pieces })

    const started = clock()
    const request = new Xhr()
    try {
      request.open('GET', url, false)
      request.responseType = 'arraybuffer'
      request.send()
    } catch (e) {
      // A network failure reads as "NetworkError", which says nothing about what
      // was being fetched or why the page wanted it.
      throw new Error(
        `Could not reach the solved-deal library at ${url} (${e?.message || e}). ` +
          'Check the connection, or switch back to random deals.',
      )
    } finally {
      spent += clock() - started
    }
    if (request.status === 0) {
      throw new Error(
        `Could not reach the solved-deal library at ${url}. ` +
          'Check the connection, or switch back to random deals.',
      )
    }
    if (request.status < 200 || request.status >= 300) {
      throw new Error(
        `The solved-deal library answered HTTP ${request.status} for ${url}. ` +
          'Switch back to random deals, or try again.',
      )
    }
    if (!isManifest) onFetch?.({ stage: 'read', pieces })
    return new Uint8Array(request.response)
  }

  return { fetch, seconds: () => spent / 1000, pieces: () => pieces }
}

const count = (n) => Number(n || 0).toLocaleString()

/**
 * What to say about the deals a run read.
 *
 * A page has no stderr, and this is the half of a library run that nothing else
 * reports. Where in the library the run started is what reproduces it, and
 * whether it read the whole library is what tells a filter that found few
 * matches in all of it from a run that stopped early.
 *
 * @param {object} input the engine's `input` report; `input.library` is
 *   `{ first_record, records, read_whole }` for a run over the library
 * @returns {{summary: string, warnings: string[]}}
 */
export function describeInput(input) {
  if (!input) return { summary: '', warnings: [] }

  const solved = input.solved || 0
  const read = input.read || 0
  const library = input.library
  const where =
    library && library.records
      ? ` from the solved-deal library, starting at deal ${count(library.first_record)} of ${count(
          library.records,
        )}`
      : ''
  const tables =
    read > 0 && solved === read
      ? ' Every one arrived with its double-dummy table, so tricks(), dds() and par() were lookups rather than searches.'
      : solved > 0
        ? ` ${count(solved)} of them arrived with double-dummy tables.`
        : ' None of them came with double-dummy tables.'
  const summary = `Read ${count(read)} deal${read === 1 ? '' : 's'}${where}.${tables}`

  const warnings = []
  if (input.unsolved) {
    warnings.push(
      `${count(input.unsolved)} deal${input.unsolved === 1 ? '' : 's'} arrived without a ` +
        'double-dummy table and will be solved on demand, which is the slow path this ' +
        'library exists to avoid.',
    )
  }
  if (library?.read_whole) {
    warnings.push(
      `This run read the whole library (${count(library.records)} deals) and stopped there. ` +
        'A longer one would come round to deals it has already seen, and average and ' +
        'frequency would count them twice.',
    )
  }
  if (input.skipped_count) {
    const reasons = (input.skipped || []).slice(0, 3).join('; ')
    warnings.push(
      `${count(input.skipped_count)} record${input.skipped_count === 1 ? '' : 's'} could not ` +
        `be read${reasons ? `: ${reasons}` : ''}. The deals around them were still read.`,
    )
  }
  for (const note of input.notes || []) warnings.push(note)

  return { summary, warnings }
}

/**
 * The one line to show while a run is reading the library.
 *
 * Worth showing because fetching is the one part of a run that waits on
 * something outside the tab, and it now happens while the run goes rather than
 * before it — so the line says which piece, and that the run carries on.
 *
 * @param {object} status from `createSyncFetcher`'s `onFetch`
 * @returns {string} empty when there is nothing to say
 */
export function libraryStatusText(status) {
  if (!status) return ''
  if (status.stage === 'manifest') return "Reading the solved-deal library's index…"
  if (status.stage === 'piece') return `Fetching the solved-deal library — piece ${status.pieces}…`
  if (status.stage === 'read') {
    return `Running the script over the library\u2019s deals — ${count(status.pieces)} piece${
      status.pieces === 1 ? '' : 's'
    } read…`
  }
  return ''
}

/**
 * What to say to someone who chose pre-solved deals for a script that asks no
 * double-dummy question.
 *
 * `usesDoubleDummy` comes from the engine, which reads it off the parsed
 * program: `t = tricks(north, notrump)` mentions one and `x = t` does not, and
 * a comment mentioning `par` is not a call. `undefined` means the script could
 * not be parsed, which is not the same as no.
 *
 * @returns {string} empty when there is nothing worth saying
 */
export function pointlessLibraryWarning(usesDoubleDummy) {
  if (usesDoubleDummy !== false) return ''
  return (
    'This script never calls tricks(), dds() or par(), so pre-solved deals buy it ' +
    'nothing — it will download the library and use none of the answers. Random deals ' +
    'will be faster.'
  )
}

/// About how long one double-dummy search takes here, in milliseconds.
///
/// Measured through `bridge-solver`, which is what `tricks()`, `dds()` and
/// `par()` reach. Deliberately a round number: it is an order of magnitude for
/// someone deciding between a download and a wait, not a promise, and it is
/// five orders of magnitude above what a deal costs otherwise.
export const SOLVE_MS = 23

/**
 * What to say to someone who chose random deals for a script that does ask a
 * double-dummy question — the costly direction, and the one that said nothing.
 *
 * The same `usesDoubleDummy` the warning above reads, read the other way. That
 * warning wastes a 640 KiB download; this one turns a run that would have taken
 * a moment into one that takes minutes, so it is worth saying what it costs
 * rather than only that it is slower.
 *
 * `produce` is the floor on the number of searches, not the number: a script
 * whose call is in the condition solves every deal it *generates*, which is far
 * more. Hence "at least".
 *
 * @param {boolean|undefined} usesDoubleDummy from the engine; `undefined` while
 *   it is loading or the script does not parse, which is not the same as no
 * @param {number} produce deals the run has been asked for
 * @returns {string} empty when there is nothing worth saying
 */
export function slowRandomWarning(usesDoubleDummy, produce) {
  if (usesDoubleDummy !== true) return ''
  const deals = Number.isFinite(produce) && produce > 0 ? Math.floor(produce) : 0
  const cost = deals ? ` — at least ${deals} × ${SOLVE_MS} ms, so about ${humanSeconds(deals * SOLVE_MS)}` : ''
  return (
    `This script calls tricks(), dds() or par(), so every deal has to be solved here` +
    `${cost}, and more again if the call is in the condition, where every deal ` +
    `generated is solved rather than every deal produced. Pre-solved deals arrive with ` +
    `the answers already in them, so the same questions are lookups.`
  )
}

/**
 * A duration in milliseconds, said the way someone waiting would say it.
 *
 * @param {number} ms
 * @returns {string}
 */
function humanSeconds(ms) {
  const seconds = ms / 1000
  if (seconds < 1) return 'under a second'
  if (seconds < 90) return `${Math.round(seconds)} seconds`
  const minutes = seconds / 60
  if (minutes < 90) return `${Math.round(minutes)} minutes`
  return `${(minutes / 60).toFixed(1)} hours`
}
