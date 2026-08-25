# Dealer Implementation Status

**Last Updated:** 2026-01-02

## Overview

This document tracks the implementation status of the dealer constraint language, including both filter functions and action keywords.

### Quick Summary

**✅ Core Features Working:**
- 25 filter functions (hcp, suits, controls, losers, shape, hascard, tens, jacks, queens, kings, aces, top2-5, c13, quality, cccc, **tricks, score, imps**)
- **Double-dummy analysis** via built-in alpha-beta solver (~1.4 deals/second)
- **Contract scoring** with full support for vulnerability, doubles, and slams
- **IMP conversion** using standard IMP table
- All arithmetic, comparison, and logical operators (including ternary `?:` and logical NOT `!`/`not`)
- Shape pattern matching (exact, wildcard, any distribution)
- Card and suit literals
- Alternative point counts (pt0-pt9)
- Hand quality evaluation (quality, cccc)
- Variables (full support for assignments and references)
- **Produce mode (`-p N`)** - stop after producing N matching deals (default: 40)
- **Generate mode (`-g N`)** - generate N total deals, report all matches (default: 10,000,000)
- Seeded generation (`-s SEED`)
- **Action keywords (`condition`, `produce`, `action`, `dealer`, `vulnerable`)**
- **Print formats (printall, printew, printpbn, printcompact, printoneline)**
- **Duration logging (performance tracking)**

**Test Status:** 130+ tests passing across all crates
- dealer-core: 20 tests (including 7 predeal tests)
- dealer-core shuffle: 7 tests
- dealer-dds: 6 tests
- dealer-eval: 57 tests (including tricks, score, imps)
- dealer-parser: 23 tests
- dealer-pbn: 16 tests

---

## Language Features

### ✅ **User-Defined Expressions (Variables)**

The dealer language supports defining reusable expressions:

```
nt_opener = hcp(north) >= 15 && hcp(north) <= 17 && shape(north, any 4333 + any 4432 + any 5332)
weak_hand = hcp(south) <= 8
nt_opener && weak_hand
```

**Implementation Model**: Variables are **runtime-evaluated**, not macros. Each variable reference is evaluated in the context of the current deal being analyzed, not textually expanded during parsing. This allows variables to dynamically respond to different hands.

**Status**: ✅ **Fully implemented**
- `Expr::Variable(String)` variant in AST
- `Program` and `Statement` types for multi-statement support
- Symbol table in evaluator (HashMap<String, Expr>)
- Variable lookup during expression evaluation (evaluates stored expression each time)
- CLI parses full programs with `parse_program()`, not just single expressions
- Supports variables referencing other variables (recursive evaluation)

**Example Usage**:
```bash
# Simple variable
printf "opener = hcp(north) >= 15\nopener" | dealer -p 10

# Multiple variables
printf "strong = hcp(north) >= 15\nlong_hearts = hearts(north) >= 5\nstrong && long_hearts" | dealer -p 10

# Variables can reference other variables
printf "points = hcp(north)\nopener = points >= 15\nopener" | dealer -p 10
```

---

## Filter Functions (Constraints)

### ✅ **Implemented**

