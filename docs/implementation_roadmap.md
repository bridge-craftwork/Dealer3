# Implementation Roadmap for dealer3 Command-Line Switches

## Current Status

**Not written down here.** `docs/command_line_comparison.md` carries it, and is
generated from clap so it cannot drift. This block used to list `-m`, `-q` and
`-V` as missing while the timeline below ticked them off as done, and claimed
`-v` still meant vulnerability a release after that changed.

What remains of this document is the *plan* — effort, value and ordering — which
is a judgement no program can generate. The phase sections below are the
original design record, kept because the reasoning is still useful; their
headings say which are delivered. **"What is left" is the section to read.**

---

## Phase 1: Essential switches — ✅ delivered

### 1.1 Version Information
**Switch**: `--version` (or `-V`)
**Effort**: Low (1 hour)
**Value**: High (standard practice)
**Implementation**:
```rust
#[arg(short = 'V', long = "version")]
version: bool,

// In main():
if args.version {
    println!("dealer3 version {}", env!("CARGO_PKG_VERSION"));
    println!("Rust implementation of dealer.exe");
    std::process::exit(0);
}
```

### 1.2 Progress Meter
**Switch**: `-m/--progress`
**Effort**: Medium (2-3 hours)
**Value**: High (useful for long-running generations)
**Implementation**:
- Add flag to Args struct
- Print progress every N deals (e.g., every 10,000)
- Show: deals generated, deals produced, time elapsed

### 1.3 Verbose Mode Toggle
**Switch**: `--verbose` (long form only to avoid conflict)
**Effort**: Low (1 hour)
**Value**: Medium (optional stats suppression)
**Implementation**:
- Default: true (always show stats, current behavior)
- `--verbose=false` or `--no-verbose`: suppress stats
- Keep all stats output controlled by this flag

### 1.4 Quiet Mode
**Switch**: `-q/--quiet`
**Effort**: Low (1 hour)
**Value**: Medium (suppress deal output, only show stats)
**Implementation**:
- When enabled, skip printing deals
- Still print statistics at end
- Useful for testing/benchmarking

---

## Phase 2: Command-line predeal — ✅ delivered

### 2.1 Compass Predeal Switches
**Switches**: `-N`, `-E`, `-S`, `-W` with card list
**Effort**: Medium (3-4 hours)
**Value**: High (convenience feature from V2_4)
**Implementation**:
```rust
#[arg(short = 'N', long = "north")]
north_predeal: Option<String>,

#[arg(short = 'E', long = "east")]
east_predeal: Option<String>,

#[arg(short = 'S', long = "south")]
south_predeal: Option<String>,

#[arg(short = 'W', long = "west")]
west_predeal: Option<String>,
```

**Example Usage**:
```bash
dealer -N "AS,KS,QS" -S "AH,KH,QH" -p 10
```

**Notes**:
- Parse card list (comma-separated)
- Apply before input file predeals
- Override input file predeals if specified

---

## Phase 3: Export and reporting — ✅ delivered

### 3.1 CSV Export
**Switch**: `-C/--csv FILENAME`
**Effort**: Medium (4-5 hours)
**Value**: Medium (analytics/post-processing)
**Implementation**:
- CSV header: deal_num,north,east,south,west,[custom_fields]
- Append mode by default
- Optional `w:filename` for truncate mode
- Include HCP, distribution, etc.

### 3.2 Title/Metadata
**Switch**: `-T/--title "TEXT"`
**Effort**: Low (1 hour)
**Value**: Low (nice to have)
**Implementation**:
- Add title to PBN output
- Include in CSV header
- Print at start of output

---

## Phase 4: Performance — ✅ delivered

### 4.1 Multi-threading
**Switch**: `-R/--threads N` (1-9 threads)
**Effort**: High (10-15 hours)
**Value**: Medium (performance)
**Implementation**:
- Use rayon for parallel deal generation
- Thread-safe RNG (one per thread)
- Aggregate results
- Requires careful synchronization

### 4.2 Swapping Modes
**Switches**: `-0`, `-2`, `-3` or `-x MODE`
**Effort**: Medium (5-6 hours)
**Value**: Low (not compatible with predeal)
**Implementation**:
- `-0`: Default (no swapping)
- `-2`: Generate deal, then swap E/W
- `-3`: Generate deal, then 5 permutations
- **Incompatible with predeal** (error if both used)

