# dealer-dds

Double-dummy analysis for dealer3: how many tricks each declarer can take in
each denomination with all four hands visible.

The search is not here. It is in
[bridge-solver](https://github.com/bridge-craftwork/bridge-solver), a port of
[macroxue/bridge-solver](https://github.com/macroxue/bridge-solver): MTD(f) over
a pattern-based transposition table with hierarchical bounds, move ordering and
fast trick estimation. This crate is the adaptor between that and dealer3, and
the memory that keeps a deal from being searched twice.

## What the adaptor has to get right

Three conversions sit between a `dealer_core::Deal` and an answer, and each is
silent when it is wrong:

- `dealer_core::Deal` to `bridge_types::Deal` (which `dealer-core` provides).
- The opening leader. `bridge_solver::Solver::new` takes the seat *on lead*,
  which is the declarer's left-hand opponent, not the declarer.
- The side. `Solver::solve` returns North/South's tricks; `tricks()` wants the
  declarer's, so East and West are `total - ns`.

`tests/dd_tables_match_an_independent_solver.rs` checks all three against
BridgeComposer's own tables for 36 real boards — 720 values that share no code
with this crate.

## The memory

A double-dummy search is milliseconds where the rest of the language is
microseconds, so what matters is how few of them run. Results are remembered per
deal, against the denomination and declarer they answer for:

- Writing `tricks(south, spades)` out longhand in four places is one search.
- Asking about several denominations is one search each, not a full table.
- A `condition` that calls `tricks()` and an `average` that calls it again —
  which the generator evaluates on different threads, a batch apart — is still
  one search.

Within a denomination the solver's cutoff and pattern caches are shared across
the four declarers, which is most of what a full table would save; the caches
run to several megabytes, so they live only as long as the deal in hand.

## API

```rust
use dealer_dds::{tricks, solve_all, DealAnalysis, Denomination};
use dealer_core::Position;

// One question, remembered for whatever asks next.
let n = tricks(&deal, Denomination::Spades, Position::North);

// Several questions about one deal, without going through the memo.
let mut analysis = DealAnalysis::new(&deal);
let nt = analysis.tricks(Denomination::NoTrump, Position::South);

// All twenty.
let table = solve_all(&deal);
```

`dealer_dds::searches()` reports how many searches the process has run, for
tests that care that an answer was remembered rather than worked out again.

## Speed

Release build, M-series Mac, whole 20-entry tables for 36 tournament boards:
20 ms for the quickest, 480 ms for the slowest, around 60 ms in the middle. A
single question about one denomination is roughly 10 ms.

`scripts/dd-bench.sh` is the regression benchmark; its budgets are the
acceptance criteria from issue #14.

## Testing

```bash
cargo test -p dealer-dds              # a few oracle boards
cargo test -p dealer-dds --release    # all 36
```

An unoptimised build of the solver is some fifteen times slower than an
optimised one, which is why the fixture is only fully checked in release.

## License

This project is released into the **public domain** under
[The Unlicense](../LICENSE). `bridge-solver`, which does the searching, is MIT
OR Apache-2.0 and is credited by `dealer --credits`.
