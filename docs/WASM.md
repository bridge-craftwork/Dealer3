# WebAssembly build

dealer3's engine compiles to WebAssembly, so scripts can be written and run in a
browser with no server. Same parser, same evaluator, same generator as the CLI.

## Building

```bash
cd wasm
./build.sh web       # ES module for the browser  -> wasm/pkg/
./build.sh nodejs    # CommonJS, used by tests    -> wasm/pkg-node/
./build.sh both
```

Requires [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/) and the
`wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`).

Current size: **~1400 KB raw, ~500 KB gzipped**, including `bridge-solver`,
which `tricks()` reaches through `dealer-dds`. The solver is about 11 KB
gzipped of that, and the readers behind `generate_from_deals` — ZRD, PBN and the
line-oriented layouts — about 22 KB gzipped: measured by building without that
entry point, which comes to 1351 KB raw and 478 KB gzipped.

## Why it works cleanly

Every browser-hostile construct — `std::fs`, `io::stdin`, `process::exit`,
`SystemTime::now()`, rayon — lives in the `dealer` binary. The library crates
(`dealer-core`, `dealer-parser`, `dealer-eval`, `dealer-pbn`) were already
portable, so the bindings are a thin wrapper rather than a port.

## Output matches the native binary

For the same seed and script, the wasm build produces byte-identical deals to
`target/release/dealer`. Verify:

```bash
cd wasm && ./build.sh nodejs
printf 'condition hcp(north) >= 12\n' > /tmp/v.dlr
../target/release/dealer /tmp/v.dlr -s 7 -p 5 -f oneline | sed 's/[[:space:]]*$//' > /tmp/native.txt
node -e '
  const w = require("./pkg-node/dealer3_wasm.js");
  console.log(JSON.parse(w.generate("condition hcp(north) >= 12\n",7,5,100000,"oneline")).deals.join("\n"));
' > /tmp/wasm.txt
diff /tmp/native.txt /tmp/wasm.txt && echo identical
```

This is why the Tier 2 regression hashes (`dealer/tests/regression_hash.rs`)
cover this build too: it is the same generator.

## API

| Export | Returns | Notes |
|---|---|---|
| `generate(script, seed, produce, max_generate, format, auto_level, round_robin, params, measure_seconds, on_progress)` | JSON | `format` is `"oneline"`, `"printall"` or `"pbn"`; `params` fills `$0`-`$9`; `measure_seconds` bounds levelling's characterizing pass |
| `generate_from_deals(script, deals, seed, produce, max_generate, format, auto_level, round_robin, params, measure_seconds, on_progress)` | JSON | The same, over deals the caller supplies: `deals` is a `Uint8Array` |
| `new Library(manifestUrl)` | object | The published solved-deal library, fetched a piece at a time; see below |
| `rpdd_manifest_url()` | string | The manifest of the library we host, for `new Library(...)` |
| `record_for_seed(seed, records)` | number | Which record a run's seed starts at — the CLI's own mapping, not a page's |
| `script_uses_double_dummy(script, params)` | bool \| undefined | Whether anything in the script can reach the solver |
| `check_script(script, params)` | JSON | Never throws — safe to call per keystroke |
| `script_params(script)` | JSON | What the script says about its own `$0`-`$9` |
| `language_info()` | JSON | Full vocabulary for completion and hover |
| `measure_budget_seconds()` | number | The default `measure_seconds`, so a page's field can show the engine's own number |
| `supports_threads()` | bool | Whether this build can deal on more than one thread at all |
| `start_threads(n)` | Promise | Threaded build only: start the pool. Needs a cross-origin isolated page |
| `version()` | string | Engine version |

### `generate`

```json
{
  "deals": ["n AKQ..."],
  "generated": 78,
  "produced": 3,
  "seconds": 0.011,
  "hit_limit": false,
  "averages": [ { "label": "Avg  ", "value": 33.333333333333336, "count": 3 } ],
  "frequencies": [
    {
      "label": "HCP South ",
      "min": 14, "max": 18,
      "bins": [ {"value":14,"count":0}, {"value":15,"count":1} ],
      "below": 0, "above": 0, "total": 3
    }
  ]
}
```