**As built**, this plan changed in two ways. `-x MODE` was dropped: it is
DealerV2_4's spelling, and the scripts that exist are written against
dealer.exe's `-0`/`-2`/`-3`. And the predeal error is per-seat rather than
blanket — only a predeal to a seat the swap actually moves is refused, so
`predeal north` with `-3`, one hand against six defensive layouts, works.

---

## Phase 5: Advanced features — partly delivered

This phase used to list four features in prose, with effort estimates and
switch letters. It went stale the way the switch table did before that was
generated: it had `tricks()` and `-R` down as work to do months after both
shipped, it named `-l` twice for two different features, and it offered
`-0` through `-9` for script parameters without noticing that three of those
letters are dealer.exe's swapping switches and already taken.

So what remains of it is in **What is left** below, which is generated from
`dealer/src/roadmap.rs` and checked against the argument parser — `-M`, `-Z`,
DL52 and library mode are all rows in that table, with the switch collisions
written down where they belong. Script parameters used to be a row there too,
and are now finished: `--param` fills one, a `# param 0 = west` comment declares
what it should be when nothing does, and `--params` lists what a script wants.

---

## Compatibility Considerations

### The `-v` conflict, as resolved

dealer3 0.1 used `-v` for vulnerability; dealer.exe uses it for verbose.

**Option B was taken**, not the Option A this document used to recommend:
compatibility with dealer.exe mattered more than the convenience of a short
vulnerability flag, because a BBO script written for the original has to run
here unchanged. So in 0.2.0:

- `-v` became verbose, matching dealer.exe
- vulnerability moved to `--vulnerable`, long form only
- `-X` forces statistics on regardless, also matching dealer.exe

See `CHANGELOG.md` for the migration note.

---

## Implementation Timeline

### Sprint 1 (Immediate - 1-2 days) ✅ **COMPLETED**
- [x] Version flag (`-V/--version`) - **COMPLETED**
- [x] Verbose toggle (`-v/--verbose`) - **COMPLETED**
- [x] Quiet mode (`-q/--quiet`) - **COMPLETED**
- [x] Remove `-v` for vulnerability, use `--vulnerable` only - **COMPLETED (Breaking Change)**
- [x] Progress meter (`-m`) - **COMPLETED**

### Sprint 2 (Near-term - 2-3 days) ✅ **COMPLETED**
- [x] Compass predeal switches (`-N/E/S/W`) - **COMPLETED**
- [x] CSV export (`-C`) - **COMPLETED**
- [x] Title metadata (`-T`) - **COMPLETED**

### Sprint 3 (Medium-term - 1 week) ✅ **COMPLETED**
- [x] Multi-threading (`-R`, plus `--batch-size`) - **COMPLETED**
- [x] Swapping modes (`-0`, `-2`, `-3`) - **COMPLETED**. dealer.exe's spelling,
      not DealerV2_4's `-x MODE`; a predeal to a seat the swap moves is refused
      rather than silently broken, so `predeal north` with `-3` still works