| Function | Description | Status |
|----------|-------------|--------|
| `hcp(position)` | High card points (4-3-2-1) | ✅ Working |
| `hearts(position)` | Number of hearts | ✅ Working |
| `spades(position)` | Number of spades | ✅ Working |
| `diamonds(position)` | Number of diamonds | ✅ Working |
| `clubs(position)` | Number of clubs | ✅ Working |
| `controls(position)` | Control count (A=2, K=1) | ✅ Working |
| `losers(position)` | Total loser count in hand | ✅ Working |
| `losers(position, suit)` | Losers in specific suit | ✅ Working |
| `shape(position, pattern)` | Shape specification | ✅ Working |
| `hascard(position, card)` | Check for specific card | ✅ Working |
| `tens(position)` | Number of tens (pt0) | ✅ Working |
| `tens(position, suit)` | Tens in specific suit | ✅ Working |
| `jacks(position)` | Number of jacks (pt1) | ✅ Working |
| `jacks(position, suit)` | Jacks in specific suit | ✅ Working |
| `queens(position)` | Number of queens (pt2) | ✅ Working |
| `queens(position, suit)` | Queens in specific suit | ✅ Working |
| `kings(position)` | Number of kings (pt3) | ✅ Working |
| `kings(position, suit)` | Kings in specific suit | ✅ Working |
| `aces(position)` | Number of aces (pt4) | ✅ Working |
| `aces(position, suit)` | Aces in specific suit | ✅ Working |
| `top2(position)` | Top 2 honors AK (pt5) | ✅ Working |
| `top2(position, suit)` | Top 2 in specific suit | ✅ Working |
| `top3(position)` | Top 3 honors AKQ (pt6) | ✅ Working |
| `top3(position, suit)` | Top 3 in specific suit | ✅ Working |
| `top4(position)` | Top 4 honors AKQJ (pt7) | ✅ Working |
| `top4(position, suit)` | Top 4 in specific suit | ✅ Working |
| `top5(position)` | Top 5 honors AKQJT (pt8) | ✅ Working |
| `top5(position, suit)` | Top 5 in specific suit | ✅ Working |
| `c13(position)` | C13 points A=6,K=4,Q=2,J=1 (pt9) | ✅ Working |
| `c13(position, suit)` | C13 points in specific suit | ✅ Working |
| `quality(position, suit)` | Suit quality metric | ✅ Working |
| `cccc(position)` | CCCC hand evaluation | ✅ Working |

**Alternative Point Counts (pt0-pt9):**
The dealer language provides 10 alternative point count functions with readable synonyms:
- `pt0` / `tens` - Count of tens
- `pt1` / `jacks` - Count of jacks
- `pt2` / `queens` - Count of queens
- `pt3` / `kings` - Count of kings
- `pt4` / `aces` - Count of aces
- `pt5` / `top2` - Top 2 honors (A, K)
- `pt6` / `top3` - Top 3 honors (A, K, Q)
- `pt7` / `top4` - Top 4 honors (A, K, Q, J)
- `pt8` / `top5` - Top 5 honors (A, K, Q, J, T)
- `pt9` / `c13` - C13 point count (A=6, K=4, Q=2, J=1)

Examples: `top3(north) >= 5`, `aces(south, spades) == 1`, `c13(north) + c13(south) >= 40`

**Loser Count Details:**
- Uses standard losing trick count algorithm
- Void: 0 losers
- Singleton: 0 if Ace, 1 otherwise
- Doubleton: 0 for AK, 1 for Ax/Kx, 2 otherwise
- 3+ cards: Start with 3, subtract 1 for each A/K/Q in top 3 positions
- Examples: `losers(north) <= 7`, `losers(south, spades) == 0`

**Shape Pattern Syntax:**
- Exact shapes: `shape(north, 5431)` - exactly 5-4-3-1 in S-H-D-C order
- Wildcard patterns: `shape(south, 54xx)` - 5 spades, 4 hearts, any minors
- Any distribution: `shape(east, any 4333)` - any 4-3-3-3 regardless of suits
- Combinations: `shape(west, any 4333 + any 5332 - 5332)` - balanced except exact 5-3-3-2
- Uses `+` for inclusion, `-` for exclusion

**Card Syntax:**
- Format: rank + suit (e.g., AS, KH, TC, 2D)
- Ranks: A, K, Q, J, T, 9, 8, 7, 6, 5, 4, 3, 2
- Suits: S (spades), H (hearts), D (diamonds), C (clubs)
- Example: `hascard(north, AS)` checks if north has ace of spades

**Suit Keywords:**
- Used as arguments to functions like `losers(position, suit)`
- Keywords: spades, hearts, diamonds, clubs (case-insensitive)
- Example: `losers(north, spades) == 0` checks for solid spade suit

**Hand Quality Metrics (Bridge World Oct 1982):**

The quality and cccc functions implement hand evaluation algorithms from Bridge World, October 1982. Both return values multiplied by 100 to use integer math (e.g., 1500 = 15.00 points).

