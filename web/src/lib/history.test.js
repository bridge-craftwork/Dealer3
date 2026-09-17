import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import {
  MAX_REVISIONS,
  MAX_SCRIPTS,
  emptyHistory,
  loadHistory,
  recordRevision,
  removeEntry,
  saveHistory,
  similarity,
} from './history.js'

const settings = { produce: 20, maxGenerate: 1000000, format: 'oneline', dealSource: 'random' }

/** File a revision and return the history, with the id kept alongside. */
function record(history, script, extra = {}, now = 1000) {
  return recordRevision(history, { script, settings, ...extra }, now)
}

const lines = (n, tag = 'x') => Array.from({ length: n }, (_, i) => `${tag}${i} = hcp(north) >= ${i}`).join('\n')

describe('similarity', () => {
  it('is 1 for the same script and 0 for nothing in common', () => {
    expect(similarity('a\nb\n', 'a\nb\n')).toBe(1)
    expect(similarity('a\nb\n', 'c\nd\n')).toBe(0)
  })

  it('ignores blank lines and indentation', () => {
    expect(similarity('a\n\n  b\n', 'a\nb')).toBe(1)
  })

  it('is the shared lines over the longer script', () => {
    expect(similarity('a\nb\nc\nd\n', 'a\nb\n')).toBe(0.5)
  })
})

describe('recordRevision: what counts as the same script', () => {
  it('files scripts with the same title together', () => {
    let { history } = record(emptyHistory(), 'title "Weak twos"\ncondition 1\n')
    ;({ history } = record(history, 'title "Weak twos"\ncondition hcp(north) > 5\n'))
    expect(history.entries).toHaveLength(1)
    expect(history.entries[0].revisions).toHaveLength(2)
    expect(history.entries[0].name).toBe('Weak twos')
  })

  it('keeps differently titled scripts apart however similar they are', () => {
    let { history } = record(emptyHistory(), 'title "One"\ncondition 1\n')
    ;({ history } = record(history, 'title "Two"\ncondition 1\n'))
    expect(history.entries).toHaveLength(2)
  })

  it('treats a PBS alias as a name', () => {
    let { history } = record(emptyHistory(), '# alias: Weak2\ncondition 1\n')
    ;({ history } = record(history, lines(10), { origin: 'pbs:Other' }))
    ;({ history } = record(history, '# alias: Weak2\ncondition 2\n'))
    expect(history.entries).toHaveLength(2)
    expect(history.entries[0].revisions).toHaveLength(2)
  })

  it('follows lineage: an edited entry gets the revision, however much it changed', () => {
    const first = record(emptyHistory(), lines(10))
    const edited = `${lines(4)}\nsomething = new\nentirely = new\n`
    const { history } = record(first.history, edited, { lineage: first.id })
    expect(history.entries).toHaveLength(1)
    expect(history.entries[0].revisions).toHaveLength(2)
  })

  it('follows origin: runs of one PBS scenario are one script', () => {
    let { history } = record(emptyHistory(), lines(10), { origin: 'pbs:Weak_2_Bids' })
    ;({ history } = record(history, lines(10, 'y'), { origin: 'pbs:Other' }))
    ;({ history } = record(history, `${lines(10)}\nextra = 1`, { origin: 'pbs:Weak_2_Bids' }))
    expect(history.entries).toHaveLength(2)
    expect(history.entries[0].origin).toBe('pbs:Weak_2_Bids')
    expect(history.entries[0].revisions).toHaveLength(2)
  })

  it('breaks lineage when the text was replaced rather than edited', () => {
    const first = record(emptyHistory(), lines(10))
    const { history } = record(first.history, lines(10, 'pasted'), { lineage: first.id })
    expect(history.entries).toHaveLength(2)
  })

  it('without name or origin, falls back to similarity with the most recent entry', () => {
    let { history } = record(emptyHistory(), lines(10))
    ;({ history } = record(history, `${lines(9)}\nchanged = 1`))
    expect(history.entries).toHaveLength(1)
    ;({ history } = record(history, lines(10, 'other')))
    expect(history.entries).toHaveLength(2)
  })

  it('keeps an entry when its script gains a title', () => {
    const first = record(emptyHistory(), lines(10))
    const { history, id } = record(first.history, `title "Named now"\n${lines(10)}`, {
      lineage: first.id,
    })
    expect(id).toBe(first.id)
    expect(history.entries).toHaveLength(1)
    expect(history.entries[0].name).toBe('Named now')
  })
})

