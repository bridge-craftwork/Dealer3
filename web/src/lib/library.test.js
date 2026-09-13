import { describe, it, expect } from 'vitest'
import {
  OLD_CACHE_NAME,
  createSyncFetcher,
  describeInput,
  forgetOldLibraryCache,
  libraryStatusText,
  pointlessLibraryWarning,
  slowRandomWarning,
} from './library.js'

const MANIFEST = 'https://example.test/manifest.json'
const PIECE = 'https://example.test/zdd/rpdd-042.zdd'

/// A synchronous `XMLHttpRequest`, as far as the fetcher uses one. `serve` says
/// what each URL answers: `{ status, bytes }`, or a thrown error for a network
/// failure.
function fakeXhr(serve) {
  const opened = []
  class FakeXhr {
    open(method, url, async) {
      opened.push({ method, url, async })
      this.url = url
    }
    send() {
      const answer = serve(this.url)
      this.status = answer.status
      this.response = new Uint8Array(answer.bytes || []).buffer
    }
  }
  return { FakeXhr, opened }
}

describe('createSyncFetcher', () => {
  it('fetches synchronously and hands back the bytes', () => {
    const { FakeXhr, opened } = fakeXhr(() => ({ status: 200, bytes: [1, 2, 3] }))
    const { fetch } = createSyncFetcher({ XhrImpl: FakeXhr })
    expect([...fetch(PIECE)]).toEqual([1, 2, 3])
    // Synchronous is the whole point: the engine asks mid-run and cannot await.
    expect(opened).toEqual([{ method: 'GET', url: PIECE, async: false }])
  })

  it('says which piece it is fetching, and that the run carries on once it is in', () => {
    const { FakeXhr } = fakeXhr(() => ({ status: 200, bytes: [] }))
    const said = []
    const { fetch, pieces } = createSyncFetcher({
      XhrImpl: FakeXhr,
      manifestUrl: MANIFEST,
      onFetch: (status) => said.push(status),
    })
    fetch(MANIFEST)
    fetch(PIECE)
    expect(said.map((s) => s.stage)).toEqual(['manifest', 'piece', 'read'])
    expect(pieces()).toBe(1)
  })

  it('counts the time spent waiting on the network', () => {
    const { FakeXhr } = fakeXhr(() => ({ status: 200, bytes: [] }))
    let now = 0
    const fetcher = createSyncFetcher({
      XhrImpl: FakeXhr,
      now: () => (now += 250),
    })
    fetcher.fetch(PIECE)
    expect(fetcher.seconds()).toBeCloseTo(0.25)
  })

  it('names the library in a network failure, where "NetworkError" does not', () => {
    const { FakeXhr } = fakeXhr(() => {
      throw new Error('NetworkError')
    })
    const { fetch } = createSyncFetcher({ XhrImpl: FakeXhr })
    expect(() => fetch(PIECE)).toThrow(/Could not reach the solved-deal library at .*rpdd-042/)
  })

  it('reports an HTTP failure rather than reading an error page as tables', () => {
    const { FakeXhr } = fakeXhr(() => ({ status: 404, bytes: [60, 104] }))
    const { fetch } = createSyncFetcher({ XhrImpl: FakeXhr })
    expect(() => fetch(PIECE)).toThrow(/answered HTTP 404/)
  })

  it('treats a request that never got an answer as unreachable', () => {
    const { FakeXhr } = fakeXhr(() => ({ status: 0 }))
    const { fetch } = createSyncFetcher({ XhrImpl: FakeXhr })
    expect(() => fetch(PIECE)).toThrow(/Could not reach/)
  })
})

describe('forgetOldLibraryCache', () => {
  it('deletes the store earlier versions kept every piece in', async () => {
    const deleted = []
    const storage = { delete: async (name) => (deleted.push(name), true) }
    expect(await forgetOldLibraryCache(storage)).toBe(true)
    expect(deleted).toEqual([OLD_CACHE_NAME])
  })

  it('is not a failure where there is no cache, or the browser refuses it', async () => {
    expect(await forgetOldLibraryCache(undefined)).toBe(false)
    const refusing = {
      delete: async () => {
        throw new Error('SecurityError')
      },
    }
    expect(await forgetOldLibraryCache(refusing)).toBe(false)
  })
})

describe('describeInput', () => {
  const library = { first_record: 4192003, records: 10485760, read_whole: false }
  const solved = {
    format: 'zrd',
    read: 500,
    solved: 500,
    unsolved: 0,
    skipped_count: 0,
    notes: [],
    library,
  }

  it('says how many deals arrived, from where, and that they came with tables', () => {
    const { summary, warnings } = describeInput(solved)
    expect(summary).toContain('Read 500 deals')
    expect(summary).toContain('4,192,003 of 10,485,760')
    expect(summary).toMatch(/Every one arrived with its double-dummy table/)
    expect(warnings).toEqual([])
  })

  it('warns when deals arrived unsolved, which is the slow path this avoids', () => {
    const { warnings } = describeInput({ ...solved, solved: 400, unsolved: 100 })
    expect(warnings.join(' ')).toMatch(/100 deals arrived without a double-dummy table/)
  })

  it('says when a run read the whole library, because more would count repeats', () => {
    const whole = { ...solved, library: { ...library, read_whole: true } }
    const { warnings } = describeInput(whole)
    expect(warnings.join(' ')).toMatch(/read the whole library \(10,485,760 deals\)/)
    expect(warnings.join(' ')).toMatch(/come round to deals it has already seen/)
  })

  it('says nothing about a library for deals that did not come from one', () => {
    const { summary } = describeInput({ ...solved, format: 'pbn', library: null })
    expect(summary).toBe(
      'Read 500 deals. Every one arrived with its double-dummy table, so tricks(), dds() and par() were lookups rather than searches.',
    )
  })

  it('names records that could not be read, and passes the reader notes on', () => {
    const { warnings } = describeInput({
      ...solved,
      skipped_count: 2,
      skipped: ['record 3: short'],
      notes: ['a note'],
    })
    expect(warnings.join(' ')).toMatch(/2 records could not be read: record 3: short/)
    expect(warnings).toContain('a note')
  })

  it('has nothing to say about a run that read nothing', () => {
    expect(describeInput(null)).toEqual({ summary: '', warnings: [] })
  })
})

describe('libraryStatusText', () => {
  it('names the piece being fetched, since a network wait shows nothing else', () => {
    expect(libraryStatusText({ stage: 'piece', pieces: 3 })).toBe(
      'Fetching the solved-deal library — piece 3…',
    )
  })

  it('says what is happening while the index is read', () => {
    expect(libraryStatusText({ stage: 'manifest' })).toMatch(/index/)
  })

  it('says the run carries on once a piece is in, rather than sitting on the fetch', () => {
    expect(libraryStatusText({ stage: 'read', pieces: 2 })).toMatch(/Running the script.*2 pieces read/)
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