**Quality Function - `quality(position, suit)`:**
Evaluates the quality of a specific suit based on length and honor cards.
- Base values: A=4×SuitFactor, K=3×SuitFactor, Q=2×SuitFactor, J=1×SuitFactor (where SuitFactor = Length × 10)
- Ten bonus: Full SuitFactor if 2+ higher honors or has J; half otherwise
- Nine bonus: Half SuitFactor if 2 higher honors, or has T, or has 8
- Long suit bonus (7+ cards): Adds points for missing honors that would be replaced
- **Note**: Quality values are multiplied by 100 to use integer math (e.g., 1500 = 15.00 points).
- Examples:
  - `quality(north, spades) >= 4000` - Strong spade suit (40.00+ quality points)
  - `quality(south, hearts) < 100` - Weak heart suit (< 1.00 quality points)

**CCCC Function - `cccc(position)`:**
Comprehensive hand evaluation combining honor strength, suit quality, and shape.
- Honor points: A=300, K=200, Q=100, with adjustments for shortage
  - Singleton K: -150, Singleton Q: -75, Doubleton Q: -25
  - Unsupported Q (no higher honor): -25
  - J: +50 if 2 higher honors, +25 if 1 higher
  - T: +25 if 2 higher honors, +25 if 1 higher + nine
- Adds suit_quality for each suit
- Shape points: +100 for each short suit (< 3 cards)
- Balanced adjustment: -50 if balanced, else ShapePoints - 100
- **Note**: CCCC values are multiplied by 100 to use integer math (e.g., 1500 = 15.00 points).
- **Automatic preprocessing**: 4-digit numbers in regular expressions work correctly (e.g., `cccc(north) >= 1500`) thanks to automatic preprocessing that distinguishes shape patterns from numeric literals.
- Examples:
  - `cccc(north) >= 1500` - Strong opening hand (15.00+ points)
  - `cccc(south) + cccc(north) >= 2400` - Game-level partnership (24.00+ combined points)

### ✅ **Double-Dummy and Scoring Functions**

| Function | Description | Status |
|----------|-------------|--------|
| `tricks(position, denomination)` | Double-dummy trick count | ✅ Working |
| `score(vulnerability, contract, tricks)` | Contract score calculation | ✅ Working |
| `imps(score_diff)` | Convert score difference to IMPs | ✅ Working |

**Tricks Function - `tricks(position, denomination)`:**
Returns the double-dummy trick count for a given declarer and denomination.
- `position`: north, south, east, west
- `denomination`: Use suit keyword (spades, hearts, diamonds, clubs) or numeric (0=C, 1=D, 2=H, 3=S, 4=NT)
- Returns: 0-13 (number of tricks)
- Examples:
  - `tricks(north, spades)` - Tricks for North declaring in spades
  - `tricks(south, 4)` - Tricks for South declaring in notrump (4=NT)
- **Note**: This function uses the built-in alpha-beta solver (~1.4 deals/second)

**Score Function - `score(vulnerability, contract, tricks)`:**
Calculates the contract score based on vulnerability, contract, and tricks taken.
- `vulnerability`: 0 = non-vulnerable, 1 = vulnerable
- `contract`: Encoded as `doubled_flag * 100 + level * 10 + strain`
  - `strain`: 0=C, 1=D, 2=H, 3=S, 4=NT
  - `doubled_flag`: 0=undoubled, 1=doubled, 2=redoubled
  - Examples: 3NT=34, 4S=43, 3NT doubled=134, 4Sx=143, 4Sxx=243
- `tricks`: 0-13 (tricks taken by declarer)
- Returns: Positive score if made, negative if failed
- Examples:
  - `score(0, 34, 9)` - 3NT non-vul making exactly = 400
  - `score(1, 43, 10)` - 4S vul making exactly = 620
  - `score(0, 34, 8)` - 3NT non-vul down 1 = -50
  - `score(0, 143, 9)` - 4S doubled non-vul down 1 = -100

**IMPs Function - `imps(score_diff)`:**
Converts a score difference to IMPs using the standard IMP table.
- `score_diff`: Any integer (positive or negative)
- Returns: IMP value (preserves sign)
- Examples:
  - `imps(420)` - 10 IMPs (410-489 range)
  - `imps(0 - 420)` - -10 IMPs
  - `imps(score(0, 34, 9) - score(0, 34, 8))` - Compare 3NT making vs down 1

