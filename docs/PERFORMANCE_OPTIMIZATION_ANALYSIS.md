# Performance Optimization Analysis: C dealer.exe vs dealer3 Rust

**Date**: 2026-01-07
**Status**: Analysis Complete - Implementation Pending

## Executive Summary

This document analyzes the key performance differences between the original C `dealer.exe` implementation and the Rust `dealer3` implementation. The C version achieves significantly better performance through pre-computed statistics and lookup tables, while the current Rust implementation recomputes values on-demand.

## Hot Loop Comparison

### Original C Approach (dealer.c)

#### Key Optimizations:

1. **Pre-computed `handstat` structure** (`analyze` function, lines 513-659)
   - Computed once per deal before evaluation
   - All values stored in a flat struct with direct array indexing
   - Uses lookup table `tblPointcount[type][rank]` for all point calculations

2. **Shape bitmap** `distrbitmaps[14][14][14][14]`
   - Pre-computed for O(1) shape matching
   - Single bitwise AND operation to match any shape pattern

3. **Direct array access in `evaltree`** (lines 1099-1262)
   - `hs[compass].hs_totalpoints` - single array lookup for HCP
   - `hs[compass].hs_length[suit]` - single array lookup for suit length
   - `hs[compass].hs_bits & pattern` - bitwise AND for shape matching
   - No function calls, no iteration - just array indexing

4. **Global state / no allocation**
   - `struct handstat hs[4]` is a global array - zero allocations
   - Tree nodes are allocated once during parsing, never touched during evaluation

### Current dealer3 Rust Approach

#### Current Issues:

1. **On-demand computation instead of pre-computed stats**
   ```rust
   pub fn hcp(&self) -> u8 {
       self.cards.iter().map(|c| c.hcp()).sum()  // Iterates 13 cards every call
   }
   ```
   - Every `hcp(north)` call iterates all 13 cards!

2. **No pre-analysis phase**
   - C does `analyze(deal)` once, then all lookups are O(1)
   - Rust recomputes stats on every function call

3. **`Vec<Card>` storage**
   - `Hand { cards: Vec<Card> }` - heap allocation
   - C uses flat `deal[52]` array with card encoding `(suit << 6) | rank`

4. **Iterator chains instead of lookup tables**
   - `self.cards.iter().filter(|c| c.suit == suit).count()` vs `hs[p].hs_length[s]`
   - Multiple passes over cards for different stats

## Proposed Rust Optimizations

### 1. Add `HandStats` struct computed once per deal

```rust
struct HandStats {
    hcp: [u8; 4],           // Per-suit HCP
    total_hcp: u8,
    lengths: [u8; 4],       // Per-suit lengths
    controls: [u8; 4],
    total_controls: u8,
    losers: [u8; 4],
    total_losers: u8,
    shape_bits: u64,        // Pre-computed shape bitmap
    // ... other stats
}
```

### 2. Use lookup tables for point values

```rust
const HCP_TABLE: [u8; 13] = [0,0,0,0,0,0,0,0,0,1,2,3,4]; // ranks 2-A
```

### 3. Compute `HandStats` once before evaluation loop

The evaluation loop should:
1. Generate/shuffle the deal
2. Call `analyze(deal)` to populate `HandStats` for all 4 hands
3. Run the condition evaluation using only O(1) lookups

### 4. Use fixed-size arrays instead of `Vec<Card>`

```rust
struct Hand {
    cards: [Card; 13],  // Stack-allocated, cache-friendly
}
```

### 5. Pre-compute shape masks

Like the C version does with `distrbitmaps`:
- Pre-compute a 4D lookup table for shape patterns
- Shape matching becomes a single bitwise AND operation

## Penguin Dealer Optimizations (5.3x Faster than dealer.exe)

Analysis of `../penguin-dealer` reveals the following key optimizations:

### 1. Fast Random Number Generation - Lookup Table Instead of Modulo

**Location**: `fast_randint.c`

The most critical optimization replaces expensive modulo operations with a pre-computed lookup table:

