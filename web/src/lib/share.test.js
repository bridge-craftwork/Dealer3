// The link, both ways round.
//
// A producer with no consumer cannot be tested, which is why the two halves
// (#98, #99) shipped together: every case here makes a link and then opens it.

import { describe, it, expect } from 'vitest'
import {
  parseFragment,
  resolveFragment,
  shareFragment,
  scenarioFragment,
  shareUrl,
  encodeDocument,
  decodeDocument,
} from './share.js'
import { makeDocument, DOCUMENT_VERSION } from './envelope.js'

const SCRIPT = `# A scenario worth sending someone
condition hcp(north) >= 15 && shape(north, any 4333 + any 4432)

action printoneline,
  average "N HCP" hcp(north)
`

const page = {
  seed: 8391,
  produce: 20,
  maxGenerate: 1000000,
  format: 'oneline',
  autoLevel: false,
  roundRobin: false,
  params: [],
  dealSource: 'random',
  newSeedEachRun: false,
}

const open = (fragment, fetchScenario) => resolveFragment(fragment, { fetchScenario })

describe('parseFragment', () => {
  it('tells the three forms apart', () => {
    expect(parseFragment('#s=Weak_2_Bids').kind).toBe('scenario')
    expect(parseFragment('#d=abc123').kind).toBe('document')
    expect(parseFragment('#k=Ab3x9').kind).toBe('key')
  })

  it('says nothing about a fragment that is not ours', () => {
    // The page has to open normally on an in-page anchor, an empty hash, or
    // whatever somebody else appended.
    expect(parseFragment('')).toBe(null)
    expect(parseFragment('#')).toBe(null)
    expect(parseFragment('#top')).toBe(null)
    expect(parseFragment(undefined)).toBe(null)
    expect(parseFragment('#utm_source=mail')).toBe(null)
  })

  it('refuses a fragment far longer than any link this makes', () => {
    expect(parseFragment('#d=' + 'a'.repeat(100 * 1024))).toBe(null)
  })
})

describe('a script somebody wrote', () => {
  it('travels whole, through deflate and back', async () => {
    const doc = makeDocument(SCRIPT, page)
    const fragment = await shareFragment(doc)
    expect(fragment.startsWith('#d=')).toBe(true)
    const { source, doc: back } = await open(fragment)
    expect(source).toBe('document')
    expect(back.script).toBe(SCRIPT)
    expect(back.settings.seed).toBe(8391)
  })

  it('is shorter for having been compressed, which is the whole reason for it', async () => {
    // Against the same document base64'd on its own, which is the alternative:
    // base64 pays four characters for every three bytes whatever it holds, so
    // the deflate step is what keeps a real script inside a few hundred
    // characters instead of a couple of thousand.
    const doc = makeDocument(SCRIPT, page)
    const plain = Math.ceil((JSON.stringify(doc).length * 4) / 3)
    const encoded = await encodeDocument(doc)
    expect(encoded.length).toBeLessThan(plain)
    expect(encoded).toMatch(/^[A-Za-z0-9_-]+$/) // base64url: nothing a URL escapes
  })

  it('carries the parameters that were typed in, not just the script', async () => {
    // The conversion this is most likely to lose. The page keeps values by
    // parameter number and the engine takes `N=TEXT`; a link that dropped them
    // would open a parameterised scenario running on its declared defaults,
    // and look as though it had worked.
    const doc = makeDocument(SCRIPT, { ...page, params: ['0=west', '1=15'] })
    const { doc: back } = await open(await shareFragment(doc))
    expect(back.settings.params).toEqual(['0=west', '1=15'])
  })

  it('carries the settings the engine never sees', async () => {
    const doc = makeDocument(SCRIPT, { ...page, dealSource: 'library', newSeedEachRun: true })
    const { doc: back } = await open(await shareFragment(doc))
    expect(back.settings.dealSource).toBe('library')
    expect(back.settings.newSeedEachRun).toBe(true)
  })
})