**Combined Usage Example:**
```
# Find deals where 3NT scores better than 4S
score(0, 34, tricks(north, 4)) > score(0, 43, tricks(north, spades))
```

---

## Operators

### ✅ **Implemented**

| Category | Operators | Status |
|----------|-----------|--------|
| **Arithmetic** | `+`, `-`, `*`, `/`, `%` | ✅ Working |
| **Comparison** | `==`, `!=`, `<`, `<=`, `>`, `>=` | ✅ Working |
| **Logical** | `&&`, `||`, `!` | ✅ Working |
| **Unary** | `-` (negation), `!` (not) | ✅ Working |
| **Ternary** | `? :` (condition ? true_expr : false_expr) | ✅ Working |

**Operator Examples:**

*Logical NOT (`!` and `not` keyword):*
```
# Using ! operator
!(hcp(north) < 10)

# Using not keyword
not (hcp(north) >= 20)

# In compound expressions
hcp(north) >= 15 && not (hearts(north) >= 5)
```

*Ternary operator:*
```
# Simple ternary
hcp(north) >= 15 ? 1 : 0

# Arithmetic in branches
hcp(north) >= 20 ? hcp(north) + 100 : hcp(north)

# Nested ternary
hcp(north) >= 15 ? (hearts(north) >= 5 ? 2 : 1) : 0
```

---

## Action Keywords

### ✅ **Implemented**

| Action | Description | Status |
|--------|-------------|--------|
| `produce N` | Generate N matching deals | ✅ Keyword & `-p` flag |
| `condition expr` | Define filter constraint | ✅ Working |
| `action printall` | Print all 4 hands (newspaper columns) | ✅ Working |
| `action printew` | Print E/W hands only | ✅ Working |
| `action printpbn` | PBN format output with metadata | ✅ Working |
| `action printcompact` | Compact 4-line format | ✅ Working |
| `action printoneline` | Single-line format | ✅ Working |
| `dealer N/E/S/W` | Set dealer position (north/east/south/west) | ✅ Working |
| `vulnerable none/NS/EW/all` | Set vulnerability | ✅ Working |
| `action average "label" expr` | Calculate average of expression (optional label) | ✅ Working |
| `action frequency "label" expr` | Display frequency distribution (optional label) | ✅ Working |
| `action frequency "label" expr min max` | Frequency with explicit range | ✅ Working |
| `predeal N/E/S/W cards` | Predeal specific cards to a position | ✅ Working |
| `csvrpt(terms...)` | Write CSV report to file (requires `-C FILE`) | ✅ Working |

**CSV Report Terms:**
- Expressions: `hcp(north)`, `controls(south)`, etc.
- Strings: `"Strong hand"`, `"Opener"`, etc. (automatically quoted with single quotes)
- Compass: `north`, `east`, `south`, `west` (outputs hand in PBN format)
- Side: `ns`, `ew` (outputs two hands)
- All hands: `deal` (outputs all four hands)

**Example Usage:**
```bash
# Full dealer.exe syntax with vulnerable and dealer
cat << 'EOF' | dealer -s 1
vulnerable ew
dealer west
nt_opener = hcp(north) >= 15 && hcp(north) <= 17
condition nt_opener
produce 5
action printpbn
EOF

# Command-line flags override input file keywords
echo "dealer south
vulnerable all
condition hcp(north) >= 20
produce 3
action printcompact" | dealer -p 10 -f oneline -d north -v none
# Will produce 10 (not 3) in oneline format (not compact) with dealer=north, vulnerable=none

# Average and frequency actions with labeled expressions
cat << 'EOF' | dealer -p 100
condition hcp(north) >= 15
action average "North HCP" hcp(north), frequency "HCP Distribution" hcp(north), printoneline
EOF
# Outputs average and frequency table to stderr:
# North HCP: 16.57
#
# HCP Distribution:
#  15     37 (37.00%)
#  16     22 (22.00%)
#  17     15 (15.00%)
#  ...

# Predeal specific cards to positions
cat << 'EOF' | dealer -p 5 -s 1
predeal north AS,AH,AD,AC
condition hcp(north) >= 4
EOF

# CSV export examples
cat << 'EOF' | dealer -p 10 -s 1 -C results.csv
condition hcp(north) >= 20
csvrpt(north, hcp(north), controls(north), "Strong hand")
EOF
# Creates results.csv with lines like:
#  K.AQ3.AK7.AQ6532,22,8,'Strong hand'

# CSV with partnerships and multiple expressions
cat << 'EOF' | dealer -p 100 -C w:partnerships.csv -q
condition hcp(north) + hcp(south) >= 25
csvrpt("NS Partnership", ns, hcp(north), hcp(south), hcp(north) + hcp(south))
EOF
# Creates/overwrites partnerships.csv (w: prefix) with format:
#  'NS Partnership',KQ4.QJ982..AKQ43 9.K54.KQT732.652,17,8,25

# CSV with all four hands
cat << 'EOF' | dealer -p 50 -C deals.csv
condition hcp(north) >= 15
csvrpt(deal, hcp(north), hcp(east), hcp(south), hcp(west))
EOF
# Outputs all four hands in PBN format with HCP for each position
```