```c
#define NUMBITS 10
#define LOOKUP_SIZE (1 << NUMBITS)  // 1024 entries
int precomputed[52 * LOOKUP_SIZE];  // 52KB table for each max value 1-52

unsigned char fast_randint(unsigned char max) {
    int result = NC;
    while (result == NC) {
        result = precomputed[((max - 1) << NUMBITS) |
                             (RANDOM() & (LOOKUP_SIZE - 1))];
    }
    return result;
}
```

**Why this is fast**:
- Eliminates expensive division/modulo: `rand() % N` requires integer division (~50 cycles → 3-4 cycles)
- Uses bitwise operations only
- 95% success rate on first lookup (avoids retry loop almost always)
- 52KB table trades memory for speed in tight hot loop

### 2. Selective Hand Analysis Based on Usage

**Location**: `dealer.c` lines 544-554

```c
int use_compass[NSUITS];  // Tracks which players are actually used

void analyze(deal d, struct handstat *hsbase) {
    for (player = COMPASS_NORTH; player <= COMPASS_WEST; ++player) {
        if (use_compass[player] == 0) {  // Skip if not used in conditions!
            continue;
        }
        // Only compute for used players
    }
}
```

**Why this is fast**:
- If a filter only checks North and South, skip East and West entirely
- Avoids 50% of analysis work in many common cases
- Parser marks `use_compass[player]` when building decision tree

### 3. Meet-in-the-Middle for Exhaustive Mode

**Location**: `dealer.c` lines 938-1141 (when `#ifdef FRANCOIS` is enabled)

For constrained deals where only 2 players have unknown cards:

```c
#define TWO_TO_THE_13 (1<<13)  // 8192

// Pre-computed tables for each half (MSB and LSB):
unsigned char exh_msb_suit_length[NSUITS][TWO_TO_THE_13];
unsigned char exh_lsb_suit_length[NSUITS][TWO_TO_THE_13];
unsigned char exh_msb_suit_points[NSUITS][TWO_TO_THE_13];
unsigned char exh_lsb_suit_points[NSUITS][TWO_TO_THE_13];

void exh_analyze_vec(int high_vec, int low_vec, struct handstat *hs) {
    // Instant hand analysis via table lookup!
    for (s = SUIT_CLUB; s <= SUIT_SPADE; s++) {
        hs0->hs_length[s] = exh_lsb_suit_length[s][low_vec] +
                            exh_msb_suit_length[s][high_vec];
        hs0->hs_points[s] = exh_lsb_suit_points[s][low_vec] +
                            exh_msb_suit_points[s][high_vec];
    }
}
```

**Why this is fast**:
- Bypasses expensive `analyze()` function entirely
- Pre-computed tables: O(1) hand evaluation
- Enumeration only ~C(26, 13) = ~10 million deals (vs 635 billion random)
- Two 8K tables instead of one 600MB table

### 4. Macro-Based Expression Evaluation Fast Path

**Location**: `dealer.c` line 1332

```c
#define interesting() ((int)evaltree(decisiontree) && right_predeal_lengths())
```

**Why this matters**:
- Called for EVERY generated deal in hot loop
- Macro avoids function call overhead
- Short-circuit AND: if tree check fails, predeal length check skipped

### 5. Efficient Hand Statistics Structure

**Location**: `dealer.h` lines 32-48

```c
struct handstat {
    int hs_length[NSUITS];           // 4 ints
    int hs_points[NSUITS];           // 4 ints (frequently accessed)
    int hs_totalpoints;              // 1 int (cached total)
    int hs_bits;                     // 1 int (pre-computed shape bitmap)
    int hs_loser[NSUITS];            // 4 ints
    int hs_totalloser;               // 1 int
    int hs_control[NSUITS];          // 4 ints
    int hs_totalcontrol;             // 1 int
    int hs_counts[idxEnd][NSUITS];   // Multiple point-count types
    int hs_totalcounts[idxEnd];      // Pre-computed totals
};

extern struct handstat hs[4];  // Global array, one per player
```

