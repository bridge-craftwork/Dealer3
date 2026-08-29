# Changelog

All notable changes to dealer3 will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
### Fixed
- **The timing split read a stale measurement on the Leveled tab.** That run
  measures nothing, but the page holds the previous levelling so the natural
  bars survive it — and the split was taking `measuring` from that while the
  total came from the run just done. It reported
  `0.99 sec = 6.01 measuring + 0.00 dealing`, the 6.01 left over from the
  levelling that produced the script. The split now comes from the run on the
  clock, so a re-run shows a plain total and no split at all.
- **Variables are classified once per script, not once per deal — a 15x
  speedup on scripts that build definitions on one another.** Whether a variable
  can reach `rnd()` decides whether its value may be cached for the deal. That
  answer depends on the expression trees alone, which never change, but it was
  worked out inside the per-deal context: every deal walked the whole definition
  graph again, once per variable.
  - Quadratic in how deeply a script nests, and paid per deal. A real scenario
    with sixty-odd variables took **9.2s** for 200,000 deals against the
    original dealer's 0.46s. It now takes 0.62s. Reported from the browser,
    where single-threaded it was sampling "a few a second" and would have taken
    a quarter of an hour.
  - The walk also shares one memo across every variable now, so classifying a
    whole script is linear in the number of definitions rather than quadratic:
    0.1 µs per variable at forty, eighty, a hundred and sixty or three hundred
    and twenty, where it used to be 2.8, 4.0, 6.6 and 13.8.
  - `extract_variables` returns a `Variables` rather than a bare map, which is
    where the answer now lives. The filter itself is unchanged: the same
    scenario produces the same 270 deals in 200,000 as the reference binary.
- **The browser's measuring probe had no clock.** `MEASURE_BUDGET_MS` bounded
  the second pass but not the 10,000-deal probe before it, so a scenario
  producing one deal in 2,800 sat there for minutes. The measuring passes now
  stop on a deadline, checked where the clock is already read for progress.
  - And a pass cut short no longer replaces a better one. With the probe using
    the whole budget the second pass got a fraction of a second, produced a
    single deal, and that one deal became the measurement — which then failed
    the never-seen check, and had briefly shown one hand type at 100% and the
    rest at nothing. Whichever pass got furthest is the one that counts.
- **Auto-level split a `condition` from its expression.** A scenario writing
  `condition` on its own line with the expression on the next got the generated
  block inserted *between* the two, so the keyword read `noLeveling` as its
  condition and the run failed at `= 1` — a line the author never wrote.
  `condition_span` reported only where the expression began; it now reports the
  statement's start as well, and the block goes before the keyword. Found in a
  real scenario, and the shape is common in the wild.
- **A hand type that never occurs is refused rather than warned about.** Making
  thin measurements a warning let a type seen *zero* times through, and a keep
  of `mix / 0` is not imprecise but impossible: no keep makes a hand that does
  not happen. The file was written anyway, claiming a mix it could not deliver
  and reporting `+-inf%`. It now names the types and says what usually causes
  one — a misspelled name, or a condition narrower than the type meant to sit
  inside it.
- **The browser reports names that are used but never defined.** The command
  line has always done this; the browser had no such check, so a misspelling
  parsed and was silently discarded. That is exactly how the scenario above went
  wrong: hand types written `not x4` where the variable is `HandType_x4`, which
  parses, matches nothing, and leaves every type but one empty. The report
  points at the first use — the parser does not carry positions into the AST, so
  the line is recovered by looking for the name in the script as the editor
  holds it, and omitted rather than guessed when it cannot be found.

