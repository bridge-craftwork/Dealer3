# dealer.exe vs DealerV2_4 Command-Line Switch Comparison

This document compares command-line switches between the original dealer.exe (Hans van Staveren) and DealerV2_4 (Greg Morse), categorized by compatibility.

---

## 1. Same in Both (Identical Behavior)

These switches work the same way in both dealer.exe and DealerV2_4:

| Switch | Description | Notes |
|--------|-------------|-------|
| `-p N` | Produce N hands (default 40) | Core feature - produce mode |
| `-g N` | Generate N hands (default 10M) | Core feature - generate mode |
| `-s N` | Random seed | Deterministic generation |
| `-h` | Help | Display usage information |
| `-0` | No swapping (default) | Default behavior, each deal generated normally |
| `-q` | Quiet mode | Suppress PBN output (useful for testing) |
| `-v` | Verbose | Toggle statistics output at end of run |
| `-V` | Version info | Display version and exit |
| `-m` | Progress meter | Show progress during long runs |

**Total: 9 switches**

---

## 2. dealer.exe Only (Not in DealerV2_4)

These switches exist in dealer.exe but were NOT included in DealerV2_4:

| Switch | Description | Reason Not in V2_4 |
|--------|-------------|-------------------|
| `-u` | Upper/lowercase AKQJT | Cosmetic feature, low priority |
| `-2` | 2-way swapping (E/W) | Implemented. Refused with a predeal to East or West |
| `-3` | 3-way swapping (E/W/S) | Implemented. `predeal north` still works |
| `-e` | Exhaust mode (alpha) | Experimental, never completed |

**Total: 4 switches**

**Note**: Swapping modes (`-2`, `-3`) were replaced in V2_4 with the `-x MODE` switch, though V2_4's implementation may differ.

---

## 3. DealerV2_4 Only (Not in dealer.exe)

These switches are new features added in DealerV2_4:

### Core Enhancements

| Switch | Description | Category |
|--------|-------------|----------|
| `-N CARDS` | Predeal cards to North | Predeal |
| `-E CARDS` | Predeal cards to East | Predeal |
| `-S CARDS` | Predeal cards to South | Predeal |
| `-W CARDS` | Predeal cards to West | Predeal |
| `-P N` | Vulnerability for Par (0-3) | Position/Vulnerability |
| `-x MODE` | Exchange mode 2\|3 (swapping) | Swapping |

### Double-Dummy Analysis (DDS Integration)

| Switch | Description | Category |
|--------|-------------|----------|
| `-M MODE` | DDS mode 1\|2 | DDS |
| `-R N` | Resources/Threads 1-9 | DDS/Performance |

### Export & Reporting

| Switch | Description | Category |
|--------|-------------|----------|
| `-C FILE` | CSV Report filename | Export |
| `-X FILE` | Export predeal holdings | Export |
| `-Z FILE` | RP zrd format export | Export |

### Advanced Features

| Switch | Description | Category |
|--------|-------------|----------|
| `-L PATH` | RP Library source path | Library |
| `-U PATH` | DealerServer pathname | Server |
| `-O POS` | OPC evaluation Opener | Evaluation |
| `-T "text"` | Title in quotes | Metadata |
| `-D LEVEL` | Debug verbosity 0-9 | Debug |

### Script Parameters

| Switch | Description | Category |
|--------|-------------|----------|
| `-0` to `-9` | Set $0-$9 script parameters | Scripting |

**Total: 21+ switches**

**V2_4 Focus Areas**:
- Predeal via command-line (4 switches)
- Double-dummy analysis with DDS library
- Multiple export formats (CSV, RP zrd, DL52)
- Advanced evaluation (OPC, Par)
- Multi-threading support
- Scripting support with parameters
- Server mode integration

---

## 4. Both But Different Meaning

These switches exist in both versions but have **different meanings or behavior**:

