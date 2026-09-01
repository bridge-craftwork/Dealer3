# Performance comparison: dealer.exe, dealer-c, DealerV2_4, dealer3

A characterization, not a proof. It answers "roughly how much faster is
dealer3, and where does its threading stop paying?" to within a few percent,
which is the accuracy the question deserves.

## The five scripts

| Script | What it does | How often to run it |
|---|---|---|
| `scripts/bench-corpus.py` | Picks scripts from Practice-Bidding-Scenarios, rewrites them for benchmarking, checks each one runs on all three programs, collapses near-duplicates, and sizes each one's workload | Only when the script selection should change |
| `scripts/bench-reference.py` | Measures dealer.exe, dealer-c and DealerV2_4 | Only when the corpus changes, or one of those builds does |
| `scripts/bench-dealer3.py` | Measures dealer3: single-threaded, default, and a thread sweep | Whenever dealer3 changes substantively |
| `scripts/bench-report.py` | Joins the two result sets into one table | After either |
| `scripts/bench-verify.py` | Checks dealer3 *agrees* with both references on the same deals | With the corpus, and after language changes |

The split is the point: the reference numbers are the slow half (every
dealer.exe run is an SSH round trip) and the half that does not change.

```bash
./dev-build.sh build --release

# once, or when the selection changes
scripts/bench-corpus.py -n 10 --all-targets

# once per corpus
export DEALERV2_BIN=../Dealer-Version-2-/macos-build/dealerv2
scripts/bench-reference.py

# every time dealer3 changes
scripts/bench-dealer3.py
scripts/bench-report.py

# iterating on a performance change - one script, same one every time
scripts/bench-dealer3.py --quick

# semantics, not speed - safe to run any time
scripts/bench-verify.py
```

Two of the reference binaries have to be built first:

- **DealerV2_4** — upstream ships a Linux x86-64 binary that will not run on
  macOS. `scripts/build-dealerv2-macos.sh` builds it, DDS and all, and
  documents the four glibc-isms that need shimming.
- **dealer-c** — the original C dealer, built natively.
  `scripts/build-dealer-c-macos.sh`. Everything it needs is already in
  `../Dealer-cleanup/`, including `__random.c`, the GNU `random()` port, so it
  uses the original RNG rather than a substitute.

Either can be absent; `bench-reference.py` says so and measures the rest.

## The four programs, and which comparison means what

| | What it is | Read it as |
|---|---|---|
| `dealer.exe` | 32-bit x86, on an ARM64 Windows VM, **under emulation** | What running dealer.exe actually costs you today |
| `dealer-c` | The same C source, built natively for arm64 — and it deals **identically** to dealer.exe | The honest speed of the original implementation |
| `dealerv2_4` | DealerV2_4, built natively for arm64 | The honest speed of the modern C fork |

DealerV2_4's deal-and-filter loop is single-threaded, and the corpus calls no
solver function, so the table above measures it single-threaded throughout. Its
double-dummy solving is a different matter: `-R` sets DDS worker threads and
table mode (`-M 2`) defaults them if unset. Any comparison involving `dds()` or
`par()` has to match `-M`, `-R` and `-L` deliberately -- and `-L` with a `.zrd`
library means no solving at all, because the file already contains the
twenty results. See the note on the solver-agreement entry below.
| `dealer3` | This project, native, `-R 1` and threaded | — |

`dealer-c` deals the same boards as `dealer.exe`, verified byte-for-byte over
400 deals at seed 1 and 200 each at seeds 42 and 12345 (modulo CRLF). The
64-bit and 32-bit builds of `__random.c` differ only in bit 31, and dealer
indexes its card table from bits 15..=30. So the two are the same program doing
the same work, and the gap between them in the table is purely the cost of x86
emulation on ARM64.

`dealer-c` exists specifically to take emulation out of the comparison. Without
it the only C reference on equal silicon would be DealerV2_4, and dealer.exe's
number would get read as "the C implementation is slow" when much of the gap is
the emulation layer. With both present, the "vs dealer-c" column is the
algorithmic comparison and "vs exe" is the operational one.

## Why ten scripts, and which ten

The corpus is vendored into this repo, so every script in it has to earn its
place. Ten distinct ones characterize the three programs better than thirty
near-identical ones, and cost far less to carry.

The PBS `.dlr` files are **generated**: a precompiler expands shared fragments
inline and brackets each with `##### Imported Script: NAME #####`. 155 of the
347 scripts pull in at least one of ten such fragments, and 46 share the exact
same set. Separately, whole families differ only in a threshold — there are
seven `Weak_NT_*` scripts whose conditions are identical once the numbers are
masked. Picking ten of those would cost ten files and measure one thing.