### Added
- **`dds(compass, strain)` and `printrpt(...)`: two more of DealerV2_4's words.**
  Both were rated unlikely-and-low-value on the roadmap; measuring the actual
  regression suite said otherwise, and both were cheap.
  - `dds` is `tricks` under another name — there it reaches the DDS library
    where `tricks` reaches GIB's solver, and dealer3 sends both through
    bridge-solver. So it is an alias, and the existing alias test proves the two
    agree deal by deal.
  - `printrpt` is `csvrpt` to the screen. DealerV2_4's own reference output is
    the same row down to the quoting and the commas, so the two share a
    renderer here rather than merely resembling one another — with a test that
    runs both and compares. Unlike `csvrpt` it works in the browser, where the
    rows join `printes` in the Text view.
  - Together they take DealerV2_4's regression suite from **29 of 61 scripts
    parsing to 36**. What blocks the rest, in order: `evalcontract` (5),
    `usereval` (2), `par` (2), `opc` (3), `ltc` (3), unsupplied `$n` parameters
    (3, which is #17), 2-D `frequency` (2), `trix` (1), decimal literals (1),
    `export` (1) and `bktfreq` (1).
  - One delta worth knowing, and left alone rather than changed quietly:
    dealer3's `csvrpt` writes a hand as `AT84.84.J7653.A3` where DealerV2_4
    writes `n AT84.84.J7653.A3 ` — its compass and a trailing space. That
    predates this and applies to both statements equally, so the two stay
    consistent with each other; matching DealerV2_4 exactly would change
    `csvrpt` output that scripts may already depend on.
- **`LevelType_*`: levelling on a decomposition of its own.** Optional, and most
  scenarios will never need it — where the categories you level are also the
  ones you talk about, `HandType_` is both.
  - It earns its place when the two come apart. Levelling five HCP bands leaves
    the inside of each band as nature had it, and the fix is to level on each
    HCP separately — but hand types are also what the deals are tagged, reported
    and **ordered** by, so `--interleave` then walked thirteen categories and
    put seven strong hands in the first thirteen boards. The fine split meant to
    correct a detail became the coarse thing a student noticed.
  - So `HandType_` stays what the deals are grouped, tagged and ordered by, and
    `LevelType_` is used for nothing but the keeps. Measured: any five
    consecutive boards now span three to five different bands.
  - The two are **independent** and need not nest; nothing checks that they do,
    because they answer different questions. Each has to partition the deals on
    its own.
  - `_Share` goes on whichever decomposition is being levelled. Setting both is
    refused — only one can be the target mix, and picking one silently would
    deliver a mix nobody asked for. So is weighting hand types when level types
    are declared, which is the natural mistake after adding them.
  - `{{level-mix:12_14}}` still names a band and still reports what that band
    will deliver. Since the decompositions are independent that cannot be read
    off the band's own rate, so the measuring pass counts the two crossed and
    the answer follows from how the band's deals fell across the levelling
    categories.
- **Generation runs in a Web Worker, with progress bars and a Cancel button.**
  It was one synchronous call on the main thread, which blocked everything for
  the length of a run.
  - The Run button never painted its disabled state, because
    `requestAnimationFrame` fires *before* paint — yielding a frame only let the
    browser reach the blocking call sooner. And a click during the freeze was
    queued by the browser and delivered the moment the tab thawed, starting a
    second run. Neither is fixable from outside the block.
  - **Cancel is `terminate()`**, which is the only thing that stops code already
    inside the wasm: a flag would need the blocked thread to come back and read
    it. The worker is recreated for the next run.
  - Progress is reported per phase, since a levelled run deals the scenario up
    to three times and one bar would appear to finish and start over. The
    measuring bar has no total until the probe finishes — that is what the probe
    is for — so it runs indeterminate rather than inventing a denominator.
  - Reports are throttled by the clock rather than by a deal count: a bare `hcp`
    condition and one calling `tricks()` differ by orders of magnitude per deal,
    so any fixed count is a flood or a silence. One is forced at the end of each
    phase, or a bar freezes short of its own total as the next one starts.
  - The bars and Cancel wait a second before appearing, so a short run does not
    flash them up and down.
  - Only `generate` moved. `check_script` and `language_info` are called while
    the editor is being built and are far too fast to be worth an await.
- **The browser reports the measuring pass and the run separately**, as
  `5.71 sec = 5.00 measuring + 0.72 dealing`. A levelled run deals the scenario
  twice — once to find out what it does, once to do it — and a single total made
  the second look slow when nearly all of the wait was the first: re-running the
  levelled scenario on its own took 4.6s against the 9s reported for levelling
  it.
- **A Copy button on both script panes**, inset top-right. Selecting the text by
  hand does not work: CodeMirror draws only the lines on screen, so Ctrl-A takes
  the whole page and dragging takes only what has been rendered. The generated
  scenario is the one most worth copying — it is what gets pasted into BBO — and
  being read-only it has no caret to select from either. Falls back to a hidden
  `<textarea>` where `navigator.clipboard` is missing or refused, and says so
  rather than flashing "Copied" when it fails.
- **`HandType_X_Share = N`: the target mix, written in the scenario.** A weight
  per hand type, defaulting to 1 — so a scenario that says nothing still gets an
  even split, and one that says something carries its own intended mix wherever
  it runs. The browser needs no control for it, and the two front ends cannot
  drift apart. Still only a variable assignment, so it parses on BBO exactly as
  the `HandType_` convention does.
  - `--level-target` overrides it, as `-s` overrides `seed`.
  - The case it exists for: levelling five HCP bands leaves the *inside* of each
    band as nature had it, so within 12-14 a 12 is far commoner than a 14.
    Making each HCP its own type fixes that but breaks the bands — thirteen even
    types give the three-wide bands more than the two-wide ones. Shares of 2 and
    3 restore both at once, and the arithmetic is the sort that ends up wrong in
    a comment.
  - `hand_types()` had to learn to skip these, or `HandType_12_Share` becomes a
    fourth hand type called `12_Share`, overlapping the real one and breaking
    the partition. The suffix is matched without regard to case for the same
    reason: an unrecognised `_share` would fail that way silently.
- **The measuring pass sizes itself.** It deals until the rarest hand type has
  been seen 2,000 times — about ±2.2% on that rate — and stops there. That
  number is what sets the precision of the whole levelling, since a keep is
  `mix / natural` and an error in the divisor is baked in for good.
  - How many deals that takes depends entirely on how rare the rarest type is,
    and the range is wide: the five-band example needs 158,000 produced, a
    thirteen-band one whose scarcest value is 0.2% of qualifying deals needs
    1,066,000. No fixed number serves both, which is why `-p` no longer sizes it
    — the old fixed pass was arbitrary and, for anything with a genuinely rare
    type, arbitrarily low.
  - **`--level-measure`** caps the deals produced (default 2,000,000) and
    **`--level-timeout`** the seconds (default 60). Reaching either warns rather
    than refusing: the file is written, and the shortfall is reported and
    stamped into it. Measuring too thin was refused before, which gave neither a
    file nor a way forward.
  - It stops on the exact deal that finishes the job rather than at the end of
    the batch it fell in. Batches are 200 deals per thread, so a batch boundary
    would make the measurement — and the file generated from it — depend on how
    many cores the machine has, and CI regenerates and diffs `examples/`.
  - A run that reaches the goal is reproducible — same seed, same file, whatever
    the machine — but one stopped by the *clock* stops wherever the clock caught
    it. Pin `--level-measure` where a build has to produce the same file every
    time. This is not hypothetical: the determinism test caught it on Windows
    CI, where a debug build was slow enough for the default timeout to fire.
  - In the browser the same thing happens against a clock rather than a deal
    count, since a page blocks while it deals: a 10,000-deal probe to find out
    how rare the rarest type is, then as long as that suggests within a few
    seconds. The example scenario went from 10,000 measured deals to 125,787,
    and its rarest band from 159 sightings to 1,589 — which took its delivered
    share from 17.9% to 21.3%, the 24% keep error being exactly the fault the
    old fixed pass could not see.
- **`predeal` may name more than one seat**, as dealer.exe's does:
  `predeal north SAKQ south SJ32` sets both. Its grammar is
  `predealargs: predealarg | predealargs predealarg`, and the reference binary
  does it; dealer3 took one compass per statement and read the second seat's
  holdings as undefined names.
  - What separates the seats is that the holdings of one are comma-separated
    and the seats are not. That matters more than it sounds, because `S` is
    both a void in spades and an abbreviation for South — matching the
    comma-list before trying another seat is what keeps
    `predeal north S,HAKQ south SJ32` at two seats rather than three.
  - Each seat becomes its own `Statement::Predeal` rather than the AST growing
    a list, so every consumer that walks the statements is unchanged. Naming a
    seat twice accumulates, as the original's repeated
    `predeal_holding(compass, ...)` does.
  - A seat given no holdings — `predeal north SAKQ south` — is now refused.
    It reads as a bare compass once the `predeal` has taken every seat that
    came with cards, and the original answers `syntax error`. Reported after
    the undefined names rather than at parse time, because `dealr west` is a
    bare compass too and there the misspelled `dealer` is the thing worth
    saying.
  - Checked against all 1,051 scripts in the Practice-Bidding-Scenarios
    corpus, which still parse.
- **`--param N=TEXT`: DealerV2_4's script parameters, `$0` to `$9`.** One script
  can then be several — `NTscripted.dls` in that project's own examples is a
  notrump opener whose range, seats and shape all come from the command line.
  - A parameter is source rather than a value. DealerV2_4's lexer scans the
    switch's text where the `$n` stood, so it can be a number, a compass, a
    shape spec or a function name: `$9($0)` with `--param 9=hcp --param 0=west`
    is `hcp(west)`. Nothing but substitution does that, so it is a preprocessor
    pass, running before the shapes are expanded — a parameter may be part of
    one, as `shape{$1, $2:d>c or h>s}` is in their regression suite.
  - **The switch differs, the syntax does not.** DealerV2_4 sets these with `-0`
    to `-9`, which are dealer.exe's swapping switches; dealer.exe wins.
  - A `$n` nothing supplies is an error, naming the line. DealerV2_4 zeroes its
    parameter table and never looks again, so an unfilled one scans an empty
    buffer and vanishes — `average $2 controls(west)` quietly becomes a valid
    statement that has lost its label. A `--param` the script never mentions is
    a warning.
- **`shape{ ... }`: François Dellacherie's shape language** (his 1997 `dpp`, which
  DealerV2_4 ships as `fdp`). `shape{north, 4M(3+3+2+)}` says what twelve
  patterns say in `shape(...)`: `5+` is at least five, `2-` at most two,
  `[3-5]` a range, `(431)` the remaining suits in any order, `M` either major
  and `m` either minor, and `:` attaches a condition on the suit lengths.
  - Expanded in the preprocessor rather than parsed, which keeps the braces out
    of the grammar — the four-digit shape literals were trouble enough on their
    own — and leaves the web editor's highlighter seeing an ordinary `shape`
    call. No helper binary either, so the wasm build has it too.
  - Where DealerV2_4 develops pattern strings, dealer3 evaluates each construct
    over the 560 distributions and renders the answer back. That is what makes a
    ten-card suit expressible: `5+Mxxx` means a ten-card major as much as a
    five-card one, and `fdp` silently drops all forty such shapes because a
    pattern is one character per suit.
  - So suit lengths now run past `9` into `:;<=` for ten to thirteen — the
    original's own convention, which `insertshape` uses internally and has never
    let a script type. `shape(north, %s:111)` is a ten-card spade suit.
  - Checked against the six worked cases in DealerV2_4's own
    `docs/FD_Shapes_examples.txt`, comparing the distributions each denotes: the
    two agree exactly, but for the ten-card suits dealer3 adds.
