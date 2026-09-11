# dealer3 web

Write and run dealer scripts in the browser. The engine is dealer3 compiled to
WebAssembly, so nothing — script, deal or keystroke — leaves the machine. The
one exception is a short link, which is made only when asked for and says so;
see [Short links](#short-links).

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
│   ├── engine.worker.js  generation, off the main thread
│   ├── library.js        the solved-deal library: fetching, caching, wording
│   ├── envelope.js       the run the engine takes, and the document a link carries
│   ├── share.js          a document to a URL fragment and back
│   ├── shortKey.js       what a short link's key looks like, for both ends
│   ├── shortLinks.js     the short-link service, behind ../functions/
│   ├── clipboard.js      copying text, with the fallback the platforms need
│   └── download.js       saving results as PBN or text
├── Reference.vue         the language reference page
├── Leveling.vue          the levelling guide, rendered from docs/
└── components/
    ├── ScenarioPicker.vue  340+ PBS scenarios, grouped and searchable
    ├── ScriptEditor.vue    CodeMirror 6, diagnostics from the real parser
    ├── DealGrid.vue        deals as bridge hands, with HCP
    ├── RichText.vue        backticked spans in descriptions, as code
    ├── CopyButton.vue      copy a pane's script, inset top-right
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

The names the levelling machinery reads — `HandType_`, `LevelType_`, `_Share`,
`levelTheDeal`, the generated block's markers and stamp — come the same way,
from `dealer-level`'s own constants, which is why the editor can colour them
without a second copy of the convention living in JavaScript. The one hand-kept
list is the `# key: value` headers, because those belong to PBS and nothing in
the engine reads them; a key on the list is coloured and anything else stays an
ordinary comment, so a mistyped header simply does not light up.

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

## Why generation runs in a worker

`engine.worker.js` owns the wasm for `generate`; everything else calls the
main-thread instance.

Generation is one synchronous call that can run for many seconds, and on the
main thread that blocks everything. The Run button never painted its disabled
state — `requestAnimationFrame` fires *before* paint, so yielding a frame only
let the browser reach the blocking call sooner — and a click during the freeze
was queued by the browser and delivered the moment the tab thawed, starting a
second run. Neither is fixable from outside the block.

So the engine reports progress through a callback the worker forwards as
messages, and **Cancel is `terminate()`**: the one form of cancellation that
works against code already inside the wasm, since a flag would need the blocked
thread to come back and read it. The worker is recreated on the next run.

Only `generate` moved. `check_script` and `language_info` are called
synchronously while the editor is being built and are far too fast to be worth
an await, so they stay where they were.

Progress is reported per phase, because a levelled run deals the scenario up to
three times and one bar would appear to finish and start over. The measuring
bar has no total until the probe finishes — how much measuring a scenario needs
depends on how rare its rarest hand type is, and that is what the probe is for —
so it runs indeterminate until then rather than inventing a denominator. The
bars and Cancel are held back for a second, so the common short run does not
flash them up and down.

## Copying a script out

Both script panes carry a **Copy** button, inset in the top-right. It is there
because selecting the text by hand does not work: CodeMirror draws only the
lines currently on screen, so Ctrl-A takes the whole page and dragging takes
only what has been rendered. The generated levelled scenario is the one most
worth copying — it is what gets pasted into BBO — and it is read-only, so there
is no caret to select from either.

`CopyButton.vue` reads the document from the editor at click time rather than
holding its own copy, and falls back to a hidden `<textarea>` when
`navigator.clipboard` is missing or refused, which it is on an insecure origin.
A failure says so rather than flashing "Copied". The clipboard itself lives in
`clipboard.js`, shared with the Share button below.

## Sharing a script by link

**Share**, beside the settings gear, copies a link that opens the script *and*
its settings on somebody else's machine. The alternative was sending a PDF,
which the recipient cannot run: they have to copy the text back out, and none of
the settings travel with it.

The payload is in the **URL fragment**, and never in the query string. Nothing
after the `#` is sent with the request, so a link reaches no server log, no cache
key and no edge rule — which is what makes this shippable with no back end, and
why it costs nothing to keep working. It is also the form that survives the trip:
a real BBO capture sent as a query string came back **403 at the edge** on
2026-08-28 while the identical payload in the fragment loaded. bridge-solver
already hands hands over this way; the cross-tool contract
(`bridge-craftwork-site#3`, item 4) settles it for every tool here.

Three forms, cheapest first, all of which open the same way:

| Fragment | Means |
|---|---|
| `#s=<scenario>` | a scenario from the list, unmodified, plus what was changed |
| `#d=<base64url>` | the document itself, deflated — no network at all |
| `#k=<key>` | a short link's key, fetched from the service below (#103) |

Sharing an untouched scenario needs no encoding at all: the recipient's page
fetches the same script from the same list, so the link is
`#s=Sup_X_By_Advancer&seed=8391` and stays readable in an email. Change one
character of it and the whole script travels instead — otherwise the recipient
would open a *different* script under the right name, which is worse than a long
link. That form is `JSON.stringify` → `CompressionStream('deflate-raw')` →
base64url, native in Safari 16.4, Chrome and Firefox, so no library: 600–900
bytes of script comes out around 350–550 characters.

### Short links

A `#d=` link is fine in an email and too long for a text message. For that one
case the share panel offers **Short link**, which stores the script and gives
back `bridge-craftwork.com/dealer3/30-days/AB3K9M2P`. It is the only part of the
page with a back end: Pages Functions in the repo's `functions/`, thin wrappers
around `lib/shortLinks.js`, holding links in a KV namespace bound in
`wrangler.jsonc`.

- **What is stored is the long link.** The value under a key is the `#d=`
  payload, so a short link is an alias for a long one and opens by the same
  `decodeDocument` and `readDocument`. The service runs both on the way in and
  refuses what a page could not open. It does not run the parser: someone
  asking for help with a script that will not parse is a good reason to send
  one.
- **Thirty days, fixed, and the link says so.** `expirationTtl` on the put, so
  Cloudflare deletes it. Not refreshed on read — a sliding expiry would be
  invisible to the person holding the link, and every refresh would spend a
  write. Fixed means the page can name the day: the share panel does, and so
  does the notice a recipient sees on opening one.
- **Offered, never automatic.** Every short link spends one of the free tier's
  daily writes, so the button appears only when the long link is over 100
  characters, and a plain Share still uploads nothing.
- **Keys** are eight characters of Crockford base32, read leniently (case, `O`
  for zero, `I` and `L` for one), so a phone's autocorrect does not break one.
- **Abuse control** is an 8 KB cap on the compressed body, the document having
  to open, the `Origin` header, and the daily write allowance, which fails
  closed: a flood costs short links for the rest of the day and nothing else.
  There is no per-IP limit, because Pages Functions cannot take the rate-limit
  binding and a KV counter would spend a write per share. A zone rate-limiting
  rule is where one would go.

`/30-days/<key>` redirects to `/?k=<key>` rather than `/#k=<key>`, because the
apex proxy does not carry a fragment through a redirect. The page moves the key
into the fragment on arrival, so there is still one place links are read from.

To run the Functions locally, build the site and then, from the repo root:
`npx wrangler pages dev --kv SHORT_LINKS`. `pages dev` does not pick the
binding up from `wrangler.jsonc`, so without `--kv` every short link is refused
as not set up.

### What a link carries, and what it does not

The engine's envelope (`run_json`) is one half of a run: script, seed, produce,
max generate, format, auto-level, round robin, params, measure budget. It
refuses fields it does not know, deliberately. A **document** is that plus the
two settings the caller acts on and the engine never sees:

- **`dealSource`** — random or the pre-solved library. The engine infers the
  source from whether deals were handed to it, so being *told* "library" would
  be a field it could not act on. The recipient's page needs it to know to fetch
  before calling.
- **`newSeedEachRun`** — the engine takes a *definite* seed, so "roll a new one"
  is resolved before the call. A demo link probably wants it on; a link showing a
  particular hand definitely wants it off.

`scenario` travels with them, so the list opens pointing at the right entry.
What must **not** travel is UI state — whether the picker or the settings panel
was open is about the sender's window.

Both shapes live in `envelope.js`, and narrowing is not a function: `runEnvelope`
names the engine's fields, so passing it a document's settings drops the caller's
by not asking for them. There is deliberately no second list to keep in step. A
document cannot be handed to `run_json` whole — that is refused, by design.

One conversion is easy to lose: the page keeps `paramValues` by parameter number,
the envelope takes `params` as `N=TEXT`, and a link carries the engine's
spelling. `paramValuesFrom` projects it back. Miss it and a shared parameterised
scenario arrives with blank fields, running on the script's declared defaults —
looking exactly as though it had worked.

### Opening one

A link **loads and stops**. Nothing runs: an unknown script can be expensive (a
double-dummy condition on shuffled deals takes minutes), the recipient should see
what they are about to run, and a page that starts work on open makes the back
button surprising.

The fragment is untrusted input, so it parses or it is refused with a sentence
worth reading — a truncated link says it was truncated, an unknown version says
so by number rather than half-loading, and a setting that means nothing falls
back to its default rather than throwing the script away with it. Inflation is
bounded: a few hundred characters of deflate can become hundreds of megabytes,
and a tab that hangs before anything is on screen is the one failure with no way
back.

A shared script never silently replaces what was in the editor. The session from
before the link is held in memory — the autosave overwrites the stored copy
within half a second — and the notice above the editor offers it back. That is
the only way back: the page never navigated, so the back button would leave the
site.

The fragment is left in the address bar, so the link can be re-opened from
history and the autosave takes over from the first edit. Declining it with
**Bring back what I had** removes it, since it must not open again on the next
reload.

## Deploying

Cloudflare Pages, configured in `../wrangler.jsonc`:

```bash
npm run build:all
npx wrangler pages deploy
```

Cloudflare Pages, not GitHub Pages: only Cloudflare can send the COOP/COEP
headers the threaded wasm build needs, and `public/_headers` sets them. Hosting
on both would have meant a second copy that quietly diverged the moment
threading landed.

**The deploy builds the threaded engine** — `npm run wasm:threaded`, one bundle
for everyone — and then reads the binary it is about to ship to check that is
what it got (`npm run check:threaded`). A single-threaded bundle here would
work, deal the same deals and use one core of however many, which is the sort of
mistake that survives for months. The page also shows the size of its thread
pool beside the engine version, for the same reason.

**Every run deals on the whole pool** — about 4x on a bare filter and 6.9x on a
real scenario, on a twelve-core machine. There used to be a rule that threaded
only double-dummy runs, because wasm's allocator is one lock and a script that
allocated per deal ran *slower* on twelve threads than on one; #88 removed that
allocation and the rule with it. `docs/WASM.md` has the measurements.

`.github/workflows/pages.yml` redeploys on every push to `main` that touches the
engine or the site. It fails loudly when the Cloudflare credential is missing
rather than going green having deployed nothing.

Live at **https://bridge-craftwork.com/dealer3/** (also `dealer3.pages.dev`).

## Viewing and saving

Deals show as **hands** by default — a compass grid with per-hand HCP and
partnership totals — because a one-line string is far harder to read than a
layout. Toggle to **Text** for the raw output.

The grid only applies to the one-line format: `printall` is already a visual
layout and PBN is a record format, so those offer Text instead rather than an
empty grid.

**None is an engine setting, not a display one.** A run gathering statistics —
HCP against tricks is the case that asked for it — never looks at a hand, so
`None` stops the engine collecting them: no deal is cloned, none is rendered,
nothing is serialised out of the worker and nothing is laid out. Over 100,000
library deals the command line's equivalent measures 0.31s against 0.09s, and
7.6 MB of output against 742 bytes. The counts, averages, frequencies, the input
report and the script's own `printes` output are all exactly what the same run
produces in any other format — `none` decides what is written, never what is
computed.

The consequences are said rather than left to be discovered. The Hands/Text
toggle goes, because there is nothing to toggle between; **Save PBN** is
disabled with a reason, since saving re-runs the script and a PBN of a run that
kept no deals would mean dealing the whole thing again — the expensive half of
what `None` was picked to avoid; **Save text** stays and writes the statistics,
built from the result on screen rather than by re-running; **Save PDF** stays
and gives the script and the statistics without boards.

The command line spells the same thing `-f none`. That is a new *value* for
`-f`, not a remapped switch — `-f` is dealer3's own, since the original picks a
format with an `action` statement — so nothing a dealer.exe script or command
line means has changed. `--interleave` is refused alongside it rather than
holding every produced deal for a reordering that is never printed.

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

## Where the deals come from

A radio pair above the editor, and the only control on that row that is about
the deals rather than about the run:

**Random deals** shuffles them here, from the seed, as this page always has.

**Pre-solved deals** draws them from
[Pavlicek's 10,485,760 solved deals](https://github.com/bridge-craftwork/rpdd-library),
so `tricks()`, `dds()` and `par()` are lookups rather than searches — the
difference between double-dummy being usable in a tab and not, since a solve is
about 23ms a deal and a browser has one thread to do it on.

Everything else means what it meant. The seed still reproduces the run; it picks
where in the library to start instead of driving a shuffle, and **the same seed
reads the same deals here as `dealer -s N --input-deals rpdd.zrd` does at a
terminal** — verified against the CLI in `wasm/verify.mjs`, because the page
does not compute that mapping. It asks the engine
(`record_for_seed`), which is the mapping the command line reduces `-s` through.

Three things the page says that a terminal would put on stderr:

* **what arrived** — "Read 20,000 deals from the solved-deal library, starting
  at deal 246,427 of 10,485,760. Every one arrived with its double-dummy
  table…", with warnings when fewer deals arrived than were asked for, when some
  came unsolved, or when a run read the whole library and a longer one would
  start repeating deals;
* **the fetch**, while it happens, since it is the one part of a run that waits
  on something outside the tab;
* **that pre-solved buys this script nothing**, when the script never calls
  `tricks()`, `dds()` or `par()`. Asked of the engine off the parsed program,
  not searched for in the text — a comment mentioning `par` is not a call.

### Where the fetching lives, and why

In the **worker**, with the rest of the engine. A `Library` is a handle into one
wasm instance's linear memory: the page has its own instance for the editor's
instant calls, and only the worker's can say which URLs a run wants, since that
answer comes from `needs()`. Fetching on the main thread would put a round trip
between every ask and its answer for nothing.

Caching is arranged to survive the worker, which Cancel terminates: pieces go
into the Cache API under `dealer3-library-v1`, and an in-memory map on top of it
saves the cache read. A piece is immutable — `rpdd-042.zdd` is the same 640 KiB
for ever — so nothing there expires; the manifest is deliberately not cached.
After a reload, a repeat run fetches only the manifest.

A run asks for at most 65,536 deals (`MAX_LIBRARY_DEALS`), which is about one
published piece. That is a download budget rather than arithmetic: a `Max
generate` of a million would otherwise pull sixteen pieces — ten megabytes — to
look at twenty deals. A filter more selective than that runs out of deals rather
than out of matches, and the report above says so.

## Script parameters

A script using `$0`-`$9` gets a row of fields above the editor, one per
parameter it mentions, and **only then** — nearly every script has none, and a
panel that is always there would cost a line of script for nothing.

Each field is labelled with what the script's own
`# param 0 = west   # the seat that opens` line says, and starts empty with that
default as its placeholder. So clearing a field returns to the declared default
rather than blanking the parameter, and only a parameter with no default and no
value stops the run — that one is marked, and Run says which it is waiting for.

Before this the browser had no way to supply a parameter at all, so a
parameterised scenario failed to parse here with an error naming a line the
reader could not act on. The `$n` occurrences alone were not enough to build a
form from: they give nowhere to put a label and no sensible starting value,
which is what the declarations supply. `dealer --params` prints the same
information in a terminal.

## Session persistence

The script in the editor, the values in its parameter fields, and the settings
beside them are kept in
`localStorage`, so a return visit picks up where the last one left off. The
starter script only appears on a genuinely first visit.

Deliberately **one** session, not a library: a history would need naming,
listing and deleting — a feature in its own right — and the common case is
simply coming back to what you had open. Results are not stored: they can run to
megabytes, they regenerate from the script and seed in milliseconds, and a
stored result could silently disagree with the script shown beside it.

Every access is guarded. `localStorage` throws outright in some privacy modes
rather than returning null, and a corrupt value means "start fresh" rather than
a page that fails to load. The deal source is restored the same way, with one
extra rule: anything but `library` reads back as random deals, so a stored value
that no longer means anything cannot start a visit by downloading a library.

Whether the scenario list is showing is kept here too, and it is the one setting
with a right answer for someone who has never chosen: anything that is not a
stored `false` reads back as open, because the list is how a first visit finds
something to run. Once closed it stays closed — someone who closed it is editing
a script, and having to close it again on every reload is the whole complaint.

## Hiding the scenario list

The picker finds a starting point and then costs 260px for as long as the script
is being edited. **‹** beside the search closes it; what is left is a 28px rail
labelled *Scenarios*, and the whole rail is the way back. A rail rather than
nothing at all, because closing it is the only thing that hides it and clicking
somewhere unmarked is not a way back anyone would find. The two `1fr` columns
take the 232px between them, so the editor and the results both grow rather than
a gap being left where the panel was.

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
the reference page's transforms of the vocabulary, the levelling guide's
markdown rendering, and the solved-deal library's half of the page: how much of
it a run asks for, the ask/supply loop against a stand-in library, both levels of
the cache, and every line the page says about what came back.

The Vue components are not covered by unit tests; they are exercised by the
browser smoke checks instead. That division is deliberate: every real bug in
this app so far has been a wiring or ordering fault that unit tests could not
see. The reference page was no exception — its first build rendered zero entries
because `npm run build` skips the wasm step, so the page ran against an engine
that predated the docs. **Build with `build:all` before checking anything in a
browser.**

## What a client that does not run scripts sees

Everything below `#app` is rendered by JavaScript, so anything that does not run
it — a crawler, a reader with scripting off, an assistant asked to read this
URL — sees only what is literally in `index.html`. That used to be a title and
one sentence of description: **883 bytes**, with 29 KB of authoritative language
reference one path segment away and nothing pointing at it.

Three things now bridge that, and none of them is visible to a user with
JavaScript:

| | |
|---|---|
| `<link rel="alternate" type="text/plain" href="reference.txt">` | the machine-readable route from the app page to the language. Relative, so it resolves both on `dealer3.pages.dev` and under the `/dealer3/` mount |
| `<noscript>` | what a non-scripting client should honestly be told, with links onward. Not markup hidden for crawlers: a person with scripting off gets exactly the same page, and it is a real answer for them |
| `public/llms.txt` | the [llms.txt](https://llmstxt.org) index, generated beside `reference.txt` by `scripts/emit-reference.mjs` |

`llms.txt` is generated rather than written because its figures — the
reference's size, the number of entries, the engine version — are true of the
build that emitted them. A hand-kept copy would be true of whichever build
someone last remembered, which is the failure this repository generates its
status tables to avoid.

**The claim worth keeping accurate** is that the reference is closed: it comes
from the engine's own vocabulary, so it cannot name a function the parser
rejects or omit one it accepts. That is the assurance that lets a model stop
guessing at a language it has barely seen, and it is only true while the file
stays generated.

One thing this does *not* solve: a model still has to be given the URL, or find
the site by searching. Nothing here helps an assistant that has never heard of
dealer3 — it helps one that has been pointed at it, which is the common case.