describe('recordRevision: how much is kept', () => {
  it('refreshes rather than duplicates an unchanged script', () => {
    let { history } = record(emptyHistory(), lines(3), {}, 1)
    ;({ history } = record(history, lines(3), {}, 2))
    expect(history.entries[0].revisions).toHaveLength(1)
    expect(history.entries[0].revisions[0].at).toBe(2)
  })

  it('keeps a few revisions per script', () => {
    let history = emptyHistory()
    for (let i = 0; i < MAX_REVISIONS + 3; i += 1) {
      ;({ history } = record(history, `title "One"\nx = ${i}\n`, {}, i))
    }
    const revisions = history.entries[0].revisions
    expect(revisions).toHaveLength(MAX_REVISIONS)
    expect(revisions[0].script).toContain(`x = ${MAX_REVISIONS + 2}`)
  })

  it('never lets one script push another out', () => {
    let { history } = record(emptyHistory(), 'title "Other"\ncondition 1\n')
    for (let i = 0; i < 100; i += 1) {
      ;({ history } = record(history, `title "Busy"\nx = ${i}\n`, {}, i))
    }
    expect(history.entries.map((entry) => entry.name)).toEqual(['Busy', 'Other'])
  })

  it('forgets the least recently used script past the limit', () => {
    let history = emptyHistory()
    for (let i = 0; i < MAX_SCRIPTS + 2; i += 1) {
      ;({ history } = record(history, `title "S${i}"\ncondition 1\n`, {}, i))
    }
    expect(history.entries).toHaveLength(MAX_SCRIPTS)
    expect(history.entries[0].name).toBe(`S${MAX_SCRIPTS + 1}`)
    expect(history.entries.some((entry) => entry.name === 'S0')).toBe(false)
  })

  it('moves a script to the top when it gets a revision', () => {
    let { history } = record(emptyHistory(), 'title "A"\ncondition 1\n')
    ;({ history } = record(history, 'title "B"\ncondition 1\n'))
    ;({ history } = record(history, 'title "A"\ncondition 2\n'))
    expect(history.entries.map((entry) => entry.name)).toEqual(['A', 'B'])
  })
})

describe('removeEntry', () => {
  it('forgets every revision of one script', () => {
    let { history } = record(emptyHistory(), 'title "A"\ncondition 1\n')
    const b = record(history, 'title "B"\ncondition 1\n')
    history = removeEntry(b.history, b.id)
    expect(history.entries.map((entry) => entry.name)).toEqual(['A'])
  })
})

describe('loadHistory / saveHistory', () => {
  function memoryStorage() {
    const map = new Map()
    return {
      getItem: (k) => (map.has(k) ? map.get(k) : null),
      setItem: (k, v) => map.set(k, String(v)),
      removeItem: (k) => map.delete(k),
      _map: map,
    }
  }

  beforeEach(() => vi.stubGlobal('localStorage', memoryStorage()))
  afterEach(() => vi.unstubAllGlobals())

  it('round-trips a history', () => {
    let { history } = record(emptyHistory(), 'title "A"\ncondition 1\n', { origin: 'demo:x' })
    ;({ history } = record(history, 'title "A"\ncondition 2\n'))
    saveHistory(history)
    const loaded = loadHistory()
    expect(loaded.entries).toHaveLength(1)
    expect(loaded.entries[0].origin).toBe('demo:x')
    expect(loaded.entries[0].revisions.map((r) => r.script)).toEqual([
      'title "A"\ncondition 2\n',
      'title "A"\ncondition 1\n',
    ])
    expect(loaded.entries[0].revisions[0].settings.format).toBe('oneline')
  })

  it('reads nothing stored, garbage and an unknown version as empty', () => {
    expect(loadHistory()).toEqual(emptyHistory())
    localStorage.setItem('dealer3:history:v1', 'not json')
    expect(loadHistory()).toEqual(emptyHistory())
    localStorage.setItem('dealer3:history:v1', JSON.stringify({ v: 99, entries: [] }))
    expect(loadHistory()).toEqual(emptyHistory())
  })

  it('drops an invalid entry and keeps the rest', () => {
    const good = record(emptyHistory(), 'title "A"\ncondition 1\n').history
    localStorage.setItem(
      'dealer3:history:v1',
      JSON.stringify({ ...good, entries: [{ id: 'h9', revisions: [{ script: 42 }] }, ...good.entries] }),
    )
    expect(loadHistory().entries.map((entry) => entry.name)).toEqual(['A'])
  })

  it('never hands out an id already in use', () => {
    const { history } = record(emptyHistory(), 'title "A"\ncondition 1\n')
    localStorage.setItem('dealer3:history:v1', JSON.stringify({ ...history, next: 1 }))
    expect(loadHistory().next).toBe(2)
  })

  it('reads blocked storage as empty and saves nothing without throwing', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('blocked')
      },
      setItem: () => {
        throw new Error('blocked')
      },
    })
    expect(loadHistory()).toEqual(emptyHistory())
    expect(() => saveHistory(record(emptyHistory(), 'condition 1\n').history)).not.toThrow()
  })

  it('drops the least recently used scripts when storage is full', () => {
    let history = emptyHistory()
    for (let i = 0; i < 4; i += 1) ({ history } = record(history, `title "S${i}"\ncondition 1\n`, {}, i))
    const map = new Map()
    vi.stubGlobal('localStorage', {
      getItem: (k) => map.get(k) ?? null,
      setItem: (k, v) => {
        if (JSON.parse(v).entries.length > 2) throw new Error('QuotaExceededError')
        map.set(k, v)
      },
    })
    const kept = saveHistory(history)
    expect(kept.entries.map((entry) => entry.name)).toEqual(['S3', 'S2'])
    expect(loadHistory().entries).toHaveLength(2)
  })
})
