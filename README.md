# dealer3

[![CI](https://github.com/bridge-craftwork/Dealer3/workflows/CI/badge.svg)](https://github.com/bridge-craftwork/Dealer3/actions)
[![License: Unlicense](https://img.shields.io/badge/license-Unlicense-blue.svg)](http://unlicense.org/)

A Rust implementation of the classic dealer.exe bridge hand generator, with full compatibility for dealer.exe and DealerV2_4 enhancements.

## Features

- **dealer.exe Compatible**: Exact RNG compatibility ensures same seed = same deals
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

### Export
- `-C FILE, --CSV FILE` - CSV export file

### Other
- `-m, --progress` - Show progress meter
- `-V, --version` - Show version information
- `-h, --help` - Show help message

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
├── gnurandom/       - GNU/glibc random() RNG implementation
├── docs/            - Documentation
└── scripts/         - Build and utility scripts
```

## Compatibility

### dealer.exe (Thomas Andrews)
✅ **100% RNG compatible** - Same seed produces identical deals
✅ **Full constraint language** - All functions and operators supported
✅ **Command-line switches** - Core switches work identically

### DealerV2_4 (Thorvald Aagaard)
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

The original dealer.exe was also released into the public domain by Thomas Andrews.

## Credits

- **Original dealer.exe**: Thomas Andrews (public domain)
- **DealerV2_4**: Thorvald Aagaard (GPLv3, independent implementation)
- **dealer3**: Rick Wilson (Unlicense)

Key contributors to dealer.exe ecosystem:
- Henk Uijterwaal, Bruce Moore, Francois Dellacherie, Robin Barker, Danil Suits, Alex Martelli, Paul Hankin, and many others

## Links

- **GitHub**: https://github.com/bridge-craftwork/Dealer3
- **Original dealer.exe**: http://www.bridgebase.com/tools/dealer/
- **DealerV2_4**: https://github.com/ed2k/dealer

## Support

For bugs, feature requests, or questions:
- Open an issue on GitHub: https://github.com/bridge-craftwork/Dealer3/issues

---

**Note**: This is an independent implementation. It is not affiliated with BridgeBase Online or the original dealer.exe project, though it maintains full compatibility with dealer.exe for script portability.