**Why this is fast**:
- All four players' stats cached globally (L1 cache friendly)
- Total values pre-computed and cached
- Sequential memory access pattern

### 6. Double-Dummy Result Caching

**Location**: `dealer.c` lines 281-299

```c
int dd(deal d, int l, int c) {
    static int cached_ngen = -1;
    static char cached_tricks[4][5];

    if (ngen != cached_ngen) {
        memset(cached_tricks, -1, sizeof(cached_tricks));
        cached_ngen = ngen;
    }
    if (cached_tricks[l][c] == -1) {
        cached_tricks[l][c] = true_dd(d, l, c);  // Expensive call
    }
    return cached_tricks[l][c];
}
```

### Summary: Penguin's Key Insights

| Optimization | Speedup Factor | Applicability to dealer3 |
|--------------|----------------|--------------------------|
| Fast RNG (no modulo) | ~10x in shuffle | High - Rust can use similar lookup |
| Selective analysis | ~2x for common filters | High - skip unused compass positions |
| Meet-in-the-middle | ~100x for exhaustive | Medium - useful for predeal scenarios |
| Macro inlining | ~1.2x | Low - Rust inlines well already |
| Cache-friendly structs | ~1.5x | High - use `[T; 4]` arrays |
| DD caching | ~50x for DD queries | High - when DD is implemented |

**The key insight is trading memory for speed**: 52KB for RNG, 8KB per exhaustive mode table, global hand stat caching. This strategy still works well on modern CPUs with large caches

## Implementation Priority (Updated with Penguin Insights)

### Phase 1: HandStats Pre-computation (Highest Impact)
- Add `HandStats` struct to `dealer-core`
- Implement `analyze()` function that computes all stats in a single pass
- Modify evaluator to use pre-computed stats instead of on-demand computation
- **New**: Track which compass positions are used and skip unused ones

### Phase 2: Fast RNG (Penguin's Top Optimization)
- Replace modulo operation in shuffle with lookup table
- Pre-compute 52KB table mapping `(max, random_bits) → result`
- Eliminates ~50 cycle integer division per card dealt

### Phase 3: Fixed-Size Arrays
- Replace `Vec<Card>` with `[Card; 13]` in `Hand` struct
- Ensure cache-friendly memory layout
- Consider `[HandStats; 4]` as a single cache-line-aligned struct

### Phase 4: Lookup Tables
- Add `HCP_TABLE`, `CONTROLS_TABLE`, etc.
- Pre-compute shape bitmaps for O(1) pattern matching

### Phase 5: Meet-in-the-Middle (For Constrained Deals)
- When 2+ hands are fully predealt, enumerate remaining possibilities
- Pre-compute statistics tables for 13-bit card vectors
- Useful for simulation scenarios with heavy predeal constraints

### Phase 6: Profile and Tune
- Use `perf` or `flamegraph` to identify remaining bottlenecks
- Consider SIMD for batch operations if justified

## Benchmarking Approach

When implementing these optimizations:

1. Use `cargo bench` with criterion for micro-benchmarks
2. Use `compare-dealer` script to measure real-world performance vs dealer.exe
3. Track both:
   - Deals per second
   - Time to produce N matching deals (includes evaluation overhead)

## Related Files

### dealer3 (Rust)
- `dealer-core/src/hand.rs` - Current `Hand` implementation
- `dealer-eval/src/lib.rs` - Expression evaluator (hot path)
- `dealer-core/src/deal.rs` - Deal generation
- `dealer-core/src/rng.rs` - RNG implementation (candidate for fast_randint optimization)

### Reference Implementations
- Original C: `../Dealer-cleanup/dealer.c`
- Penguin: `../penguin-dealer/dealer.c`
- Penguin fast RNG: `../penguin-dealer/fast_randint.c`
- Penguin headers: `../penguin-dealer/dealer.h`

## Notes

The recent 9.6x speedup (commit c6fccb0) from using references instead of cloning `Expr` trees addressed AST traversal overhead, but did not address the fundamental issue of on-demand stat computation vs pre-computed stats. The optimizations described here would provide additional significant improvements.
