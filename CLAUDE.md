# dealer3 Project Context

## Project Overview

dealer3 is a Rust implementation of dealer.exe (bridge hand generator) with full compatibility for the original dealer.exe command-line interface and support for DealerV2_4 enhancements.

**Key Achievement**: Phase 0 and Phase 1 complete - dealer3 is now **fully compatible** with essential dealer.exe command-line behavior!

## Current Status

- **Version**: 0.2.0 (unreleased, pre-1.0)
- **Last Updated**: 2026-01-01
- **Phase 0**: ✅ COMPLETE (Breaking changes for dealer.exe compatibility)
- **Phase 1**: ✅ COMPLETE (Essential dealer.exe switches)
- **Phase 2**: 🚧 IN PROGRESS (DealerV2_4 enhancements - CSV export complete)

## Architecture

### Crate Structure
```
dealer3/
├── gnurandom/          - Exact dealer.exe RNG implementation (64-bit state)
├── dealer-core/        - Deal generation, hand analysis (HCP, controls, shape)
├── dealer-pbn/         - PBN format I/O
├── dealer-parser/      - Constraint language parser (pest PEG grammar)
├── dealer-eval/        - Expression evaluator (variables, functions, operators)
└── dealer/             - CLI application (main binary)
```

### Key Design Decisions

