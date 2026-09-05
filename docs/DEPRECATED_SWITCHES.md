# Deprecated Switches in dealer3

This document lists all dealer.exe switches that are **not supported** in dealer3 and explains why. When users try to use these switches, they receive helpful error messages guiding them to alternatives.

---

## Status

Three switches are recognised and refused with an explanation.

The swapping switches `-2` and `-3` used to be on this list. They are
**implemented** now — see `docs/command_line_comparison.md` — with the one
combination that would go wrong, a predeal to a seat the swap moves, refused
explicitly. The original allows that combination and silently loses the
predealt cards.

---

## Deprecated Switches

### 1. `-e` - Exhaust Mode

**Status**: ❌ Not Supported
**Reason**: Experimental feature never completed

**Error Message**:
```
Error: Switch '-e' (exhaust mode) is not supported in dealer3.

Reason: Exhaust mode was an experimental alpha feature in dealer.exe
        that was never completed or documented.

Suggestion: Remove the '-e' switch from your command.
```

**What it was supposed to do**:
- Unknown - experimental feature marked as "alpha version" in dealer.exe
- Never documented in the manual
- Never completed by original author

**Why not supported**:
- No clear specification of what it should do
- Not documented in dealer.exe manual
- No known users depending on this feature

---

### 2. `-u` - Upper/Lowercase Toggle

**Status**: ✅ Accepted and ignored — no longer deprecated

**What it does in dealer.exe**: nothing. `case 'u'` sets `uppercase = 1`, which
is read only by

```c
#define representation (uppercase ? ucrep : lcrep );
```

and that macro is never invoked anywhere in `dealer.c`. `lcrep` appears exactly
twice in the file — its own definition and that dead macro — while every output
path calls `ucrep` directly. Confirmed against the binary: its output is
byte-identical with and without the switch, so honours are `AKQJT` either way.

**Why dealer3 accepts it**: refusing a switch that does nothing would break a
command line that works on BBO, for no gain. dealer3 prints upper-case honours
already, so `-u` changes nothing here either — which is the compatible answer.
`-v` prints a note so nobody believes it took effect.
- Could be added in future if there's demand

---

### 3. `-l` - Library Mode

**Status**: ❌ Not Supported
**Reason**: Conflicting meanings in dealer.exe vs DealerV2_4

**Error Message**:
```
Error: Switch '-l' (library mode) is not supported in dealer3.

In dealer.exe, '-l N' reads deals from Ginsberg's library.dat,
starting at index N. Those deals carry pre-solved double-dummy
tricks, which is what made the switch worth having.

Suggestion: use '--input-deals' to filter deals from a file.
            Reading a solved library is tracked as issue #61.
```

**What it did in dealer.exe**:
- `-l N` set `loading = 1` and `loadindex = N`, reading pre-generated deals from
  M. Ginsberg's `library.dat` from that index onward
- Those records carry pre-solved double-dummy results — `dealer.c` reads
  `libdeal.tricks[dn]` rather than searching — which is the point of the switch:
  `tricks()` becomes a lookup

**What it does in DealerV2_4**: nothing. **There is no `-l`.**

This page used to say it exported "DL52 format". That was wrong, and wrong in a
way worth recording, because it drove a roadmap item and a message dealer3
printed at users. DealerV2_4's option string is
`hmquvVg:p:s:x:C:D:L:M:O:P:R:T:N:E:S:W:X:U:0-9` — no lowercase `l` — and the
binary answers `-l` with its usage message. "DL52" appears nowhere in
DealerV2_4's source, headers or user guide; the only occurrences anywhere were
in dealer3's own documentation, citing each other.

What DealerV2_4 does have is **`-L`**, naming a path to Richard Pavlicek's
solved-deal library in ZRD format — the same idea as dealer.exe's `-l`, a
different file. That is worth having, and is tracked as
[#61](https://github.com/bridge-craftwork/Dealer3/issues/61) with the format
work in bridge-encodings#20.

**Why `-l` is still not supported**:
- `--input-deals` already covers filtering deals from a file, in dealer3's own
  way and without an index
- A library's value is its pre-solved tricks, which `--input-deals` cannot carry
  today — that is what #61 adds

---

## Implementation Details

### Code Location
File: [dealer/src/main.rs](../dealer/src/main.rs)

```rust
// Deprecated switches - parse them to show helpful error messages
#[arg(short = 'e', hide = true)]
exhaust: bool,

#[arg(short = 'u', hide = true)]
uppercase: bool,

#[arg(short = 'l', hide = true)]
library: bool,
```

**Note**: `hide = true` prevents these switches from showing in `--help` output.

### Error Handling
Each deprecated switch is checked in `main()` before normal execution:

```rust
if args.exhaust {
    eprintln!("Error: Switch '-e' (exhaust mode) is not supported in dealer3.");
    eprintln!();
    eprintln!("Reason: Exhaust mode was an experimental alpha feature in dealer.exe");
    eprintln!("        that was never completed or documented.");
    ...
    std::process::exit(1);
}
```

---

## Testing

### Manual Tests

All deprecated switches have been tested:

```bash
# Test each deprecated switch
$ echo "hcp(north) >= 20" | dealer -e -p 1
Error: Switch '-e' (exhaust mode) is not supported in dealer3.
...

$ echo "hcp(north) >= 20" | dealer -u -v -p 1
Note: -u is accepted and ignored; honours are always upper case.
...

$ echo "hcp(north) >= 20" | dealer -l -p 1
Error: Switch '-l' (library mode) is not supported in dealer3.
...
```

✅ All tests passing with helpful error messages

---

## User Impact

### Positive Impact
- ✅ Clear error messages help users understand what's wrong
- ✅ Provides migration guidance
- ✅ Explains *why* features aren't supported
- ✅ Better than silent failures or cryptic errors

### Affected Users
- Users who tried experimental `-e` flag (likely none)
- Nobody: `-u` never changed dealer.exe's output either
- Users who used library.dat with `-l` (advanced users only)

### Migration Path
- **Swapping (`-2`, `-3`)**: nothing to do — they work. Only a predeal to a
  seat the swap moves is refused, and the original was silently wrong there.
- **Exhaust (`-e`)**: Remove switch (feature never worked)
- **Uppercase (`-u`)**: nothing to do; accepted and ignored, as in dealer.exe
- **Library (`-l`)**: Remove switch, wait for future library support

---

## Future Considerations

### Could Be Added Later
1. ~~**Upper/lowercase toggle (`-u`)**~~: accepted and ignored; it is a no-op in dealer.exe too
2. **Solved-deal libraries**: reading deals *and* their double-dummy tables, so
   `tricks()` becomes a lookup. Tracked as
   [#61](https://github.com/bridge-craftwork/Dealer3/issues/61), against
   Pavlicek's ZRD format rather than Ginsberg's `library.dat`, because ZRD is
   specified, downloadable and has a reference decoder.

### Will NOT Be Added
1. **Swapping modes (`-2`, `-3`)**: Fundamentally incompatible with predeal
2. **Exhaust mode (`-e`)**: No specification of what it should do

---

## Related Documents

- [Command-Line Switch Requirements](command_line_switch_requirements.md)
- [dealer.exe vs DealerV2_4 Switches](dealer_vs_dealer2_switches.md)
- [CHANGELOG](CHANGELOG.md)
- [Phase 0 Completion](PHASE_0_COMPLETION.md)

---

**Status**: ✅ Complete
**Version**: 0.2.0 (unreleased)
**Last Updated**: 2026-01-01
