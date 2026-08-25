# Changelog

All notable changes to dealer3 will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `--input-deals -` now reads deals from stdin, so deals can be piped in without a
  temporary file. Requires the script to be passed as a file argument, since stdin is
  otherwise consumed by the script itself; a clear error is emitted if both would
  contend for stdin.
- Integration test coverage for `--input-deals` (PBN, oneline, stdin, limits, conflicts)

### Changed
- Unreadable deals encountered while reading `--input-deals` are now skipped with a
  warning and a total reported at exit, rather than aborting the run
- Documented `--input-deals` in the README and CLI design notes

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
