// Rendering a docs/ markdown file as a page on this site.
//
// The guide has one source — `docs/leveling-guide.md` — read three ways: as
// markdown on GitHub, as a PDF built in CI, and as this page. Rendering that
// same file rather than keeping a second copy is the arrangement the language
// reference already has with `vocabulary.rs`: one thing that can be wrong,
// rather than two that can disagree.

import { marked } from 'marked'

/// Where a repo-relative link should point when the page is on the web.
///
/// GitHub spells a file and a directory differently — `blob` against `tree` —
/// and gets the wrong one wrong rather than redirecting, so a link to
/// `../examples/` has to be told which it is. A trailing slash is what says so,
/// and it is how the markdown already writes directories.
const REPO = 'https://github.com/bridge-craftwork/Dealer3'

/// The directory the rendered document lives in, so `../examples/` resolves.
const DOC_DIR = 'docs'

/// Resolve a link written for the repo into one that works from a web page.
///
/// The markdown is written to be read in the repo, where `leveling-strategy.md`
/// and `../examples/` are the natural spellings. Left alone they would 404 here,
/// so they are pointed back at GitHub — all of them, since a relative link that
/// happens to resolve to something on this site is the more dangerous case.
///
/// In-page anchors are left exactly as they are: those are the document's own
/// contents list, and they have to stay on the page.
export function resolveLink(href) {
  if (!href) return href
  if (href.startsWith('#')) return href
  if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith('//')) return href

  // Split any anchor off before joining, or it lands inside the path.
  const hash = href.indexOf('#')
  const path = hash === -1 ? href : href.slice(0, hash)
  const fragment = hash === -1 ? '' : href.slice(hash)

  const kind = path.endsWith('/') ? 'tree' : 'blob'
  const out = []
  for (const part of `${DOC_DIR}/${path}`.split('/')) {
    if (part === '.' || part === '') continue
    if (part === '..') out.pop()
    else out.push(part)
  }
  return `${REPO}/${kind}/main/${out.join('/')}${fragment}`
}

/// The anchor GitHub would give a heading of this text.
///
/// GitHub's rules rather than anything of our own, because the document is
/// written to be read there too and its own `#the-one-thing-to-get-right`
/// links have to land in both places: lowercase, punctuation dropped, spaces
/// to hyphens.
export function slug(text) {
  return text
    .toLowerCase()
    .trim()
    .replace(/[^\w\- ]+/g, '')
    .replace(/\s+/g, '-')
}

/// Render a docs/ markdown document to HTML for this site.
export function renderGuide(markdown) {
  const renderer = new marked.Renderer()

  renderer.heading = ({ tokens, depth }) => {
    const html = renderer.parser.parseInline(tokens)
    const id = slug(html.replace(/<[^>]*>/g, ''))
    return `<h${depth} id="${id}">${html}</h${depth}>\n`
  }

  renderer.link = ({ href, title, tokens }) => {
    const text = renderer.parser.parseInline(tokens)
    const target = resolveLink(href)
    const away = !target.startsWith('#')
    const attrs = [
      `href="${target}"`,
      title ? `title="${title}"` : '',
      away ? 'target="_blank" rel="noopener noreferrer"' : '',
    ]
      .filter(Boolean)
      .join(' ')
    return `<a ${attrs}>${text}</a>`
  }

  // Wide tables scroll inside their own box rather than pushing the page out.
  // Several here are five and six columns of measurements.
  renderer.table = (token) => {
    const cell = (c, tag, align) =>
      `<${tag}${align ? ` align="${align}"` : ''}>${renderer.parser.parseInline(c.tokens)}</${tag}>`
    const head = token.header.map((c, i) => cell(c, 'th', token.align[i])).join('')
    const body = token.rows
      .map((row) => `<tr>${row.map((c, i) => cell(c, 'td', token.align[i])).join('')}</tr>`)
      .join('')
    return `<div class="table-scroll"><table><thead><tr>${head}</tr></thead><tbody>${body}</tbody></table></div>\n`
  }

  return marked.parse(markdown, { renderer, gfm: true })
}