So candidates are collapsed before selection. Two scripts are the same
benchmark when they import the same fragments *and* their normalized bodies —
comments stripped, integers masked — are at least 85% similar. Requiring the
import set to match too means two scripts are never merged merely because both
inline the same large shared fragment; what must be alike is the part that is
theirs. The threshold is not a guess: measured across the real corpus, members
of a family score **1.000** against each other while genuinely different
scripts score **0.04–0.22**, so nothing sits near 0.85.

Survivors are then picked along two axes at once — round-robin across distinct
import sets, taking from each the script that most extends the cost-per-deal
range covered so far. Different import sets mean different shared machinery is
exercised; different cost per deal means a change that only helps cheap or only
helps expensive conditions cannot hide. `corpus.json` records, for each row,
how many near-duplicates it stands for and which they were.

## Iterating on a change

The whole corpus is a slow loop. `--quick` (or `-1`) runs one script — the
median cost per deal, chosen deterministically, so two `--quick` runs are
comparable to each other:

```bash
scripts/bench-dealer3.py --quick --no-sweep     # fastest signal
scripts/bench-dealer3.py --quick                # with the thread sweep
scripts/bench-dealer3.py --scripts Bergen_Raises,Negative_Double
```

It is on `bench-reference.py` and `bench-verify.py` too. Use the full corpus
for anything being recorded or compared across revisions.

## Reading the two dealer3 numbers

They answer different questions and should not be collapsed into one headline.

**Single-threaded (`-R 1`) is how efficient the main code path is.** It is one
core doing one deal's work, directly comparable with the reference programs,
none of which thread their deal loop. It is also the stable number: a
single-threaded process gets a full core even on a machine that is otherwise
busy, so repeats land within a percent or two.

**Threaded (`auto`) is how effective we are with the cores available.** It is
wall-clock throughput, not one core's effort, so it belongs in a different
column of the mind. It is the number that matters in practice, because a normal
run is threaded.

As measured here, on an Apple M4 Pro (8P + 4E), against the natively-built
original C dealer:

| | vs `dealer-c` |
|---|---|
| dealer3 `-R 1` | ~11% slower |
| dealer3 `-R 1` vs DealerV2_4 | ~14% faster |
| **dealer3 threaded** | **~4.4x faster** |

The threaded figure is the real win, and it is a **floor rather than a
ceiling**: the machine it was measured on was not idle, and where a
single-threaded run simply takes one free core, a twelve-thread run contends
with whatever else is running for the other eleven. That is also why the
threaded rows are the noisiest in the report — repeats vary by tens of percent
where single-threaded repeats vary by one or two — so read them to two
significant figures at most.

## What is actually being measured

**Deals evaluated per second, at a fixed deal count.** Not "time to produce 40
matching hands". The three programs do not deal the same boards — dealer3
deliberately stopped reproducing dealer.exe's sequence in 0.5.0 — so producing
40 matches is a different amount of work for each of them, decided by whose
shuffle happens to hit the condition sooner. `-g N` makes all three evaluate
exactly N deals against the same condition, and that is comparable. `-p` is
pinned high enough never to bind, and every sample is checked to confirm the
run really did generate N deals.

**Condition evaluation, not output.** Each corpus script's `action` block is
replaced with a cheap `average`, so the number reflects filter throughput
rather than how fast `printall` writes to a pipe.

Three traps the harness exists to avoid, each of which silently produces a
plausible wrong number:

- **A script's own `produce`/`generate` beats the command line.** In `dealer.c`
  `yyparse()` runs after `getopt()`, and the limits are only defaulted if still
  zero. 120 of the 347 PBS scripts carry such a line. The corpus strips them.
- **`-v` turns dealer.exe's timing *off*.** Its verbose flag defaults to on and
  the switch XORs it (`dealer.c:1608`). The VM's build is older than the local
  source and has no `-X` to force it on either, and `-v` there has a separate
  known bug tied to whether an odd or even number of PBN rows were written. So
  that target is passed no verbosity flag at all.
- **dealer.exe's own clock is broken on Windows.** It prints
  `Time needed 0.000 sec` however long it ran. That target is therefore
  wall-clocked, with SSH and startup overhead measured once (~0.28s) and
  subtracted. dealer3 and DealerV2_4 report honest times and are trusted.
  Every result records which clock it used.

**dealer.exe is measured under emulation, and that is not a footnote.** The
Windows VM is ARM64 with 4 cores, while `dealer.exe` is a PE32/i386 binary, so
it runs through Windows-on-ARM's x86 emulation layer. `bench-reference.py`
records the VM's architecture in its results and prints a warning when it sees
ARM64. This is why `dealer-c` and `dealerv2_4` are in the table: they are the
same lineage on the same silicon, with no emulation in the way.

