import { describe, it, expect } from 'vitest'
import {
  MAX_REVISIONS,
  MAX_SCRIPTS,
  emptyHistory,
  mergeScripts,
  noteLastRun,
  recordRevision,
} from './history.js'
import { readDocument } from './envelope.js'
import { historyFile, readScriptFile, scriptFile } from './scriptFile.js'

const settings = readDocument({ v: 1, script: 'x', settings: { produce: 20, seed: 7 } }).settings
const lastRun = { seconds: 0.65, generated: 100000, produced: 100, at: '2026-09-09T14:32:45Z' }

/** A history of the given scripts, unrelated to each other, oldest first. */
function historyOf(...scripts) {
  let history = emptyHistory()
  scripts.forEach((script, i) => {
    history = recordRevision(history, { script, settings, origin: `demo:${i}` }, 1000 + i).history
  })
  return history
}

/** Through a file and back, as import would see it. */
const roundTrip = (history) =>
  readScriptFile(JSON.stringify(historyFile(history, '2026-11-02T00:00:00Z')), 9999)

describe('scriptFile: one script', () => {
  const doc = { v: 1, script: 'hcp(north) > 10', settings }
  const revision = { script: doc.script, settings, at: 1, lastRun }

  it('is the document, with the last run beside it', () => {
    expect(scriptFile(doc, revision)).toEqual({ ...doc, lastRun })
  })

  it('leaves the last run out once the script has been edited', () => {
    expect(scriptFile({ ...doc, script: 'hcp(south) > 10' }, revision)).not.toHaveProperty('lastRun')
  })

  it('leaves the last run out once the settings have changed', () => {
    const other = { ...doc, settings: { ...settings, produce: 21 } }
    expect(scriptFile(other, revision)).not.toHaveProperty('lastRun')
  })

  it('is not bothered by the order settings were stored in', () => {
    const reordered = Object.fromEntries(Object.entries(settings).reverse())
    expect(scriptFile(doc, { ...revision, settings: reordered })).toHaveProperty('lastRun')
  })

  it('has no last run to carry before a first run', () => {
    expect(scriptFile(doc, undefined)).toEqual(doc)
  })

  it('reads back as one script, taking its name from its title', () => {
    const text = JSON.stringify({ ...doc, script: 'title "Stayman"\nhcp(north) > 10', lastRun })
    const [script, ...rest] = readScriptFile(text, 5000)
    expect(rest).toEqual([])
    expect(script.name).toBe('Stayman')
    expect(script.revisions[0].lastRun).toEqual(lastRun)
    expect(script.revisions[0].at).toBe(5000)
  })
})

describe('readScriptFile: all of it or none of it', () => {
  const good = { v: 1, script: 'a', settings }
  const file = (scripts) => JSON.stringify({ v: 1, scripts })

  it('refuses what is not JSON', () => {
    expect(() => readScriptFile('nope', 1)).toThrow(/not JSON/)
  })

  it('refuses JSON that is not an export, in words about a file', () => {
    expect(() => readScriptFile('{"hello":1}', 1)).toThrow(/This file/)
    expect(() => readScriptFile('[1,2]', 1)).toThrow(/This file/)
  })

  it('refuses a whole file for one bad version of one script', () => {
    const scripts = [
      { name: '', origin: '', revisions: [good] },
      { name: '', origin: '', revisions: [good, { v: 1, script: '   ' }] },
    ]
    expect(() => readScriptFile(file(scripts), 1)).toThrow(/carries no script/)
  })

  it('refuses a script with no versions, and a file with no scripts', () => {
    expect(() => readScriptFile(file([{ revisions: [] }]), 1)).toThrow(/Script 1 .* no versions/)
    expect(() => readScriptFile(file([]), 1)).toThrow(/no scripts/)
  })

  it('refuses a file from a newer version', () => {
    const text = JSON.stringify({ v: 2, scripts: [{ revisions: [good] }] })
    expect(() => readScriptFile(text, 1)).toThrow(/newer dealer3/)
  })

  it('drops a damaged last run rather than the script: it is only ever carried', () => {
    const text = JSON.stringify({ ...good, lastRun: { seconds: 'fast' } })
    expect(readScriptFile(text, 1)[0].revisions[0]).not.toHaveProperty('lastRun')
  })
})

describe('mergeScripts', () => {
  it('restores a backup into an empty History exactly', () => {
    let history = historyOf('aaa\nbbb', 'ccc\nddd')
    history = recordRevision(
      history,
      { script: 'aaa\nbbb\neee', settings, lineage: history.entries[1].id },
      2000,
    ).history
    history = noteLastRun(history, history.entries[0].id, 'aaa\nbbb\neee', lastRun)

    const { history: restored, dropped } = mergeScripts(emptyHistory(), roundTrip(history))

    expect(dropped).toBe(0)
    const shape = (h) => h.entries.map(({ name, origin, revisions }) => ({ name, origin, revisions }))
    expect(shape(restored)).toEqual(shape(history))
  })

  it('changes nothing when the same file is imported twice', () => {
    const history = historyOf('aaa\nbbb', 'ccc\nddd')
    const once = mergeScripts(emptyHistory(), roundTrip(history)).history
    const twice = mergeScripts(once, roundTrip(history)).history
    expect(twice).toEqual(once)
  })

  it('keeps unrelated scripts apart even when they look alike', () => {
    // Similar enough that the rule for someone editing would fold them together.
    const history = {
      ...emptyHistory(),
      entries: [
        { id: 'h1', name: '', origin: '', revisions: [{ script: 'a\nb\nc\nd', settings, at: 2 }] },
        { id: 'h2', name: '', origin: '', revisions: [{ script: 'a\nb\nc\ne', settings, at: 1 }] },
      ],
    }
    expect(mergeScripts(emptyHistory(), roundTrip(history)).history.entries).toHaveLength(2)
  })

  it('adds to what is here, joining a script by its name', () => {
    const here = {
      ...emptyHistory(),
      next: 2,
      entries: [{ id: 'h1', name: 'Stayman', origin: '', revisions: [{ script: 'one', settings, at: 10 }] }],
    }
    const { history } = mergeScripts(here, [
      { name: 'stayman', origin: '', revisions: [{ script: 'two', settings, at: 20 }] },
      { name: '', origin: '', revisions: [{ script: 'other', settings, at: 5 }] },
    ])
    expect(history.entries.map((e) => e.id)).toEqual(['h1', 'h2'])
    expect(history.entries[0].revisions.map((r) => r.script)).toEqual(['two', 'one'])
    expect(history.next).toBe(3)
  })

  it('keeps to the limits, and says how many scripts did not fit', () => {
    const many = Array.from({ length: MAX_SCRIPTS + 2 }, (_, i) => ({
      name: '',
      origin: '',
      revisions: [{ script: `s${i}`, settings, at: i }],
    }))
    const { history, dropped } = mergeScripts(emptyHistory(), many)
    expect(history.entries).toHaveLength(MAX_SCRIPTS)
    expect(dropped).toBe(2)
    expect(history.entries[0].revisions[0].script).toBe(`s${MAX_SCRIPTS + 1}`)

    const versions = Array.from({ length: MAX_REVISIONS + 1 }, (_, i) => ({ script: `v${i}`, settings, at: i }))
    const merged = mergeScripts(emptyHistory(), [{ name: '', origin: '', revisions: versions }])
    expect(merged.history.entries[0].revisions).toHaveLength(MAX_REVISIONS)
  })
})