describe('a scenario nobody changed', () => {
  const doc = makeDocument(SCRIPT, { ...page, scenario: 'Sup_X_By_Advancer' })

  it('travels as its name, and reads as a link rather than as base64', async () => {
    const fragment = await shareFragment(doc, { pristineScript: SCRIPT })
    expect(fragment).toBe('#s=Sup_X_By_Advancer&seed=8391')
  })

  it('is fetched from the list when it is opened', async () => {
    const asked = []
    const { source, doc: back } = await open(
      await shareFragment(doc, { pristineScript: SCRIPT }),
      (slug) => {
        asked.push(slug)
        return Promise.resolve(SCRIPT)
      },
    )
    expect(asked).toEqual(['Sup_X_By_Advancer'])
    expect(source).toBe('scenario')
    expect(back.script).toBe(SCRIPT)
    expect(back.settings.seed).toBe(8391)
    expect(back.settings.scenario).toBe('Sup_X_By_Advancer')
  })

  it('sends the whole script the moment a character of it changes', async () => {
    // Otherwise the recipient opens a different script under the right name,
    // which is worse than a long link.
    const edited = makeDocument(SCRIPT + '# and one more line\n', {
      ...page,
      scenario: 'Sup_X_By_Advancer',
    })
    expect((await shareFragment(edited, { pristineScript: SCRIPT })).startsWith('#d=')).toBe(true)
  })

  it('states what was changed and stays quiet about what was not', async () => {
    const fragment = scenarioFragment(
      makeDocument(SCRIPT, {
        ...page,
        scenario: 'Weak_2_Bids',
        produce: 60,
        format: 'pbn',
        roundRobin: true,
        dealSource: 'library',
        params: ['0=west'],
      }),
    )
    const query = new URLSearchParams(fragment.slice(1))
    expect(query.get('produce')).toBe('60')
    expect(query.get('format')).toBe('pbn')
    expect(query.get('roundRobin')).toBe('1')
    expect(query.get('dealSource')).toBe('library')
    expect(query.getAll('params')).toEqual(['0=west'])
    // Untouched, so unsaid: this is what keeps the form short enough to read.
    expect(query.has('maxGenerate')).toBe(false)
    expect(query.has('autoLevel')).toBe(false)
  })

  it('opens with everything the fragment stated', async () => {
    const { doc: back } = await open(
      '#s=Weak_2_Bids&seed=5&produce=60&format=pbn&roundRobin=1&dealSource=library&params=0%3Dwest',
      () => Promise.resolve(SCRIPT),
    )
    expect(back.settings).toMatchObject({
      seed: 5,
      produce: 60,
      format: 'pbn',
      roundRobin: true,
      dealSource: 'library',
      params: ['0=west'],
      scenario: 'Weak_2_Bids',
    })
  })

  it('opens on the defaults for everything it did not', async () => {
    const { doc: back } = await open('#s=Weak_2_Bids', () => Promise.resolve(SCRIPT))
    expect(back.settings.produce).toBe(20)
    expect(back.settings.format).toBe('oneline')
    expect(back.settings.dealSource).toBe('random')
    // No seed named, so the reader rolls one rather than everyone seeing the
    // same hands from a link that never said which.
    expect('seed' in back.settings).toBe(false)
  })

  it('says so when the scenario cannot be fetched', async () => {
    await expect(
      open('#s=No_Such_Thing', () => Promise.reject(new Error('No dealer script for No_Such_Thing (HTTP 404)'))),
    ).rejects.toThrow(/No dealer script/)
  })
})

describe('a link that is not what it claims', () => {
  it('says a short link cannot be opened yet, rather than opening nothing', async () => {
    await expect(open('#k=Ab3x9')).rejects.toThrow(/short link/i)
  })

  it('says a truncated link was truncated', async () => {
    // The common failure by a distance: mail clients wrap long URLs, and the
    // fragment is the end of one.
    const whole = await encodeDocument(makeDocument(SCRIPT, page))
    await expect(open(`#d=${whole.slice(0, whole.length - 12)}`)).rejects.toThrow(/damaged/)
  })

  it('refuses base64 that is not base64', async () => {
    await expect(decodeDocument('not base64 !!')).rejects.toThrow(/damaged/)
  })

  it('refuses a version it does not know rather than half-loading it', async () => {
    const future = await encodeDocument({ v: DOCUMENT_VERSION + 1, script: SCRIPT, settings: {} })
    await expect(open(`#d=${future}`)).rejects.toThrow(/newer dealer3/)
  })

  it('will not be talked into inflating a bomb', async () => {
    // A fragment of a few hundred characters can inflate to hundreds of
    // megabytes, and a tab that hangs before anything is on screen is the one
    // failure with no way back.
    const zeros = new Uint8Array(4 * 1024 * 1024)
    const stream = new Blob([zeros]).stream().pipeThrough(new CompressionStream('deflate-raw'))
    const packed = new Uint8Array(await new Response(stream).arrayBuffer())
    let binary = ''
    for (const byte of packed) binary += String.fromCharCode(byte)
    const base64url = btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
    await expect(decodeDocument(base64url)).rejects.toThrow(/more than any script/)
  })
})

describe('shareUrl', () => {
  it('keeps the page it was made on, wherever that is served from', () => {
    // Relative everywhere else in this app for the same reason: the site is
    // both `dealer3.pages.dev` and `/dealer3/` under the apex.
    expect(
      shareUrl(
        { origin: 'https://bridge-craftwork.com', pathname: '/dealer3/', search: '' },
        '#s=Weak_2_Bids',
      ),
    ).toBe('https://bridge-craftwork.com/dealer3/#s=Weak_2_Bids')
  })
})
