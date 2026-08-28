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

/// Every image in `docs/images/`, bundled and content-hashed by Vite.
///
/// Bundled rather than fetched from raw.githubusercontent: the guide's pictures
/// are screenshots of this app, and a URL pointing at `main` would show the
/// image that is on `main` — so a new one 404s until it is pushed, and an
/// edited one serves the old copy to anyone whose cache has it. Going through
/// the bundler keeps the picture and the page it explains in the same build.
///
/// Keyed by path relative to `docs/`, which is how the markdown writes them.
/// (Three levels up, not two: this file is `web/src/lib/`, one deeper than
/// `Leveling.vue`. `../../docs` from here is `web/docs`, which does not exist —
/// and a glob that matches nothing is silent.)
const IMAGES = Object.fromEntries(
  Object.entries(
    import.meta.glob('../../../docs/images/*', { eager: true, query: '?url', import: 'default' }),
  ).map(([path, url]) => [path.replace('../../../docs/', ''), url]),
)

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

/// Resolve an image written for the repo into the bundled asset.
///
/// Separate from `resolveLink` because the answer is a different kind of thing:
/// a link wants a page a reader can browse, an image wants the file itself.
///
/// `images` is injected in tests; the default is the bundle. An image the
/// bundle does not have throws rather than rendering a broken box, since a
/// picture that silently fails to load is exactly the fault that survives
/// every unit test.
export function resolveImage(src, images = IMAGES) {
  if (!src) return src
  if (/^[a-z][a-z0-9+.-]*:/i.test(src) || src.startsWith('//')) return src

  const out = []
  for (const part of `${DOC_DIR}/${src}`.split('/')) {
    if (part === '.' || part === '') continue
    if (part === '..') out.pop()
    else out.push(part)
  }
  const key = out.slice(1).join('/')
  const url = images[key]
  if (!url) {
    throw new Error(
      `the guide references ${src}, which is not in docs/images/ — ` +
        `add the file, or fix the path`,
    )
  }
  return url
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
///
/// `images` is injected by the tests, which run outside the bundler and so have
/// no glob to resolve against.
export function renderGuide(markdown, images = IMAGES) {
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

  // A figure rather than a bare image: these are screenshots of the app, and
  // the alt text is doing real work for anyone who cannot see them.
  renderer.image = ({ href, title, text }) => {
    const caption = title ? `<figcaption>${title}</figcaption>` : ''
    const alt = (text || '').replace(/\s+/g, ' ').replace(/"/g, '&quot;')
    return `<figure><img src="${resolveImage(href, images)}" alt="${alt}" loading="lazy">${caption}</figure>`
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
