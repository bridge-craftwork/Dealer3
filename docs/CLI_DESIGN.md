# Dealer CLI Design

Command line interface design based on original dealer.c implementation.

## Command Line Syntax

```bash
dealer [OPTIONS] [inputfile]
```

## Options

### Generation Control

**`-g N`** - Generate mode
- Generate exactly N deals
- Report all deals that match constraints
- Default: 10,000,000
- Use case: Statistical analysis, frequency studies
- Example: `dealer -g 100000 < constraints.txt`

**`-p N`** - Produce mode
- Generate as many deals as needed to find N matching deals
- Stop after finding N successful matches
- Default: 40 (or maxgenerate if specified)
- Use case: Finding specific example hands
- Example: `dealer -p 100 < constraints.txt`

**`-s SEED`** - Random seed
- Set the random number generator seed
- Enables reproducible deal generation
- Default: Current time (non-deterministic)
- Example: `dealer -s 12345 -p 10 < constraints.txt`

### Output Control

**`-v`** - Verbose mode
- Toggle verbose output
- Shows additional information during generation
- Default: Off

**`-q`** - Quiet mode
- Suppress progress/status messages
- Only show final results
- Default: Off

**`-m`** - Progress meter
- Toggle progress meter display
- Shows generation progress during long runs
- Default: Off (or On, depending on compile options)

**`-u`** - Uppercase cards
- Use uppercase letters for card representation
- Example: "AH" instead of "Ah"
- Default: Off

### Special Modes

**`-0`, `-2`, `-3`** - Swap modes
- Enable different card swapping algorithms (0, 2, or 3)
- Used for specific deal generation patterns
- Default: None

**`-e`** - Exhaustive mode
- Enable exhaustive generation (if compiled with FRANCOIS support)
- Generates all possible deals matching constraints
- Default: Off

**`-l N`** - Load from library
- Load hands from library file at index N
- Pre-generated deal library feature
- Default: None

**`--input-deals SOURCE`** - Read deals instead of generating them
- Reads deals from `SOURCE` and applies the script's constraints to each
- PBN and oneline formats are auto-detected
- `-` reads from stdin; the script must then be given as a file argument, since
  stdin is otherwise consumed by the script itself
- `--seed` is ignored, and predeal is rejected as a conflict — both only apply to
  generation
- `-p` stops after that many matches; `-g` caps how many deals are read; exhausting
  the input before `-p` is satisfied exits cleanly
- Unrecognised lines are skipped, so PBN metadata and stats output can be piped in
  directly. Because unreadable content is indistinguishable from metadata, callers
  needing certainty should compare the reported deal count against an expected total
- Default: Off (deals are generated)

### Help & Info

**`-h`, `-?`** - Help
- Display usage information and exit

**`-V`** - Version
- Display version information and exit

## Input File

**Positional argument**: `[inputfile]`
- Path to constraint definition file (.dlr file)
- If omitted, reads from stdin
- Contains dealer constraint language syntax

Example:
```bash
dealer -p 100 constraints.dlr
dealer -g 10000 < myfile.dlr
echo "hcp(north) >= 15" | dealer -p 10
```

## Default Behavior

When run with no arguments:
```bash
dealer
```
- Reads constraints from stdin
- Uses maxgenerate = 10,000,000
- Uses maxproduce = 40
- Random seed from current time
- Reports matching deals to stdout

## Common Usage Patterns

### Generate 1000 deals, show all matches
```bash
dealer -g 1000 < strong_hands.dlr
```

### Find 50 specific deals
```bash
dealer -p 50 < notrump_opener.dlr
```

### Reproducible generation with seed
```bash
dealer -s 42 -p 100 < test.dlr
```

### Quiet mode for scripting
```bash
dealer -q -p 1000 < constraints.dlr > output.txt
```

### Statistical analysis with large sample
```bash
dealer -g 1000000 < shape_study.dlr
```

## Implementation Notes

### Priority
1. If `-p` is specified, use produce mode (stop after N matches)
2. If `-g` is specified, use generate mode (generate exactly N deals)
3. If neither, use default maxgenerate

### Seed Handling
```c
// If no seed specified
seed = time(NULL);
srandom(seed);
```

### Input Processing
```c
// If inputfile specified
yyin = fopen(inputfile, "r");

// If not specified
yyin = stdin;
```

## Output Format

The dealer program outputs matching deals in various formats:
- Human-readable text format
- PBN (Portable Bridge Notation) format
- Statistics and frequency analysis
- Custom action-based output

## Exit Codes

- 0: Success
- Non-zero: Error (file not found, parse error, etc.)

## Compatibility Goals

Our Rust implementation should:
1. ✅ Support `-g` and `-p` modes (core requirement)
2. ✅ Support `-s` for reproducible testing
3. ✅ Read from stdin or file
4. ✅ Use same default values
5. ⚠️  Extended features (-0, -2, -3, -e, -l) can be deferred
6. ✅ Add `--legacy` flag for strict dealer.exe compatibility
7. ✅ Add modern features (parallel processing, JSON output, etc.)

## Proposed Extensions

Beyond original dealer, we could add:
- `--threads N` - Number of parallel worker threads
- `--json` - Output in JSON format
- `--format <fmt>` - Specify output format (text, pbn, json, csv)
- `--stats` - Show generation statistics
- `--timeout <seconds>` - Abort after time limit
- `--legacy` - 100% dealer.exe compatible mode
- `--verbose-errors` - Detailed constraint evaluation errors
