// The page a client that does not run scripts receives.
//
// Everything below `#app` is rendered by JavaScript, so a crawler, an assistant
// asked to read the URL, or a reader with scripting off sees only what is
// literally in `index.html`. That was a title and one sentence — 883 bytes —
// while the language reference sat one path segment away with nothing pointing
// at it.
//
// These assert on the source file rather than on rendered output, because the
// source file is what ships: Vite copies `index.html` through, and anything
// removed from it is removed from what a fetcher gets. Nothing else in the
// suite would notice its absence, which is the whole reason for this file.

import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { renderLlmsText, SITE } from './referenceText.js'

const html = readFileSync(new URL('../../index.html', import.meta.url), 'utf8')

describe('the page a non-scripting client gets', () => {
  it('points at the plain-text reference from the head', () => {
    // The one machine-readable thread from the app URL — which is what someone
    // will actually paste — to the language itself.
    expect(html).toMatch(/<link\s+[^>]*rel="alternate"[^>]*>/s)
    expect(html).toMatch(/rel="alternate"[\s\S]*?type="text\/plain"/)
    expect(html).toMatch(/rel="alternate"[\s\S]*?href="reference\.txt"/)
  })

  it('links the reference relatively, so both hosts resolve it', () => {
    // The site is served at `dealer3.pages.dev` and under the `/dealer3/`
    // mount. An absolute path would be right on one and wrong on the other.
    expect(html).not.toMatch(/href="\/reference\.txt"/)
    expect(html).not.toMatch(/href="https?:[^"]*reference\.txt"/)
  })

  it('says what dealer3 is, and where to read the language, without scripting', () => {
    const noscript = html.match(/<noscript>([\s\S]*?)<\/noscript>/)?.[1]
    expect(noscript).toBeTruthy()
    expect(noscript).toContain('reference.txt')
    expect(noscript).toMatch(/bridge/i)
  })

  it('describes itself in more than a phrase', () => {
    // A model with nothing else quotes the description, so it has to carry the
    // shape of the tool rather than a slogan.
    const description = html.match(/name="description"\s+content="([^"]*)"/)?.[1]
    expect(description).toBeTruthy()
    expect(description.length).toBeGreaterThan(120)
  })
})

describe('llms.txt', () => {
  /** A miniature `language_info()`, shaped as the engine's is. */
  const info = {
    function_groups: ['Hand evaluation'],
    function_docs: [
      { name: 'hcp', group: 'Hand evaluation', summary: 'High card points.', alias_of: null, note: null },
    ],
    operator_docs: [],
    statement_docs: [],
  }

  it('links the reference, which is the only reason it exists', () => {
    expect(renderLlmsText(info, '1.0.0', 29081)).toContain(`${SITE}/reference.txt`)
  })

  it('reports the size and entry count of the build that emitted it', () => {
    // Generated rather than written for this reason: these are true of the
    // build that wrote them, where a hand-kept copy would be true of whichever
    // build someone last remembered.
    const text = renderLlmsText(info, '2.3.4', 40960)
    expect(text).toContain('40 KB')
    expect(text).toContain('engine 2.3.4')
    expect(text).toContain('1 entries')
  })

  it('states that the reference is closed, which is what makes it worth reading', () => {
    // A model's alternative to reading this is guessing, and it will guess
    // plausibly. The claim that the list is exhaustive is what makes it stop —
    // and it is only true while the file stays generated from the engine.
    expect(renderLlmsText(info, '1.0.0', 29081)).toMatch(/cannot name a function that does not exist/)
  })
})
