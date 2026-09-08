/**
 * Fails unless the wasm in a directory is the threaded build.
 *
 *   node scripts/check-threaded.mjs dist          # what is about to be deployed
 *   node scripts/check-threaded.mjs src/wasm      # what was just built
 *
 * The deploy runs `npm run wasm:threaded`, and if that ever silently becomes
 * the single-threaded build again — a reverted script, a workflow edit, a
 * cached artefact — the site would still work. It would simply use one core of
 * twelve, and look like a slow scenario. Nothing else in the pipeline can tell
 * the difference, so this reads the binary and says which build it is.
 *
 * See `src/lib/wasmThreads.js` for what it reads and why those two signals.
 */
import { readFile, readdir } from 'node:fs/promises'
import { join } from 'node:path'
import { inspectWasm } from '../src/lib/wasmThreads.js'

/// Every `.wasm` under `dir`, one level down as well: a build puts it in
/// `assets/`, and `src/wasm` holds it at the top.
async function wasmFiles(dir) {
  const found = []
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) found.push(...(await wasmFiles(path)))
    else if (entry.name.endsWith('.wasm')) found.push(path)
  }
  return found
}

const dir = process.argv[2]
if (!dir) {
  console.error('usage: check-threaded.mjs <directory>')
  process.exit(2)
}

let files
try {
  files = await wasmFiles(dir)
} catch (e) {
  console.error(`Cannot read ${dir}: ${e.message}`)
  process.exit(2)
}

// No wasm at all is a failure too: an empty directory would otherwise pass a
// check whose whole job is to say what shipped.
if (files.length === 0) {
  console.error(`No .wasm found under ${dir}, so there is nothing to ship or to check.`)
  process.exit(1)
}

let bad = 0
for (const file of files) {
  const report = inspectWasm(new Uint8Array(await readFile(file)))
  if (report.threaded) {
    console.log(`✓ ${file}: threaded (shared memory, start_threads)`)
  } else {
    bad++
    console.error(`✗ ${file}: NOT the threaded build — ${report.reasons.join('; ')}`)
  }
}

if (bad) {
  console.error('')
  console.error('The site is served cross-origin isolated so the engine can deal on several')
  console.error('threads. A single-threaded bundle here would work and be about four times')
  console.error('slower, with nothing on the page to say so. Build it with:')
  console.error('    npm run wasm:threaded')
  process.exit(1)
}
