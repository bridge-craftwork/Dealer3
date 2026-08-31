# Performance Pre-test Results

**Date**: 2026-01-07
**Test Case**: `../Practice-Bidding-Scenarios/dlr/gerber_by_opener.dlr`
**Parameters**: `-p 4 -s 42` (produce 4 deals, seed 42)

## Summary

| Implementation | Avg Time (s) | Generated Hands | Relative Speed |
|----------------|--------------|-----------------|----------------|
| Penguin C      | 0.64         | 920,282         | 1.0x (baseline)|
| Original C     | 2.43         | 1,506,263       | 3.8x slower    |
| Rust dealer3   | 5.49         | 1,506,263       | 8.6x slower    |

**Note**: Penguin dealer uses a different RNG, so it generates fewer hands to find 4 matches.
Comparing Rust to Original C (same RNG): Rust is **2.3x slower**.

## Detailed Results

### Rust dealer3 (release build)

```
Run 1: 5.45s user 0.02s system 99% cpu 5.483 total
Run 2: 5.51s user 0.02s system 99% cpu 5.539 total
Run 3: 5.52s user 0.02s system 99% cpu 5.542 total

Generated 1506263 hands
Produced 4 hands
Initial random seed 42
Time needed    5.574 sec (self-reported)
```

**Average**: 5.49s

### Original C dealer (c-dealer)

```
Run 1: 2.40s user 0.01s system 99% cpu 2.414 total
Run 2: 2.45s user 0.02s system 99% cpu 2.477 total
Run 3: 2.43s user 0.01s system 99% cpu 2.447 total

Generated 1506263 hands
Produced 4 hands
Initial random seed 42
Time needed    2.429 sec (self-reported)
```

**Average**: 2.43s

### Penguin C dealer (penguin-dealer)

```
Run 1: 0.63s user 0.00s system 99% cpu 0.636 total
Run 2: 0.64s user 0.00s system 99% cpu 0.647 total
Run 3: 0.63s user 0.00s system 99% cpu 0.635 total

Generated 920282 hands
Produced 4 hands
Initial random seed 42
Time needed    0.638 sec (self-reported)
```

**Average**: 0.64s

## Analysis

1. **Rust vs Original C**: Rust is 2.3x slower when processing the same number of hands (1,506,263).
   - This suggests optimization opportunities in the Rust implementation.

2. **Penguin dealer**: Uses a different RNG that finds matches faster (920K vs 1.5M hands).
   - Even accounting for fewer hands, Penguin processes ~1.4M hands/second vs Original C at ~620K hands/second.
   - Penguin appears to have additional optimizations beyond just RNG differences.

3. **Rust throughput**: ~274K hands/second (1,506,263 / 5.49s)

4. **Original C throughput**: ~620K hands/second (1,506,263 / 2.43s)

5. **Penguin throughput**: ~1.44M hands/second (920,282 / 0.64s)

## Target

To match Original C performance, Rust needs to improve by **2.3x**.
To match Penguin performance (per-hand), Rust needs to improve by **5.3x**.

## Test File Characteristics

The `gerber_by_opener.dlr` file is a complex constraint file with:
- 50+ variable assignments
- Multiple `shape()` calls with complex patterns
- `top4()` function calls
- `hcp()` function calls
- `losers()` function calls
- Ternary expressions
- Logical combinations (and/or/not)

This makes it a good benchmark for expression evaluation performance.

---

## Optimization Attempt: HandStats Pre-computation

**Date**: 2026-01-07 (later session)

### Changes Made

Implemented items 1-4 from `docs/PERFORMANCE_OPTIMIZATION_ANALYSIS.md`:

1. **HandStats struct** (`dealer-core/src/stats.rs`):
   - Pre-computed statistics for a hand (HCP, controls, losers, shape, rank counts, etc.)
   - Lookup tables for HCP, controls, C13 points
   - Single-pass `analyze()` function that computes all stats in O(13) time

2. **Modified EvalContext** to compute all hand stats eagerly at creation time

3. **Updated all evaluator functions** to use pre-computed stats instead of iterating through cards

### Results

**After optimization**: ~5.75s (vs 5.49s baseline)

No improvement - actually slightly worse. The overhead of computing all stats for all 4 hands upfront roughly equals the savings from O(1) lookups.

### Analysis

The optimization assumes multiple lookups per hand justify upfront computation. However:

1. **Per-deal analysis overhead**: Computing stats for all 4 hands = 52 card iterations
2. **Original approach**: Only iterates cards when actually needed
3. **Many deals fail early**: Short-circuit evaluation means many constraints fail before accessing all hands

The pre-computation only helps when the same hand's stats are accessed many times. For this test case with heavy variable caching (50+ variables), the caching already eliminates redundant computation.

