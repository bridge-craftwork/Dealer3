# dealer3

[![CI](https://github.com/bridge-craftwork/Dealer3/workflows/CI/badge.svg)](https://github.com/bridge-craftwork/Dealer3/actions)
[![License: Unlicense](https://img.shields.io/badge/license-Unlicense-blue.svg)](http://unlicense.org/)

A Rust implementation of the classic dealer.exe bridge hand generator. It runs
dealer.exe's scripts and accepts its command line, and supports DealerV2_4
enhancements. It does **not** reproduce dealer.exe's deals: a seed gives you a
dealer3 sequence, not the original's.

## Features

- **dealer.exe Compatible**: Runs dealer.exe scripts unchanged; filter semantics verified against it
- **DealerV2_4 Enhancements**: Command-line predeal switches, CSV export, title metadata
- **Constraint Language**: Full dealer.exe expression language with variables
- **Multiple Output Formats**: PBN, compact, one-line, and more
- **Cross-Platform**: Builds on Linux, macOS, and Windows
- **Public Domain**: Released under The Unlicense

## Quick Start

### Installation

```bash
cargo install --path dealer
```

### Usage

Generate 10 deals where North has 15+ HCP:
```bash
echo "hcp(north) >= 15" | dealer -p 10
```

Use a constraint file:
```bash
dealer -p 10 < constraints.dl
```

Predeal specific cards (DealerV2_4 format):
```bash
echo "hcp(north) >= 0" | dealer -E S8743,HA9,D642,CQT64 -W SQ965,HK63,DAQJT,CA5 -p 5
```

## Command-Line Options

### Core Generation
- `-p N, --produce N` - Produce N matching deals (default: 40)
- `-g N, --generate N` - Generate N total deals (default: 1,000,000)
- `-s N, --seed N` - Random seed for reproducible results

### Output Format
- `-f FORMAT, --format FORMAT` - Output format: printall, printew, printpbn, printcompact, printoneline
- `-q, --quiet` - Suppress deal output, only show statistics
- `-v, --verbose` - Show statistics at end

### Predeal (DealerV2_4 Compatible)
- `-N CARDS, --north CARDS` - Predeal cards to North (e.g., SAKQ,HA)
- `-E CARDS, --east CARDS` - Predeal cards to East
- `-S CARDS, --south CARDS` - Predeal cards to South
- `-W CARDS, --west CARDS` - Predeal cards to West

### PBN Options
- `-d POS, --dealer POS` - Dealer position (N/E/S/W)
- `--vulnerable VULN` - Vulnerability (None/NS/EW/All)
- `-T TEXT, --title TEXT` - Title metadata for PBN output

### Deal Input
- `--input-deals SOURCE` - Read deals from a file instead of generating them; use `-` for stdin
- `--input-offset N` - Start a solved-deal library at record N (records, separators included)
- `--input-limit N` - Read this many deals from it, wrapping round the file when it runs short

### Export
- `-C FILE, --CSV FILE` - CSV export file

### Other
- `-m, --progress` - Show progress meter
- `-V, --version` - Show version information
- `-h, --help` - Show help message

## Filtering Existing Deals

`--input-deals` reads deals from a file instead of generating them, then applies the
script's constraints as usual. The format is detected from the content: a Pavlicek
`.zrd` library, PBN, oneline or printall, whatever the file is called.

```bash
# Filter an existing PBN file
dealer filter.dlr --input-deals hands.pbn -f pbn

# Read deals from stdin (script must be a file argument, since stdin is taken)
cat hands.pbn | dealer filter.dlr --input-deals - -f oneline

# A binary library reads the same way, so fetching one needs no HTTP client here
curl -s https://example.org/deals.zrd | dealer filter.dlr --input-deals -

# Forty deals out of a ten-million-record library, from a reproducible place in it
dealer filter.dlr --input-deals rpdd.zrd -s 42 -g 5000

# Check how many deals in a file satisfy a constraint
echo "hcp(north) >= 15" > strong.dlr
dealer strong.dlr --input-deals hands.pbn -q -X
```

This makes filter behaviour reproducible independently of the RNG, which is how
dealer3's regression tests compare constraint evaluation against dealer.exe.

Notes:

- `--seed` picks where in a **solved-deal library** to start reading, so the same
  seed reads the same deals there, exactly as it deals the same deals when
  generating. For PBN and one-line files it is ignored — those are read in the
  order somebody wrote them.
- `--input-offset` names a starting record outright instead. It counts *records*,
  separators included, because that is what the file numbers; `--input-limit`
  counts deals, because deals are what a run spends. Past the end of the file
  wraps round to the beginning.
- `--input-limit` larger than the library repeats deals, and `average` and
  `frequency` then count each repeat — a sample with replacement rather than a
  bigger sample. The run says so when it happens. `-g` on its own never repeats:
  it is a ceiling on what gets read, not a demand.
- It cannot be combined with predeal, since predeal only applies to generation.
- `-p` and `-g` still apply: `-p` stops once that many deals match, `-g` caps how many
  are read. Running out of input before `-p` is satisfied is not an error.
- Lines that are not recognised as deals are ignored, so PBN metadata and previous
  stats output can be piped straight in. Check the reported `Generated N hands` count
  to confirm every deal you expected was actually read.
- A file whose name disagrees with its content — a `.zrd` holding PBN, or the reverse
  — is read for what it holds, and the disagreement is reported.

## Constraint Language

dealer3 supports the full dealer.exe constraint language:

### Functions
- `hcp(HAND)` - High card points
- `shape(HAND, S-H-D-C)` - Exact shape (e.g., shape(north, 4432))
- `spades(HAND)`, `hearts(HAND)`, `diamonds(HAND)`, `clubs(HAND)` - Suit lengths
- `hascard(HAND, CARD)` - Check for specific card

### Keywords
- `condition` - Main constraint expression
- `action` - Output action (printall, printew, printpbn, etc.)
- `produce N` - Number of deals to produce
- `dealer POSITION` - Dealer position
- `vulnerable TYPE` - Vulnerability
- `predeal POSITION CARDS` - Predeal cards
- `average EXPR` - Calculate average over matching deals
- `frequency EXPR` - Generate frequency distribution

### Example Constraint File

```
// Strong NT opening
opener = hcp(north) >= 15 and hcp(north) <= 17
balanced = shape(north, 4432) or shape(north, 4333) or shape(north, 5332)

condition opener and balanced
produce 20
action printpbn
dealer north
vulnerable none
```

## Building

### Requirements
- Rust 1.70 or later
- Cargo

### Build from Source

```bash
# Clone the repository
git clone https://github.com/bridge-craftwork/Dealer3.git
cd Dealer3

# Build all crates
cargo build --release

# Run tests
cargo test --workspace

# Install
cargo install --path dealer
```

### Windows Cross-Compilation (from macOS/Linux)

```bash
./scripts/windows/build-windows.sh
```

See [docs/BUILDING_WINDOWS.md](docs/BUILDING_WINDOWS.md) for details.

## Project Structure

```
dealer3/
├── dealer/          - Main CLI binary
├── dealer-core/     - Deal generation and card logic
├── dealer-parser/   - Constraint language parser (PEG grammar)
├── dealer-eval/     - Expression evaluator
├── dealer-pbn/      - PBN format I/O
├── docs/            - Documentation
└── scripts/         - Build and utility scripts
```

## Compatibility

### dealer.exe
✅ **Full constraint language** - All functions and operators supported
✅ **Command-line switches** - Core switches work identically
✅ **Filter semantics verified** - checked against dealer.exe by the Tier 1
   regression corpora (see [Regression Testing](docs/REGRESSION_TESTING.md))

⚠️ **Deal sequences differ.** dealer3 uses xoshiro256++, not the GNU `random()`
of the original. The same seed does *not* reproduce dealer.exe's deals. Scripts
port unchanged; specific deals do not. See the [CHANGELOG](docs/CHANGELOG.md).

### DealerV2_4 (Greg Morse)
✅ **Predeal switches** - `-N/-E/-S/-W` for command-line predeal
✅ **CSV export** - `-C` for analytics output
✅ **Title metadata** - `-T` for PBN output

## Documentation

- [CHANGELOG](docs/CHANGELOG.md) - Version history and breaking changes
- [Filter Language Status](docs/FILTER_LANGUAGE_STATUS.md) - Feature implementation status
- [Command-Line Switches](docs/command_line_switch_requirements.md) - Switch compatibility
- [Building for Windows](docs/BUILDING_WINDOWS.md) - Cross-compilation guide
- [Implementation Roadmap](docs/implementation_roadmap.md) - Future features

## Testing

Run the test suite:
```bash
cargo test --workspace
```

Compare output with dealer.exe:
```bash
# Generate test file
echo "hcp(north) >= 15" > test.dl

# Test dealer3
cat test.dl | ./target/release/dealer -s 1 -p 10 > dealer3.out

# Compare with dealer.exe (if available)
cat test.dl | dealer.exe -s 1 -p 10 > dealer.out
diff dealer3.out dealer.out
```

## Performance

dealer3 generates millions of deals per second with efficient constraint evaluation:
- Fast deal generation using optimized RNG
- Low memory footprint
- Efficient constraint evaluation

## License

This project is released into the **public domain** under [The Unlicense](LICENSE).

You are free to use, modify, distribute, and incorporate this software for any purpose, with or without modification, with no restrictions.

The original dealer was written by Hans van Staveren and dedicated to the public
domain.

## Credits

- **Original dealer**: Hans van Staveren (public domain)
- **DealerV2_4**: Greg Morse, with Thorvald Aagaard contributing (GPLv3, independent implementation)
- **dealer3**: Rick Wilson (Unlicense)

### Third-party work dealer3 builds on

**Richard Pavlicek** — the solved-deal library and the binary formats that carry
it. `rpdd.zrd` holds 10,485,760 random deals with the complete twenty-cell
double-dummy table for each, published at
[rpbridge.net](http://www.rpbridge.net/) and © 2007 Richard Pavlicek. His
`rpdd.txt` records that solving them took almost two years of computer time.
dealer3 reads that format (`--input-deals foo.zrd`), so `tricks()`, `dds()` and
`par()` become lookups rather than searches, and understands the companion
`.zdd` and the deal-generation scheme that rebuilds the deals from it.

*dealer3 ships none of this data.* The library is a courtesy download from its
author and carries no redistribution grant; obtain it from
[rpbridge.net](http://www.rpbridge.net/) yourself. DealerV2_4 reads the same
format with its `-L` switch.

[`THIRD-PARTY-NOTICES`](THIRD-PARTY-NOTICES) lists every package a distributed
copy is built from, with its licence and copyright, and ships in the release
archives. `web/public/THIRD-PARTY-NOTICES` is the same for the WebAssembly build,
which is a different graph — it links wasm-bindgen and js-sys and never sees
clap. Both are generated from what cargo resolved, by
`scripts/third-party-notices.py`, and checked in CI: a hand-kept notice file goes
stale the moment a dependency moves, and a stale notice asserts something untrue
about what a copy contains. Nothing here pins dependency versions, so the files
carry none — and they only ever grow, because a machine with an older lock and a
fresh CI checkout legitimately resolve different sets. CI fails only when
something resolved that the file does not credit, which is the direction that
matters.

**Hanhong Xue** — the double-dummy search core. dealer3 solves through
[bridge-solver](https://github.com/bridge-craftwork/bridge-solver), whose search
is a Rust reimplementation of
[macroxue/bridge-solver](https://github.com/macroxue/bridge-solver), © 2013-2026
Hanhong Xue, dual-licensed Apache-2.0 or MIT. dealer3's own code is public
domain, but binaries and the WebAssembly build link that work, so those licences
travel with anything distributed.

### Key contributors to the dealer ecosystem

- Henk Uijterwaal, who maintained dealer through the era this work is based on
  and wrote its PBN support
- Bruce Moore, Francois Dellacherie, Robin Barker, Danil Suits, Alex Martelli,
  Paul Hankin, and many others

## Links

- **GitHub**: https://github.com/bridge-craftwork/Dealer3
- **Original dealer.exe**: http://www.bridgebase.com/tools/dealer/
- **DealerV2_4**: https://github.com/dealerv2/Dealer-Version-2-
- **Pavlicek's solved deals**: http://www.rpbridge.net/

## Support

For bugs, feature requests, or questions:
- Open an issue on GitHub: https://github.com/bridge-craftwork/Dealer3/issues

---

**Note**: This is an independent implementation. It is not affiliated with
BridgeBase Online or the original dealer project, though it keeps compatibility
with dealer.exe's script language and command line so that scripts are portable.
Deal sequences are not portable and are not meant to be.