`generated`, `produced` and `seconds` correspond to the CLI's trailing stats
block. Scripts also declare `average` and `frequency` statements, and those
results come back as **data, not the CLI's ASCII table**, so a page can render a
real chart:

```
Frequency HCP South :        ->   bins: [{value:14,count:0},{value:15,count:1},…]
   14           0
   15           1
```

`below` and `above` are the CLI's `Low` and `High` rows — observations outside a
declared range. **Show them.** They are easy to omit when drawing a chart from
`bins` alone, and a script whose range is too narrow otherwise looks like it
simply produced fewer deals.

`bins` is contiguous and zero-filled across the range, so it can be plotted
directly without filling gaps.

Averages are returned at full `f64` precision rather than the CLI's `%g`
rounding, so the page chooses its own formatting. `count` is the number of deals
contributing, for showing "over N deals" or greying out a small sample.

`max_generate` bounds the work. A browser tab has no Ctrl-C, so a selective
filter must not be able to hang it. **Surface `hit_limit`** rather than silently
showing a short result — it distinguishes "no more matches exist" from "ran out
of budget".

`max_generate` bounds **the run that was asked for, and not the characterizing
pass that a levelled run makes first**. That pass is bounded by
`measure_seconds` — seconds, `undefined` for the engine's own default, and
`measure_budget_seconds()` says what that default is so a page need not keep a
second copy of it. Deals are the wrong currency for it: how many deals a sighting
of the rarest hand type costs is precisely what the pass exists to find out. The
command line spells the same limit `--level-timeout`.

The exception is `generate_from_deals`, where the deals are a finite pile rather
than a tap: there both passes share `max_generate` as the command line does, and
whichever of the two limits arrives first stops the measuring.

At most `MAX_RETURNED_DEALS` (500) deals are returned, since a script may ask for
tens of thousands to build a histogram and a page cannot show them all.
Statistics still accumulate over every matching deal, so `produced` can exceed
`deals.length`.

### `generate_from_deals`

Runs the script over deals the caller hands over, rather than dealing any. Same
arguments as `generate` with the file's bytes inserted second, and the same JSON
back with one field added.

**The browser is the HTTP client.** Nothing in the engine fetches, opens or
names a file — JS does that and passes the bytes:

```js
const bytes = new Uint8Array(await (await fetch("/library.zrd")).arrayBuffer())
const result = JSON.parse(w.generate_from_deals(
  "condition hcp(north) >= 15\n", bytes, 1, 40, 1000000, "oneline",
  false, false, [], null, null))
console.log(result.input)
// { format: "zrd", read: 10, solved: 10, unsolved: 0,
//   separators: 0, skipped: [], skipped_count: 0, notes: [] }
```

A dropped file or a file input works the same way: `new
Uint8Array(await file.arrayBuffer())`.

The format is decided by what the bytes *are*, not what they were called — a
Pavlicek `.zrd` library, PBN, or the one-line and printall layouts — through
`dealer-run`'s reader, which is the one `--input-deals` uses at the terminal.
There is deliberately no second decoder: a file read in a tab and the same file
read at a terminal cannot come to different conclusions about what is in it.

Records that carry double-dummy tables — a library's, or PBN's
`[DoubleDummyTricks]` and `[OptimumResultTable]` — bring them along, so a script
calling `tricks()` over a solved file solves nothing.

**Read `input`.** It is the only thing that says how much arrived:

| field | what it says |
|---|---|
| `format` | which reader handled the bytes: `"zrd"`, `"pbn"` or `"lines"` |
| `read` | deals the run was handed. Compare it against what you sent |
| `solved` | of those, how many arrived with a double-dummy table |
| `unsolved` | the rest, which are solved on demand |
| `separators` | section separators, which are not deals |
| `skipped` | records that could not be read, with reasons, at most 10 |
| `skipped_count` | how many were skipped altogether |
| `notes` | worth saying, but not a failure |

