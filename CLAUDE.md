# dealer3 Project Context

## Project Overview

dealer3 is a Rust implementation of dealer.exe (bridge hand generator). It is
compatible with the original's **script language and command-line interface**,
and supports DealerV2_4 enhancements. It is deliberately **not** compatible with
the original's deal sequence: a seed does not reproduce dealer.exe's boards,
which is what went with legacy mode in 0.5.0.

## Current Status

- **Version**: 1.0.0
- **Last Updated**: 2026-08-29
- **Switches**: see `docs/command_line_comparison.md`, which is generated. This
  line used to carry a count and it was wrong by ten.
- **Language**: every word the original accepts is implemented, bar `evalcontract`,
  which the original itself aborts on; 25 functions under 40 spellings
- **Also shipping**: a WebAssembly build and a browser app at
  https://dealer.bridge-classroom.org, with a generated language reference

**Do not write status figures into a document by hand.** Both tables below are
generated from the code and verified by `cargo test`, because the hand-kept
versions drifted badly — the switch table listed `-R` as unimplemented months
after it shipped, and the language page listed `tricks`, `score` and `imps` as
to-do while its own summary said they worked.

| Question | Where it is answered |
|---|---|
| Which switches work, and how they compare to dealer.exe and DealerV2_4 | `docs/command_line_comparison.md` (generated from clap) |
| Which functions, operators and statements the language accepts | `docs/FILTER_LANGUAGE_STATUS.md` (generated from `vocabulary.rs`) |
| How to level a scenario's hand types, from either front end | `docs/leveling-guide.md`, also at https://dealer.bridge-classroom.org/leveling.html |
| Why levelling works the way it does, and what it costs | `docs/leveling-strategy.md`, with a worked pair in `examples/` |
| What is still missing, with the reasons | the "Where dealer3 still differs" table in that same file |

```bash
cargo test -p dealer                  # verifies both documents
UPDATE_DOCS=1 cargo test -p dealer    # rewrites their generated tables
```

## Architecture

### Crate Structure
```
dealer3/
├── dealer-core/        - Deal generation, hand analysis (HCP, controls, shape)
├── dealer-pbn/         - PBN format I/O
├── dealer-parser/      - Constraint language parser (pest PEG grammar)
├── dealer-eval/        - Expression evaluator (variables, functions, operators)
├── dealer-level/       - Levelling arithmetic: keeps, target mixes, the generated block
├── dealer-run/         - The generate loop, shared by both front ends
├── dealer-dds/         - Double-dummy solving, behind `tricks()` and `dds()`
├── wasm/               - WebAssembly bindings (its own workspace, excluded from the root)
└── dealer/             - CLI application (main binary)
```

**`dealer-run` is where generating lives.** Dealing, testing the condition,
classifying against `HandType_`/`LevelType_`, accumulating `average` and
`frequency`, levelling a scenario and threading it are all there, behind one
entry point:

```rust
dealer_run::run(script, options, host) -> RunReport
```

A front end supplies a `RunHost`, which is three methods and none of them about
generating: when to stop, when a pass ended, and where a produced deal goes.
`dealer/src/main.rs` parses arguments and renders; `wasm/src/lib.rs` reads the
page's settings and renders. **Neither has a generate loop, and neither should
grow one** — they each had one until 2026-08-29, they drifted, and the drift was
invisible until someone compared them.

A levelled run makes two passes and the second is a filter over the first, so
the engine keeps what the first pass matched and re-uses it. That cache is
private to `dealer-run`; a front end cannot see it, which is the point.

### Key Design Decisions