### Next Steps

Consider:
1. **Profile the actual bottleneck** - it may not be in hand stat computation
2. **Item 3**: Replace `Vec<Card>` with `[Card; 13]` for better cache locality
3. **Expression tree optimization** - reduce tree traversal overhead
4. **Variable caching improvements** - may be the bigger win

---

## Investigation: Function Call Caching

**Date**: 2026-01-07 (continued)

### Hypothesis

Direct function calls like `hcp(south)` (which appears 13 times in the test file) are NOT cached by the variable caching system. Only variables get cached.

### Testing

Tried multiple approaches:
1. **Eager HandStats for all 4 hands** - ~5.75s (slightly worse than baseline)
2. **Lazy HandStats** (compute on first access) - ~5.67s (same as baseline)
3. **Added fn_cache HashMap** - increased overhead, no benefit

### Key Finding

The HandStats approach provides O(1) lookups after initial computation, BUT:
- Computing stats for a hand = iterate 13 cards once
- Original approach (e.g., `hand.hcp()`) = iterate 13 cards once per call
- With variable caching, each expression is only evaluated once anyway

So `hcp(south)` called 13 times in the source file actually evaluates to:
- First use via a variable → compute once, cache result
- Subsequent uses of same variable → O(1) lookup from variable cache

The existing variable caching already provides the benefit we were trying to add.

### Conclusion

The ~2.3x slowdown vs C is NOT due to:
- ❌ Lack of function call caching
- ❌ Recomputing hand statistics

It IS likely due to:
- Expression tree traversal overhead (Rust enum matching vs C direct calls)
- HashMap overhead for variable lookups
- Memory allocation patterns
- The difference in how dealer.exe structures its evaluation

### Code Reverted

All HandStats/function-caching changes reverted to baseline. Focus should be on profiling to identify the actual bottleneck.

---

## Successful Optimization: HashMap & Hashing Improvements

**Date**: 2026-01-07 (later session)

### Profiling Discovery

Used macOS `sample` profiler to identify actual bottlenecks:

```
Call graph analysis showed:
- ~55% time in dealer_eval::eval (recursive expression evaluator)
- Significant time in SipHash operations (HashMap lookups)
- String::clone calls for variable names
- HashMap insert/resize operations
```

**Root cause identified**: Standard HashMap with String keys was causing:
1. String cloning on every cache insert (`name.clone()`)
2. SipHash (cryptographic-strength) hashing on every lookup
3. HashMap resize/rehash operations

### Optimizations Implemented

#### 1. Replace `HashMap<String, i32>` with `HashMap<&str, i32>` for cache

Instead of cloning String keys for the variable value cache, use borrowed `&str` references.

**Key changes**:
```rust
// Before:
cache: RefCell<HashMap<String, i32>>
ctx.cache.borrow_mut().insert(name.clone(), value);

// After:
cache: RefCell<HashMap<&'a str, i32>>
ctx.cache.borrow_mut().insert(key.as_str(), value);  // No clone!
```

**Result**: 5.49s → 4.06s (**26% faster**)

#### 2. Use FxHashMap instead of HashMap

FxHashMap uses a much faster (non-cryptographic) hash function designed for compilers.
Added `rustc-hash` crate dependency.

**Key changes**:
```rust
// Before:
use std::collections::HashMap;
cache: RefCell<HashMap<&'a str, i32>>

// After:
use rustc_hash::FxHashMap;
cache: RefCell<FxHashMap<&'a str, i32>>
```

**Result**: 4.06s → 3.12s (**23% faster**)

#### 3. Use FxHashMap for variables map too

Applied FxHashMap to the variables HashMap (the mapping from variable names to expressions).

**Result**: 3.12s → 2.76s (**12% faster**)

### Final Results

| Implementation | Time (s) | vs Baseline | vs Original C |
|----------------|----------|-------------|---------------|
| Baseline Rust  | 5.49     | 1.0x        | 2.26x slower  |
| + &str keys    | 4.06     | 1.35x faster| 1.67x slower  |
| + FxHashMap cache | 3.12  | 1.76x faster| 1.28x slower  |
| + FxHashMap vars | 2.76   | **2.0x faster** | **1.14x slower** |

### Summary

- **Total speedup**: 2.0x (5.49s → 2.76s)
- **Rust is now only 14% slower than Original C** (was 126% slower)
- **Throughput**: ~546K hands/second (was ~274K)

### Key Lessons

1. **Profile before optimizing**: The actual bottleneck was HashMap overhead, not hand stat computation
2. **Avoid String cloning in hot paths**: Use &str references when possible
3. **Use faster hashers for non-security-critical code**: FxHashMap is much faster than SipHash
4. **Small changes can have big impact**: These changes were minimal code modifications with 2x speedup
