import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'

import { renderGuide, resolveLink, slug } from './guide.js'

const GUIDE = readFileSync(
  fileURLToPath(new URL('../../../docs/leveling-guide.md', import.meta.url)),
  'utf8',
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
  const html = renderGuide(GUIDE)

  it('gives headings the anchors the contents list points at', () => {
    expect(html).toContain('id="the-one-thing-to-get-right"')
    expect(html).toContain('id="naming-the-hand-types"')
  })

  it('puts tables in their own scroll box', () => {
    expect(html).toContain('<div class="table-scroll"><table>')
  })

  it('renders fenced code without escaping it into prose', () => {
    expect(html).toContain('<pre>')
    expect(html).toContain('HandType_12_14')
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
