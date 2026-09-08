// The solved-deal library, from the page's side.
//
// Richard Pavlicek solved 10,485,760 deals over nearly two years of computer
// time. A deal that arrives already solved makes `tricks()`, `dds()` and
// `par()` lookups rather than searches — about 23ms a deal saved, which is the
// difference between double-dummy being usable in a tab and not.
//
// Only the tables are published, as 640 KiB pieces; the deals are a pure
// function of their index and the engine recreates them. Everything about which
// piece holds which deal — the boundaries, the stitching, the wrap at the end —
// belongs to the wasm `Library`, and none of it is repeated here. What this
// module holds is the page's side of the bargain:
//
//   * fetching, because `fetch` is asynchronous and a wasm export cannot await
//     one, so the library says what it needs and is told;
//   * remembering what has been fetched, so changing a script does not spend
//     another 640 KiB;
//   * how much of the library one run should ask for;
//   * and what to say about what came back.
//
// It does NOT work out where in the library a seed starts. That mapping is the
// engine's `record_for_seed`, and doing it here in JavaScript is the one
// mistake nothing would catch: a hash that looked perfectly reasonable would
// simply read different deals from the same seed than the command line does,
// with both runs looking healthy. See `wasm/verify.mjs`.

/// Where fetched pieces are kept between visits.
///
/// Versioned in the name so a change of shape is a new store rather than a
/// migration. A piece never changes — `rpdd-042.zdd` is the same 640 KiB
/// forever — so nothing here expires.
export const CACHE_NAME = 'dealer3-library-v1'

/// The most deals one run will ask the library for.
///
/// A download budget, not arithmetic: this is roughly one published piece, so
/// a run costs one fetch, or two where it straddles a boundary. Asking for
/// everything a `Max generate` of a million allows would be sixteen pieces —
/// ten megabytes — to look at twenty deals.
///
/// It bounds how selective a filter the library can satisfy, and the run report
/// says how many deals were actually read, so a script that filters harder than
/// this can see that it ran out of deals rather than out of matches.
export const MAX_LIBRARY_DEALS = 65536

/// Pieces held in memory at once, beyond which the oldest is dropped.
///
/// Each is 640 KiB of tables. The browser's cache still has them, so dropping
/// one costs a cache read rather than a fetch.
const MEMORY_PIECES = 8

/// How many ask/supply rounds before something is wrong.
///
/// The protocol takes two: the manifest, then the pieces it names. A third is
/// slack; a fourth means the library is asking for something it is never
/// satisfied by, and looping for ever on a network resource is worse than
/// failing.
const MAX_ROUNDS = 4

/**
 * How many deals to ask the library for.
 *
 * Enough to filter through — `Max generate` is what the page already means by
 * "how much work is this allowed" — but never more than the budget above, and
 * never more than the library holds, which would ask it to come round to deals
 * it has already served.
 *
 * At least `produce`, so that asking for more deals than the budget is a short
 * run reported honestly rather than a request refused.
 */
export function dealsToRequest({ produce = 1, maxGenerate = 0, totalDeals = 0 } = {}) {
  const wanted = Math.max(1, Math.min(maxGenerate || MAX_LIBRARY_DEALS, MAX_LIBRARY_DEALS), produce)
  return totalDeals > 0 ? Math.min(wanted, totalDeals) : wanted
}

/**
 * Bytes for one library URL, remembered in memory and in the browser's cache.
 *
 * Two levels because they fail differently. The `Map` is free and lives as long
 * as the worker; the Cache API survives a cancelled run — which terminates the
 * worker — and a reload, and is where "changing the script does not refetch"
 * actually holds. Neither is required: a browser with no `caches` (an insecure
 * origin, a private window in some browsers) falls back to the map, and one
 * with neither still works, slowly.
 *
 * Everything but the manifest is treated as immutable, because it is: a piece
 * is named after the deals in it. The manifest can gain chunks, so it is
 * remembered only in memory and re-read on the next visit.
 *
 * @param {object} options
 * @param {string} options.manifestUrl the one URL not written to the cache
 * @param {Function} [options.fetchImpl] for tests; defaults to global `fetch`
 * @param {CacheStorage} [options.cacheStorage] for tests; defaults to `caches`
 * @param {string} [options.cacheName]
 * @returns {(url: string) => Promise<Uint8Array>}
 */
export function createLibraryFetcher({
  manifestUrl = '',
  fetchImpl = undefined,
  cacheStorage = undefined,
  cacheName = CACHE_NAME,
} = {}) {
  const doFetch = fetchImpl || ((url) => globalThis.fetch(url))
  const storage = cacheStorage === undefined ? globalThis.caches : cacheStorage
  const memory = new Map()
  let opening = null

  // Opened once and never re-attempted: a browser that refuses the cache once
  // refuses it every time, and asking again per piece would be a rejected
  // promise per fetch.
  const store = () => {
    if (!storage) return Promise.resolve(null)
    if (!opening) opening = Promise.resolve(storage.open(cacheName)).catch(() => null)
    return opening
  }

  const remember = (url, bytes) => {
    memory.set(url, bytes)
    while (memory.size > MEMORY_PIECES) {
      const oldest = memory.keys().next().value
      if (oldest === undefined) break
      memory.delete(oldest)
    }
    return bytes
  }

  return async function piece(url) {
    const held = memory.get(url)
    if (held) return held

    const persist = url !== manifestUrl
    const cache = persist ? await store() : null
    if (cache) {
      const hit = await cache.match(url).catch(() => null)
      if (hit && hit.ok) return remember(url, new Uint8Array(await hit.arrayBuffer()))
    }

    let response
    try {
      response = await doFetch(url)
    } catch (e) {
      // A network failure reads as "Failed to fetch", which says nothing about
      // what was being fetched or why the page wanted it.
      throw new Error(
        `Could not reach the solved-deal library at ${url} (${e?.message || e}). ` +
          'Check the connection, or switch back to random deals.',
      )
    }
    if (!response.ok) {
      throw new Error(
        `The solved-deal library answered HTTP ${response.status} for ${url}. ` +
          'Switch back to random deals, or try again.',
      )
    }
    const buffer = await response.arrayBuffer()
    if (cache) {
      // Failing to cache is not failing to fetch: a full quota should cost a
      // refetch next time, not the run.
      await cache.put(url, new Response(buffer)).catch(() => {})
    }
    return remember(url, new Uint8Array(buffer))
  }
}

