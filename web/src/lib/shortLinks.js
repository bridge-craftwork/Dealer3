// Short links, for sending a script by text message (#103).
//
// A `#d=` link carries its own script and needs no server, but it runs to
// 350-550 characters: fine in an email, too long for a text. This is the one
// case that needs a service, and this file is that service — the Pages
// Functions under `functions/` are thin wrappers that hand it their KV binding,
// so everything here is testable without Cloudflare.
//
// ## What is stored is the long link
//
// The value under a key is the `#d=` payload, exactly as the long link carries
// it: the document, deflated and base64url'd. A short link is therefore an
// alias for a long one and nothing more, opened by the same `decodeDocument`
// and `readDocument` as every other link — and checked by them on the way in,
// so the service refuses anything the page could not open. Compressed, the
// 8 KB cap below holds around 25 KB of script, which covers the largest of
// the PBS scenarios with room left over.
//
// ## Thirty days, fixed
//
// `expirationTtl` on the put, so Cloudflare deletes it and there is no cleanup
// to write. Deliberately NOT refreshed on read: these are meant to be opened
// within the week they were sent, a sliding expiry would be invisible to the
// person holding one, and re-putting on every read would spend the free tier's
// scarce write allowance on reads. Fixed means the page can name the day a link
// stops working, which is the thing a recipient can actually plan around.
//
// ## Abuse control
//
// An unauthenticated write endpoint is a pastebin. What bounds it here: the
// body cap, the document having to open, the `Origin` check, and the free
// tier's daily write allowance, which fails closed — a flood costs short links
// for the rest of the day and nothing else, because the long link needs no
// service. Per-IP limiting is not here: Pages Functions cannot take the
// rate-limit binding, and a KV counter would spend a write per share on the very
// allowance it was protecting. A zone rate-limiting rule is the place for it.

import { decodeDocument, encodeDocument } from './share.js'
import { readDocument } from './envelope.js'
import { EXPIRED, SHORT_LINK_DAYS, newKey, normalizeKey } from './shortKey.js'

const TTL_SECONDS = SHORT_LINK_DAYS * 24 * 60 * 60

/// The largest body accepted: the `d` payload, which is already compressed.
export const MAX_BODY_BYTES = 8 * 1024

/// Where a write may come from. The apex, where the page lives, and its own
/// Pages addresses — production and the per-branch previews — so a preview
/// deploy can be tried end to end. `wrangler pages dev` serves on localhost.
const ALLOWED_ORIGINS = [
  /^https:\/\/bridge-craftwork\.com$/,
  /^https:\/\/([a-z0-9-]+\.)?dealer3\.pages\.dev$/,
  /^http:\/\/(localhost|127\.0\.0\.1)(:\d+)?$/,
]

/** Whether a write from this `Origin` is one this page made. */
export function originAllowed(origin) {
  return !!origin && ALLOWED_ORIGINS.some((pattern) => pattern.test(origin))
}

/**
 * `POST /api/short` — store a `d` payload, and answer with its key.
 *
 * @param {Request} request the body is the `#d=` payload, as text
 * @param {KVNamespace} kv
 * @param {{now?: () => number, key?: () => string}} options for tests
 * @returns {Promise<Response>}
 */
export async function createShortLink(request, kv, { now = Date.now, key = newKey } = {}) {
  if (!originAllowed(request.headers.get('origin'))) {
    return refuse(403, 'Short links can only be made from the dealer3 page.')
  }

  // Read with a budget rather than trusting Content-Length, which the client
  // supplies.
  const body = await readCapped(request, MAX_BODY_BYTES)
  if (body == null) {
    return refuse(
      413,
      'This script is too large for a short link. The long link carries it in full, and ' +
        'never expires.',
    )
  }

  // The same two checks a recipient's page makes, so nothing is stored that
  // could not be opened. Re-encoded from what they return, so what is kept is
  // the document as this version reads it and not whatever else arrived with it.
  let payload
  try {
    payload = await encodeDocument(readDocument(await decodeDocument(body.trim())))
  } catch (e) {
    return refuse(400, e?.message || String(e))
  }

  // Said apart from the allowance running out, which it would otherwise be
  // reported as: a deploy that lost its binding looks exactly like a busy day.
  if (!kv) return refuse(503, 'Short links are not set up on this copy of the page.')

  const expires = new Date(now() + TTL_SECONDS * 1000).toISOString()
  try {
    for (let attempt = 0; attempt < 3; attempt++) {
      const candidate = key()
      if ((await kv.get(candidate)) != null) continue
      await kv.put(candidate, payload, { expirationTtl: TTL_SECONDS, metadata: { expires } })
      return json(201, { key: candidate, expires })
    }
  } catch (e) {
    // On the free tier this is the day's write allowance running out, far more
    // often than it is anything else. Logged, so the dashboard can say which.
    console.error('short link: KV refused the write', e?.message || e)
    return refuse(
      503,
      'Short links are unavailable just now — most likely used up for today. The long link ' +
        'works, and never expires.',
    )
  }
  return refuse(503, 'Could not find a free short link. Try again.')
}

/**
 * `GET /api/short/<key>` — the `d` payload a key holds, and when it stops.
 *
 * @param {string} rawKey as it appeared in the path
 * @param {KVNamespace} kv
 * @returns {Promise<Response>}
 */
export async function readShortLink(rawKey, kv) {
  const key = normalizeKey(rawKey)
  if (!key) return refuse(404, EXPIRED)
  if (!kv) return refuse(503, 'Short links are not set up on this copy of the page.')
  const { value, metadata } = await kv.getWithMetadata(key)
  // KV cannot tell an expired key from one that never existed, and nor can
  // this. Expiry is the likely one.
  if (value == null) return refuse(404, EXPIRED)
  return json(200, { d: value, expires: metadata?.expires ?? null })
}

/**
 * `GET /30-days/<key>` — the link people are sent. Hands over to the page.
 *
 * To `/?k=<key>` rather than `/#k=<key>`: the apex proxy maps a redirect back
 * into its own path space and does not carry a fragment through, so a `#k=`
 * would arrive as a page with no link in it. An eight-character key in a query
 * string reaches no rule that a script would; the page moves it into the
 * fragment on arrival, where every other link lives.
 *
 * @param {string} rawKey
 * @returns {Response}
 */
export function redirectShortLink(rawKey) {
  const key = normalizeKey(rawKey) ?? encodeURIComponent(String(rawKey || '').slice(0, 32))
  return new Response(null, {
    status: 302,
    headers: { location: `/?k=${key}`, 'cache-control': 'no-store' },
  })
}

async function readCapped(request, limit) {
  if (!request.body) return ''
  const reader = request.body.getReader()
  const chunks = []
  let total = 0
  for (;;) {
    const { value, done } = await reader.read()
    if (done) break
    total += value.length
    if (total > limit) {
      await reader.cancel()
      return null
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

function json(status, body) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' },
  })
}

function refuse(status, error) {
  return json(status, { error })
}
