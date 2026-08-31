# Multithreading Design for dealer3

**Date**: 2026-01-07
**Status**: Implemented. Updated 2026-08-25: legacy mode was removed in 0.5.0;
the sections describing it are retained as history and marked as such.

## Overview

dealer3 generates deals with xoshiro256++ and stateless per-deal generation,
which parallelises freely: each deal derives from its own `u64` seed, so workers
share no state and output does not depend on thread count.

> **Historical:** a second mode (`--legacy`) once used a port of the GNU
> `random()` from the original C dealer to reproduce its exact deal sequence. It
> was single-threaded by necessity, since that generator's shuffle state carries
> from one deal to the next. It was removed in 0.5.0 once the regression suite
> could verify filter behaviour without it — see `docs/REGRESSION_TESTING.md`.
> Sections below that describe it are kept for context.

## Performance Results

### Benchmark: `debug_double_advancer.dlr` with `-g 20000000 -p 10`

| Implementation | Time | CPU % | Speedup |
|----------------|------|-------|---------|
| C dealer.exe | 16.1s | 99% | 1.0x (baseline) |
| Rust dealer3 legacy | 14.0s | 99% | 1.15x |
| Rust dealer3 fast (12 threads) | 3.1s | 866% | **5.2x** |

### Benchmark: `hcp(north) >= 15` with `-p 500000`

| Threads | Time | CPU % | Speedup |
|---------|------|-------|---------|
| 1 | 2.92s | 103% | 1.0x |
| 2 | 1.75s | 184% | 1.7x |
| 4 | 1.10s | 318% | 2.7x |
| 8 | 0.84s | 544% | 3.5x |
| 12 | 0.74s | 770% | 3.9x |

### Key Findings

- **Core generation is well parallelized** - 770-866% CPU utilization with 12 threads
- **Rust was 1.15x faster than C** even in the single-threaded legacy mode (removed in 0.5.0)
- **Fast mode achieves 5x+ speedup** over C dealer.exe
- **Output serialization is a bottleneck** - writing output reduces parallelism significantly
- **Filter evaluation is significant** - complex filters add overhead per deal

## Architecture

### Fast Mode (Default)

Fast mode uses stateless deal generation where each deal depends only on its seed, enabling embarrassingly parallel execution.

```
┌─────────────────────────────────────────────────────────────┐
│                    FastSupervisor                           │
│  - Generates seed sequence (trivially fast)                 │
│  - Dispatches seeds to workers in batches                   │
│  - Collects and orders results by serial number             │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ dispatches (serial_num, seed) - 16 bytes
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                       Worker Pool (rayon)                   │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐       │
│  │ Worker  │  │ Worker  │  │ Worker  │  │ Worker  │  ...  │
│  │   0     │  │   1     │  │   2     │  │   3     │       │
│  │         │  │         │  │         │  │         │       │
│  │ seed -> │  │ seed -> │  │ seed -> │  │ seed -> │       │
│  │  deal   │  │  deal   │  │  deal   │  │  deal   │       │
│  └─────────┘  └─────────┘  └─────────┘  └─────────┘       │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ returns (serial_num, deal, passed)
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     Result Processing                       │
│  - Results sorted by serial number                          │
│  - Matching deals output in order                           │
│  - Statistics accumulated                                   │
└─────────────────────────────────────────────────────────────┘
```

#### Key Design Decisions

1. **Stateless Generation**: Each deal is generated from just a seed using:
   - xoshiro256++ RNG (fast, high-quality, 256-bit state)
   - SplitMix64 for seed expansion
   - Fisher-Yates shuffle (same algorithm as dealer.exe)

2. **Minimal Work Units**: Only 16 bytes per work unit (serial number + seed)
   - Compare to legacy: ~300 bytes (RNG state + shuffle state)

3. **No Shared State**: Workers are fully independent - no locks, no contention

4. **Deterministic Output**: Same seed always produces same sequence of deals

### Legacy Mode (`--legacy`) — removed in 0.5.0

Described here for historical context. Legacy mode preserved exact dealer.exe
compatibility by using:
- A port of GNU `random()`, which reproduced dealer.exe's deal sequence
- Sequential shuffle-state-dependent generation
- Single-threaded execution only

