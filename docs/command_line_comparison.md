# Command-line switches: dealer3 vs dealer.exe vs DealerV2_4

Three implementations of the same idea:

1. **dealer.exe** — Henk Uijterwaal's original.
2. **dealer3** — this project.
3. **DealerV2_4** — Thorvald Aagaard's expanded version.

## The table is generated

The dealer3 column is read from the argument parser at test time, not written
down, because the hand-maintained version of this document drifted: it listed
`-R` as unimplemented months after it shipped, never gained `--input-deals`,
`--timeout`, `--stats-on` or `--batch-size`, and still claimed "v0.2.0, 18
switches" when there were 24.

`dealer/src/switches.rs` holds the part no program here can answer — what the
other two implementations do — and `cargo test -p dealer` fails when a switch is
added without a row, or when this file falls behind. Regenerate with:

```bash
UPDATE_DOCS=1 cargo test -p dealer
```

<!-- BEGIN GENERATED: switches -->

dealer3 implements **32 of the 42 switches** listed here. The dealer3 column is read from the argument parser itself, so it cannot drift; the other two columns are reference data (see `dealer/src/switches.rs` for their provenance).

In the dealer3 column ✅ is implemented and ⚠️ means the switch is parsed and then refused with an explanation, so a script using it gets told rather than ignored. In the other two columns ✅ means the same meaning, ⚠️ a different one, and — not present at all.

### Generation

| Switch | What it does | dealer3 | dealer.exe | DealerV2_4 | Notes |
|---|---|---|---|---|---|
| `-p`, `--produce` | Stop after N deals have matched | ✅ | ✅ | ✅ | Default 40, as in the original. |
| `-g`, `--generate` | Stop after dealing N hands | ✅ | ✅ | ✅ | Default 10,000,000. Whichever limit is reached first ends the run. |
| `-s`, `--seed` | Random seed | ✅ | ✅ | ✅ | Default is the clock. dealer3 has its own RNG (xoshiro256++), so a seed does not reproduce dealer.exe's deals — that went with legacy mode. |
| `-t`, `--timeout` | Give up after N seconds | ✅ | — | — |  |
| `--write-leveled` | Write a copy of the script with its hand types levelled | ✅ | — | — | Measures how often each `HandType_*` variable comes up, works out the keep rate for each, and writes them into the copy. `-p` sets the sample. See `docs/leveling-strategy.md`. |
| `--level-target` | Target mix for `--write-leveled`: even, or one weight per hand type | ✅ | — | — |  |
| `--level-budget` | Budget for `--write-leveled`, in deals dealt per deal kept | ✅ | — | — | When a target costs more than this, exactness is relaxed rather than the rarest type sacrificed: every type moves the same fraction toward its target. |
| `--rnd-seed` | Shift the stream `rnd()` draws from | ✅ | — | — | `rnd()` is reproducible without it. The original draws from the generator it shuffles with, so calling it there changes the deals; dealer3 keeps the two apart. |
| `-0` | No swapping (the default) | ✅ | ✅ | ⚠️ -x MODE | The three swapping switches override one another, so the last one wins. |
| `-2` | Two-way swapping: deal each shuffle again with East and West exchanged | ✅ | ✅ | ⚠️ -x MODE | Refused alongside a predeal to East or West, which it would move. The original allows it and loses the predealt cards without saying so. |
| `-3` | Three-way swapping: six deals a shuffle, rotating East, South and West | ✅ | ✅ | ⚠️ -x MODE | `predeal north` still works, since North never moves — a fixed hand against six defensive layouts is what the switch is for. |

### Output

| Switch | What it does | dealer3 | dealer.exe | DealerV2_4 | Notes |
|---|---|---|---|---|---|
| `--interleave` | Order the output so each hand type appears before any repeats | ✅ | — | — | Needs `HandType_*` variables to classify against. Rare types are spread across the run rather than exhausted early. See `docs/leveling-strategy.md`. |
| `-f`, `--format` | Output format | ✅ | — | — | The original selects a format with an `action` statement instead. |
| `-d`, `--dealer` | Dealer position | ✅ | — | — | The original uses the `dealer` statement, which dealer3 also accepts. |
| `--vulnerable` | Vulnerability | ✅ | — | ⚠️ -P sets vulnerability for par | Long form only. `-v` is verbose, as in the original — this was the 0.2.0 breaking change. |
| `-v`, `--verbose` | Toggle the closing statistics | ✅ | ✅ | ✅ |  |
| `-X`, `--stats-on` | Force statistics on, past any -v | ✅ | ✅ | ⚠️ -X exports predeal holdings | In dealer.exe's getopt string but not its usage line. DealerV2_4 reuses the letter for something else entirely. |
| `-q`, `--quiet` | Suppress the deals, keep the statistics | ✅ | ✅ | ✅ |  |
| `-m`, `--progress` | Progress meter | ✅ | ✅ | ✅ | Every 10,000 deals. |
| `-V`, `--version` | Print the version and exit | ✅ | ✅ | ✅ |  |
| `--license` | Print the licence and exit | ✅ | — | — |  |
| `--credits` | Print credits and exit | ✅ | — | — |  |
| `-h`, `--help` | Print help and exit | ❌ | ✅ | ✅ |  |