1. **RNG**: xoshiro256++, in `dealer-core/src/rng.rs`. A seed reproduces a
   dealer3 run, not a dealer.exe one.
   - The port of GNU `random()` that once did reproduce dealer.exe's deals was
     extracted in 0.5.0 and lives in
     [dealer-legacy-shuffle](https://github.com/bridge-craftwork/dealer-legacy-shuffle),
     published on crates.io. Nothing in this workspace depends on it, and CI
     asserts as much.
   - **What this file used to say here was wrong.** It described dealer.exe as
     doing 64-bit arithmetic. The binary is PE32/i386 — 32-bit — and Windows is
     LLP64, so `long` is 32 bits there in any case. The 64-bit behaviour came
     from Mach-O x86_64 objects built from the same `__random.c` on macOS, not
     from dealer.exe. The deal sequences the old work predicted were still
     correct: the two data models differ only in bit 31, and dealer indexes its
     card table from bits 15..=30. The disassembly and captured vectors are in
     dealer-legacy-shuffle's `PROVENANCE.md`; do not re-derive them.
2. **Parse Once, Evaluate Many**: AST is Clone + Send + Sync for efficient parallel evaluation
3. **Breaking Change (0.2.0)**: `-v` changed from vulnerability to verbose (matches dealer.exe)
   - Use `--vulnerable` (long form only) for vulnerability
4. **Deprecated Switches**: Parse and show helpful errors for `-e` and `-l`. `-2`
   and `-3` are implemented (swapping modes); `-u` is accepted and ignored,
   because it does nothing in dealer.exe either

## Implemented Features

### Command-Line Switches (14 total)
- ✅ `-p N` / `--produce N` - Produce N matching deals (default: 40)
- ✅ `-g N` / `--generate N` - Generate N total deals (default: 10M)
- ✅ `-s SEED` / `--seed SEED` - Random seed
- ✅ `-f FORMAT` / `--format FORMAT` - Output format (oneline, printall, printew, printpbn, printcompact)
- ✅ `-d POS` / `--dealer POS` - Dealer position (N/E/S/W)
- ✅ `--vulnerable VULN` - Vulnerability (None/NS/EW/All) - **long form only**
- ✅ `-v` / `--verbose` - Verbose output (matches dealer.exe)
- ✅ `-V` / `--version` - Version info (matches dealer.exe)
- ✅ `-q` / `--quiet` - Quiet mode (matches dealer.exe)
- ✅ `-m` / `--progress` - Progress meter every 10K deals (matches dealer.exe)
- ✅ `-e`, `-l` - Deprecated switches (helpful error messages)
- ✅ `-2`, `-3` - Swapping modes (implemented)
- ✅ `-u` - Accepted and ignored, as in dealer.exe

### Filter Language Features
- ✅ **Functions**: hcp, controls, shape, hearts, spades, diamonds, clubs, losers, suit_quality, cccc
- ✅ **Operators**: Arithmetic (+, -, *, /, %), Comparison (==, !=, <, <=, >, >=), Logical (&&, ||, !), Ternary (? :)
- ✅ **Keywords**: condition, produce, generate, action (printall/printew/printpbn/printcompact/printoneline), dealer, vulnerable, predeal, average, frequency
- ✅ **Variables**: Assignment and lookup (e.g., `opener = hcp(north) >= 15`)
- ✅ **Predeal**: Assign specific cards before shuffling (matches dealer.exe exactly)
- ✅ **Average/Frequency**: Calculate statistics over matching deals

## Important Files to Know

### Documentation (Always Check These First!)
- `docs/FILTER_LANGUAGE_STATUS.md` - The language, generated from `vocabulary.rs`
- `docs/command_line_comparison.md` - Switch comparison, generated from clap
- `docs/WASM.md` - The WebAssembly build and its API
- `web/README.md` - The browser app and the language reference page
- `docs/CHANGELOG.md` - Breaking changes and migration guide
- `docs/command_line_switch_requirements.md` - CLI switch strategy and status
- `docs/PHASE_0_COMPLETION.md` - Phase 0 implementation report
- `docs/DEPRECATED_SWITCHES.md` - Deprecated switches documentation
- `docs/implementation_roadmap.md` - Implementation roadmap
- `docs/dealer_vs_dealer2_switches.md` - Switch compatibility matrix

### Source Code (Main Entry Points)
- `dealer/src/main.rs` - CLI application with argument parsing
- `dealer-parser/src/grammar.pest` - PEG grammar for constraint language
- `dealer-eval/src/lib.rs` - Expression evaluator
- `dealer-core/src/rng.rs` - xoshiro256++ RNG

### Tests
- `cargo test` - Run all tests (118 passing)
- All crates have comprehensive test coverage

## Common Tasks

### Building and Testing

**Use `./dev-build.sh` for local builds, not bare cargo.** This workspace depends on sibling bridge crates (bridge-types, bridge-encodings, bridge-solver) as git dependencies, with `[patch]` overrides in `.cargo/config.toml` redirecting them to local checkouts in `../`. Cargo never lets a `[patch]` override an existing `Cargo.lock` pin, so once a lock exists, bare `cargo build` can silently compile the GitHub revisions of those crates instead of your local edits. The script verifies each patched crate actually resolved to a local checkout and fails loudly if not. (This repo does not commit `Cargo.lock`, so there is no committed lock to protect — the script's swap step is a no-op here, but the verification still matters.)

```bash
./dev-build.sh build --release # Build all crates (verified against local checkouts)
./dev-build.sh test            # Run all tests
cargo install --path dealer    # Install to ~/.cargo/bin/dealer
```

### Running Examples
```bash
# Produce 10 hands with 20+ HCP in North
echo "hcp(north) >= 20" | dealer -p 10 -s 1

# Generate 100K deals and report all matches
echo "hcp(north) >= 20" | dealer -g 100000 -s 1

# Verbose output with progress meter
echo "hcp(north) >= 20" | dealer -v -m -p 100

# Quiet mode (only statistics)
echo "hcp(north) >= 20" | dealer -q -v -p 100

# PBN format with vulnerability
echo "hcp(north) >= 15" | dealer --vulnerable NS -f pbn -p 5

# Predeal specific cards
cat << 'EOF' | dealer -p 3
predeal north AS,KS,QS
predeal south AH,KH,QH
condition hcp(north) + hcp(south) >= 12
EOF
```

## Next Steps

Compass predeal, CSV export and title metadata are all **done** — this list said
otherwise for months, which is why the status tables are now generated.

The only language issue still open is BBO strict mode, `--bbo-strict` (#13, low
priority) — a warning switch, not a missing word.

Remaining switch gaps are the DealerV2_4-only ones (`-M`, `-Z`, `-U`, `-O`,
`-D`); see the generated comparison table.

## Development Guidelines

1. **Never remap dealer.exe switches** - compatibility is critical for BBO
2. **Test coverage required** - all new features need tests
3. **Update documentation** - keep FILTER_LANGUAGE_STATUS.md and related docs current
4. **Breaking changes only pre-1.0** - we're still 0.x, but be careful
5. **Match dealer.exe behavior exactly** for implemented features
6. **Pre-commit checks** - Before committing, always run and fix:
   - `cargo fmt --all` - Format all code
   - `cargo clippy --workspace --all-targets --all-features -- -D warnings` - Fix all clippy warnings
   - `cargo test --workspace` - Ensure all tests pass
7. **Code quality standards**:
   - No `unwrap()` or `expect()` outside test code - use proper error handling
   - No `println!()` in library code (CLI binaries are OK)
   - All public functions must have doc comments (`///`)
   - All `unsafe` blocks must have a comment explaining why they're safe
   - No `TODO` comments without issue numbers (except in WIP branches)

## Git Configuration

Use SSH for all GitHub operations:
- Clone/push/pull: `git@github.com:bridge-craftwork/repo.git` (not `https://`)
- Remote URLs should use SSH format

## Related Projects

All located alongside this repo, under the same GitHub directory:

| Project | Description | Relationship |
|---------|-------------|--------------|
| [bridge-types](../bridge-types) | Core bridge types | sibling |
| [bridge-solver](../bridge-solver) | Double-dummy solver | sibling |
| [Bridge-Parsers](../Bridge-Parsers) | PBN/LIN file parsing | sibling |
| [pbn-to-pdf](../pbn-to-pdf) | PDF generation | sibling |
| [bridge-wrangler](../bridge-wrangler) | CLI tool for PBN operations | sibling |

## Known Issues

Tracked on GitHub rather than listed here, so this file cannot go stale:
`gh issue list`. One piece of history is worth knowing before touching
anything:

`tricks()` used to take minutes per solve and saturate every core (#14). It now
goes through `bridge-solver` and takes about ten milliseconds, remembered per
deal — so it is safe to put in a probe. `scripts/dd-bench.sh` is the guard.

## Source Material & Reference Implementations

### Original dealer (Hans van Staveren; maintained by Henk Uijterwaal)
**Location**: `../Dealer-cleanup/`

The upstream README opens "Dealer by Hans van Staveren". Henk Uijterwaal
maintained it through the era this code comes from — `pbn.c` is his, and the
binary reports `$Author: henk $`, `$Revision: 1.24 $`, 2003-08-05.

**Key Files**:
- `dealer` - Reference C dealer binary (macOS build)
- `dealer.c` - Main source code
- `scan.l` - Flex lexer for input language
- `defs.y` - Bison parser grammar

**Purpose**:
- Compatibility testing — the script language and the CLI, not the deals
- Reference for ambiguous behavior

### Windows VM Access (for running dealer.exe)

**The host and the login are `ssh_runner`'s business, not yours.** They are not
written down here and are not meant to be read anywhere else — go through
`win-dealer` or `ssh_runner.py`, which already know how to connect. Never put an
address, a username or a hostname into a file in this repository.

**Preferred Method**: Use the shell alias `win-dealer` to run dealer.exe. The
`compare-dealer` alias is **superseded** — see "Testing Against dealer.exe"
below for what replaced it and why.

#### win-dealer - Run dealer.exe on Windows VM
```bash
# Run with a .dlr file (supports relative paths, auto-converts to Windows G: path)
win-dealer -p 10 -s 42 test-data/dlr-test/pruned.dlr

# Pipe conditions via stdin
echo "hcp(north) >= 20" | win-dealer -p 10 -s 1

# With custom timeout (default 10s)
win-dealer -t 60 -p 100 -s 42 large-test.dlr

# Show help
win-dealer -h
```

#### compare-dealer — superseded, do not reach for it
`scripts/compare-dealer.sh` diffed the two binaries' boards one for one. That
only ever worked while `--legacy` could reproduce dealer.exe's deal sequence,
which was removed in 0.5.0, so the diff is now meaningless. The script refuses
to run without `COMPARE_DEALER_FORCE=1` and is kept only in case someone reworks
it to compare through `--input-deals`.

**If the wrappers are not enough**, go through `ssh_runner.py` in
Practice-Bidding-Scenarios' `build-scripts-mac/` rather than calling `ssh`
by hand. It handles the connection, maps the drives (an SSH session does not
inherit them) and converts a Mac path to its Windows one:

```python
from ssh_runner import run_windows_command, mac_to_windows_path
run_windows_command(f"dealer -p 10 -s 42 {mac_to_windows_path(path)}")
```

Write the script under the shared GitHub directory first, so the path converts.

**Notes**:
- The Windows VM has `dealer` in PATH at `C:\Dealer\dealer.exe`
- The shared drive maps the Mac's GitHub directory via Parallels, so a file at
  `$HOME/Development/GitHub/dealer3/foo.dlr` is reachable from Windows
- Shell aliases defined in `~/.zshrc`, scripts in `scripts/` directory

### DealerV2 (Greg Morse's expanded version)
**Location**: `/tmp/dealerv2` (cloned locally)
**GitHub**: https://github.com/dealerv2/Dealer-Version-2-
**Purpose**: Reference for extended features (DDS, CSV export, additional switches)
**Key Files**:
- `src/dealaction_subs.c` - CSV report implementation (ACT_CSVRPT)
- `src/mainsubs.c` - Command-line option parsing (including -C switch)
- `src/*.y` - Yacc grammar for csvrpt() action
- `docs/README_DealerV2.pdf` - 50 page user guide

## Testing Against dealer.exe

### Preferred Method: test-filter.py
The two programs no longer deal the same boards, so nothing is gained by
diffing their output. What still compares cleanly is what a script *means*:

```bash
# Compare the two over the same deals: dealer.exe emits what it produced,
# dealer3 replays it, and the statistics are diffed. This is the strongest
# check there is — averages, frequencies in either dimension and the deal
# layouts all compare exactly, because both are looking at identical cards.
scripts/compare-stats.py -p 200 -s 1 test.dlr

# Have dealer.exe produce the deals a script matches, then check dealer3
# accepts every one of them. Anything less than 100% is a real difference.
scripts/test-filter.py -p 20 -s 1 test.dlr

# Build a committed corpus for the regression tiers
scripts/generate-corpus.py -p 20 -s 1 test.dlr

# Run dealer.exe directly when you need to see its own output
win-dealer -p 10 -s 1 test.dlr
```

See `docs/REGRESSION_TESTING.md` for the tiers and how a corpus is replayed.

### Key Compatibility Tests

**Not deal-for-deal.** `compare-dealer` compared the two binaries' boards
one for one, which needed legacy mode; that went in 0.5.0 and the script with
it. What is compared now is what a script *means*:

1. **Filter semantics** — `scripts/test-filter.py` has dealer.exe produce the
   deals a script matches, then feeds those deals to dealer3 with the same
   script; all of them should pass. It compares what a condition *means*, which
   needs no shared deal sequence
2. **Corpora** — `scripts/generate-corpus.py`, replayed by the regression tiers
3. **Output format** — PBN, printall and the rest must match exactly
4. **Edge cases** — predeal, rare constraints, boundary conditions

## Additional Working Directories

- `../Dealer-cleanup/` - Reference C dealer source and binary
- `/private/tmp` - Temporary workspace for test output and experiments
- this repo - main working directory

## Quick Reference: Version History

- **0.1.0**: Initial release with basic functionality
- **0.2.0** (unreleased): Breaking changes for dealer.exe compatibility
  - `-v` now means verbose (was vulnerability)
  - `--vulnerable` for vulnerability (long form only)
  - Added `-V`, `-q`, `-m` switches
  - Deprecated switch detection

## When Starting a New Session

1. Check `FILTER_LANGUAGE_STATUS.md` for current feature status
2. Check `docs/implementation_roadmap.md` for next priorities
3. Run `cargo test` to verify all tests passing
4. Check git status to see current branch and changes

## Notifications

Send Pushover notifications when work is blocked or completed:

```bash
pushover "message" "title"    # title defaults to "Claude Code"
```

**When to notify:**
- Waiting for user input or permission
- Task completed after extended work
- Build/test failures that need attention
- Any situation where work is paused and user may not notice