That port now lives in
[dealer-legacy-shuffle](https://github.com/bridge-craftwork/dealer-legacy-shuffle).
Earlier revisions of this document described it as 64-bit and said dealer.exe
was too; the binary is in fact PE32/i386, and the 64-bit behaviour came from
macOS builds of the same source. See that repository's `PROVENANCE.md`.

```
┌─────────────────────────────────────────────────────────────┐
│                    DealGenerator (removed)                  │
│  - Owned a single legacy-shuffle RNG instance                │
│  - Each deal depends on previous shuffle state              │
│  - Must be sequential for determinism                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ generates deals one at a time
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     Sequential Output                       │
│  - Exact match with dealer.exe for same seed                │
│  - Byte-for-byte compatible output                          │
└─────────────────────────────────────────────────────────────┘
```

## Implementation Details

### RNG: xoshiro256++

Fast mode uses xoshiro256++ (Vigna & Blackman), chosen for:
- **Speed**: One of the fastest high-quality PRNGs
- **Quality**: Passes BigCrush and PractRand statistical tests
- **Period**: 2^256 - 1 (effectively infinite)
- **Stateless seeding**: SplitMix64 expands any u64 seed to full state

```rust
// Seed expansion using SplitMix64
pub fn seed_from_u64(seed: u64) -> Self {
    let mut z = seed;
    let mut state = [0u64; 4];
    for s in &mut state {
        z = z.wrapping_add(0x9e3779b97f4a7c15);
        let mut x = z;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        *s = x ^ (x >> 31);
    }
    Self { s: state }
}
```

### Unbiased Index Generation

Uses Lemire's nearly divisionless method to avoid modulo bias:

```rust
pub fn next_index(&mut self, n: u32) -> u32 {
    // Fast path for powers of 2
    if n.is_power_of_two() {
        return self.next_u32() & (n - 1);
    }

    // Lemire's method - avoids division in common case
    let mut x = self.next_u32();
    let mut m = (x as u64) * (n as u64);
    let mut l = m as u32;

    if l < n {
        let t = n.wrapping_neg() % n;
        while l < t {
            x = self.next_u32();
            m = (x as u64) * (n as u64);
            l = m as u32;
        }
    }

    (m >> 32) as u32
}
```

### Stateless Deal Generation

```rust
pub fn generate_deal_from_seed_no_predeal(seed: u64) -> Deal {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);

    // Fisher-Yates shuffle
    let mut deck: [u8; 52] = std::array::from_fn(|i| i as u8);
    for i in (1..52).rev() {
        let j = rng.next_index((i + 1) as u32) as usize;
        deck.swap(i, j);
    }

    // Distribute to hands
    Deal::from_deck(&deck)
}
```

### Predeal Support

Predeal uses a two-phase approach:
1. Place predealt cards in their designated positions
2. Fisher-Yates shuffle only the remaining cards into remaining slots

This maintains full parallelism while supporting predeal constraints.

## Command-Line Interface

```
-R N, --threads N     Number of worker threads (default: 0 = auto-detect)

                      Required for exact deal sequence matching with dealer.exe

--batch-size N        Work units per batch (default: auto = 200 × threads)
```

## Validation

The `deal-validator` tool validates that fast mode produces correct deals:

```bash
# Validate deals against filter
./dealer -p 1000 -s 42 -f oneline filter.dlr | ./deal-validator filter.dlr

# Output:
# === Validation Summary ===
# Filter file: filter.dlr
# Total deals: 1000
# Passed:      1000 (100.0%)
# Failed:      0 (0.0%)
# ✅ VALIDATION PASSED: All 1000 deals match filter
```

## Trade-offs

### Fast Mode
- **Pros**: 5x+ speedup, scales with cores, fully parallel
- **Cons**: Different deal sequences than dealer.exe (same statistical properties)

### Legacy Mode (removed in 0.5.0)
- **Pros**: Exact dealer.exe compatibility, byte-for-byte identical output
- **Cons**: Single-threaded only, no parallelism possible due to shuffle-state
  dependency; and it carried a third-party RNG whose licence terms this project
  could not waive

## Future Optimizations

Potential improvements identified through profiling:

1. **Async output buffering** - Use separate output thread to avoid blocking workers
2. **Batch result streaming** - Output results as batches complete rather than waiting
3. **SIMD filter evaluation** - Vectorize simple HCP/shape checks
4. **Profile-guided optimization (PGO)** - Could gain 10-20% additional performance

## Files

- `dealer-core/src/rng.rs` - Xoshiro256PlusPlus implementation
- `dealer-core/src/fast_deal.rs` - Stateless deal generation
- `dealer/src/fast_parallel.rs` - FastSupervisor for parallel dispatch
- `dealer/tests/regression_hash.rs` - pins that output is thread-count independent

## References

- [xoshiro / xoroshiro generators](https://prng.di.unimi.it/) - Vigna & Blackman
- [Lemire's nearly divisionless method](https://lemire.me/blog/2019/06/06/nearly-divisionless-random-integer-generation-on-various-systems/)
- [Fisher-Yates shuffle](https://en.wikipedia.org/wiki/Fisher%E2%80%93Yates_shuffle)