### Predeal

| Switch | What it does | dealer3 | dealer.exe | DealerV2_4 | Notes |
|---|---|---|---|---|---|
| `-N`, `--north` | Predeal cards to North | ✅ | — | ✅ | Format `S8743,HA9,D642,CQT64`, as DealerV2_4 writes it. |
| `-E`, `--east` | Predeal cards to East | ✅ | — | ✅ |  |
| `-S`, `--south` | Predeal cards to South | ✅ | — | ✅ |  |
| `-W`, `--west` | Predeal cards to West | ✅ | — | ✅ |  |

### Reporting

| Switch | What it does | dealer3 | dealer.exe | DealerV2_4 | Notes |
|---|---|---|---|---|---|
| `--stats-json` | Report the statistics as JSON instead of tables | ✅ | — | — | For a tool rather than a reader: full precision, and the sample size behind each average. Pair with `-q` for a stdout that is nothing but JSON. See `docs/leveling-strategy.md`. |
| `-C`, `--CSV` | Write a CSV report to a file | ✅ | — | ✅ | Appends by default; `w:filename` truncates. Driven by the `csvrpt` statement. |
| `-T`, `--title` | Title for PBN output | ✅ | — | ✅ |  |

### Performance

| Switch | What it does | dealer3 | dealer.exe | DealerV2_4 | Notes |
|---|---|---|---|---|---|
| `-R`, `--threads` | Worker threads, 0 to auto-detect | ✅ | — | ✅ | DealerV2_4 uses it for its double-dummy solver; here it parallelises generation. |
| `--batch-size` | Work units per batch when parallel | ✅ | — | — |  |

### Reading deals in

| Switch | What it does | dealer3 | dealer.exe | DealerV2_4 | Notes |
|---|---|---|---|---|---|
| `--input-deals` | Filter deals from a file instead of generating | ✅ | ⚠️ -l replays from a library file by index | ⚠️ -L names a library path, -l exports DL52 | Reads PBN or one-line, auto-detected, `-` for stdin. Unrecognised lines are skipped, so check the reported count. |

### Recognised but not supported

| Switch | What it does | dealer3 | dealer.exe | DealerV2_4 | Notes |
|---|---|---|---|---|---|
| `-u` | Upper-case the honour cards in output | ⚠️ rejected with a message | ✅ | — | Cosmetic. |
| `-e` | Exhaust mode | ⚠️ rejected with a message | ⚠️ compiled out; prints "not included" | — | Never finished in the original either. |
| `-l` | Replay deals from a library file | ⚠️ rejected with a message | ✅ | ⚠️ -l exports DL52 | `--input-deals` covers this use case in dealer3's own way. |
| `--legacy` | The old single-threaded RNG mode | ⚠️ rejected with a message | — | — | Removed in 0.5.0; still parsed so a script using it gets an explanation. |

### Not implemented

| Switch | What it does | dealer3 | dealer.exe | DealerV2_4 | Notes |
|---|---|---|---|---|---|
| `-M` | Double-dummy solver mode | ❌ | — | ✅ | dealer3 has `tricks()` but no mode switch. |
| `-Z` | Export in RP zrd format | ❌ | — | ✅ |  |
| `-U` | DealerServer path | ❌ | — | ✅ |  |
| `-O` | OPC evaluation for the opener | ❌ | — | ✅ |  |
| `-D` | Debug verbosity 0-9 | ❌ | — | ✅ |  |
<!-- END GENERATED: switches -->

## Sources

dealer.exe's switches are its own getopt string, from `dealer.c` in
`Dealer-cleanup`:

```c
getopt (argc, argv, "023ehuvmqXp:g:s:l:V")
```

`-X` is in there but missing from the program's own usage line, so the usage
line is not a reliable source for what it accepts.

DealerV2_4's column comes from `dealer_vs_dealer2_switches.md`, compiled from
its manual, and has not been re-verified against a V2_4 build. Treat it as the
weaker of the two columns.

## The `-v` breaking change

dealer3 0.1 used `-v` for vulnerability. dealer.exe uses it for verbose, and
compatibility with dealer.exe matters more, so 0.2.0 moved vulnerability to
`--vulnerable` (long form only) and gave `-v` its original meaning. See
`CHANGELOG.md`.

## Where the letters collide

Three letters mean different things in different implementations, which is why
the table marks them ⚠️ rather than ✅:

- `-X` — force statistics on (dealer.exe, dealer3) vs export predeal holdings
  (V2_4).
- `-l` — replay from a library by index (dealer.exe) vs export DL52 (V2_4).
  dealer3 does neither; `--input-deals` covers the first use case in its own
  way.
- `-P` — vulnerability for par in V2_4; dealer3 uses `--vulnerable`.
