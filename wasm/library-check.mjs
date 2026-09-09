// Drive `Library` from JavaScript, the way a page will.
//
//   cd wasm && ./build.sh nodejs && node library-check.mjs
//
// `cargo test` inside wasm/ already checks the arithmetic, on the host, in
// Rust. What it cannot check is the boundary: that `needs()` arrives as an
// array with a `.length`, that a `Uint8Array` handed to `supply()` reaches
// Rust as `&[u8]`, that `zrd()` comes back as bytes `generate_from_deals`
// accepts, and that the getters are getters. Those are wasm-bindgen's, they
// are not exercised by a host build, and they are where the wiring bugs
// actually happen.
//
// The library it serves is the ten-record fixture with the deals thrown away —
// which is exactly what a .zdd chunk is — cut into two chunks of five. So the
// run that comes back must be the fixture's own records, including across the
// join between the two chunks, and that is checked byte for byte.

import { createRequire } from 'module'
import { runEnvelope } from '../web/src/lib/envelope.js'
import { readFileSync } from 'fs'
import { dirname, join } from 'path'
import { fileURLToPath } from 'url'

const here = dirname(fileURLToPath(import.meta.url))
const w = createRequire(import.meta.url)('./pkg-node/dealer3_wasm.js')

const RECORD = 23, DEAL = 13, TABLE = 10, PER_CHUNK = 5
const fixture = new Uint8Array(
  readFileSync(join(here, '..', 'dealer-run', 'tests', 'fixtures', 'rpdd_10First.zrd')))
const MANIFEST = 'https://example.test/data/manifest.json'

let failures = 0
const check = (name, ok, detail = '') => {
  if (ok) console.log(`  ✓ ${name}${detail && ' ' + detail}`)
  else { console.error(`  ✗ ${name}: ${detail}`); failures++ }
}
const same = (a, b) => a.length === b.length && a.every((byte, i) => byte === b[i])

/** The table halves of five of the fixture's records: one .zdd chunk. */
const chunk = (n) => {
  const out = new Uint8Array(PER_CHUNK * TABLE)
  for (let i = 0; i < PER_CHUNK; i++) {
    const at = (n * PER_CHUNK + i) * RECORD
    out.set(fixture.subarray(at + DEAL, at + RECORD), i * TABLE)
  }
  return out
}

const manifest = new TextEncoder().encode(JSON.stringify({
  schema: 1, record_bytes: TABLE, deals_per_chunk: PER_CHUNK, total_deals: 10,
  chunks: [{ file: 'zdd/a.zdd', first_deal: 0, deals: PER_CHUNK },
           { file: 'zdd/b.zdd', first_deal: PER_CHUNK, deals: PER_CHUNK }],
}))
/** What a `fetch` would return, served from the fixture instead. */
const serve = (url) => {
  if (url === MANIFEST) return manifest
  if (url.endsWith('a.zdd')) return chunk(0)
  if (url.endsWith('b.zdd')) return chunk(1)
  throw new Error(`asked for a URL this library does not publish: ${url}`)
}

const lib = new w.Library(MANIFEST)
check('nothing is known before the manifest arrives', lib.total_deals === undefined)

// The loop a page runs, exactly as documented in docs/WASM.md.
const asked = []
let need
while ((need = lib.needs(3, 5)).length) {
  for (const url of need) { asked.push(url); lib.supply(url, serve(url)) }
}

check('the manifest is asked for first, then both chunks the run crosses',
  asked.length === 3 && asked[0] === MANIFEST
    && asked[1].endsWith('zdd/a.zdd') && asked[2].endsWith('zdd/b.zdd'),
  `(${asked.map((u) => u.split('/').pop()).join(', ')})`)
check('total_deals is known once the manifest is in', lib.total_deals === 10)
check('manifest_url is what it was pointed at', lib.manifest_url === MANIFEST)

const zrd = lib.zrd(3, 5)
check('a run across the chunk boundary is the library\'s own records, byte for byte',
  same(zrd, fixture.subarray(3 * RECORD, 8 * RECORD)))

// And the point of emitting .zrd: the existing run reads it unchanged.
const out = JSON.parse(w.run_json(
  runEnvelope('condition 1\naction printoneline, average "N HCP" hcp(north)\n',
    { seed: 1, produce: 40, maxGenerate: 1000000, format: 'oneline' }),
  zrd, undefined))
check('the bytes feed run_json unchanged',
  out.input?.format === 'zrd' && out.input?.read === 5 && out.produced === 5,
  JSON.stringify(out.input))
check('every deal arrived with its table, so tricks() is a lookup',
  out.input?.solved === 5 && out.input?.unsolved === 0)

while ((need = lib.needs(9, 2)).length) for (const url of need) lib.supply(url, serve(url))
check('a run past the end wraps to the beginning',
  same(lib.zrd(9, 2), new Uint8Array(
    [...fixture.subarray(9 * RECORD, 10 * RECORD), ...fixture.subarray(0, RECORD)])))

let refused = null
try { lib.supply('https://example.test/data/zdd/nope.zdd', chunk(0)) }
catch (e) { refused = String(e) }
check('bytes for a URL it never asked for are refused', refused !== null, refused ?? '')

console.log(failures ? `\n${failures} failure(s)` : '\nthe library serves what it should')
process.exit(failures ? 1 : 0)
