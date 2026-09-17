import { describe, it, expect } from 'vitest'
import { DEMOS, buildDemos } from './demos.js'
import { DOCUMENT_FORMATS } from './envelope.js'

// Every bundled demo, checked exhaustively. A demo that fails to open is the
// most embarrassing bug the tab could have, and the set is small. Whether each
// script *parses* is checked by the engine itself, in dealer/tests/web_demos.rs.
describe('the bundled demos', () => {
  const files = import.meta.glob('../demos/*.json', { eager: true, import: 'default' })

  it('are all there — none dropped for failing to read', () => {
    // buildDemos skips a document that does not read, so a count is the only
    // way a broken file shows up here. NT Ladder is the one not from a file.
    expect(Object.keys(files).length).toBeGreaterThan(0)
    expect(DEMOS).toHaveLength(Object.keys(files).length + 1)
  })

  it.each(DEMOS.map((demo) => [demo.id, demo]))('%s has a title, a description and usable settings', (_, demo) => {
    expect(demo.title).not.toBe('')
    expect(demo.title).not.toBe(demo.id)
    expect(demo.description.length).toBeGreaterThan(20)
    expect(DOCUMENT_FORMATS).toContain(demo.document.settings.format)
    expect(demo.document.script.trim()).not.toBe('')
  })

  it.each(Object.entries(files))('%s states the settings it needs rather than inheriting defaults', (_, value) => {
    // A demo's settings are part of what it demonstrates: one relying on a
    // default would change when the default does.
    for (const key of ['produce', 'format', 'dealSource']) {
      expect(value.settings).toHaveProperty(key)
    }
  })

  it('puts NT Ladder last, levelling', () => {
    const last = DEMOS.at(-1)
    expect(last.id).toBe('90-nt-ladder')
    expect(last.document.settings.autoLevel).toBe(true)
    expect(last.document.script).toContain('HandType_')
  })
})

describe('buildDemos', () => {
  const doc = (script, settings = {}) => ({ v: 1, script, settings })

  it('titles a demo from its script and orders by id', () => {
    const demos = buildDemos({
      '../demos/02-b.json': doc('title "Second"\n# About two.\ncondition 1\n'),
      '../demos/01-a.json': doc('title "First"\n# About one.\ncondition 1\n'),
    })
    expect(demos.map((demo) => [demo.id, demo.title, demo.description])).toEqual([
      ['01-a', 'First', 'About one.'],
      ['02-b', 'Second', 'About two.'],
    ])
  })

  it('leaves out a document that does not read', () => {
    const demos = buildDemos({
      '../demos/01-a.json': doc('title "Fine"\ncondition 1\n'),
      '../demos/02-b.json': { v: 99, script: 'condition 1' },
    })
    expect(demos.map((demo) => demo.id)).toEqual(['01-a'])
  })

  it('lets a demo built in code name its own title and description', () => {
    const [demo] = buildDemos({}, [
      { id: 'x', title: 'Given', description: 'Said here.', document: doc('condition 1\n') },
    ])
    expect([demo.title, demo.description]).toEqual(['Given', 'Said here.'])
  })
})