**Best of N runs.** The fastest run is the least contaminated by whatever else
the machine was doing. The median and the spread are kept alongside; a spread
over 15% is flagged, because it means the machine was busy rather than that the
code changed.

## Separating the shuffle from the filter

dealer3 rewrote the RNG and the shuffle, and the originals were slow, so a
large part of any headline ratio is deal *generation* rather than condition
*evaluation*. Those are different pieces of work that improve for different
reasons, and one deals/sec number cannot tell them apart — on a cheap condition
the shuffle dominates, on an expensive one it barely registers.

So the corpus carries one synthetic entry, `_shuffle_baseline`, that is not
from PBS and is not subject to the duplicate or diversity rules. Its condition
is as close to free as the language allows — a single `hcp()` and a comparison
— so its throughput is essentially generation alone. `bench-report.py`
subtracts its cost per deal from each script's to get evaluation cost, and
reports the two separately along with each program's generation share.

The subtraction is done in **nanoseconds per deal, not in rates** — times
subtract honestly and rates do not. Where a real script comes out cheaper than
the baseline the difference is noise, so evaluation cost is floored at zero
rather than reported as negative.

Its threshold (`hcp(north) >= 24`) is rare but not impossible, so `average`
always has at least some samples; a condition nothing satisfies would leave
each program to decide what to print for an empty average, which is not worth
finding out.

## Reading the thread sweep

`bench-dealer3.py` runs each script at 1, 2, 3 … threads and reports speedup
against single-threaded.

The distinction that matters is **plateau versus regression**. A curve that
flattens past the physical core count is ordinary saturation. A curve that
turns down — more workers, less throughput — is contention, and that is a bug.

**Sweep runs must be long enough or the curve is fiction.** At a few hundred
milliseconds a run is mostly thread startup and the final merge, and the shape
that comes out is noise. This is not hypothetical: the first measurement taken
while building this harness showed throughput *regressing* past 6 threads, and
it was an artifact of single short runs. Best-of-three at a properly sized
workload showed clean monotonic scaling on the same script. `bench-dealer3.py`
now pilots the highest thread count and scales the whole sweep up until even
the fastest configuration runs past `--min-sweep-seconds`, so the trap is
closed by default rather than left to the reader.

To check a curve is real, vary the workload and see whether its shape holds:

```bash
scripts/bench-dealer3.py --sweep-only --scale 0.2   # short
scripts/bench-dealer3.py --sweep-only --scale 4     # long
```

If the turnover moves with workload size, the cost is per-run. If it stays put,
it is contention in the generate loop. `--batch-sizes` adds the work-unit size
as a second dimension, the obvious suspect for shared state.

One caveat worth keeping in mind: this measures fixed-work runs, which is the
right way to compare the three programs, but it is *not* what a user feels. A
typical invocation is `-p 40` — a short run where thread startup is a real
fraction of the total. Threading will always look better here than it does
there.

## Correctness, from the same corpus

A throughput number means nothing if the three programs disagree about what a
script *says*, so `bench-verify.py` reuses the corpus for that too. Per script,
per reference:

1. The reference runs with `printpbn` added to its action list, emitting both
   the deals it produced and its statistics.
2. Those deals go into dealer3 through `--input-deals`, script unchanged.
3. dealer3 must match **every** deal the reference matched, and must report the
   same `average` over them.

Both programs are then looking at identical cards, so a disagreement is a real
difference in what a word means and never an artefact of the shuffle — which is
what makes this work at all, given the three stopped dealing alike in 0.5.0.

Statistics are compared **by value, not as text**. The three do not agree on
how to print an average: dealer.exe and dealer3 write `label: 10.66`, while
DealerV2_4 writes `label: Mean=   10.6600, Std Dev= …, Sample Size=50`. Those
are the same answer; comparing the strings would fail on every line and bury
any real difference in cosmetic noise.

This is `scripts/test-filter.py`'s idea — which covers dealer.exe only, and
acceptance only — widened to both references and to the statistics, and run
across the whole corpus in one command. The PBN plumbing is imported from
`scripts/compare-stats.py` rather than copied, so the two cannot drift apart.

## Files

```
bench/
  corpus.json          what got selected, what was rejected and why
  corpus/*.dlr         the normalized scripts, committed so results are comparable
  results/
    reference-<rev>.json   dealer.exe + DealerV2_4, keyed to the corpus revision
    dealer3-<rev>.json     dealer3, keyed to the dealer3 revision
  .staging/, .verify/      scratch, not committed
```

Reference results are keyed to the corpus revision they were measured against.
`bench-report.py` warns if you compare across a corpus change, because the
per-script deal counts are calibrated and numbers taken against a different
corpus describe different amounts of work.
