# Implementation Roadmap for dealer3 Command-Line Switches

## Current Status

**Not written down here.** `docs/command_line_comparison.md` carries it, and is
generated from clap so it cannot drift. This block used to list `-m`, `-q` and
`-V` as missing while the timeline below ticked them off as done, and claimed
`-v` still meant vulnerability a release after that changed.

What remains of this document is the *plan* — effort, value and ordering — which
is a judgement no program can generate.

---

## Phase 1: Essential Missing Features (High Priority)

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

## Phase 2: Command-Line Predeal (Medium Priority)

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

## Phase 3: Export and Reporting (Medium Priority)

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

## Phase 4: Performance Features (Medium-Low Priority)

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

---

## Phase 5: Advanced Features (Low Priority)

### 5.1 Double-Dummy Analysis (DDS Integration)
**Switches**: `-M MODE`, `-R THREADS`
**Effort**: Very High (20-30 hours)
**Value**: Medium-High (advanced users)
**Requirements**:
- Integrate DDS library (C++ FFI)
- Implement `tricks()` function
- Add DDS mode selection
- Thread pool management

### 5.2 Library Mode
**Switch**: `-l FILENAME`
**Effort**: High (8-10 hours)
**Value**: Low (niche feature)
**Implementation**:
- Read pre-generated deals from file
- Skip shuffling, use file deals
- Fast tricks() evaluation (if DDS available)

### 5.3 Export Formats
**Switches**: `-Z/--zrd`, `-l/--dl52`
**Effort**: Medium (3-4 hours each)
**Value**: Low (format-specific)
**Implementation**:
- RP zrd format writer
- DL52 format writer
- Optional DDS results inclusion

### 5.4 Script Parameters
**Switches**: `-0` through `-9`
**Effort**: Low-Medium (2-3 hours)
**Value**: Low (scripting)
**Implementation**:
- Store as global variables `$0`-`$9`
- Make available in expressions
- Useful for parameterized scripts

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

### Sprint 3 (Medium-term - 1 week) — partly done
- [x] Multi-threading (`-R`, plus `--batch-size`) - **COMPLETED**
- [ ] Swapping modes (`-x`) - recognised and rejected, not implemented

### Sprint 4 (Long-term - 2-3 weeks) — partly done
- [x] DDS integration - **COMPLETED** via `tricks()`, `score()`, `imps()`, but
      unusably slow on the legacy solver (#14)
- [ ] Library mode - `--input-deals` covers the common case; `-l` is rejected
- [ ] Export formats (`-Z` zrd, DL52)
- [ ] Script parameters (`$0`-`$9`)

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

## Priority Matrix

| Feature | Effort | Value | Priority | Order |
|---------|--------|-------|----------|-------|
| Version flag | Low | High | 🔴 Critical | 1 |
| Progress meter | Medium | High | 🔴 Critical | 2 |
| Verbose toggle | Low | Medium | 🟡 High | 3 |
| Quiet mode | Low | Medium | 🟡 High | 4 |
| Compass predeal | Medium | High | 🟡 High | 5 |
| CSV export | Medium | Medium | 🟢 Medium | 6 |
| Title metadata | Low | Low | 🟢 Medium | 7 |
| Multi-threading | High | Medium | 🔵 Low | 8 |
| Swapping modes | Medium | Low | 🔵 Low | 9 |
| DDS integration | Very High | Medium | ⚪ Future | 10 |
| Library mode | High | Low | ⚪ Future | 11 |
| Export formats | Medium | Low | ⚪ Future | 12 |

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