The command line prints all of this to stderr and a page has no stderr, which is
why it comes back with the results. Neither `produced` nor `hit_limit` can stand
in for it: a run that exhausts the deals it was given has not hit its budget, so
it stops short and looks exactly like success. A truncated download that read 40
deals of 4,000 shows up in `read` and nowhere else.

`seed` no longer decides which deals appear, since they are given, but it is
still what `rnd()` draws from and what orders an interleaved set.

`predeal` is refused rather than ignored — it arranges cards into deals this
program shuffles, and there is nothing for it to do to deals that arrived
already dealt. The command line refuses the same combination.

The bytes are decoded in full, so the caller decides how much to hand over: a
library is 23 bytes a record, and slicing the `Uint8Array` before passing it
reads a window of one. (Issue #65 covers an offset and limit in the engine
itself.)

### `Library`

Deals that arrive with their double-dummy tables, so `tricks()`, `dds()` and
`par()` are lookups rather than searches. There are 10,485,760 of them — Richard
Pavlicek's, solved over almost two years of computer time — but the file is
241 MB, which is not a download a tab can make to look at five deals.

So only the tables are published, as 640 KiB pieces, and the deals are not
published at all: they are a pure function of their index, and `rpdd-reader`
recreates them here at about 640ns each. `Library` joins the two and hands back
`.zrd` bytes for `generate_from_deals` — the same bytes, the same reader and the
same run as `--input-deals` at a terminal. There is deliberately no second run
path.

**The page does the fetching.** `fetch` is asynchronous and a wasm export cannot
await one, so the library says what it needs and is told:

```js
const lib = new Library(rpdd_manifest_url())

let need
while ((need = lib.needs(index, count)).length)
  for (const url of need)
    lib.supply(url, new Uint8Array(await (await fetch(url)).arrayBuffer()))

const result = JSON.parse(w.generate_from_deals(
  script, lib.zrd(index, count), seed, 40, 1000000, "oneline",
  false, false, [], null))
```

The first round asks for the manifest and the second for the chunks it names;
the caller fetches URLs and never learns which is which. Cache them however you
like — a `Map`, the Cache API, IndexedDB — and hand the same bytes back.

| Member | Returns | Notes |
|---|---|---|
| `needs(firstDeal, count)` | `string[]` | URLs still wanted. Empty means `zrd` will answer |
| `supply(url, bytes)` | — | Bytes for a URL `needs` returned. Throws for one it did not |
| `zrd(firstDeal, count)` | `Uint8Array` | The run, for `generate_from_deals`. Throws while anything is missing |
| `manifest_url` | string | What it was pointed at |
| `total_deals` | number \| undefined | Known once the manifest has been supplied |
| `forget_chunks()` | — | Release held pieces, keeping the manifest |

A page asks for **deals from index N** and never computes a piece number, an
offset or a wrap. Which piece holds a deal, how a run crossing a boundary is
stitched and how one past the end wraps to the beginning are all derived from
the manifest, inside `rpdd-reader`. `firstDeal` past the end of the library
wraps, so an index may be derived from a seed without knowing the size.

The layout is data, not code: `deals_per_chunk`, `record_bytes`, `total_deals`
and every chunk's path and first deal come from the manifest, and chunk paths
are resolved relative to it. Point `new Library(...)` at a different manifest of
the same shape and it works.

The browser app uses this behind its **Pre-solved deals** radio: `web/src/lib/library.js`
holds the page's half — fetching, caching and how much to ask for — and drives it
from inside the engine worker, since a `Library` is a handle into one wasm
instance's memory and cannot be passed to another thread.

### `record_for_seed`

```js
const first = record_for_seed(seed, lib.total_deals)   // then needs/zrd from there
```

Where a seed starts in a library of `records` records. **A page must not work
this out for itself.** The mapping is
`Xoshiro256PlusPlus::seed_from_u64(seed).next_u64() % records` — the same
`deal_input::Start::Seed` the command line reduces `-s` through, so `dealer -s 7
--input-deals rpdd.zrd` and a browser run with seed 7 read the same deals. A
JavaScript hash, however reasonable, lands somewhere else, and nothing fails:
the two front ends simply disagree, healthily, for ever. `wasm/verify.mjs`
compares a handful of seeds against the CLI's own note rather than trusting the
agreement.

