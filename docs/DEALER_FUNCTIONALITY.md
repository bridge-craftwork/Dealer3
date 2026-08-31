# Dealer - Bridge Hand Generator

## Overview

Dealer is a command-line tool for generating bridge deals (hands) that match specific constraints. It uses Monte Carlo simulation to randomly generate deals until finding ones that satisfy user-defined conditions.

## Core Functionality

### Basic Operation
1. **Input**: Reads a formatted input file containing constraints
2. **Processing**: Randomly generates bridge deals using the dealer.exe-compatible RNG
3. **Evaluation**: Tests each generated deal against the constraints
4. **Output**: Writes matching deals to an output file

### Constraint Language

Constraints are written in a domain-specific language that allows complex expressions:

**Example Constraints:**
```
hearts(north) == 5 && hcp(south) <= 13
spades(north) >= 4 && hcp(north) >= 15
```

**Constraint Specification:**
https://www.bridgebase.com/tools/dealer/Manual/input.html

The language supports:
- Hand evaluation functions (hcp, hearts, spades, clubs, diamonds, etc.)
- Logical operators (&&, ||, !)
- Comparison operators (==, !=, <, >, <=, >=)
- Arithmetic expressions
- Custom shape and distribution checks

### Command Line Interface

#### Generation Modes

**Generate Mode (`generate m`):**
- Generate exactly `m` random deals
- Report all deals that match the constraints
- Use case: Statistical analysis, frequency studies

**Produce Mode (`produce n`):**
- Generate as many deals as needed to find `n` matching deals
- Stop after finding `n` successful matches
- Use case: Finding specific example hands

#### Example Usage
```bash
dealer -g 10000 < input.txt > output.txt    # Generate 10,000 deals
dealer -p 100 < input.txt > output.txt      # Produce 100 matching deals
```

## Technical Requirements

### Platform Support
- **macOS** (primary development target)
- **Windows** (cross-compilation)
- **Linux** (cross-compilation)

Distribute as native executables for each platform.

### Performance Goals

**Parallelism Strategy:**
- Parallelize constraint evaluation across deals
- Each thread can generate and evaluate deals independently
- Aggregate results from parallel workers
- Target: Utilize all available CPU cores efficiently

**RNG Compatibility:**
- Use xoshiro256++ (`dealer-core/src/rng.rs`) for deal generation
- Maintain reproducibility with seed support for testing

### Compatibility Requirements

**Compatibility with dealer.exe:**
- Accept its scripts and its command line unchanged
- Maintain existing input file format and syntax
- Preserve output format for downstream tools
- **Not** its deal sequence: a seed reproduces a dealer3 run, not a dealer.exe
  one. That went with legacy mode in 0.5.0; see `docs/REGRESSION_TESTING.md`
  for how filter semantics are checked without it

**Version Compatibility Flag:**
- Filter semantics verified against dealer.exe via the Tier 1 regression corpora
- Default mode: Allow new features from DealerV2_4

## Source Code References

### Original Dealer
**Repository:** https://github.com/ThorvaldAagaard/Dealer
**Language:** C
**Key Components:**
- Parser for constraint language
- Deal generation and evaluation engine
- Output formatting

### DealerV2_4 (Enhanced Version, Greg Morse)
**Repository:** https://github.com/dealerv2/Dealer-Version-2-
**Mirror:** https://github.com/ThorvaldAagaard/DealerV2_4
**Additional Features:**
- Extended constraint functions
- Additional output formats
- Performance improvements
- Enhanced statistical analysis

## Architecture Goals

### Rust Implementation Benefits
1. **Memory Safety**: Eliminate C-style memory bugs
2. **Concurrency**: Safe parallelism with Rust's ownership model
3. **Cross-Platform**: Single codebase for all platforms
4. **Performance**: Match or exceed C performance
5. **Maintainability**: Modern tooling and package management

### Modular Design

```
dealer3/
├── dealer-parser/       # Input file parser and constraint AST
├── dealer-eval/         # Deal evaluation engine
├── dealer-core/         # Deal generation and core logic
└── dealer/              # CLI application and main binary
```

### Key Components to Implement

1. **Parser** (`dealer-parser`)
   - Lexer for constraint language
   - Parser producing AST (Abstract Syntax Tree)
   - Support full dealer.exe syntax
   - Extensible for DealerV2_4 features

2. **Evaluator** (`dealer-eval`)
   - AST interpreter/evaluator
   - Built-in functions (hcp, hearts, etc.)
   - Shape analysis functions
   - Custom user-defined functions (future)

3. **Deal Generator** (`dealer-core`)
   - Bridge deal representation (52 cards, 4 hands)
   - Deal generation using xoshiro256++
   - Seeded generation for reproducibility
   - Efficient card shuffle algorithms

4. **Parallel Engine**
   - Worker thread pool
   - Deal generation and evaluation pipeline
   - Result aggregation
   - Progress reporting

5. **CLI** (`dealer`)
   - Argument parsing (-g, -p, --seed, etc.)
   - Input/output file handling
   - Statistics and reporting
   - Error handling and user feedback

## Migration Strategy

### Phase 1: Foundation (Current)
- [x] xoshiro256++ RNG in dealer-core
- [ ] Deal representation and basic generation
- [ ] Simple constraint parser (subset of syntax)

### Phase 2: Core Functionality
- [ ] Full constraint parser
- [ ] Complete evaluator implementation
- [ ] Serial (single-threaded) execution
- [ ] Verify output matches dealer.exe

### Phase 3: Performance
- [ ] Parallel evaluation engine
- [ ] Performance benchmarking
- [ ] Optimization for hot paths

### Phase 4: Extended Features
- [ ] DealerV2_4 enhanced functions
- [ ] Additional output formats
- [ ] Statistical analysis features
- [ ] Compatibility mode flag

### Phase 5: Distribution
- [ ] Cross-compilation setup
- [ ] Binary releases for all platforms
- [ ] Documentation and examples
- [ ] Migration guide for dealer.exe users

## Success Criteria

1. **Correctness**: Filter semantics match dealer.exe (verified by regression corpora)
2. **Performance**: >= dealer.exe speed in serial, significant speedup with parallelism
3. **Usability**: Drop-in replacement for dealer.exe users
4. **Extensibility**: Easy to add DealerV2_4 features
5. **Cross-Platform**: Native binaries for macOS, Windows, Linux

## Open Questions

1. Should we support streaming output for long-running generate operations?
2. What level of backward compatibility is required for error messages?
3. Should we add a REPL/interactive mode for constraint development?
4. What output formats beyond standard text (JSON, CSV, etc.)?
5. Should parallelism be automatic or user-configurable (e.g., --threads N)?

## Next Steps

1. Analyze dealer.c constraint parser to understand full syntax
2. Design AST representation for constraints
3. Implement basic deal generation and card representation
4. Build minimal parser for simple constraints
5. Create end-to-end prototype with serial execution