| Switch | dealer.exe Meaning | DealerV2_4 Meaning | Compatibility |
|--------|-------------------|-------------------|---------------|
| `-l PATH` | **Read from library.dat** (M. Ginsberg's pre-generated deals for fast tricks() evaluation) | **DL52 format export** (write deals to file in DL52 format) | ⚠️ **INCOMPATIBLE** - Completely different purposes |

**Total: 1 switch with conflict**

**Critical Note**: The `-l` switch has a fundamental incompatibility:
- **dealer.exe**: INPUT mode - reads pre-generated deals from library.dat for fast bridge.exe/GIB integration
- **DealerV2_4**: OUTPUT mode - writes deals to file in DL52 format for export

This is a **breaking change** between versions that could cause confusion or errors if scripts are ported from dealer.exe to DealerV2_4.

---

## Summary Statistics

| Category | Count | Percentage |
|----------|-------|------------|
| Same in both | 9 | ~30% |
| dealer.exe only | 4 | ~13% |
| DealerV2_4 only | 21+ | ~70% |
| Both but different | 1 | ~3% |
| **Total dealer.exe switches** | **13** | |
| **Total DealerV2_4 switches** | **29+** | |

---

## Key Insights

### Backward Compatibility
- **70% compatible**: Core switches (`-p`, `-g`, `-s`, `-h`, `-0`, `-q`, `-v`, `-V`, `-m`) work identically
- **1 breaking change**: `-l` switch has completely different meaning
- **Deprecated features**: Swapping modes (`-2`, `-3`) and exhaust mode (`-e`) not ported to V2_4

### V2_4 Innovations
DealerV2_4 represents a **major enhancement** with:
1. **Command-line predeal**: 4 new switches (`-N`, `-E`, `-S`, `-W`) for convenience
2. **DDS integration**: Double-dummy solver with multi-threading
3. **Export formats**: CSV, RP zrd, DL52 for interoperability
4. **Advanced evaluation**: OPC, Par calculations
5. **Scripting support**: Parameters `$0`-`$9` for flexible scripts
6. **Server mode**: Integration with DealerServer

### Migration Considerations
When porting scripts from dealer.exe to DealerV2_4:
- ✅ Core generation switches work unchanged
- ✅ Predeal syntax in input files still works (keyword-based)
- ⚠️ Remove `-l library.dat` usage (different meaning in V2_4)
- ⚠️ Swapping modes (`-2`, `-3`) need conversion to `-x 2` or `-x 3` for DealerV2_4. dealer3 keeps the original spelling.
- ⚠️ Exhaust mode (`-e`) not available
- ✅ Consider using new `-N/E/S/W` switches for predeal convenience

---

## Recommendations for dealer3

Based on this analysis, dealer3 should:

### High Priority (Maximum Compatibility)
1. ✅ Implement core switches (`-p`, `-g`, `-s`) - **DONE**
2. ✅ Implement `-h` help - **DONE**
3. ❌ Add `-V` version info - **TODO**
4. ❌ Add `-q` quiet mode - **TODO**
5. ❌ Add `-m` progress meter - **TODO**

### Medium Priority (V2_4 Features)
6. ❌ Add `-N/E/S/W` predeal switches - **TODO** (predeal keyword already works)
7. ❌ Add `-C` CSV export - **TODO**
8. ❌ Add `-T` title metadata - **TODO**

### Low Priority (Advanced Features)
9. ~~Swapping modes~~ - done, using dealer.exe's `-0`/`-2`/`-3` rather than V2_4's `-x MODE`
10. DDS integration (`-M`, `-R`) - High effort, requires external library
11. Export formats (`-Z` RP zrd, `-l` DL52) - Niche use cases

### Avoid Conflicts
- **DO NOT** implement dealer.exe's `-l` (library.dat reader) to avoid confusion with V2_4
- **CONSIDER** keeping our `-v` for vulnerability (more useful than verbose toggle)
- Document differences clearly for users migrating from dealer.exe

---

## dealer.exe Feature Coverage in DealerV2_4

| dealer.exe Feature | In DealerV2_4? | Notes |
|-------------------|---------------|-------|
| Core generation (`-p`, `-g`, `-s`) | ✅ Yes | Identical |
| Help (`-h`) | ✅ Yes | Identical |
| Version (`-V`) | ✅ Yes | Identical |
| Quiet mode (`-q`) | ✅ Yes | Identical |
| Verbose (`-v`) | ✅ Yes | Identical |
| Progress meter (`-m`) | ✅ Yes | Identical |
| No swapping (`-0`) | ✅ Yes | Identical |
| 2-way swap (`-2`) | ✅ Yes | dealer3 keeps `-2`; V2_4 spells it `-x 2` |
| 3-way swap (`-3`) | ✅ Yes | dealer3 keeps `-3`; V2_4 spells it `-x 3` |
| Upper/lowercase (`-u`) | ❌ No | Cosmetic, dropped |
| Exhaust mode (`-e`) | ❌ No | Experimental, never finished |
| Library mode (`-l`) | ⚠️ Different | Now means DL52 export, not input |

**Coverage: 9/13 (69%) of dealer.exe switches work identically in V2_4**

