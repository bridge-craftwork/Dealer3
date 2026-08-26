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

Current size: **~1015 KB raw, ~351 KB gzipped**, including the double-dummy
solver pulled in transitively via `dealer-dds`.

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
| `generate(script, seed, produce, max_generate, format)` | JSON | `format` is `"oneline"` or `"printall"` |
| `check_script(script)` | JSON | Never throws — safe to call per keystroke |
| `language_info()` | JSON | Full vocabulary for completion and hover |
| `version()` | string | Engine version |

### `generate`

```json
{ "deals": ["n AKQ..."], "generated": 156, "produced": 20, "hit_limit": false }
```

`max_generate` bounds the work. A browser tab has no Ctrl-C, so a selective
filter must not be able to hang it. **Surface `hit_limit`** rather than silently
showing a short result — it distinguishes "no more matches exist" from "ran out
of budget".

### `check_script`

```json
{ "ok": false, "error": "Parse error:  --> 1:23\n...", "line": 1, "column": 23 }
```

Returns JSON rather than throwing, so an editor can call it on every keystroke.
Line and column come from the parser itself, so editor diagnostics agree with the
engine by construction — not by regex approximation.

### `language_info`

Function names, keywords, actions, positions, vulnerabilities and operators, from
`dealer_parser::vocabulary`. That module is checked against `grammar.pest` by
`dealer-parser/tests/vocabulary_matches_grammar.rs`, so an editor built on this
cannot advertise a function the parser does not accept.

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

## Known gaps

- **PBN output is not exposed.** `format_printpbn` calls `chrono::Local::now()`
  for the `[Date]` tag, whose behaviour under `wasmbind` is unverified. Adding it
  should pass the date in from JS rather than reading a clock.
- **Double-dummy functions (`tricks`, `score`, `imps`) are untested in wasm.**
  They compile and link — `bridge-solver` already ships to browsers elsewhere —
  but no test exercises them through these bindings yet.