1. **RNG Compatibility**: Uses exact GNU random() with 64-bit state matching dealer.exe binary
   - **Critical Discovery**: dealer.exe uses 64-bit arithmetic throughout (not 32-bit!)
   - Replicated via reverse-engineering using RNG probe tools
   - State array: `i64[31]` with BSD TYPE_3 polynomial (x^31 + x^3 + 1)
   - Non-standard LCG constant: `1103515145` (from Thorvald's dealer source)
   - 64-bit arithmetic shift (sarq) + 64-bit LONG_MAX mask creates unique negative states
   - 310-iteration warmup phase (10 * rand_deg)
   - **Verified**: Seed 1 produces first value 269167349 (matches dealer.exe exactly)
2. **Parse Once, Evaluate Many**: AST is Clone + Send + Sync for efficient parallel evaluation
3. **Breaking Change (0.2.0)**: `-v` changed from vulnerability to verbose (matches dealer.exe)
   - Use `--vulnerable` (long form only) for vulnerability
4. **Deprecated Switches**: Parse and show helpful errors for `-2`, `-3`, `-e`, `-u`, `-l`

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
- ✅ `-2`, `-3`, `-e`, `-u`, `-l` - Deprecated switches (helpful error messages)

### Filter Language Features
- ✅ **Functions**: hcp, controls, shape, hearts, spades, diamonds, clubs, losers, suit_quality, cccc
- ✅ **Operators**: Arithmetic (+, -, *, /, %), Comparison (==, !=, <, <=, >, >=), Logical (&&, ||, !), Ternary (? :)
- ✅ **Keywords**: condition, produce, generate, action (printall/printew/printpbn/printcompact/printoneline), dealer, vulnerable, predeal, average, frequency
- ✅ **Variables**: Assignment and lookup (e.g., `opener = hcp(north) >= 15`)
- ✅ **Predeal**: Assign specific cards before shuffling (matches dealer.exe exactly)
- ✅ **Average/Frequency**: Calculate statistics over matching deals

## Important Files to Know

### Documentation (Always Check These First!)
- `docs/FILTER_LANGUAGE_STATUS.md` - Complete feature implementation status
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
- `gnurandom/src/lib.rs` - dealer.exe-compatible RNG

### Tests
- `cargo test` - Run all tests (118 passing)
- All crates have comprehensive test coverage

## Common Tasks

### Building and Testing
```bash
cargo build --release          # Build all crates
cargo test                     # Run all tests
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

## Next Steps (Phase 2)

Priority features for next implementation:
1. Compass predeal switches (`-N/E/S/W CARDS`)
2. CSV export (`-C FILE`)
3. Title metadata (`-T "text"`)
4. BBO strict mode (`--bbo-strict`)

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

All located at `/Users/rick/Development/GitHub/`:

| Project | Description | Relationship |
|---------|-------------|--------------|
| [bridge-types](../bridge-types) | Core bridge types | sibling |
| [bridge-solver](../bridge-solver) | Double-dummy solver | sibling |
| [Bridge-Parsers](../Bridge-Parsers) | PBN/LIN file parsing | sibling |
| [pbn-to-pdf](../pbn-to-pdf) | PDF generation | sibling |
| [bridge-wrangler](../bridge-wrangler) | CLI tool for PBN operations | sibling |

## Known Issues

1. ⚠️ Warning: unused function `vulnerability_type_to_vulnerability` in main.rs (cleanup needed)
2. Statistics always shown even without `-v` (minor, user-friendly behavior)

## Source Material & Reference Implementations

### Original dealer.exe (Henk Uijterwaal)
**Location**: `/Users/rick/Development/GitHub/Dealer-cleanup/`

**Key Files**:
- `dealer` - Reference C dealer binary (macOS build)
- `dealer.c` - Main source code
- `scan.l` - Flex lexer for input language
- `defs.y` - Bison parser grammar

**Purpose**:
- Compatibility testing - our output must match exactly
- Reference for ambiguous behavior
- RNG verification (we matched the 64-bit state implementation)

### Windows VM Access (for running dealer.exe)
**IP Address**: `10.211.55.5`
**Username**: `rick`

**Preferred Method**: Use the shell aliases `win-dealer` and `compare-dealer` for all Windows dealer.exe testing.

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

#### compare-dealer - Compare dealer3 vs dealer.exe output
```bash
# Compare output from both dealers (uses development build by default)
compare-dealer -p 10 -s 1 test.dlr

# Pipe conditions via stdin
echo "shape(north, any 6xxx)" | compare-dealer -p 10 -s 1

# Show raw output from both runs
compare-dealer -p 10 -s 1 -o test.dlr

# Skip pretest (pretest runs -p 1 first to quickly detect failures)
compare-dealer --no-pretest -p 100 -s 1 test.dlr

# Use a specific Rust binary instead of development build
compare-dealer -r /path/to/dealer -p 10 -s 1 test.dlr

# Show help
compare-dealer -h
```

**Manual SSH** (only if scripts don't work):
```bash
# Map G: drive and run dealer (drive mapping required each session)
ssh rick@10.211.55.5 'net use G: "\\Mac\Home\Development\GitHub" >nul 2>&1 & dealer -p 10 -s 42 G:\dealer3\test.dlr'

# Simple inline expressions (no drive mapping needed)
ssh rick@10.211.55.5 "echo 'hcp(north) >= 20' | dealer -p 10 -s 1"
```

**Notes**:
- The Windows VM has `dealer` in PATH at `C:\Dealer\dealer.exe`
- G: drive maps to `/Users/rick/Development/GitHub` via Parallels shared folders
- Files at `/Users/rick/Development/GitHub/dealer3/foo.dlr` become `G:\dealer3\foo.dlr`
- Shell aliases defined in `~/.zshrc`, scripts in `scripts/` directory

### DealerV2 (Hans van Staveren, expanded version)
**Location**: `/tmp/dealerv2` (cloned locally)
**GitHub**: https://github.com/dealerv2/Dealer-Version-2-
**Purpose**: Reference for extended features (DDS, CSV export, additional switches)
**Key Files**:
- `src/dealaction_subs.c` - CSV report implementation (ACT_CSVRPT)
- `src/mainsubs.c` - Command-line option parsing (including -C switch)
- `src/*.y` - Yacc grammar for csvrpt() action
- `docs/README_DealerV2.pdf` - 50 page user guide

## Testing Against dealer.exe

### Preferred Method: compare-dealer
Use `compare-dealer` for all compatibility testing. It automatically:
- Runs both Rust dealer3 and Windows dealer.exe with the same arguments
- Compares deals, produced count, and generated count
- Runs a quick pretest (-p 1) to catch failures fast
- Shows timing comparison and warns if Rust is significantly slower

```bash
# Basic comparison
compare-dealer -p 10 -s 1 test.dlr

# With stdin input
echo "hcp(north) >= 20" | compare-dealer -p 10 -s 1

# Show raw output to debug differences
compare-dealer -p 10 -s 1 -o test.dlr
```

### Key Compatibility Tests
1. **RNG sequence** - Same seed must produce identical deals
2. **Output format** - PBN, printall, etc. must match exactly
3. **Edge cases** - Predeal, rare constraints, boundary conditions
4. **Statistics** - Generated/produced counts must match

## Additional Working Directories

- `/Users/rick/Development/GitHub/Dealer-cleanup/` - Reference C dealer source and binary
- `/private/tmp` - Temporary workspace for test output and experiments
- `/Users/rick/Development/GitHub/dealer3/` - This project (main working directory)

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
