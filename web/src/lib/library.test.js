import { describe, it, expect, vi } from 'vitest'
import {
  MAX_LIBRARY_DEALS,
  createLibraryFetcher,
  dealsToRequest,
  describeInput,
  learnLibrarySize,
  libraryStatusText,
  pointlessLibraryWarning,
  slowRandomWarning,
  supplyLibraryPieces,
} from './library.js'

const MANIFEST = 'https://example.test/data/manifest.json'
const PIECE = (n) => `https://example.test/data/zdd/rpdd-00${n}.zdd`

/// A stand-in for the wasm `Library`, with the same ask/supply protocol: the
/// manifest first, then the pieces a run crosses. Nothing here is the real
/// arithmetic — which piece holds which deal is Rust's, and tested there — only
/// the shape of the conversation the page has to hold up its end of.
function fakeLibrary({ dealsPerChunk = 10, totalDeals = 100 } = {}) {
  const held = new Set()
  let manifestIn = false
  return {
    manifest_url: MANIFEST,
    get total_deals() {
      return manifestIn ? totalDeals : undefined
    },
    needs(firstDeal, count) {
      if (!manifestIn) return [MANIFEST]
      const wanted = []
      const first = Math.floor(firstDeal / dealsPerChunk)
      const last = Math.floor((firstDeal + count - 1) / dealsPerChunk)
      for (let n = first; n <= last; n++) if (!held.has(n)) wanted.push(PIECE(n))
      return wanted
    },
    supply(url) {
      if (url === MANIFEST) manifestIn = true
      else held.add(Number(url.match(/rpdd-0*(\d+)\.zdd$/)[1]))
    },
    held,
  }
}

describe('dealsToRequest', () => {
  it('asks for as much as the run is allowed to look at', () => {
    expect(dealsToRequest({ produce: 20, maxGenerate: 5000, totalDeals: 10485760 })).toBe(5000)
  })

  it('never asks for more than the download budget', () => {
    expect(dealsToRequest({ produce: 20, maxGenerate: 1000000, totalDeals: 10485760 })).toBe(
      MAX_LIBRARY_DEALS,
    )
  })

  it('never asks for more than the library holds, which would repeat deals', () => {
    expect(dealsToRequest({ produce: 20, maxGenerate: 1000000, totalDeals: 10 })).toBe(10)
  })

  it('still asks for as many as the run must produce', () => {
    expect(dealsToRequest({ produce: 400, maxGenerate: 10, totalDeals: 10485760 })).toBe(400)
  })
})

describe('learnLibrarySize', () => {
  it('fetches the manifest and no piece, because the seed cannot pick one yet', async () => {
    const lib = fakeLibrary()
    const asked = []
    const fetchPiece = vi.fn(async (url) => {
      asked.push(url)
      return new Uint8Array()
    })
    expect(await learnLibrarySize(lib, fetchPiece)).toBe(100)
    expect(asked).toEqual([MANIFEST])
  })

  it('says so when a library never announces its size', async () => {
    const lib = { total_deals: undefined, manifest_url: MANIFEST, needs: () => [], supply() {} }
    await expect(learnLibrarySize(lib, async () => new Uint8Array())).rejects.toThrow(
      /did not say how many deals/,
    )
  })
})

