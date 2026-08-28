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
│   ├── reference.js      shaping the vocabulary into reference sections
│   ├── guide.js          rendering a docs/ markdown file as a page
│   └── download.js       saving results as PBN or text
├── Reference.vue         the language reference page
├── Leveling.vue          the levelling guide, rendered from docs/
└── components/
    ├── ScenarioPicker.vue  340+ PBS scenarios, grouped and searchable
    ├── ScriptEditor.vue    CodeMirror 6, diagnostics from the real parser
    ├── DealGrid.vue        deals as bridge hands, with HCP
    ├── RichText.vue        backticked spans in descriptions, as code
    └── ResultsPanel.vue    deals, averages, frequency charts
```

Three pages, all Vite entries: `index.html` (the app), `reference.html` (the
language reference) and `leveling.html` (the levelling guide). The app and the
reference share the engine chunk, and the reference pulls in none of the editor.
The guide loads neither: its text is inlined at build time.

## Five things worth knowing

**Diagnostics come from the parser, not a regex.** `check_script()` is the same
pest parser the CLI uses, so a squiggle means the engine will reject the script,
and the line and column are the parser's own.

**Highlighting is derived, not duplicated.** The CodeMirror tokenizer is built at
runtime from `language_info()`, which comes from `dealer_parser::vocabulary` —
the same list checked against `grammar.pest` by two tests. Highlighting cannot
advertise a function the parser rejects, or miss one it accepts. That is the bug
that left 19 functions uncoloured in the VS Code extension for years.

**The language reference is generated, not written.** `reference.html` renders
every function, operator and statement from that same `language_info()`, so it
cannot list something the parser rejects or leave out something it accepts. The
descriptions live in `dealer-parser/src/vocabulary.rs` beside the word lists,
where `tests/vocabulary_docs.rs` holds them to it: **adding a function to the
grammar fails the build until it is documented**. Every example on the page is
parsed by that test and *evaluated* by `dealer-eval`'s, so no snippet in the
reference is one the engine would reject.

**The levelling guide has one source, and it is not here.** `leveling.html`
renders `docs/leveling-guide.md` — the same file GitHub shows and
`.github/workflows/docs-pdf.yml` builds the PDF from — inlined at build time by
Vite's `?raw`. A second copy written for the web would be a second thing to keep
right, and the guide is mostly measurements. `lib/guide.js` does the rendering
and rewrites the document's repo-relative links back to GitHub, since
`../examples/` means nothing here; `guide.test.js` checks every link in the
built page resolves and every contents entry lands on a heading that exists.

Because the markdown is a build input, `pages.yml` redeploys on a change to it.
Without that the site would go stale while the repo looked current.

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

**Save PDF** opens the browser's print dialog, from which "Save as PDF" produces
a document with the script in colour, the statistics, the first 12 boards and a
link back to the site.

No PDF library is involved, and none should be: the browser's own print pipeline
keeps the text **selectable** and the link **live**, which is the whole point —
the script is meant to be copied out of the PDF and pasted back in. The usual
HTML-to-PDF libraries rasterise via canvas, which would render the script as an
image and defeat that, at a cost of 100 kB+ gzipped. This costs ~2 kB.

`src/print.css` hides the app and reveals `PrintView.vue`, a second document
built for paper. It is a separate component rather than print rules over the
live UI because what belongs on paper is genuinely different: no picker, no
controls, the script as a static listing, and only the first few boards.

**Save PBN** and **Save text** download the results. Selecting text out of the
page is clumsy (a select-all takes the whole document), and a long run is
thousands of lines. Saving re-runs the generator rather than reformatting what is
on screen: the displayed deals are capped at 500, and PBN needs the engine's own
formatter. Generation is deterministic for a given seed, so the saved file
matches what was shown.

PBN output carries the script's `dealer` and `vulnerable` settings in its tags.

## Seed

The seed defaults to a **random** value, matching the CLI where `-s` defaults to
the clock. Most of the time the question is "show me hands like this", not "show
me these exact hands", and a fixed default quietly answers the second.

**The seed on screen is always the seed that produced what is shown**, or
reproducing a result becomes guesswork. That is the guarantee, and the
**Random** checkbox beside the field keeps it while saving the click: when it is
ticked, Run rolls a new seed *before* the run and writes it into the field, so
what is on screen still describes what is on screen. Unticked, Run reuses the
seed as it stands and the same deals come back.

Rolling before rather than after is what made a separate "new seed" button
unnecessary — there is no other reason to want one. A restored session keeps its
seed, so a reload reproduces what was there.

## Session persistence

The script in the editor and the parameters beside it are kept in
`localStorage`, so a return visit picks up where the last one left off. The
starter script only appears on a genuinely first visit.

Deliberately **one** session, not a library: a history would need naming,
listing and deleting — a feature in its own right — and the common case is
simply coming back to what you had open. Results are not stored: they can run to
megabytes, they regenerate from the script and seed in milliseconds, and a
stored result could silently disagree with the script shown beside it.

Every access is guarded. `localStorage` throws outright in some privacy modes
rather than returning null, and a corrupt value means "start fresh" rather than
a page that fails to load.

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

Cases covering manifest parsing, the language derivation — tokenizer
classification, longest-first matching, case-insensitivity, completion shape —
the reference page's transforms of the vocabulary, and the levelling guide's
markdown rendering.

The Vue components are not covered by unit tests; they are exercised by the
browser smoke checks instead. That division is deliberate: every real bug in
this app so far has been a wiring or ordering fault that unit tests could not
see. The reference page was no exception — its first build rendered zero entries
because `npm run build` skips the wasm step, so the page ran against an engine
that predated the docs. **Build with `build:all` before checking anything in a
browser.**
