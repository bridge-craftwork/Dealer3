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
import { renderReferenceText, expectedNames } from '../src/lib/referenceText.js'

const WASM_JS = new URL('../src/wasm/dealer3_wasm.js', import.meta.url)
const WASM_BIN = new URL('../src/wasm/dealer3_wasm_bg.wasm', import.meta.url)
const OUT = new URL('../public/reference.txt', import.meta.url)

let engine
try {
  engine = await import(WASM_JS.href)
} catch {
  console.error('The wasm engine is not built. Run `npm run wasm` first.')
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
console.log(
  `reference.txt: ${text.length} characters, ` +
    `${expectedNames(info).length} entries, engine ${version}`,
)