describe('supplyLibraryPieces', () => {
  it('fetches every piece the run crosses, and then stops asking', async () => {
    const lib = fakeLibrary({ dealsPerChunk: 10 })
    lib.supply(MANIFEST)
    const asked = []
    const fetchPiece = async (url) => {
      asked.push(url)
      return new Uint8Array()
    }
    const { fetched } = await supplyLibraryPieces(lib, 8, 5, { fetchPiece })
    expect(asked).toEqual([PIECE(0), PIECE(1)])
    expect(fetched).toBe(2)
    expect(lib.needs(8, 5)).toEqual([])
  })

  it('fetches nothing for a region already held, which is what makes a re-run free', async () => {
    const lib = fakeLibrary()
    lib.supply(MANIFEST)
    const fetchPiece = vi.fn(async () => new Uint8Array())
    await supplyLibraryPieces(lib, 0, 5, { fetchPiece })
    expect(fetchPiece).toHaveBeenCalledTimes(1)
    await supplyLibraryPieces(lib, 0, 5, { fetchPiece })
    expect(fetchPiece).toHaveBeenCalledTimes(1)
  })

  it('reports each piece as it goes, so a 640 KiB wait is not a frozen page', async () => {
    const lib = fakeLibrary({ dealsPerChunk: 10 })
    lib.supply(MANIFEST)
    const seen = []
    await supplyLibraryPieces(lib, 8, 5, {
      fetchPiece: async () => new Uint8Array(),
      onProgress: (status) => seen.push(`${status.done}/${status.total}`),
    })
    expect(seen).toEqual(['0/2', '1/2', '2/2'])
  })

  it('asks for a round\'s pieces at once, rather than a round trip each', async () => {
    const lib = fakeLibrary({ dealsPerChunk: 10 })
    lib.supply(MANIFEST)
    // The first piece is held open, and the question is whether the second was
    // asked for anyway. Awaiting each fetch in turn — which is what this used
    // to do, and what every other test in this file was happy with — leaves
    // the second unasked until the first answers, and a cold run pays both
    // waits end to end instead of the longer of the two.
    let answerFirst
    const held = new Promise((resolve) => {
      answerFirst = resolve
    })
    const asked = []
    const fetchPiece = vi.fn(async (url) => {
      asked.push(url)
      if (asked.length === 1) await held
      return new Uint8Array()
    })

    const running = supplyLibraryPieces(lib, 8, 5, { fetchPiece })
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(asked).toEqual([PIECE(0), PIECE(1)])

    answerFirst()
    await running
  })

  it('supplies pieces in the order asked for, whatever order they arrive in', async () => {
    const lib = fakeLibrary({ dealsPerChunk: 10 })
    lib.supply(MANIFEST)
    const supplied = []
    const realSupply = lib.supply.bind(lib)
    lib.supply = (url, bytes) => {
      supplied.push(url)
      return realSupply(url, bytes)
    }
    // The second piece answers immediately, the first takes a turn of the
    // event loop, so they arrive backwards.
    const fetchPiece = vi.fn(async (url) => {
      if (url === PIECE(0)) await Promise.resolve()
      return new Uint8Array()
    })
    await supplyLibraryPieces(lib, 8, 5, { fetchPiece })
    expect(supplied).toEqual([PIECE(0), PIECE(1)])
  })

  it('gives up rather than looping for ever on a library it cannot satisfy', async () => {
    const insatiable = {
      manifest_url: MANIFEST,
      total_deals: 100,
      needs: () => [PIECE(0)],
      supply() {},
    }
    await expect(
      supplyLibraryPieces(insatiable, 0, 5, { fetchPiece: async () => new Uint8Array() }),
    ).rejects.toThrow(/kept asking/)
  })
})

describe('createLibraryFetcher', () => {
  const bytes = (n) => new Uint8Array([n, n, n])
  const respond = (body) => ({ ok: true, status: 200, arrayBuffer: async () => body.buffer })

  it('fetches once and remembers, so changing the script does not refetch', async () => {
    const fetchImpl = vi.fn(async () => respond(bytes(1)))
    const piece = createLibraryFetcher({ manifestUrl: MANIFEST, fetchImpl, cacheStorage: null })
    expect(Array.from(await piece(PIECE(0)))).toEqual([1, 1, 1])
    await piece(PIECE(0))
    expect(fetchImpl).toHaveBeenCalledTimes(1)
  })

  it('writes pieces to the browser cache but not the manifest, which can change', async () => {
    const put = vi.fn(async () => {})
    const cacheStorage = { open: async () => ({ match: async () => null, put }) }
    const fetchImpl = vi.fn(async () => respond(bytes(2)))
    const piece = createLibraryFetcher({ manifestUrl: MANIFEST, fetchImpl, cacheStorage })
    await piece(MANIFEST)
    expect(put).not.toHaveBeenCalled()
    await piece(PIECE(0))
    expect(put).toHaveBeenCalledTimes(1)
  })

  it('reads a piece back from the cache without touching the network', async () => {
    const cached = { ok: true, arrayBuffer: async () => bytes(3).buffer }
    const cacheStorage = { open: async () => ({ match: async () => cached, put: async () => {} }) }
    const fetchImpl = vi.fn()
    const piece = createLibraryFetcher({ manifestUrl: MANIFEST, fetchImpl, cacheStorage })
    expect(Array.from(await piece(PIECE(0)))).toEqual([3, 3, 3])
    expect(fetchImpl).not.toHaveBeenCalled()
  })

  it('works where the cache is refused, which is a private window not a failure', async () => {
    const cacheStorage = { open: async () => { throw new Error('denied') } }
    const fetchImpl = vi.fn(async () => respond(bytes(4)))
    const piece = createLibraryFetcher({ manifestUrl: MANIFEST, fetchImpl, cacheStorage })
    expect(Array.from(await piece(PIECE(0)))).toEqual([4, 4, 4])
  })

  it('names the library in a network failure, where "Failed to fetch" does not', async () => {
    const fetchImpl = async () => {
      throw new TypeError('Failed to fetch')
    }
    const piece = createLibraryFetcher({ manifestUrl: MANIFEST, fetchImpl, cacheStorage: null })
    await expect(piece(PIECE(0))).rejects.toThrow(/solved-deal library.*rpdd-000\.zdd/s)
  })

  it('reports an HTTP failure rather than reading an error page as tables', async () => {
    const fetchImpl = async () => ({ ok: false, status: 404, arrayBuffer: async () => new ArrayBuffer(0) })
    const piece = createLibraryFetcher({ manifestUrl: MANIFEST, fetchImpl, cacheStorage: null })
    await expect(piece(PIECE(0))).rejects.toThrow(/HTTP 404/)
  })
})