`records` is the library being read, not a chunk: the seed names a deal in the
library, and the piece follows from it. A partial library answers differently,
which is correct — the seed names a position in the file it was given.

### `script_uses_double_dummy`

`true`, `false`, or `undefined` when the script does not parse — which is not
the same as `false`, and a page must not tell someone their script asks for
nothing when it could not read the script at all.

The question a page asks before offering to fetch a library: a script that never
calls `tricks()`, `dds()` or `par()` gains nothing from deals that arrive with
the answers. Answered by `dd_demand::touches_solver` off the parsed program, so
`t = tricks(north, notrump)` counts through the variable that reads it and a
comment mentioning `par` does not count at all.

### `check_script`

```json
{ "ok": false, "error": "Parse error:  --> 1:23\n...", "line": 1, "column": 23 }
```

Returns JSON rather than throwing, so an editor can call it on every keystroke.
Line and column come from the parser itself, so editor diagnostics agree with the
engine by construction — not by regex approximation.

### `script_params`

```json
{
  "ok": true,
  "error": null,
  "params": [
    {"index": 0, "default": "west", "description": "the seat that opens",
     "declared_on": 1, "used_on": 4},
    {"index": 1, "default": null, "description": null,
     "declared_on": null, "used_on": 4}
  ]
}
```

Every parameter the script uses, declares, or both — ordered by number. This is
what lets a page ask for the ones it needs: the `$n` occurrences alone give it
nowhere to put a label and no starting value, which is what a script's own
`# param 0 = west   # the seat that opens` line supplies.

`default` is `null` where nothing declares one, and that parameter must be
supplied or the run fails. `used_on` is `null` for a declaration the script never
mentions — harmless, but usually a `$7` lost to an edit, so worth showing.

Parameters reach `generate` and `check_script` as `--param`'s own `N=TEXT`
strings, so a value copied out of a browser field pastes straight into a
terminal. A parameter left out falls back to the script's declared default.
`ok` is false only for a malformed declaration line.

### `language_info`

Function names, keywords, actions, positions, vulnerabilities and operators, from
`dealer_parser::vocabulary`. That module is checked against `grammar.pest` by
`dealer-parser/tests/vocabulary_matches_grammar.rs`, so an editor built on this
cannot advertise a function the parser does not accept.

It also carries the **documentation** for that vocabulary — `function_docs`,
`operator_docs`, `statement_docs`, `action_docs`, `function_groups` and
`not_supported` — which is what `web/reference.html` renders:

```json
{
  "function_docs": [
    {
      "name": "cccc",
      "group": "Hand evaluation",
      "signature": "cccc(compass)",
      "summary": "Whole-hand evaluation by the algorithm published in …",
      "example": "cccc(north) >= 1200",
      "alias_of": null,
      "note": "Honours are valued by suit with penalties for …"
    }
  ],
  "operator_docs": [{ "symbol": "!", "word": "not", "precedence": 1, "…": "…" }],
  "not_supported": [{ "name": "notrumps", "instead": "In `tricks`, write notrump as the number 4." }]
}
```

`precedence` is 1 for the tightest binding; operators come back in that order,
which `vocabulary_docs.rs` enforces. `alias_of` marks a second spelling — `pt0`
for `tens`, `loser` for `losers` — so a page can show it without repeating the
description. Descriptions use backticks around code, the only markup they carry.

`dealer-parser` has no serde dependency, so these shapes are restated in
`wasm/src/lib.rs` and copied across field by field.

`tests/vocabulary_docs.rs` fails the build when a function in `FUNCTIONS` has no
entry — so a new function cannot reach the grammar without being documented —
and parses every example. `dealer-eval/tests/doc_examples_evaluate.rs` then
*runs* them, and pins `quality`, `cccc`, `losers`, `c13`, `controls` and `top5`
to the values dealer.exe produces for the same predealt deal.

## Syntax highlighting

`dealer-parser/syntaxes/dlr.tmLanguage.json` is a TextMate grammar generated from
the same vocabulary:

