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
| `generate(script, seed, produce, max_generate, format, auto_level, round_robin, params, on_progress)` | JSON | `format` is `"oneline"`, `"printall"` or `"pbn"`; `params` fills `$0`-`$9` |
| `generate_from_deals(script, deals, seed, produce, max_generate, format, auto_level, round_robin, params, on_progress)` | JSON | The same, over deals the caller supplies: `deals` is a `Uint8Array` |
| `check_script(script, params)` | JSON | Never throws — safe to call per keystroke |
| `script_params(script)` | JSON | What the script says about its own `$0`-`$9` |
| `language_info()` | JSON | Full vocabulary for completion and hover |
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
  false, false, [], null))
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

The current build is **single-threaded**. Shared memory in wasm requires
`SharedArrayBuffer`, which requires `COOP`/`COEP` response headers:

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

**GitHub Pages cannot set custom headers, so a threaded build cannot be hosted
there.** Cloudflare Pages can, via a `_headers` file.

Deal generation is stateless per seed, so a threaded build produces *identical*
output — only faster. Any threaded build should feature-detect
`SharedArrayBuffer` and fall back to single-threaded rather than failing, so the
page still works where the headers are absent.

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

## Known gaps

- **PBN output is not exposed.** `format_printpbn` calls `chrono::Local::now()`
  for the `[Date]` tag, whose behaviour under `wasmbind` is unverified. Adding it
  should pass the date in from JS rather than reading a clock.
- **Double-dummy functions (`tricks`, `score`, `imps`) are untested in wasm.**
  They compile and link — `bridge-solver` already ships to browsers elsewhere —
  but no test exercises them through these bindings yet.
