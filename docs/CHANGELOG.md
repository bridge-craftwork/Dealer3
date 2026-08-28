# Changelog

All notable changes to dealer3 will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
  - Refuses, before dealing anything: a generated file fed back in, a missing
    placeholder, a `levelTheDeal` nothing uses, a `roll` that is not the safe
    form, types measured on fewer than 500 deals, and types that leave a deal
    unclassified.
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
