# dealer3 web

Write and run dealer scripts in the browser. The engine is dealer3 compiled to
WebAssembly, so nothing — script, deal or keystroke — leaves the machine.

## Running it

```bash
npm install
npm run dev        # builds the wasm, then starts Vite
```

`npm run build:all` produces a deployable `dist/`. `npm run build` skips the wasm
step, for when only the front end changed.

Requires [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/).

## Layout

```
src/
├── lib/
│   ├── engine.js         wasm loader and typed wrapper
│   ├── pbsScenarios.js   PBS manifest + script access (vendored)
│   └── dlrLanguage.js    CodeMirror language, built from the engine's vocabulary
└── components/
    ├── ScenarioPicker.vue  340+ PBS scenarios, grouped and searchable
    ├── ScriptEditor.vue    CodeMirror 6, diagnostics from the real parser
    └── ResultsPanel.vue    deals, averages, frequency charts
```

## Three things worth knowing

**Diagnostics come from the parser, not a regex.** `check_script()` is the same
pest parser the CLI uses, so a squiggle means the engine will reject the script,
and the line and column are the parser's own.

**Highlighting is derived, not duplicated.** The CodeMirror tokenizer is built at
runtime from `language_info()`, which comes from `dealer_parser::vocabulary` —
the same list checked against `grammar.pest` by two tests. Highlighting cannot
advertise a function the parser rejects, or miss one it accepts. That is the bug
that left 19 functions uncoloured in the VS Code extension for years.

**Scenarios are fetched, not bundled.** `pbsScenarios.js` reads the manifest that
Practice-Bidding-Scenarios CI builds, straight from raw.githubusercontent.com,
which serves permissive CORS. No backend, no build-time copy, and the list is
never stale. Vendored from `Bridge-Classroom/src/utils/pbsScenarios.js`.

## Deploying

Cloudflare Pages, configured in `../wrangler.jsonc`:

```bash
npm run build:all
npx wrangler pages deploy
```

Pages rather than GitHub Pages because it can send the COOP/COEP headers a
threaded wasm build will need. `public/_headers` already sets them.

## Editor choice

CodeMirror 6, not Monaco. Monaco was tried first, on the assumption that loading
`dlr.tmLanguage.json` directly was the way to share one definition with VS Code.
Once the tokenizer was derived from `language_info()` instead — a stronger
guarantee, since it comes from the parser rather than a parallel file — Monaco's
advantage disappeared and only its size remained:

| | gzipped |
|---|---|
| Monaco | 590 kB, plus 66 kB CSS and a 231 kB worker |
| CodeMirror 6 | **114 kB**, no separate CSS or worker |

Both drive off the same vocabulary, so the choice is presentation only.

## Tests

```bash
npm test
```

24 cases covering manifest parsing and the language derivation — tokenizer
classification, longest-first matching, case-insensitivity, and completion shape.
The Vue components are not covered by unit tests; they are exercised by the
browser smoke checks instead.
