// Putting a run in a link, and taking one back out.
//
// ## Why the fragment, and never the query string
//
// Everything after `#` stays in the browser. It is not sent with the request,
// so it reaches no server log, no cache key and no edge rule — which is what
// makes this shippable with no back end at all, and the reason a link works
// for ever and costs nothing to keep working.
//
// It is also the form that survives the trip. A real BBO capture sent as a
// query string came back **403 at the edge** on 2026-08-28, while the identical
// payload in the fragment loaded; bridge-solver already hands hands over this
// way for that reason, and the cross-tool contract (bridge-craftwork-site#3,
// item 4) settles it for every tool here. Do not redesign it.
//
// ## Three forms, one load path
//
//   #s=<scenario>   a scenario from the list, unmodified, plus what was changed
//   #d=<base64url>  the document itself, deflated — no network at all
//   #k=<key>        fetched from the short-link service (#103), `shortLinks.js`
//
// All three resolve to the same document, so opening one is one path with
// three sources rather than three features.
//
// ## The cheap case is the common one
//
// Someone sharing a scenario they picked from the list has not changed a
// character of it, and there is nothing to encode: the recipient can fetch the
// same script from the same place. `#s=Sup_X_By_Advancer&seed=8391` against
// some five hundred characters of base64 is the difference between a link
// somebody will read and one they will only paste.
//
// A modified script takes the second form. 600-900 bytes of dealer script
// deflates to roughly 250-400, which is 350-550 base64 characters — ugly in an
// email and still strictly better than sending a PDF nobody can run.

import { DOCUMENT_VERSION, readDocument, settingsDelta } from './envelope.js'
import { EXPIRED, SHORT_LINK_PATH, normalizeKey } from './shortKey.js'

/// Longer than this and it is not a link anyone made from this page: the whole
/// point of the `#s=` form is that the big ones are rare. Refusing early keeps
/// a hostile fragment from being decompressed at all.
const MAX_FRAGMENT_CHARS = 64 * 1024

/// What a fragment is allowed to inflate to. Deflate is happy to turn a few
/// hundred bytes into a few hundred megabytes, and a link that hangs the tab
/// before anything is on screen is the one failure with no way back.
const MAX_DOCUMENT_BYTES = 512 * 1024

/// Booleans in the readable form. `1` rather than `true` because these are read
/// by people as often as by the page.
const TRUE = ['1', 'true', 'yes', 'on']

/**
 * Which of the three forms a fragment is, if it is one at all.
 *
 * Anything else — no fragment, someone else's fragment, an in-page anchor —
 * returns null, and the page carries on as if it had been opened plainly.
 *
 * @param {string} hash `location.hash`, with or without its `#`
 * @returns {{kind: string}|null}
 */