```bash
python3 scripts/generate-tmlanguage.py                      # dealer3 only
python3 scripts/generate-tmlanguage.py --also-update-vscode # + PBS extension
```

Two tests keep it honest — `vocabulary_matches_grammar.rs` (vocabulary vs the
PEG) and `tmlanguage_matches_vocabulary.rs` (grammar file vs vocabulary). Before
they existed the shipped grammar was missing 19 functions (`tens`, `jacks`,
`queens`, `kings`, `aces`, `top2`–`top5`, `pt0`–`pt9`) and the `csvrpt` keyword,
and highlighted two functions that do not exist (`control`, `imp`).

Monaco can load this file directly via `vscode-textmate` + `vscode-oniguruma`,
so the web editor and the VS Code extension share one definition.

## Threading

Two builds from one source:

| | `./build.sh web` | `./build.sh threaded` |
|---|---|---|
| Toolchain | stable | a pinned nightly with `rust-src` |
| Memory | ordinary | shared, `+atomics` |
| Exports | — | `start_threads(n)` |
| Used by | `npm run wasm`, local development | **the deployed site** |

**The site ships the threaded build.** `.github/workflows/pages.yml` runs
`npm run wasm:threaded` and then reads the binary it is about to deploy —
`npm run check:threaded`, which looks for shared memory and a `start_threads`
export — because a single-threaded bundle there would work, deal the same
deals, and use one core of however many. That is a failure nothing else can see.

Shared memory in wasm *is* `SharedArrayBuffer`, which a browser only provides to
a cross-origin isolated page:

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

`web/public/_headers` sends both. **GitHub Pages cannot set custom headers**,
which is why the site is on Cloudflare Pages.

### One build, not two

A threaded module carries shared memory whether or not the page it lands on has
`SharedArrayBuffer`, so the question was whether it would refuse to instantiate
where there is none — a page that fails to start, which is worse than a slow
one. It does not. On a page served *without* COOP/COEP, in Chromium 153 and
WebKit 26.6: `SharedArrayBuffer` is undefined, `crossOriginIsolated` is false,
`new WebAssembly.Memory({shared: true})` succeeds anyway, the module
instantiates, and the run produces the same deals on one thread.

So one bundle serves everyone. `engine.worker.js` asks for a pool only when
`crossOriginIsolated`, and carries on when it cannot start one. iOS and iPadOS
have had `SharedArrayBuffer` under COOP/COEP since Safari 15.2, and WebKit
scales much as Chromium does (2.87 M deals/s at one thread, 11.06 at eight).

### What threads buy, and what they cannot change

A thread count changes how long a run takes and nothing else: generation is
stateless per seed, so the deals, the statistics and the levelling are identical
at any count — verified against the single-threaded browser build and the Node
build, byte for byte.

Chromium 153 on an M4 Pro, `condition hcp(north) >= 20`, 16M deals, median of
three:

| threads | 1 | 2 | 4 | 6 | 8 | 12 |
|---|---|---|---|---|---|---|
| M deals/s | 2.83 | 4.73 | 8.05 | 10.14 | 11.80 | 11.12 |
| vs one | 1.00x | 1.67x | 2.85x | 3.58x | 4.17x | 3.93x |

The single-threaded build managed 2.73 in the same session, so the atomics cost
nothing at one thread.

### Why most runs still deal on one thread

That script is the best case and not a typical one. **wasm's allocator is a
single dlmalloc behind a single lock**, so anything a script allocates per deal
becomes a queue that every worker stands in. Same machine, same session, 400k
deals, 12 threads against 1:

| script | 1 thread | 12 threads | |
|---|---|---|---|
| `condition hcp(north) >= 20` | 2.5M/s | 6.4M/s | 2.55x |
| `… and controls(north) >= 4` | 2.8M/s | 10.8M/s | 3.82x |
| `… and shape(north, any 4333)` | 2.7M/s | 8.8M/s | 3.30x |
| `x = hcp(north)` then `condition x >= 20` | 2.1M/s | 0.5M/s | **0.26x** |
| `… and losers(north) <= 5` | 2.2M/s | 0.6M/s | **0.28x** |
| a real scenario (Jacoby 2NT) | 269k/s | 105k/s | **0.40x** |