describe('describeInput', () => {
  const solved = { format: 'zrd', read: 500, solved: 500, unsolved: 0, skipped_count: 0, notes: [] }
  const library = { firstDeal: 4192003, totalDeals: 10485760, requested: 500 }

  it('says how many deals arrived and that they came with tables', () => {
    const { summary, warnings } = describeInput(solved, library)
    expect(summary).toContain('Read 500 deals')
    expect(summary).toContain('4,192,003 of 10,485,760')
    expect(summary).toMatch(/Every one arrived with its double-dummy table/)
    expect(warnings).toEqual([])
  })

  it('warns when fewer deals arrived than were asked for', () => {
    const short = describeInput({ ...solved, read: 40, solved: 40 }, library)
    expect(short.warnings.join(' ')).toMatch(/Asked for 500 deals and read 40/)
  })

  it('warns when deals arrived unsolved, which is the slow path this avoids', () => {
    const { warnings } = describeInput({ ...solved, solved: 400, unsolved: 100 }, library)
    expect(warnings.join(' ')).toMatch(/100 deals arrived without a double-dummy table/)
  })

  it('warns when a run reads the whole library, because repeats would be counted twice', () => {
    const whole = { firstDeal: 3, totalDeals: 500, requested: 500 }
    const { warnings } = describeInput(solved, whole)
    expect(warnings.join(' ')).toMatch(/come round to deals it has already seen/)
  })

  it('names records that could not be read, and passes the reader notes on', () => {
    const { warnings } = describeInput(
      { ...solved, skipped_count: 2, skipped: ['record 3: short'], notes: ['a note'] },
      library,
    )
    expect(warnings.join(' ')).toMatch(/2 records could not be read: record 3: short/)
    expect(warnings).toContain('a note')
  })

  it('has nothing to say about a run that read nothing', () => {
    expect(describeInput(null)).toEqual({ summary: '', warnings: [] })
  })
})

describe('libraryStatusText', () => {
  it('names the piece being fetched, since a network wait shows nothing else', () => {
    expect(libraryStatusText({ stage: 'pieces', done: 0, total: 2 })).toBe(
      'Fetching the solved-deal library — piece 1 of 2…',
    )
  })

  it('says what is happening while the index is read', () => {
    expect(libraryStatusText({ stage: 'manifest', done: 0, total: 1 })).toMatch(/index/)
  })

  it('moves on once every piece is in, rather than sitting on the last one', () => {
    expect(libraryStatusText({ stage: 'pieces', done: 2, total: 2 })).toMatch(/Running the script/)
  })

  it('has nothing to say when nothing is being fetched', () => {
    expect(libraryStatusText(null)).toBe('')
  })
})

describe('pointlessLibraryWarning', () => {
  it('warns when a script asks no double-dummy question', () => {
    expect(pointlessLibraryWarning(false)).toMatch(/never calls tricks\(\), dds\(\) or par\(\)/)
  })

  it('says nothing when the script does ask one', () => {
    expect(pointlessLibraryWarning(true)).toBe('')
  })

  it('says nothing when the script could not be read, which is not a no', () => {
    expect(pointlessLibraryWarning(undefined)).toBe('')
  })
})

describe('slowRandomWarning', () => {
  it('warns when a script that solves is told to shuffle its own deals', () => {
    const said = slowRandomWarning(true, 200)
    expect(said).toMatch(/calls tricks\(\), dds\(\) or par\(\)/)
    expect(said).toMatch(/pre-solved/i)
  })

  it('says what it costs, not just that it is slower', () => {
    // "Slower" is not a reason to choose differently; five seconds is. 200
    // deals at 23 ms each is the arithmetic someone can check.
    const said = slowRandomWarning(true, 200)
    expect(said).toMatch(/200 × 23 ms/)
    expect(said).toMatch(/5 seconds/)
  })

  it('scales with what was asked for', () => {
    expect(slowRandomWarning(true, 10000)).toMatch(/4 minutes/)
  })

  it('says a condition costs more again than a produce count', () => {
    expect(slowRandomWarning(true, 200)).toMatch(/condition/)
  })

  it('says nothing when the script asks no double-dummy question', () => {
    expect(slowRandomWarning(false, 200)).toBe('')
  })

  it('says nothing when the script could not be read, which is not a yes', () => {
    expect(slowRandomWarning(undefined, 200)).toBe('')
  })

  it('still warns when the count is missing, without inventing a time', () => {
    const said = slowRandomWarning(true, NaN)
    expect(said).toMatch(/every deal has to be solved here/)
    expect(said).not.toMatch(/ms/)
  })
})
