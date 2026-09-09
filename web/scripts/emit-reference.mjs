/**
 * Emits `public/reference.txt` — the language reference as plain text.
 *
 * Runs before `vite build` (see the `prebuild` script), so Vite copies the
 * result out of `public/` verbatim and it lands at the root of the deploy,
 * reachable at /reference.txt on this project's own domain and at
 * /dealer3/reference.txt through the site mount.
 *
 * It loads the SAME wasm engine the page loads and calls the SAME
 * `language_info()`, so the text and the page cannot drift into describing
 * different languages. Requires `npm run wasm` to have run first; the module
 * will not be there otherwise, and failing loudly here beats shipping a site
 * whose reference is silently a build old.
 */
import { readFile, writeFile } from 'node:fs/promises'
import { renderReferenceText, renderLlmsText, expectedNames } from '../src/lib/referenceText.js'

// The threaded build's glue pulls in wasm-bindgen-rayon's worker helper, and
// that helper registers a message listener on `self` as soon as it is imported.
// Node has neither `self` nor a global `addEventListener`, so importing the
// browser build here died with "self is not defined" — which this script then
// reported as "the wasm engine is not built", naming the wrong fault entirely.
// Stubs are enough: nothing here starts a thread pool, so the listener it
// registers is never used.
globalThis.self ??= globalThis
globalThis.addEventListener ??= () => {}

const WASM_JS = new URL('../src/wasm/dealer3_wasm.js', import.meta.url)
const WASM_BIN = new URL('../src/wasm/dealer3_wasm_bg.wasm', import.meta.url)
const OUT = new URL('../public/reference.txt', import.meta.url)
const LLMS = new URL('../public/llms.txt', import.meta.url)

let engine
try {
  engine = await import(WASM_JS.href)
} catch (e) {
  // With the reason. "Run npm run wasm" is right when the module is missing and
  // actively misleading when it is there and threw on the way in.
  console.error('Cannot load the wasm engine — run `npm run wasm` first if it is not built.')
  console.error(`  ${e?.message || e}`)
  process.exit(1)
}

// The `--target web` build expects to fetch its own binary; in Node we hand it
// the bytes directly.
await engine.default({ module_or_path: await readFile(WASM_BIN) })

const info = JSON.parse(engine.language_info())
const version = engine.version()
const text = renderReferenceText(info, version)

// A dropped section is the failure worth guarding against: the file would still
// look like a reference while missing a third of the language, and nothing
// downstream would notice. Cheap to check, so check.
const missing = expectedNames(info).filter((name) => !text.includes(name))
if (missing.length) {
  console.error(
    `reference.txt is missing ${missing.length} entr${missing.length === 1 ? 'y' : 'ies'} ` +
      `the engine declares: ${missing.slice(0, 12).join(', ')}${missing.length > 12 ? ' …' : ''}`,
  )
  process.exit(1)
}

// #2 asks for >20000 characters as the acceptance test for this being real
// content rather than a shell. Assert it here so the build catches it, not a
// curl three deploys later.
if (text.length < 20000) {
  console.error(`reference.txt is only ${text.length} characters — expected well over 20000.`)
  process.exit(1)
}

await writeFile(OUT, text, 'utf8')

// The index that points at it. A model handed the app URL gets 883 bytes of
// page shell and no route onward, because everything else is rendered by
// script it does not run; this and the `<link rel="alternate">` in index.html
// are the whole of the thread from one to the other. Emitted here so its
// figures describe the build that wrote them.
const llms = renderLlmsText(info, version, text.length)
if (!llms.includes('reference.txt')) {
  console.error('llms.txt does not link the reference, which is the only reason it exists.')
  process.exit(1)
}
await writeFile(LLMS, llms, 'utf8')

console.log(
  `reference.txt: ${text.length} characters, ` +
    `${expectedNames(info).length} entries, engine ${version}` +
    `\nllms.txt: ${llms.length} characters`,
)
