// Short links (#103), both ends at once.
//
// The page's half (`share.js`) is driven against the service's half
// (`shortLinks.js`) through a `fetch` that routes to the handlers the Pages
// Functions call, and an in-memory KV that records what was asked of it. So a
// link is made and opened exactly as it would be, with nothing but Cloudflare
// itself left out.

import { describe, it, expect } from 'vitest'
import { existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import {
  createShortLink,
  readShortLink,
  redirectShortLink,
  originAllowed,
  MAX_BODY_BYTES,
} from './shortLinks.js'
import { EXPIRED, KEY_LENGTH, SHORT_LINK_PATH, newKey, normalizeKey } from './shortKey.js'
import {
  encodeDocument,
  fetchShortLink,
  parseFragment,
  requestShortLink,
  resolveFragment,
  shortKeyFromQuery,
  shortLinkUrl,
} from './share.js'
import { makeDocument } from './envelope.js'

const SCRIPT = `# Somebody wants help with this one
condition hcp(north) >= 15 && shape(north, any 4333 + any 4432)
action printoneline
`

const doc = makeDocument(SCRIPT, {
  seed: 8391,
  produce: 20,
  maxGenerate: 1000000,
  format: 'oneline',
  dealSource: 'random',
})

const ORIGIN = 'https://bridge-craftwork.com'
const PAGE = `${ORIGIN}/dealer3/`
const THIRTY_DAYS = 30 * 24 * 60 * 60

/// Workers KV, as far as this service uses it, with a log of every call — the
/// writes are the scarce allowance, so what is written is worth asserting on.
function memoryKv({ failPuts = false } = {}) {
  const store = new Map()
  const log = []
  return {
    store,
    log,
    async get(key) {
      log.push(['get', key])
      return store.get(key)?.value ?? null
    },
    async getWithMetadata(key) {
      log.push(['getWithMetadata', key])
      const hit = store.get(key)
      return { value: hit?.value ?? null, metadata: hit?.metadata ?? null }
    },
    async put(key, value, options) {
      log.push(['put', key, options])
      if (failPuts) throw new Error('KV put() limit exceeded for the day.')
      store.set(key, { value, metadata: options?.metadata ?? null })
    },
  }
}

/// The Pages routes, as `fetch` — what `functions/` does, without Pages.
function service(kv, { origin = ORIGIN, key } = {}) {
  return async (url, init = {}) => {
    const { pathname } = new URL(url)
    const route = pathname.replace(/^\/dealer3/, '')
    if (route === '/api/short' && init.method === 'POST') {
      const request = new Request(url, { ...init, headers: { ...init.headers, origin } })
      return createShortLink(request, kv, key ? { key } : {})
    }
    const read = route.match(/^\/api\/short\/([^/]+)$/)
    if (read) return readShortLink(decodeURIComponent(read[1]), kv)
    return new Response('<!doctype html>not found', { status: 404 })
  }
}

const post = (body, headers = { origin: ORIGIN }) =>
  new Request(`${PAGE}api/short`, { method: 'POST', headers, body })

describe('a short link, made and opened', () => {
  it('opens the same script and settings it was made from', async () => {
    const kv = memoryKv()
    const fetch = service(kv)
    const { key } = await requestShortLink(doc, { base: PAGE, fetch })
    expect(key).toHaveLength(KEY_LENGTH)

    const opened = await resolveFragment(`#k=${key}`, {
      fetchShort: (k) => fetchShortLink(k, { base: PAGE, fetch }),
    })
    expect(opened.source).toBe('short')
    expect(opened.doc.script).toBe(SCRIPT)
    expect(opened.doc.settings.seed).toBe(8391)
  })

  it('stores exactly what a long link would carry', async () => {
    // The whole design: a short link is an alias for a `#d=` one.
    const kv = memoryKv()
    const { key } = await requestShortLink(doc, { base: PAGE, fetch: service(kv) })
    const stored = kv.store.get(key).value
    const opened = await resolveFragment(`#d=${stored}`)
    expect(opened.doc).toEqual((await resolveFragment(`#d=${await encodeDocument(doc)}`)).doc)
  })

  it('shares a script that does not parse, which may be why it is being sent', async () => {
    const broken = makeDocument('condition hcp(north >= ', { seed: 1, produce: 1, maxGenerate: 1, format: 'oneline' })
    const kv = memoryKv()
    const { key } = await requestShortLink(broken, { base: PAGE, fetch: service(kv) })
    const opened = await resolveFragment(`#k=${key}`, {
      fetchShort: (k) => fetchShortLink(k, { base: PAGE, fetch: service(kv) }),
    })
    expect(opened.doc.script).toBe('condition hcp(north >= ')
  })

  it('opens a key a phone has lower-cased', async () => {
    const kv = memoryKv()
    const fetch = service(kv)
    const { key } = await requestShortLink(doc, { base: PAGE, fetch })
    const opened = await resolveFragment(`#k=${key.toLowerCase()}`, {
      fetchShort: (k) => fetchShortLink(k, { base: PAGE, fetch }),
    })
    expect(opened.doc.script).toBe(SCRIPT)
  })
})

describe('thirty days, fixed', () => {
  it('expires thirty days after it was made, and says when', async () => {
    const kv = memoryKv()
    const now = Date.UTC(2026, 8, 10, 12)
    const response = await createShortLink(post(await encodeDocument(doc)), kv, { now: () => now })
    expect(response.status).toBe(201)
    const { key, expires } = await response.json()
    expect(expires).toBe('2026-10-10T12:00:00.000Z')

    const [, , options] = kv.log.find(([op]) => op === 'put')
    expect(options.expirationTtl).toBe(THIRTY_DAYS)

    const read = await (await readShortLink(key, kv)).json()
    expect(read.expires).toBe(expires)
  })

  it('writes nothing when a link is read', async () => {
    // Deliberately not a sliding expiry: re-putting on read would spend the
    // scarce write allowance on the plentiful operation.
    const kv = memoryKv()
    const { key } = await (await createShortLink(post(await encodeDocument(doc)), kv)).json()
    const writes = kv.log.filter(([op]) => op === 'put').length
    for (let i = 0; i < 5; i++) await readShortLink(key, kv)
    expect(kv.log.filter(([op]) => op === 'put').length).toBe(writes)
  })

  it('says an unknown key has expired, through the page', async () => {
    const fetch = service(memoryKv())
    await expect(
      resolveFragment('#k=ZZZZZZZZ', { fetchShort: (k) => fetchShortLink(k, { base: PAGE, fetch }) }),
    ).rejects.toThrow(EXPIRED)
  })

  it('says a key that is not a key has expired, rather than something went wrong', async () => {
    const response = await readShortLink('not-a-key!', memoryKv())
    expect(response.status).toBe(404)
    expect((await response.json()).error).toBe(EXPIRED)
  })
})

describe('what the service refuses', () => {
  it('refuses a write from another site', async () => {
    const kv = memoryKv()
    const response = await createShortLink(
      post(await encodeDocument(doc), { origin: 'https://example.com' }),
      kv,
    )
    expect(response.status).toBe(403)
    expect(kv.log.filter(([op]) => op === 'put')).toHaveLength(0)
  })

  it('refuses a write with no origin at all', async () => {
    const response = await createShortLink(post(await encodeDocument(doc), {}), memoryKv())
    expect(response.status).toBe(403)
  })

  it('takes the apex, pages.dev, its previews and a local wrangler', () => {
    expect(originAllowed('https://bridge-craftwork.com')).toBe(true)
    expect(originAllowed('https://dealer3.pages.dev')).toBe(true)
    expect(originAllowed('https://short-links-103.dealer3.pages.dev')).toBe(true)
    expect(originAllowed('http://localhost:8788')).toBe(true)
    expect(originAllowed('https://bridge-craftwork.com.evil.example')).toBe(false)
    expect(originAllowed('https://notdealer3.pages.dev')).toBe(false)
    expect(originAllowed('http://bridge-craftwork.com')).toBe(false)
  })

  it('refuses a body over the cap, before reading all of it', async () => {
    const kv = memoryKv()
    const response = await createShortLink(post('A'.repeat(MAX_BODY_BYTES + 1)), kv)
    expect(response.status).toBe(413)
    expect((await response.json()).error).toMatch(/long link/)
  })

  it('refuses what a page could not open', async () => {
    for (const body of ['', 'not base64!', 'AAAA', await encodeText('{"v":1}')]) {
      const kv = memoryKv()
      const response = await createShortLink(post(body), kv)
      expect(response.status, body).toBe(400)
      expect(kv.log.filter(([op]) => op === 'put')).toHaveLength(0)
    }
  })

  it('stores the document as this version reads it, not what else arrived', async () => {
    const kv = memoryKv()
    const payload = await encodeText(
      JSON.stringify({ ...doc, settings: { ...doc.settings, pickerOpen: true }, extra: 'x'.repeat(100) }),
    )
    const { key } = await (await createShortLink(post(payload), kv)).json()
    const opened = await resolveFragment(`#d=${kv.store.get(key).value}`)
    expect(opened.doc.settings).not.toHaveProperty('pickerOpen')
    expect(kv.store.get(key).value.length).toBeLessThan(payload.length)
  })

  it('says short links are used up when the write allowance is', async () => {
    const response = await createShortLink(post(await encodeDocument(doc)), memoryKv({ failPuts: true }))
    expect(response.status).toBe(503)
    expect((await response.json()).error).toMatch(/used up for today.*long link/s)
  })

  it('says a missing binding is a missing binding, not a busy day', async () => {
    const response = await createShortLink(post(await encodeDocument(doc)), undefined)
    expect(response.status).toBe(503)
    expect((await response.json()).error).toMatch(/not set up/)
    expect((await (await readShortLink('AB3K9M2P', undefined)).json()).error).toMatch(/not set up/)
  })

  it('reaches the page as a sentence', async () => {
    const fetch = service(memoryKv({ failPuts: true }))
    await expect(requestShortLink(doc, { base: PAGE, fetch })).rejects.toThrow(/used up for today/)
  })

  it('says so, from a copy of the page with no service behind it', async () => {
    const fetch = async () => new Response('<!doctype html>', { status: 405 })
    await expect(requestShortLink(doc, { base: PAGE, fetch })).rejects.toThrow(/not available/)
  })

  it('draws again on a collision, and never overwrites', async () => {
    const kv = memoryKv()
    const keys = ['AAAAAAAA', 'AAAAAAAA', 'BBBBBBBB']
    const key = () => keys.shift()
    const first = await (await createShortLink(post(await encodeDocument(doc)), kv, { key })).json()
    const second = await (await createShortLink(post(await encodeDocument(doc)), kv, { key })).json()
    expect([first.key, second.key]).toEqual(['AAAAAAAA', 'BBBBBBBB'])
  })
})

describe('keys', () => {
  it('are eight characters of Crockford base32', () => {
    for (let i = 0; i < 200; i++) expect(newKey()).toMatch(/^[0-9A-HJKMNP-TV-Z]{8}$/)
  })

  it('read the way Crockford says they should', () => {
    expect(normalizeKey('ab3k9m2p')).toBe('AB3K9M2P')
    expect(normalizeKey('AB3K-9M2P')).toBe('AB3K9M2P')
    expect(normalizeKey('O1IL0000')).toBe('01110000')
    expect(normalizeKey('AB3K9M2')).toBeNull()
    expect(normalizeKey('AB3K9M2U')).toBeNull()
    expect(normalizeKey('')).toBeNull()
  })

  it('reach the page from the query and back into the fragment', () => {
    expect(shortKeyFromQuery('?k=AB3K9M2P')).toBe('AB3K9M2P')
    expect(shortKeyFromQuery('?other=1')).toBeNull()
    expect(parseFragment('#k=ab3k9m2p')).toEqual({ kind: 'key', key: 'AB3K9M2P' })
  })
})

describe('the link people are sent', () => {
  it('says how long it lasts, and lives under the page on either host', () => {
    expect(shortLinkUrl({ origin: ORIGIN, pathname: '/dealer3/' }, 'AB3K9M2P')).toBe(
      'https://bridge-craftwork.com/dealer3/30-days/AB3K9M2P',
    )
    expect(shortLinkUrl({ origin: 'https://dealer3.pages.dev', pathname: '/' }, 'AB3K9M2P')).toBe(
      'https://dealer3.pages.dev/30-days/AB3K9M2P',
    )
    expect(shortLinkUrl({ origin: ORIGIN, pathname: '/dealer3/index.html' }, 'AB3K9M2P')).toBe(
      'https://bridge-craftwork.com/dealer3/30-days/AB3K9M2P',
    )
  })

  it('is short enough to text', () => {
    expect(`${ORIGIN}/dealer3/${SHORT_LINK_PATH}/AB3K9M2P`.length).toBeLessThanOrEqual(60)
  })

  it('is served by a Function at the path the page builds', () => {
    // The directory name and the constant are one promise, made twice.
    const route = new URL(`../../../functions/${SHORT_LINK_PATH}/[key].js`, import.meta.url)
    expect(existsSync(fileURLToPath(route))).toBe(true)
  })

  it('hands over to the page with the key in the query', () => {
    const response = redirectShortLink('ab3k9m2p')
    expect(response.status).toBe(302)
    expect(response.headers.get('location')).toBe('/?k=AB3K9M2P')
  })

  it('passes a broken key through, so the page can say it has expired', () => {
    expect(redirectShortLink('<script>').headers.get('location')).toBe('/?k=%3Cscript%3E')
  })
})

/// A document as `#d=` would carry it, from JSON text rather than an object,
/// for payloads `encodeDocument` would never make.
async function encodeText(json) {
  const stream = new Blob([json]).stream().pipeThrough(new CompressionStream('deflate-raw'))
  const bytes = new Uint8Array(await new Response(stream).arrayBuffer())
  return btoa(String.fromCharCode(...bytes)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}