export function parseFragment(hash) {
  const text = String(hash || '').replace(/^#/, '')
  if (!text || text.length > MAX_FRAGMENT_CHARS) return null
  const query = new URLSearchParams(text)
  // In order of cheapness, which is also the order they should be preferred in
  // if a hand-edited fragment somehow carries two.
  const slug = query.get('s')
  if (slug) return { kind: 'scenario', slug, query }
  const data = query.get('d')
  if (data) return { kind: 'document', data }
  const key = query.get('k')
  // Normalised here so a key a phone has lower-cased still opens; one that is
  // not a key at all is still a short link, just a broken one, and is said so.
  if (key) return { kind: 'key', key: normalizeKey(key) ?? key }
  return null
}

/**
 * Open a fragment: the document it names, and which form it came in.
 *
 * `fetchScenario` is passed in rather than imported so this stays testable
 * without a network, and so the one caller that needs a scenario is the one
 * that supplies the way to get it.
 *
 * `fetchShort` likewise: it answers `{d, expires}` for a key, the `d` being
 * exactly what a long link would carry — so a short link opens by the same path
 * as a `#d=` one, and differs only in knowing when it stops working.
 *
 * @param {string} hash `location.hash`
 * @param {{fetchScenario?: (slug: string) => Promise<string>,
 *          fetchShort?: (key: string) => Promise<{d: string, expires: string|null}>}} sources
 * @returns {Promise<{source: string, doc: object, expires?: string|null}|null>}
 * @throws {Error} with a message meant to be shown
 */
export async function resolveFragment(hash, { fetchScenario, fetchShort } = {}) {
  const found = parseFragment(hash)
  if (!found) return null

  if (found.kind === 'key') {
    if (typeof fetchShort !== 'function') {
      throw new Error('This is a short link, and there is no way to open one here.')
    }
    const { d, expires = null } = await fetchShort(found.key)
    return { source: 'short', doc: readDocument(await decodeDocument(d)), expires }
  }

  if (found.kind === 'document') {
    return { source: 'document', doc: readDocument(await decodeDocument(found.data)) }
  }

  if (typeof fetchScenario !== 'function') {
    throw new Error('This link names a scenario, and there is no way to fetch one here.')
  }
  const script = await fetchScenario(found.slug)
  return {
    source: 'scenario',
    doc: readDocument({
      v: DOCUMENT_VERSION,
      script,
      settings: { ...settingsFromQuery(found.query), scenario: found.slug },
    }),
  }
}

/**
 * The fragment for a document, in the shortest form that can carry it.
 *
 * `pristineScript` is the scenario's text as it was fetched. Equal to the
 * document's script, the script itself need not travel; changed by so much as
 * a comment, it must, or the recipient would open a different script under the
 * right name — which is worse than a long link.
 *
 * @param {object} doc a document, from `makeDocument`
 * @param {{pristineScript?: string|null}} options
 * @returns {Promise<string>} the fragment, `#` and all
 */
export async function shareFragment(doc, { pristineScript = null } = {}) {
  if (doc?.settings?.scenario && pristineScript != null && pristineScript === doc.script) {
    return scenarioFragment(doc)
  }
  return `#d=${await encodeDocument(doc)}`
}

/** `#s=<scenario>` plus whatever was changed from the defaults. */
export function scenarioFragment(doc) {
  const query = new URLSearchParams()
  query.set('s', doc.settings.scenario)
  for (const [key, value] of Object.entries(settingsDelta(doc.settings))) {
    // Already said, as the `s` this whole form is built around.
    if (key === 'scenario') continue
    if (Array.isArray(value)) {
      for (const item of value) query.append(key, item)
    } else if (typeof value === 'boolean') {
      query.set(key, value ? '1' : '0')
    } else {
      query.set(key, String(value))
    }
  }
  return `#${query.toString()}`
}

// --- Short links (#103) ---------------------------------------------------
//
// The service is `shortLinks.js`, behind `functions/`. These are the page's
// half: asking for a key, opening one, and the link a key makes.

/**
 * The short link for a key: `<this page>/30-days/<key>`.
 *
 * Built against the page's own directory, so it is `/dealer3/30-days/…` on the
 * apex and `/30-days/…` on pages.dev, with nothing hard-coded to either.
 */
export function shortLinkUrl(location, key) {
  return new URL(`${SHORT_LINK_PATH}/${key}`, `${location.origin}${location.pathname}`).href
}

/**
 * The key a `?k=` names, if it names one.
 *
 * A short link arrives as `/?k=<key>` — see `redirectShortLink` for why a query
 * and not a fragment — and the page moves it into the fragment on arrival, so
 * there is still one place a link is read from.
 */
export function shortKeyFromQuery(search) {
  const key = new URLSearchParams(String(search || '')).get('k')
  return key ? key.slice(0, 32) : null
}

/// Where the service answers, relative to the page — so `/dealer3/api/short`
/// through the apex and `/api/short` on pages.dev.
const SERVICE = 'api/short'

/**
 * Store a document and get its short link's key.
 *
 * @param {object} doc a document, from `makeDocument`
 * @param {{base: string, fetch?: typeof fetch}} options `base` is the page's URL
 * @returns {Promise<{key: string, expires: string}>}
 * @throws {Error} with a message meant to be shown
 */
export async function requestShortLink(doc, { base, fetch: get = fetch } = {}) {
  const payload = await encodeDocument(doc)
  let response
  try {
    response = await get(new URL(SERVICE, base).href, {
      method: 'POST',
      headers: { 'content-type': 'text/plain' },
      body: payload,
    })
  } catch {
    throw new Error('Could not reach the short-link service. The long link works without it.')
  }
  const body = await readJson(response)
  if (response.ok && body?.key) return { key: body.key, expires: body.expires ?? null }
  throw new Error(
    body?.error ||
      // A static host with no service behind it — a local build, say — answers
      // 404 or 405 with a page, not with JSON.
      'Short links are not available from this copy of the page. The long link works without them.',
  )
}

/**
 * What a short link's key holds. Shaped to be passed as `fetchShort`.
 *
 * @param {string} key
 * @param {{base: string, fetch?: typeof fetch}} options
 * @returns {Promise<{d: string, expires: string|null}>}
 * @throws {Error} with a message meant to be shown
 */
export async function fetchShortLink(key, { base, fetch: get = fetch } = {}) {
  let response
  try {
    response = await get(new URL(`${SERVICE}/${encodeURIComponent(key)}`, base).href)
  } catch {
    throw new Error('Could not reach the short-link service to open this link. Try again shortly.')
  }
  const body = await readJson(response)
  if (response.ok && typeof body?.d === 'string') return { d: body.d, expires: body.expires ?? null }
  throw new Error(response.status === 404 ? body?.error || EXPIRED : body?.error || 'This short link could not be opened.')
}

async function readJson(response) {
  try {
    return await response.json()
  } catch {
    return null
  }
}

/** The whole link: this page, wherever it is being served from, plus the fragment. */
export function shareUrl(location, fragment) {
  return `${location.origin}${location.pathname}${location.search}${fragment}`
}

/// The `#s=` form's settings, read back from ordinary `name=value` pairs.
///
/// Spelled out in full rather than abbreviated. This form exists to be short
/// enough to read, and a link saying `p=20&f=pbn` is shorter still and says
/// nothing to the person holding it — while a second vocabulary is one more
/// thing to keep in step with the document.
function settingsFromQuery(query) {
  const settings = {}
  for (const key of ['seed', 'produce', 'maxGenerate', 'measureSeconds']) {
    const value = query.get(key)
    if (value != null && value !== '' && Number.isFinite(Number(value))) {
      settings[key] = Number(value)
    }
  }
  for (const key of ['autoLevel', 'roundRobin', 'newSeedEachRun']) {
    const value = query.get(key)
    if (value != null) settings[key] = TRUE.includes(value.toLowerCase())
  }
  for (const key of ['format', 'dealSource']) {
    const value = query.get(key)
    if (value) settings[key] = value
  }
  const params = query.getAll('params')
  if (params.length) settings.params = params
  return settings
}

// --- The wire: deflate, then base64url ------------------------------------
//
// `CompressionStream` is native in Safari 16.4, Chrome and Firefox, so this
// needs no library. `deflate-raw` rather than `deflate` or `gzip`: the wrapper
// bytes are pure cost in a link, and both ends are this file.

/** Whether this browser can make a `#d=` link at all. */
export function canCompress() {
  return typeof CompressionStream === 'function' && typeof DecompressionStream === 'function'
}

/** A document as the base64url of its deflated JSON. */
export async function encodeDocument(doc) {
  if (!canCompress()) {
    throw new Error(
      'This browser cannot compress, so it cannot make a link. Safari 16.4, Chrome and ' +
        'Firefox all can.',
    )
  }
  const json = JSON.stringify(doc)
  const stream = new Blob([json]).stream().pipeThrough(new CompressionStream('deflate-raw'))
  return toBase64Url(new Uint8Array(await new Response(stream).arrayBuffer()))
}

/** And back: base64url, inflate, parse. Throws a showable message on any of it. */
export async function decodeDocument(text) {
  if (!canCompress()) {
    throw new Error(
      'This browser cannot decompress, so it cannot open this link. Safari 16.4, Chrome ' +
        'and Firefox all can.',
    )
  }
  let json
  try {
    json = await inflate(fromBase64Url(text))
  } catch (e) {
    // A truncated link is the common cause by a distance: mail clients wrap
    // long URLs, and the fragment is the end of one.
    throw new Error(
      `This link is damaged and cannot be read (${e?.message || e}). It may have been cut ` +
        'short — check that the whole of it was copied.',
    )
  }
  try {
    return JSON.parse(json)
  } catch {
    throw new Error('This link is damaged: what it holds is not a dealer3 script.')
  }
}

function toBase64Url(bytes) {
  let binary = ''
  // `String.fromCharCode(...bytes)` overflows the argument list on a large
  // enough script, and does it only for the large ones.
  const CHUNK = 0x8000
  for (let at = 0; at < bytes.length; at += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(at, at + CHUNK))
  }
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

function fromBase64Url(text) {
  if (!/^[A-Za-z0-9_-]+$/.test(text)) throw new Error('it is not base64url')
  const padded = text.replace(/-/g, '+').replace(/_/g, '/')
  const binary = atob(padded.padEnd(Math.ceil(padded.length / 4) * 4, '='))
  const bytes = new Uint8Array(binary.length)
  for (let at = 0; at < binary.length; at++) bytes[at] = binary.charCodeAt(at)
  return bytes
}

/// Inflate with a budget, reading the stream rather than awaiting all of it:
/// `Response.text()` would have the whole bomb in memory before anything could
/// object to its size.
async function inflate(bytes) {
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('deflate-raw'))
  const reader = stream.getReader()
  const chunks = []
  let total = 0
  for (;;) {
    const { value, done } = await reader.read()
    if (done) break
    total += value.length
    if (total > MAX_DOCUMENT_BYTES) {
      await reader.cancel()
      throw new Error('it holds far more than any script')
    }
    chunks.push(value)
  }
  const all = new Uint8Array(total)
  let at = 0
  for (const chunk of chunks) {
    all.set(chunk, at)
    at += chunk.length
  }
  return new TextDecoder().decode(all)
}