- **`[HandType "..."]` in PBN output.** A variable whose name starts with
  `HandType` names a category of hand, and each PBN record carries the one it
  matched — so a practice set can be sorted or interleaved afterwards without
  the categories being reimplemented outside the script, which is how a
  definition and its description drift apart.
  - A naming convention rather than new syntax, deliberately: a script using it
    still parses on the original dealer, and these scenarios run on BBO. The
    cost is that the parser cannot catch a misspelling, so the names found are
    reported in the statistics and in `--stats-json`.
  - Types have to partition the deals. Two matching one deal is refused; a deal
    matching none is untagged, which is not an error.
  - `format_printpbn` takes a `PbnBoard` struct rather than eight positional
    arguments.
- **`--write-leveled FILE`** measures a scenario's hand types and writes a copy
  with the levelling filled in — the whole two-step method as one command.
  `--level-target` takes `even` or one weight per type; `--level-budget` caps the
  cost in deals dealt per deal kept, relaxing exactness rather than sacrificing
  the rarest type when a target costs more than that.
  - `{{level-mix:22_24}}` in the stock file is replaced by that type's share of
    the result, so the text a student reads cannot drift from the keeps.
  - The `### BEGIN GENERATED LEVELING ###` placeholder is optional. Naming the
    hand types says everything the levelling needs, so a scenario without one
    gets the block written in above its condition and `and levelTheDeal` added
    to the condition — which is found with the parser, since the original's
    grammar lets it be a bare expression and most scenarios write it that way.
  - Refuses, before dealing anything: a generated file fed back in, a scenario
    with no condition to gate, a `levelTheDeal` nothing uses, a `roll` that is
    not the safe form, types measured on fewer than 500 deals, and types that
    leave a deal unclassified.
  - A target weight of `0` excludes its type exactly — written `level_X = 0`,
    not rounded up to the one-in-a-thousand a threshold can express. A keep that
    is genuinely too small for the roll's range is refused, since rounding it
    either way leaves the file disagreeing with its own header.