**CSV File Modes:**
- `-C filename` or `--CSV filename` - Append mode (default) - adds to existing file
- `-C w:filename` or `--CSV w:filename` - Write mode - overwrites file

**PBN Title Metadata:**
```bash
# Add custom title to PBN output
echo "hcp(north) >= 20" | dealer -p 10 -f pbn -T "Strong Opening Hands"
# Or use long form
echo "hcp(north) >= 20" | dealer -p 10 -f pbn --title "Strong Opening Hands"
# Output includes: [Event "Strong Opening Hands"]
```

# Multiple predeal statements
cat << 'EOF' | dealer -p 3
predeal north AS,KS,QS
predeal south AH,KH,QH
condition hcp(north) + hcp(south) >= 12
EOF
# North gets ASKSQs, South gets AHKHQH
```

**Implementation Details:**
- Action keywords can be overridden by command-line flags (CLI has highest priority)
- `condition` sets the constraint expression (equivalent to standalone expression)
- `produce N` sets default produce count (overridden by `-p N` flag if specified)
- `action` sets output format (overridden by `-f FORMAT` flag if specified)
- `dealer` sets dealer position (overridden by `-d POS` flag if specified)
- `vulnerable` sets vulnerability (overridden by `-v VULN` flag if specified)
- `average` calculates and displays average of expression over all matching deals
  - Optional string literal label for labeling output
  - Printed to stderr after all deals are generated
  - Multiple average statements can be used in one program
- `frequency` displays frequency distribution tables for expressions
  - Optional string literal label for labeling output
  - Optional explicit range (min max) for table display
  - Auto-detects range from data if not specified
  - Shows count and percentage for each value
  - Printed to stderr after all deals are generated
  - Multiple frequency statements can be used in one program
- `predeal` assigns specific cards to a position before shuffling
  - Syntax: `predeal position card1,card2,...` (e.g., `predeal north AS,KH,QD`)
  - Cards use standard notation: rank (A,K,Q,J,T,9-2) + suit (S,H,D,C)
  - Multiple predeal statements can assign cards to different positions
  - Shuffle algorithm skips predealt cards (matches dealer.exe exactly)
  - Error if same card dealt twice or more than 13 cards to one position
  - Affects the random number sequence (rebuilds internal lookup table)
- Precedence: Command-line flags > Input file keywords > Defaults
- Backward compatible: simple expressions still work with command-line flags

### ❌ **Not Implemented**

#### Print Actions
- `print(expression)` - Print custom expression
- `printes` - Print in ES format

#### Control Commands
- `generate N` - Generate exactly N deals (report all matches)
- `pointcount name values` - Define custom point count
- `altcount name values` - Alternative counting method

---

## Current CLI Implementation

### Command-Line Arguments

| Argument | Description | Status |
|----------|-------------|--------|
| `-p N` / `--produce N` | Produce N matching deals (default: 40). Mutually exclusive with `-g` | ✅ Implemented |
| `-g N` / `--generate N` | Generate N total deals, report all matches (default: 10,000,000). Mutually exclusive with `-p` | ✅ Implemented |
| `-s SEED` / `--seed SEED` | Set random seed for reproducible results | ✅ Implemented |
| `-f FORMAT` / `--format FORMAT` | Output format (oneline, printall, printew, printpbn, printcompact) | ✅ Implemented |
| `-d POS` / `--dealer POS` | Dealer position for PBN (N/E/S/W) | ✅ Implemented |
| `--vulnerable VULN` | Vulnerability for PBN (None/NS/EW/All) | ✅ Implemented |
| `-v` / `--verbose` | Verbose output, prints statistics at end of run (matches dealer.exe) | ✅ Implemented |
| `-V` / `--version` | Print version information and exit (matches dealer.exe) | ✅ Implemented |
| `-q` / `--quiet` | Quiet mode - suppress deal output, only show statistics (matches dealer.exe) | ✅ Implemented |
| `-m` / `--progress` | Show progress meter during generation (every 10,000 deals, matches dealer.exe) | ✅ Implemented |

**Note**: The `-v` switch was changed from vulnerability to verbose to match dealer.exe behavior. Use `--vulnerable` (long form) for vulnerability setting.

### Produce vs Generate Mode

**Produce Mode (`-p N`, default):**
- Stops after producing N **matching** deals
- Use when you want a specific number of hands that meet your criteria
- Example: `-p 10` generates deals until 10 matches are found
- Default: 40 deals

**Generate Mode (`-g N`):**
- Generates exactly N **total** deals and reports all matches
- Use when you want to test a rare condition or gather statistics
- Example: `-g 100000` generates 100,000 deals and shows all that match
- Default: 10,000,000 deals

**Examples:**
```bash
# Produce mode: Stop after finding 10 strong openings
echo "hcp(north) >= 20" | dealer -p 10

