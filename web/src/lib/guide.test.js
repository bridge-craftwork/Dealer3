import { describe, expect, it } from 'vitest'
import { readdirSync, readFileSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'

import { renderGuide, resolveImage, resolveLink, slug } from './guide.js'

const GUIDE = readFileSync(
  fileURLToPath(new URL('../../../docs/leveling-guide.md', import.meta.url)),
  'utf8',
)

// Stands in for the Vite glob over `docs/images/`, which only exists inside a
// build. Keyed the way the markdown writes the paths.
const IMAGES = Object.fromEntries(
  readdirSync(fileURLToPath(new URL('../../../docs/images', import.meta.url))).map((name) => [
    `images/${name}`,
    `/assets/${name}`,
  ]),
)

describe('resolveLink', () => {
  it('keeps in-page anchors on the page', () => {
    expect(resolveLink('#the-one-thing-to-get-right')).toBe('#the-one-thing-to-get-right')
  })

  it('leaves absolute URLs alone', () => {
    expect(resolveLink('https://example.com/x')).toBe('https://example.com/x')
    expect(resolveLink('mailto:someone@example.com')).toBe('mailto:someone@example.com')
  })

  it('points a sibling docs/ file at the repo', () => {
    expect(resolveLink('leveling-strategy.md')).toBe(
      'https://github.com/bridge-craftwork/Dealer3/blob/main/docs/leveling-strategy.md',
    )
  })

  it('climbs out of docs/ for a repo-root path', () => {
    expect(resolveLink('../examples/README.md')).toBe(
      'https://github.com/bridge-craftwork/Dealer3/blob/main/examples/README.md',
    )
  })

  // GitHub 404s a directory under `blob` rather than redirecting to `tree`,
  // which is exactly the sort of link that looks fine until it is clicked.
  it('spells a directory link `tree`, not `blob`', () => {
    expect(resolveLink('../examples/')).toBe(
      'https://github.com/bridge-craftwork/Dealer3/tree/main/examples',
    )
    expect(resolveLink('leveling-strategy.md')).toContain('/blob/main/')
  })

  it('keeps an anchor out of the path when rewriting', () => {
    expect(resolveLink('leveling-strategy.md#the-roll-variable')).toBe(
      'https://github.com/bridge-craftwork/Dealer3/blob/main/docs/leveling-strategy.md#the-roll-variable',
    )
  })
})

describe('resolveImage', () => {
  it('resolves a docs image to the bundled asset', () => {
    expect(resolveImage('images/hand-types-panel.png', IMAGES)).toBe(
      '/assets/hand-types-panel.png',
    )
  })

  it('leaves an absolute or data URL alone', () => {
    expect(resolveImage('https://example.com/a.png', IMAGES)).toBe('https://example.com/a.png')
    expect(resolveImage('data:image/png;base64,AAAA', IMAGES)).toBe('data:image/png;base64,AAAA')
  })

  // A picture that silently fails to load is the fault that survives every
  // unit test, so a missing one is an error at render rather than a broken box
  // in the page.
  it('throws on an image the bundle does not have', () => {
    expect(() => resolveImage('images/not-there.png', IMAGES)).toThrow(/not in docs\/images/)
  })
})

describe('slug', () => {
  // GitHub's rules, so the document's own contents list lands in both places.
  it('matches what GitHub would anchor a heading to', () => {
    expect(slug('The one thing to get right')).toBe('the-one-thing-to-get-right')
    expect(slug('Choosing a target mix, and what it costs')).toBe(
      'choosing-a-target-mix-and-what-it-costs',
    )
    expect(slug('What it refuses to do, and what to do about it')).toBe(
      'what-it-refuses-to-do-and-what-to-do-about-it',
    )
  })
})

describe('renderGuide', () => {
  const html = renderGuide(GUIDE, IMAGES)

  it('gives headings the anchors the contents list points at', () => {
    expect(html).toContain('id="a-worked-example"')
    expect(html).toContain('id="the-magic-words"')
    expect(html).toContain('id="how-good-is-the-result"')
  })

  it('puts tables in their own scroll box', () => {
    expect(html).toContain('<div class="table-scroll"><table>')
  })

  it('renders fenced code without escaping it into prose', () => {
    expect(html).toContain('<pre>')
    expect(html).toContain('HandType_12_14')
  })

  it('renders an image as a figure with alt text and a caption', () => {
    expect(html).toContain('<figure><img src="/assets/hand-types-panel.png"')
    expect(html).toMatch(/<img [^>]*alt="[^"]{20,}"/)
    expect(html).toContain('<figcaption>')
  })

  // Rendering the whole guide is itself the check that every picture it
  // references exists, since resolveImage throws on one that does not.
  it('references only images the repo has', () => {
    const srcs = [...html.matchAll(/<img [^>]*src="([^"]+)"/g)].map((m) => m[1])
    expect(srcs.length).toBeGreaterThan(0)
  })

  it('sends every link somewhere that resolves', () => {
    // The wiring fault this guards: a repo-relative link rendered as-is is a
    // 404 on the site, and nothing about the page looks wrong until it is
    // clicked.
    const hrefs = [...html.matchAll(/href="([^"]+)"/g)].map((m) => m[1])
    expect(hrefs.length).toBeGreaterThan(10)
    for (const href of hrefs) {
      expect(href.startsWith('#') || /^https?:/.test(href)).toBe(true)
    }
  })

  it('links every entry in the contents list to a heading that exists', () => {
    const ids = new Set([...html.matchAll(/<h[23] id="([^"]+)"/g)].map((m) => m[1]))
    const anchors = [...html.matchAll(/href="#([^"]+)"/g)].map((m) => m[1])
    expect(anchors.length).toBeGreaterThan(5)
    for (const anchor of anchors) {
      expect(ids, `#${anchor} has no heading`).toContain(anchor)
    }
  })
})

describe('the guide itself', () => {
  // The figures the CLI and the browser actually enforce. Both are load-bearing
  // — a levelling measured on too little is the one error producing more deals
  // cannot fix — so the guide is held to naming them.
  it('states both sample-size floors', () => {
    expect(GUIDE).toContain('500')
    expect(GUIDE).toContain('50 sightings')
    expect(GUIDE).toContain('10,000')
  })

  it('carries no hand-written switch or language status table', () => {
    // Those are generated and verified by `cargo test`; a second copy here
    // would drift. CLAUDE.md is emphatic about it.
    expect(GUIDE).not.toMatch(/✅|❌|switches implemented/i)
  })
})