- **`--interleave`** orders the output so each hand type appears before any
  repeats, which is what turns a 500-board set from correct-in-aggregate into
  usable one hand at a time. Rare types are spread across the whole run rather
  than exhausted early: with shares of 20/20/20/20/10/10, the four common types
  appear in every round and the two rare ones in alternate rounds, so every
  round holds five deals rather than six and then four. Bucket sizes are
  reported on stderr, leaving stdout pure PBN.
  - Within a round the types come out in a shuffled order, derived from the
    seed and the round number. Declaration order every round reads as the
    natural frequency the levelling exists to remove, and is something a
    student learns rather than something they practise against.
  - Boards are numbered by where they land, not by when they were dealt, in
    every format that carries a number. The ordering lives in the file, and
    `[Board]` is what a reader sorts or indexes on; numbering in production
    order would let any such reader undo the ordering without an error. Dealer
    and vulnerability rotate with the number, so they follow too.
  - Refused alongside `--write-leveled`: that run measures the scenario as it
    stands, so there is no practice set to walk through. Level first, then
    interleave the generated file.
- **The last three words of the original's language: `print`, `printes` and
  `rnd`** (#15). All three were reserved and refused; all three now work, and
  their output is byte-identical to dealer.exe's on the same deal.
  - `printes(<expression> | "string" | \n, ...)` prints a line of your own per
    matching deal. Nothing is added between terms and no line ends unless you
    ask for one — and a line ending is a bare `\n` in the list, not an escape
    inside a string, because the original's lexer reads no escapes between
    quotes. Verified against the reference binary rather than inferred.
  - `print(<compass>, ...)` lays a seat's hands out at the end of the run, four
    boards to a line-printer page, spades down to clubs, a form feed after each
    seat. Seats come out north, east, south, west whatever order they are named
    in, as in the original.
  - `rnd(bound)` gives a random number in `0..bound`. **It draws from a stream
    of its own, seeded from the deal**, rather than from the generator dealer3
    shuffles with. The original shares one generator, so calling `rnd()` there
    changes which deals come out; that is an artefact of having one generator,
    and copying it here is not possible anyway, since deals are worked out in
    parallel and a shared stream would make output depend on thread scheduling.
    As it is, the same seed gives the same answers whatever `-R` or
    `--batch-size` say.
  - `--rnd-seed N` shifts that stream, for a different draw from the same
    deals. Long form only: the short letters are dealer.exe's.
  - In the browser, `printes` output appears at the top of the Text view rather
    than going to a terminal. `print` is refused there: a paginated hand record
    with form feeds has nowhere to go on a page, and quietly dropping it would
    leave a script looking as though it had run.
- `evalcontract` is now reserved in the grammar, so it fails loudly instead of
  being read as an undefined variable that silently matched nothing. The
  original parses it and then aborts on an assertion, so there is nothing to be
  compatible with.

### Added
- **`--stats-json`** reports `average` and `frequency` results as JSON instead of
  tables: full precision rather than the tables' six significant digits, and the
  sample size behind each average. Pair with `-q` for a stdout that is nothing
  but JSON. Written for the levelling workflow in
  `docs/leveling-strategy.md`, where a build step measures a scenario's natural
  mix and computes keep rates from it — dividing by a rate parsed out of `%g`
  output, from a label that might contain a colon, is not a foundation.
  - `hand_types` carries each type's name, the number of deals that matched it
    and its share of the run, in declaration order. Verifying that a levelled
    scenario delivered its mix is the reason to read this at all, and the run
    has already counted them — a scenario should not have to carry an `average`
    statement per type to be told what it produced.
- **`docs/leveling-strategy.md`**, describing how to level a scenario in one
  measurement and one calculation, the portable `roll` construct `rnd()` needs,
  and the closed-form cost of a target mix.
- **`docs/leveling-guide.md`**, the how-to companion to that paper: the
  `HandType_` convention, the generated block, the switches, the browser's
  Auto-level box, and — first, because it is the one error levelling cannot
  recover from — how many deals to measure over.
  - The keeps are `mix / natural`, so a relative error in a measured rate is
    baked in permanently and producing more deals never averages it out. The
    command line refuses under 500 sightings of a type; the browser goes ahead
    from 50 and reports the count. Both numbers are in the guide with the
    reason, and a worked case: a 10,000-deal pass that saw `22_24` 159 times
    against a true rate predicting 129 leaves that band delivering 16.7%
    instead of 20%, for good.
  - One source, three readings. It is markdown in the repo,
    <https://dealer.bridge-classroom.org/leveling.html> on the site — a third
    Vite entry that inlines the file with `?raw`, so `pages.yml` now watches the
    markdown as a build input — and a PDF from
    `.github/workflows/docs-pdf.yml`, which builds this and the strategy paper
    on a change to either.
  - It does **not** live in `docs/FILTER_LANGUAGE_STATUS.md`. Levelling is a
    layer over the language rather than part of it, and that page is generated
    from `vocabulary.rs` with a test to keep it that way.

### Fixed
- **A variable holding `rnd()` is no longer cached for the deal.** Every mention
  draws again, including through another variable, which is what the original
  does and what BBO reports for the same script. The per-deal variable cache
  arrived as a pure optimisation, back when every expression in the language was
  a function of the deal alone; `rnd()` was the first that was not, and made the
  cache observable. Only variables that can reach `rnd()` are affected —
  everything else stays cached, and a 109-variable script from the
  Practice-Bidding-Scenarios corpus shows no measurable change.
- **A variable may be named after a keyword** — `conditionMet`, `actionList`,
  `printewFoo`, `produce5`, `dealerN` and the rest (#12). Every statement rule
  now checks that its keyword ends where it appears to, rather than matching the
  front of a longer name.
  - Half of this was a parse error and half of it was silent. `action` and the
    `print*` statements take no required argument, so `actionList = 1` parsed as
    an empty `action` plus an assignment to `List`: the script ran to its
    generate limit, matched nothing and exited 0.
  - dealer.exe accepts all of these names — its lexer takes the longest token —
    so this moves toward compatibility, not away.
- **`vulnerable ewer` is now the syntax error dealer.exe calls it**, rather than
  being read as `vulnerable ew` with a stray `er` after it. The missing word
  boundary on the vulnerability values is the same defect from the other side.

### Added
- **The original's swapping switches, `-0`, `-2` and `-3`.** One shuffle, several
  deals: `-2` deals each shuffle again with East and West exchanged, `-3` runs
  East, South and West through all six arrangements, `-0` asks for the default.
  The three override one another, so the last one written wins, as under getopt.
  They were previously parsed only to be refused.
  - **A predeal to a seat the swap moves is refused**, rather than silently
    broken. The original applies the swap without telling the shuffle, which
    tracks predealt cards by position, so predealt cards move to the wrong seat
    on the first swap and are gone by the second shuffle, with no message. Only
    the seats actually at risk are refused: `predeal north` with `-3` — a fixed
    hand against six defensive layouts — works, and so does `predeal south`
    with `-2`.
  - `-g` still counts and stops exactly, whatever the batch size, and output is
    unchanged by thread count. Every nth deal of a swapped run is the deal the
    same seed produces without the switch.
  - Not combinable with `--input-deals`, which supplies deals rather than
    shuffling them.

### Changed
- **`tricks()` now goes through `bridge-solver`.** A solve was over fifteen
  minutes and saturated every core, which put every double-dummy script out of
  reach; it is now about ten milliseconds. The 1000-deal, two-denomination
  workload in issue #14 went from days to thirty seconds.
  - Double-dummy results are remembered per deal against the denomination and
    declarer they answer for, so writing the call out longhand several times,
    asking about several denominations, or calling it from both a `condition`
    and an `average` costs one search each — including across the worker
    threads generation uses.
  - `scripts/dd-bench.sh` is the regression benchmark, with the budgets issue
    #14 set as its acceptance criteria.
  - The browser build grows by about 11 KB gzipped, to ~386 KB.

### Removed
- **The hand-rolled alpha-beta solver in `dealer-dds`.** It was correct and
  unusably slow, and nothing reaches it any more. `DoubleDummySolver`,
  `SolveResultWithLine` and the game-state machinery are gone; `Denomination`,
  `DoubleDummyResult` and `TrickResult` stay, alongside the new `DealAnalysis`,
  `tricks()` and `solve_all()`.
- **BREAKING: legacy mode (`--legacy`) and the ported GNU `random()`.**
  `-s/--seed` no longer reproduces dealer.exe's deal sequence. Scripts port
  unchanged and filter semantics are unaffected — only the specific deals for a
  given seed differ. The flag is still parsed and reports its removal rather than
  failing with an unknown-argument error; it will be dropped entirely in a future
  release (target: 2027).
  - The `gnurandom` crate is removed from the workspace. Its xoshiro256++
    implementation, which was always the production RNG, moved to
    `dealer-core/src/rng.rs`. The GNU `random()` port derived from Berkeley
    `random.c` and carried a 1983 UC Regents notice; it now lives in the
    `dealer-legacy-shuffle` repository, which handles its attribution.
  - Also removed: the `dealer-test` crate, `tools/rng-experiments/`, and the
    golden shuffle tests, all of which existed to verify the legacy RNG.
  - `scripts/compare-dealer.sh` is superseded: it compared deals one-for-one,
    which required legacy mode. Use `scripts/test-filter.py` or
    `scripts/generate-corpus.py` instead.

### Changed
- CI's "Check dealer.exe Compatibility" job, which only asserted output was
  non-empty, is replaced by a job running both regression tiers explicitly and
  verifying `gnurandom` is absent from the dependency tree

### Added
- `--input-deals -` now reads deals from stdin, so deals can be piped in without a
  temporary file. Requires the script to be passed as a file argument, since stdin is
  otherwise consumed by the script itself; a clear error is emitted if both would
  contend for stdin.
- Integration test coverage for `--input-deals` (PBN, oneline, stdin, limits, conflicts)
- **Tier 1 regression corpora** — deal sequences captured once from dealer.exe and
  committed under `dealer/tests/corpus/`, replayed by `corpus_replay` to verify
  script parsing and filter semantics against the reference implementation. These
  tests never invoke dealer.exe, so they run anywhere including CI.
- `scripts/generate-corpus.py` for creating new corpora, and
  `docs/REGRESSION_TESTING.md` documenting the process
- **Tier 2 regression hashes** — `regression_hash` pins dealer3's own output at
  fixed seeds, covering generation and filtering together, including the predeal
  path that Tier 1 cannot reach (`--input-deals` rejects predeal by design).
  Hashes are committed in `dealer/tests/regression_hashes.txt` and regenerated
  with `UPDATE_REGRESSION_HASHES=1`. Uses FNV-1a rather than `DefaultHasher`,
  which is not stable across Rust releases.
- Tests asserting output is independent of thread count and stable across runs

### Changed
- Unreadable deals encountered while reading `--input-deals` are now skipped with a
  warning and a total reported at exit, rather than aborting the run
- Documented `--input-deals` in the README and CLI design notes

### Fixed
- `dealer-parser/tests/fixtures/Stayman.dlr` contained the literal text
  `404: Not Found` instead of a script; replaced with a real Stayman scenario

## [0.4.0] - 2026-01-21

### Added
- **solver-diag binary** - Diagnostic tool for bridge solver debugging and analysis
- **PBN file format specification** - Added documentation for PBN format

### Changed
- Refactored solver CLI into separate directory structure with clap-based argument parsing
- Fixed `-v/--verbose` behavior - stats are now hidden by default, `-v` shows them (matches dealer.exe)
- Average and frequency output now goes to stdout instead of stderr (matches dealer.exe)

### Fixed
- Verbose flag logic was inverted - now correctly matches dealer.exe behavior

## [0.3.0] - 2026-01-07

### Added
- **Fast parallel mode** (default) - 5x+ speedup over C dealer.exe using xoshiro256++ RNG
- `--legacy` flag for dealer.exe-compatible single-threaded mode with GNU random
- `deal-validator` binary for validating deals against filter files
- Chained comparison support (`a == b == c`)
- Positional input file argument (`dealer file.dlr` instead of `dealer < file.dlr`)
- Allow `-g` and `-p` to be used together

### Changed
- Default mode now uses fast parallel execution with xoshiro256++ RNG
- Use `-R N` to control thread count (0 = auto-detect, default)
- Legacy mode (`--legacy`) required for exact dealer.exe deal sequence matching

### Performance
- **5.2x faster** than C dealer.exe (12 threads, complex filter)
- **3.9x speedup** with 12 threads vs single-threaded
- Rust single-threaded is 1.15x faster than C dealer.exe
- FxHashMap and reference-based evaluation for 9.6x eval speedup

### Fixed
- Predeal parsing for suit-only holdings (S, H, D, C)
- Parser compatibility with dlr test suite
- dealer.exe PBN verbose bug handling in compare-dealer
- Generate limit (10M) and average format (%g) matching dealer.exe

## [0.2.0] - 2026-01-01

### Added
- `-v/--verbose` switch to enable verbose output (matches dealer.exe -v behavior)
- `-V/--version` switch to print version information and exit (matches dealer.exe -V behavior)
- `-q/--quiet` switch to suppress deal output, only showing statistics (matches dealer.exe -q behavior)
- `-m/--progress` switch to show progress meter during generation (matches dealer.exe -m behavior)
- `--vulnerable` long-form option for setting vulnerability
- Deprecated switch detection with helpful error messages for `-2`, `-3`, `-e`, `-u`, `-l`

### Changed
- **BREAKING**: Removed `-v` short form for vulnerability setting
  - Use `--vulnerable` instead (long form only)
  - This change makes dealer3 compatible with dealer.exe where `-v` means verbose
  - Migration: Replace `-v none` with `--vulnerable none` in your scripts

### Removed
- **BREAKING**: `-v` short option no longer sets vulnerability (use `--vulnerable` instead)

### Deprecated
- `-2` (2-way swapping) - Not supported, incompatible with predeal
- `-3` (3-way swapping) - Not supported, incompatible with predeal
- `-e` (exhaust mode) - Not supported, experimental feature never completed
- `-u` (upper/lowercase) - Not supported, cosmetic feature
- `-l` (library mode) - Not supported, conflicting meanings in dealer.exe vs DealerV2_4

## [0.1.0] - 2024-12-01

### Added
- Initial release with core functionality
- Support for dealer.exe constraint language
- Predeal support matching dealer.exe exactly
- Multiple output formats (printall, printew, printpbn, printcompact, printoneline)
- Average and frequency actions
- Command-line switches: `-p`, `-g`, `-s`, `-f`, `-d`, `-v` (vulnerability)

---

## Migration Guide: 0.1.0 → 0.2.0 (Unreleased)

### Command-Line Switches

**Before (0.1.0)**:
```bash
dealer -v none -p 10          # -v for vulnerability
dealer -v NS -f pbn           # -v for vulnerability
```

**After (0.2.0+)**:
```bash
dealer --vulnerable none -v -p 10    # --vulnerable (long), -v for verbose
dealer --vulnerable NS -f pbn        # --vulnerable (long)
```

### Why This Change?

The `-v` switch in dealer.exe means "verbose" (toggle statistics output). Using it for vulnerability created incompatibility with:
1. BridgeBase Online (BBO), which uses dealer.exe
2. Scripts written for dealer.exe
3. User expectations from dealer.exe

By changing to `--vulnerable` (long form only), we:
- ✅ Match dealer.exe behavior for `-v` (verbose)
- ✅ Enable BBO compatibility
- ✅ Add `-V` for version (standard practice)
- ✅ Keep vulnerability support via clear long form

### Backward Compatibility

This is a **pre-1.0 breaking change**. Since dealer3 is still in 0.x development, we can make this change before the 1.0 release to ensure maximum compatibility with dealer.exe and BBO.

After this change, dealer3 will be **command-line compatible** with most dealer.exe scripts.