# Generate mode: Check 1000 deals for strong openings
echo "hcp(north) >= 20" | dealer -g 1000

# In generate mode, you might find 0, 1, or many matches
# In produce mode, you'll always get exactly N matches (unless generation limit reached)
```

### Default Behavior

- **Input**: Reads constraint from stdin
- **Output**: Oneline format to stdout (default)
- **Statistics**: Printed to stderr (generated count, produced count, seed, duration)
- **Seed**: Microsecond-resolution timestamp if not specified
- **Mode**: Produce mode with 40 deals (unless `-g` or `produce` keyword specified)

---

## Architecture Status

### Crates

| Crate | Purpose | Status |
|-------|---------|--------|

| `dealer-core` | Deal generation | ✅ Complete (13 tests) |
| `dealer-pbn` | PBN format I/O | ✅ Basic (9 tests) |
| `dealer-parser` | Constraint parsing | ✅ Expanded (20 tests) |
| `dealer-eval` | Expression evaluation | ✅ Expanded (45 tests) |
| `dealer` | CLI application | ✅ Basic (produce mode) |

### Test Coverage

- **Total Tests**: 102 passing
- **Variables**: 9 tests for variable assignment, lookup, and recursive evaluation
- **Preprocessing**: 7 tests for 4-digit number disambiguation
- **Quality/CCCC**: 7 tests for hand evaluation functions (2 unit tests, 3 evaluation tests, 2 integration tests)
- **Print Formats**: 9 tests for output formatting (printall, printew, printpbn, printcompact, oneline)
- **Action Keywords**: Parser tests for condition, produce, action statements
- **Coverage**: All core constraint functions, alternative point counts, hand quality metrics, variables, automatic preprocessing, action keywords, and print formats implemented
- **Missing**: Statistical functions, double-dummy analysis

### Preprocessing System

The parser includes an automatic preprocessing step that solves the ambiguity between 4-digit shape patterns and 4-digit numeric literals:

**Problem**: In PEG parsers, `shape(north, 5242)` and `cccc(north) >= 1500` both contain 4-digit numbers, but only the first should be parsed as a shape pattern.

**Solution**: Before parsing, all input is preprocessed to mark 4-digit numbers inside `shape()` functions with a `%s` prefix:
- `shape(north, 5242)` → `shape(north, %s5242)` (marked as shape pattern)
- `cccc(north) >= 1500` → unchanged (numeric literal)
- `shape(north, any 4333 - 4333)` → `shape(north, any 4333 - %s4333)` (only mark non-"any" patterns)

The grammar is then designed to require the `%s` marker for pure-digit shape patterns, while wildcards (e.g., `54xx`) and "any"-prefixed patterns don't need it. This allows users to write natural expressions like `cccc(north) >= 1500` without workarounds.

---

## Limitations of Current Implementation

### Parser Limitations
1. ✅ ~~Only parses constraint expressions, not action blocks~~ **IMPLEMENTED**
2. ✅ ~~No support for full dealer input format~~ **IMPLEMENTED** (`condition`, `produce`, `action`, `average`, `frequency` keywords working)

### Evaluator Limitations
1. 22 core functions implemented (hcp, 4 suits, controls, losers, shape, hascard, tens-aces, top2-5, c13, quality, cccc)
2. No double-dummy analysis (tricks)
3. No scoring functions (score, imps)
4. No statistical aggregation

### CLI Limitations
1. ✅ ~~Only "produce" mode (no "generate" mode)~~ **IMPLEMENTED** (Both `-p` produce and `-g` generate modes working)
2. ✅ ~~Output format hardcoded to printoneline~~ **IMPLEMENTED** (5 formats available via `-f` flag or `action` keyword)
3. ✅ ~~No action language support~~ **IMPLEMENTED** (condition, produce, action, dealer, vulnerable, average, frequency, predeal keywords working)
4. ✅ ~~No predeal support~~ **IMPLEMENTED** (predeal keyword assigns cards to positions before shuffling, matches dealer.exe exactly)
5. ✅ ~~No average output~~ **IMPLEMENTED** (average action calculates and displays statistics)
6. ✅ ~~No frequency output~~ **IMPLEMENTED** (frequency action displays distribution tables)

---

## Dealer Language Architecture

The full dealer language has two parts:

1. **Condition Section** - Filter expressions ✅ **IMPLEMENTED**
2. **Action Section** - Output and statistics (partially implemented)

Example full dealer input:
```
# Variables
nt_opener = hcp(north) >= 15 && hcp(north) <= 17 && shape(north, any 4333 + any 4432 + any 5332)

