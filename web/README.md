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
│   ├── cardFormatting.js card/suit primitives and oneline parsing (vendored)
│   ├── dlrLanguage.js    CodeMirror language, built from the engine's vocabulary
│   └── download.js       saving results as PBN or text
└── components/
    ├── ScenarioPicker.vue  340+ PBS scenarios, grouped and searchable
    ├── ScriptEditor.vue    CodeMirror 6, diagnostics from the real parser
    ├── DealGrid.vue        deals as bridge hands, with HCP
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

Cloudflare Pages, not GitHub Pages: only Cloudflare can send the COOP/COEP
headers a threaded wasm build will need, and `public/_headers` already sets
them. Hosting on both would have meant a second copy that quietly diverged the
moment threading landed.

`.github/workflows/pages.yml` redeploys on every push to `main` that touches the
engine or the site. It fails loudly when the Cloudflare credential is missing
rather than going green having deployed nothing.

Live at **https://dealer.bridge-classroom.org** (also `dealer3.pages.dev`).

## Viewing and saving

Deals show as **hands** by default — a compass grid with per-hand HCP and
partnership totals — because a one-line string is far harder to read than a
layout. Toggle to **Text** for the raw output.

The grid only applies to the one-line format: `printall` is already a visual
layout and PBN is a record format, so those offer Text instead rather than an
empty grid.

`cardFormatting.js` is vendored from `Bridge-Classroom/src/utils/cardFormatting.js`.
Its `HandDisplay.vue` was **not**: at 521 lines it is built for an interactive
table — clickable cards, a selector popup, per-card marks, dynamic fit — and
almost none of that applies to a static grid. The primitives were the reusable
part.

Averages and frequencies both render as bars. Averages share one scale set by
the largest value shown, so they compare against each other rather than each
filling its own row. A negative average gets no bar rather than a misleading
one — these are arbitrary script expressions and `100 * (x - y)` can legitimately
go below zero — but the number is always shown.

**Save PBN** and **Save text** download the results. Selecting text out of the
page is clumsy (a select-all takes the whole document), and a long run is
thousands of lines. Saving re-runs the generator rather than reformatting what is
on screen: the displayed deals are capped at 500, and PBN needs the engine's own
formatter. Generation is deterministic for a given seed, so the saved file
matches what was shown.

PBN output carries the script's `dealer` and `vulnerable` settings in its tags.

## Editor appearance

The editor is dark (One Dark) on an otherwise light page. Syntax palettes are
designed for dark grounds, and the same colours that read clearly there are
washed out on white — which is how CodeMirror's default highlight style looked
here. The editor's border and status strip are matched to it (`--editor-bg`,
`--editor-line`) so it reads as one component rather than a hole in the page.

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