### Sprint 4 (Long-term - 2-3 weeks) — partly done
- [x] DDS integration - **COMPLETED** via `tricks()`, `score()`, `imps()`,
      solved by `bridge-solver` and remembered per deal (#14)
- [ ] Library mode - `--input-deals` covers the common case; `-l` is rejected
- [ ] Export formats (`-Z` zrd, DL52)
- [x] Script parameters (`$0`-`$9`) - **COMPLETED**. `--param 1=west` rather than
      DealerV2_4's `-1`, which is dealer.exe's swapping switch; a script declares
      its own defaults in a `# param 1 = 15` comment, which the original's lexer
      skips, so a parameterised scenario still runs on BBO (#17)

### Unplanned, and shipped anyway
Five switches arrived without ever appearing in this document: `--input-deals`,
`-t/--timeout`, `-X/--stats-on`, `--license` and `--credits`. Plus the whole
WebAssembly build and browser app, which predate none of these phases.

---

## Testing Strategy

### For Each New Switch
1. Unit tests for argument parsing
2. Integration tests for functionality
3. Compatibility tests (if applicable)
4. Documentation updates
5. Example usage in README

### Regression Testing
- Ensure existing switches still work
- Verify no conflicts between switches
- Test mutually exclusive options

---

## Documentation Requirements

### For Each Switch
- Help text (clap provides this)
- Long-form documentation
- Examples in README
- Comparison with dealer.exe (if different)
- Comparison with DealerV2_4 (if applicable)

### Update Files
- `FILTER_LANGUAGE_STATUS.md` - Add switch documentation
- `README.md` - Add usage examples
- `--help` output - Auto-generated by clap
- Changelog - Document new features

---

## What is left

<!-- BEGIN GENERATED: priority-matrix -->

Only what is **left**. A finished item is deleted rather than ticked, and anything that delivers a switch is checked against the argument parser, so this table cannot quietly describe work that has already happened.

Priority is derived from effort and value rather than written down beside them.

| Priority | What | Effort | Value | Issue | Notes |
|---|---|---|---|---|---|
| 🔵 Unlikely | `par(side)`: the par contract | Low | Low |  | Needs all 20 double-dummy results, which bridge-solver already provides for `tricks()`, `score()` and `imps()`. DealerV2_4 sets its vulnerability with `-P`, which has a row of its own in the switch table. |
| 🔵 Unlikely | Decimal literals, `6.25` and `.5` | Medium | Low |  | DealerV2_4 reads them as hundredths, which is what lets `altcount` weight a card at 0.75 and `ltc` count in halves. dealer3's numbers are integers. |
| 🔵 Unlikely | Double-dummy solver mode | Medium | Low |  | DealerV2_4's `-M`, which prints a double-dummy table per deal. The solver behind it is in place; this is the switch and its output format. |
| 🔵 Unlikely | Export in DL52 format | Medium | Low |  | DealerV2_4 spells it `-l`, which is dealer.exe's library switch — so as with the script parameters, the spelling here would have to differ. |
| 🔵 Unlikely | Export in RP zrd format | Medium | Low |  |  |
| 🔵 Unlikely | The length-bias form of `predeal`, `spades(north) == 5` | Medium | Low |  | The original's, not DealerV2_4's: `predealarg : SUIT '(' COMPASS ')' CMPEQ NUMBER` in `defs.y` calls `bias_deal`, which biases the shuffle rather than fixing cards. Rejected loudly today; the same thing can be written in the condition, at the cost of dealing and discarding instead of dealing to fit. |
| 🔵 Unlikely | Two-dimensional `frequency` | Medium | Low |  | The original takes a second expression and range and prints marginals. |
| 🔵 Unlikely | `--bbo-strict`: warn when a script will behave differently on BBO | Medium | Low | [#13](https://github.com/bridge-craftwork/Dealer3/issues/13) | Rick judged it unlikely to bite. |
| 🔵 Unlikely | `bktfreq`: frequency in buckets, one and two dimensional | Medium | Low |  | Adjacent to the two-dimensional `frequency` below but not the same thing: that one is the original's, this one groups a range into buckets. |
| 🔵 Unlikely | `export(side)` and `export(compass)` | Medium | Low |  | The statement behind DealerV2_4's `-X`, which writes predeal holdings. |
| 🔵 Unlikely | `ltc(compass)`, `ltc(compass, suit)`: the modern losing trick count | Medium | Low |  | Not a spelling of `losers()`. DealerV2_4 keeps both words, and this one counts in half-losers — so it needs decimal literals to be worth having. |
| 🔵 Unlikely | Exhaust mode | High | Low |  | Never finished in the original either; the code is compiled out. |
| 🔵 Unlikely | Library mode: replay deals by index | High | Low |  | `--input-deals` already covers the common case in dealer3's own way. |
| 🔵 Unlikely | `opc(side[, strain])` and the `opener` statement | High | Low |  | Official Point Count: a whole evaluation system, not a function. `-O` has a row in the switch table already. Worth knowing that `opener west` parses today as two bare identifiers and is silently ignored, so an OPC script runs and quietly means something else. |
| 🔵 Unlikely | `usereval(...)`: user-supplied evaluation tables | High | Low |  | Sets DealerV2_4's `userserver_reqd`, so it comes with the DealerServer that `-U` names — a second process, not a language feature. |
<!-- END GENERATED: priority-matrix -->

The twelve-row matrix that used to sit here had no status column, and nine of
its rows were finished — so it read as a plan for work that had already
happened. It also listed only command-line switches, by which point the switches
were the part that was nearly done and the language was the part that was not.

For what *has* been delivered, read `command_line_comparison.md` and
`FILTER_LANGUAGE_STATUS.md`, both generated from the code.

---

## Success Criteria

### Phase 1 Complete
- All standard switches implemented (`-V`, `-m`, `-q`, `--verbose`)
- 100% test coverage for new switches
- Documentation updated
- No breaking changes to existing functionality

### Full Compatibility
- Can run most dealer.exe scripts unchanged
- Can run most DealerV2_4 scripts (excluding DDS features)
- Clear documentation of differences
- Migration guide for users

