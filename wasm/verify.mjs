// Verify the wasm bindings against the native CLI.
//
// The bindings re-implement the CLI's generate loop — filter, averages,
// frequencies — so they can drift from it. This runs both over the same scripts
// and seeds and compares deals and statistics.
//
//   cd wasm && ./build.sh nodejs && node verify.mjs
//
// Requires a release build of the CLI: ./dev-build.sh build --release

import { createRequire } from 'module'
import { execFileSync } from 'child_process'
import { writeFileSync, mkdtempSync } from 'fs'
import { tmpdir } from 'os'
import { join, dirname } from 'path'
import { fileURLToPath } from 'url'

const here = dirname(fileURLToPath(import.meta.url))
const w = createRequire(import.meta.url)('./pkg-node/dealer3_wasm.js')
const CLI = join(here, '..', 'target', 'release', 'dealer')
const tmp = mkdtempSync(join(tmpdir(), 'dealer3-verify-'))

const CASES = [
  { name: 'simple filter', seed: 1, produce: 5, script: 'condition hcp(north) >= 15\n' },
  { name: 'shape + variables', seed: 42, produce: 4,
    script: 'bal = shape(north, any 4333 + any 4432 + any 5332)\ncondition bal && hcp(north) >= 12\n' },
  { name: 'averages', seed: 7, produce: 6,
    script: 'condition hcp(north) >= 10\naction printoneline,\n  average "N HCP" hcp(north),\n  average "S HCP" hcp(south)\n' },
  { name: 'frequency with range', seed: 3, produce: 8,
    script: 'condition hcp(north) >= 8\naction printoneline,\n  frequency "NHCP" (hcp(north), 10, 16)\n' },
  { name: 'frequency out of range', seed: 11, produce: 10,
    script: 'condition hcp(north) >= 0\naction printoneline,\n  frequency "Narrow" (hcp(north), 11, 13)\n' },
  { name: 'predeal', seed: 5, produce: 4,
    script: 'predeal north SAKQ,HAK\ncondition hcp(south) >= 8\n' },
  // `printes`, which the CLI writes to a terminal and the bindings return as a
  // block. Checked with `-q`, which suppresses the CLI's deal output and leaves
  // only what the script printed itself.
  { name: 'printes', seed: 17, produce: 5, quiet: true,
    script: 'condition hcp(north) >= 11\naction printes("N=", hcp(north), " S=", hcp(south), \\n)\n' },
  // Double-dummy, which is the one function whose answer comes from outside
  // the evaluator. Two denominations and a frequency, so it also covers the
  // per-deal memo: the browser and the CLI each have to search once per
  // (deal, denomination, declarer) and reach the same numbers.
  { name: 'tricks in two denominations', seed: 13, produce: 6,
    script: 'condition hcp(north) >= 12\naction printoneline,\n'
      + '  average "NT" tricks(north, notrump),\n'
      + '  average "S" tricks(north, spades),\n'
      + '  frequency "NT" (tricks(north, notrump), 0, 13)\n' },
]

// The CLI prints "Label: value" then "Frequency Label:" tables then stats.
function parseCli(out) {
  const deals = [], averages = [], frequencies = []
  let generated = null, produced = null, current = null
  for (const raw of out.split('\n')) {
    const line = raw.trimEnd()
    if (line.startsWith('n ')) { deals.push(line); continue }
    let m
    if ((m = line.match(/^Generated (\d+) hands$/))) { generated = +m[1]; current = null; continue }
    if ((m = line.match(/^Produced (\d+) hands$/)))  { produced = +m[1];  current = null; continue }
    if ((m = line.match(/^Frequency (.*):$/)))       { current = { label: m[1], bins: [], below: 0, above: 0 }; frequencies.push(current); continue }
    if (current) {
      if ((m = line.match(/^Low\s+(\d+)$/)))  { current.below = +m[1]; continue }
      if ((m = line.match(/^High\s+(\d+)$/))) { current.above = +m[1]; continue }
      if ((m = line.match(/^\s*(-?\d+)\s+(\d+)$/))) { current.bins.push({ value: +m[1], count: +m[2] }); continue }
    }
    if ((m = line.match(/^(.*?): (-?[\d.e+-]+)$/)) && !line.startsWith('Time')) {
      averages.push({ label: m[1], value: parseFloat(m[2]) }); continue
    }
  }
  return { deals, generated, produced, averages, frequencies }
}