/**
 * Learn how big the library is, fetching the manifest and nothing else.
 *
 * The size has to be known before the seed can name a starting deal, and the
 * starting deal before any piece can be chosen — so this round deliberately
 * fetches only the first URL `needs` asks for, which is the manifest. Taking
 * the whole list would pull a 640 KiB piece chosen by an index nobody has
 * worked out yet.
 *
 * @param {object} lib the wasm `Library`
 * @param {(url: string) => Promise<Uint8Array>} fetchPiece
 * @param {(status: object) => void} [onProgress]
 * @returns {Promise<number>} how many deals the library holds
 */
export async function learnLibrarySize(lib, fetchPiece, onProgress) {
  for (let round = 0; lib.total_deals === undefined && round < MAX_ROUNDS; round++) {
    const [url] = lib.needs(0, 1)
    if (!url) break
    onProgress?.({ stage: 'manifest', done: 0, total: 1, url })
    lib.supply(url, await fetchPiece(url))
  }
  if (lib.total_deals === undefined) {
    throw new Error(
      `The library at ${lib.manifest_url} did not say how many deals it holds.`,
    )
  }
  return lib.total_deals
}

/**
 * Fetch and supply every piece the run needs, so `lib.zrd(...)` can answer.
 *
 * The loop is the protocol from `docs/WASM.md`: ask what is missing, fetch it,
 * hand it back, ask again. A piece already held — from an earlier run, or from
 * the cache — is not asked for, which is what makes a second run over the same
 * region free.
 *
 * @returns {Promise<{fetched: number}>} how many pieces this call had to get
 */
export async function supplyLibraryPieces(lib, firstDeal, count, { fetchPiece, onProgress } = {}) {
  let fetched = 0
  for (let round = 0; round < MAX_ROUNDS; round++) {
    const needed = lib.needs(firstDeal, count)
    if (!needed.length) return { fetched }
    for (let i = 0; i < needed.length; i++) {
      onProgress?.({ stage: 'pieces', done: i, total: needed.length, url: needed[i] })
      lib.supply(needed[i], await fetchPiece(needed[i]))
      fetched++
    }
    onProgress?.({ stage: 'pieces', done: needed.length, total: needed.length })
  }
  throw new Error(
    'The solved-deal library kept asking for more pieces than it could use; ' +
      'nothing was read. This is a bug — please report it.',
  )
}

const count = (n) => Number(n || 0).toLocaleString()

/**
 * What to say about the deals a run read.
 *
 * A page has no stderr, and this is the half of a library run that nothing else
 * reports: a run handed forty deals from a library of four thousand produces
 * fewer matches and looks exactly like a selective filter. `read` against what
 * was asked for is the only thing that tells the two apart.
 *
 * @param {object} input the engine's `input` report
 * @param {object} [library] `{ firstDeal, totalDeals, requested }` for a run
 *   drawn from the solved-deal library
 * @returns {{summary: string, warnings: string[]}}
 */
export function describeInput(input, library = null) {
  if (!input) return { summary: '', warnings: [] }

  const solved = input.solved || 0
  const read = input.read || 0
  const where =
    library && library.totalDeals
      ? ` from the solved-deal library, starting at deal ${count(library.firstDeal)} of ${count(
          library.totalDeals,
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
  const requested = library?.requested
  if (requested && read < requested) {
    warnings.push(
      `Asked for ${count(requested)} deals and read ${count(read)}. ` +
        'The run had fewer deals to filter than it expected, so a short result here ' +
        'is not necessarily a selective condition.',
    )
  }
  if (input.unsolved) {
    warnings.push(
      `${count(input.unsolved)} deal${input.unsolved === 1 ? '' : 's'} arrived without a ` +
        'double-dummy table and will be solved on demand, which is the slow path this ' +
        'library exists to avoid.',
    )
  }
  if (library?.totalDeals && requested >= library.totalDeals) {
    warnings.push(
      `This run read the whole library (${count(library.totalDeals)} deals). ` +
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
 * The one line to show while pieces of the library are being fetched.
 *
 * Worth showing at all because this is the only part of a run that waits on
 * something outside the tab: generating is immediate, and a page that sits
 * still for a second and a half with nothing said reads as broken.
 *
 * @param {object} status from `learnLibrarySize` and `supplyLibraryPieces`
 * @returns {string} empty once there is nothing left to fetch
 */
export function libraryStatusText(status) {
  if (!status) return ''
  if (status.stage === 'manifest') return "Reading the solved-deal library's index…"
  if (status.done >= status.total) return 'Running the script over the library\u2019s deals…'
  return `Fetching the solved-deal library — piece ${status.done + 1} of ${status.total}…`
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