The line is not the size of the script but whether evaluating a deal allocates:
a script with no variables never touches the per-deal cache, and one variable
assignment makes it a hash map per deal. Every scenario anyone actually runs has
variables.

What does pay is searching. `tricks()` over deals the run shuffled is about ten
milliseconds a deal, which dwarfs the contention: 321 deals/s on one thread, 522
on four, 537 on twelve — and that is the case this was asked for, a
hundred-thousand-deal double-dummy run using a twelfth of the machine.

So `threads_for` in `wasm/src/lib.rs` uses the pool for a script that reaches
the solver over deals it dealt itself, and one thread for everything else.
Supplied deals arrive already solved, so they fall on the ordinary side. This is
a wasm judgement and not the engine's — the command line has a real allocator
and threads everything. When per-deal allocation goes the way `Hand`'s did, that
function should become `threads_available()` and the measurements be redone.

`wasm/build.sh` carries the history of why threading used to be worse still.

## Verifying against the CLI

The bindings re-implement the CLI's generate loop — filter, predeal, averages,
frequencies — so they can drift from it. `wasm/verify.mjs` runs both over the
same scripts and seeds and diffs deals and statistics:

```bash
cd wasm && ./build.sh nodejs && node verify.mjs
```

This is not theoretical: it caught predeal being silently ignored in the wasm
path, which produced plausible-looking deals that simply did not honour the
script's `predeal` lines. Run it after changing either the bindings or the CLI's
generation loop.

It passes `--input-offset 0` to the CLI, which is what makes it a comparison:
without it the seed chooses where in a library to start (#65), and the two sides
then read the same records in different rotations.

`wasm/library-check.mjs` does the same job for `Library`, against the JS
boundary rather than the CLI:

```bash
cd wasm && ./build.sh nodejs && node library-check.mjs
```

`cargo test` inside `wasm/` already checks the arithmetic, in Rust, on the host.
What it cannot check is that `needs()` arrives as an array, that a `Uint8Array`
handed to `supply()` reaches Rust as `&[u8]`, and that `zrd()` comes back as
bytes `generate_from_deals` accepts. Those are `wasm-bindgen`'s, and they are
where the wiring bugs happen. It serves the ten-record fixture as two `.zdd`
chunks and checks the run byte for byte, across the join and around the wrap.

## Testing the bindings off a browser

`cargo test` inside `wasm/` runs the entry points on the host, not in a browser:
the only thing that stood in the way was the clock, and `now_ms` reads
`SystemTime` off wasm rather than `Date.now()`. What it reads has no effect on
which deals come out.

That is what covers `generate_from_deals` — it runs over
`dealer-run/tests/fixtures/rpdd_10First.zrd` and asserts the deals are the
file's ten, that the filter still applies to them, and what `input` says about
each format. It is not a browser, so it does not cover the JS boundary itself:
that a `Uint8Array` arrives as `&[u8]` is `wasm-bindgen`'s, and is exercised by
building and calling the package (`./build.sh nodejs`).

The same fixture covers `Library`, cut up the way the library is published: its
ten records become two chunks of five tables with the deals thrown away, served
through the ask/supply loop, and the run that comes back must be **the fixture's
own records byte for byte** — including one that crosses the boundary between
the two chunks and one that runs off the end and wraps. That is the check that
catches an off-by-one in a chunk's starting deal, which would otherwise pair
every deal with its neighbour's table: legal deals, well-formed tables, right
lengths, wrong answers. `rpdd-reader` runs the same comparison against the whole
241 MB library, which is not committed anywhere and which its tests skip when it
is absent.

## Known gaps

- **PBN output is not exposed.** `format_printpbn` calls `chrono::Local::now()`
  for the `[Date]` tag, whose behaviour under `wasmbind` is unverified. Adding it
  should pass the date in from JS rather than reading a clock.
- **Double-dummy functions (`tricks`, `score`, `imps`) are untested in wasm.**
  They compile and link — `bridge-solver` already ships to browsers elsewhere —
  but no test exercises them through these bindings yet.