let failures = 0
const fail = (c, msg) => { failures++; console.log(`  ✗ ${c}: ${msg}`) }

for (const c of CASES) {
  const path = join(tmp, `${c.name.replace(/\W+/g, '_')}.dlr`)
  writeFileSync(path, c.script)

  // A `printes` case is compared on what the script printed, not on deals: the
  // CLI interleaves the two on stdout, so `-q` is what separates them.
  if (c.quiet) {
    const printed = execFileSync(CLI,
      [path, '-s', String(c.seed), '-p', String(c.produce), '-q'],
      { encoding: 'utf8' })
    const fromWasm = JSON.parse(w.generate(c.script, c.seed, c.produce, 500000, 'oneline', false, false)).printes
    if (printed !== fromWasm) {
      fail(c.name, `printes differs\n      cli:  ${JSON.stringify(printed)}\n      wasm: ${JSON.stringify(fromWasm)}`)
    } else {
      console.log(`  \u2713 ${c.name} (${printed.split('\n').length - 1} lines)`)
    }
    continue
  }

  const cli = parseCli(execFileSync(CLI,
    [path, '-s', String(c.seed), '-p', String(c.produce), '-f', 'oneline', '-X'],
    { encoding: 'utf8' }))
  const wasm = JSON.parse(w.generate(c.script, c.seed, c.produce, 500000, 'oneline', false, false))

  if (JSON.stringify(cli.deals) !== JSON.stringify(wasm.deals)) {
    fail(c.name, `deals differ (cli ${cli.deals.length}, wasm ${wasm.deals.length})`)
    continue
  }
  if (cli.generated !== wasm.generated) fail(c.name, `generated ${cli.generated} vs ${wasm.generated}`)
  if (cli.produced !== wasm.produced)   fail(c.name, `produced ${cli.produced} vs ${wasm.produced}`)

  if (cli.averages.length !== wasm.averages.length) {
    fail(c.name, `average count ${cli.averages.length} vs ${wasm.averages.length}`)
  } else {
    cli.averages.forEach((a, i) => {
      // The CLI prints %g (6 significant digits); wasm returns full f64.
      if (Math.abs(a.value - wasm.averages[i].value) > 1e-4) {
        fail(c.name, `average "${a.label}" ${a.value} vs ${wasm.averages[i].value}`)
      }
    })
  }

  if (cli.frequencies.length !== wasm.frequencies.length) {
    fail(c.name, `frequency count ${cli.frequencies.length} vs ${wasm.frequencies.length}`)
  } else {
    cli.frequencies.forEach((f, i) => {
      const g = wasm.frequencies[i]
      if (JSON.stringify(f.bins) !== JSON.stringify(g.bins)) {
        fail(c.name, `frequency "${f.label}" bins differ\n      cli:  ${JSON.stringify(f.bins)}\n      wasm: ${JSON.stringify(g.bins)}`)
      }
      if (f.below !== g.below) fail(c.name, `frequency "${f.label}" below ${f.below} vs ${g.below}`)
      if (f.above !== g.above) fail(c.name, `frequency "${f.label}" above ${f.above} vs ${g.above}`)
    })
  }

  if (!failures) console.log(`  ✓ ${c.name} (generated=${wasm.generated} produced=${wasm.produced})`)
}

console.log(failures
  ? `\n${failures} mismatch(es) between wasm and the native CLI`
  : `\nall ${CASES.length} cases match the native CLI`)
process.exit(failures ? 1 : 0)