# Condition
condition nt_opener

# Actions (partially working)
produce 100
action printpbn    # ✅ Print formats working

# Not yet implemented:
# average hcp(north)
# frequency shape(north)
```

**Current implementation:**
- ✅ Variables and condition expressions fully working
- ✅ Print format actions fully working (printall, printew, printpbn, printcompact, printoneline)
- ✅ Produce directive fully working
- ✅ Average action fully working (calculates averages over matching deals)
- ✅ Frequency action fully working (displays distribution tables with counts and percentages)
- ✅ Predeal directive fully working (assigns specific cards to positions before shuffling)

---

## Next Steps for Full Implementation

### High Priority
1. ✅ ~~Add `-g` / `--generate` mode~~ **IMPLEMENTED**
2. ✅ ~~Parse and handle action blocks~~ **IMPLEMENTED**
3. ✅ ~~Multiple output format support~~ **IMPLEMENTED**
4. ✅ ~~Statistical actions (average)~~ **IMPLEMENTED**
5. ✅ ~~Frequency action~~ **IMPLEMENTED**
6. ✅ ~~Predeal support~~ **IMPLEMENTED**

### Medium Priority
7. ✅ ~~Vulnerability/dealer position~~ **IMPLEMENTED**
8. Performance optimization for large deal generation

### Low Priority
9. Double-dummy analysis (tricks) - requires DDS library
10. Scoring functions (score, imps)
11. Additional evaluation metrics

---

## Testing Strategy

### Current Testing
- Unit tests for each component
- Golden tests for shuffle algorithm
- Integration tests for basic constraints

### Needed Testing
- Comparison tests against dealer.exe output
- Statistical accuracy tests
- Performance benchmarks
- Edge case coverage (void suits, yarborough, etc.)

---

## References

- **Dealer Manual**: https://www.bridgebase.com/tools/dealer/Manual/input.html
- **Original Dealer**: https://github.com/ThorvaldAagaard/Dealer
- **DealerV2_4**: https://github.com/ThorvaldAagaard/DealerV2_4
