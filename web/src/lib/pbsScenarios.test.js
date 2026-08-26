import { describe, it, expect, vi, afterEach } from 'vitest'
import { fetchScenarioManifest, suitSymbols, prettifyLabel } from './pbsScenarios.js'

const manifest = {
  layout: [
    { type: 'major', title: 'Bidding Scenarios' },
    { type: 'section', title: 'Beginners' },
    { type: 'row', buttons: [{ name: 'A_Script' }, { name: '---' }] },
    { type: 'row', buttons: [{ name: 'B_Script' }, { name: 'Never_Published' }] },
    { type: 'section', title: 'Empty Section' },
    { type: 'row', buttons: [{ name: 'Also_Missing' }] },
  ],
  scenarios: {
    A_Script: { buttonText: '1!S Opening', chat: '--- header\\nFirst real line.\\nSecond.' },
    B_Script: { buttonText: 'Second', chat: '' },
    Never_Published: { buttonText: 'Gone', missing: true },
    Also_Missing: { buttonText: 'Gone too', missing: true },
  },
}

afterEach(() => vi.unstubAllGlobals())

function stubFetch(body, ok = true, status = 200) {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
    ok, status, json: async () => body, text: async () => body,
  }))
}

describe('suitSymbols', () => {
  it('replaces PBS suit tokens', () => {
    expect(suitSymbols('1!S then 2!H')).toBe('1♠ then 2♥')
  })
  it('tolerates empty input', () => {
    expect(suitSymbols('')).toBe('')
    expect(suitSymbols(undefined)).toBe('')
  })
})

describe('prettifyLabel', () => {
  it('turns a file name into a label', () => {
    expect(prettifyLabel('Fourth_Suit-Forcing')).toBe('Fourth Suit Forcing')
  })
})

describe('fetchScenarioManifest', () => {
  it('groups scenarios under their section', async () => {
    stubFetch(manifest)
    const { sections } = await fetchScenarioManifest('release')
    expect(sections).toHaveLength(1)
    expect(sections[0].label).toBe('Beginners')
    expect(sections[0].items.map((i) => i.file)).toEqual(['A_Script', 'B_Script'])
  })

  it('drops layout spacers and unpublished scenarios', async () => {
    stubFetch(manifest)
    const { sections } = await fetchScenarioManifest()
    const files = sections.flatMap((s) => s.items.map((i) => i.file))
    expect(files).not.toContain('---')
    // `missing` marks a layout entry whose script was never published; showing
    // it would offer the user something that cannot load.
    expect(files).not.toContain('Never_Published')
  })

  it('drops sections left empty after filtering', async () => {
    stubFetch(manifest)
    const { sections } = await fetchScenarioManifest()
    expect(sections.map((s) => s.label)).not.toContain('Empty Section')
  })

  it('applies suit symbols to button text', async () => {
    stubFetch(manifest)
    const { sections } = await fetchScenarioManifest()
    expect(sections[0].items[0].label).toBe('1♠ Opening')
  })

  it('uses the first meaningful chat line as the description', async () => {
    stubFetch(manifest)
    const { sections } = await fetchScenarioManifest()
    // The leading `--- header` line is a title marker, not a description.
    expect(sections[0].items[0].description).toBe('First real line.')
  })

  it('reports a readable error when the manifest is unavailable', async () => {
    stubFetch(null, false, 404)
    await expect(fetchScenarioManifest()).rejects.toThrow(/scenario list.*404/i)
  })
})
