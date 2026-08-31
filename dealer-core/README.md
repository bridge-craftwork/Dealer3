# dealer-core

Core data structures and deal generation for the dealer bridge hand generator.

## Features

- **Card Representation**: Efficient card, suit, and rank types
- **Hand Analysis**: HCP calculation, distribution, shapes, controls
- **Deal Generation**: Random deal generation using dealer.exe-compatible RNG
- **Deterministic**: Seeded generation for reproducibility

## Data Structures

### Card Types
- `Suit`: Clubs, Diamonds, Hearts, Spades
- `Rank`: Two through Ace (with HCP values)
- `Card`: A combination of suit and rank

### Hand
A `Hand` represents 13 cards with analysis functions:
- `hcp()` - High Card Points (A=4, K=3, Q=2, J=1)
- `controls()` - Control count (A=2, K=1)
- `distribution()` - Card distribution by suit length
- `shape()` - Shape pattern (e.g., "5-4-3-1")
- `is_balanced()` - Check if hand is balanced (4-3-3-3, 4-4-3-2, 5-3-3-2)
- `suit_length(suit)` - Count cards in a specific suit

### Deal
A `Deal` represents a complete bridge deal with 4 hands (North, East, South, West).

### FastDealGenerator
Generates random deals using xoshiro256++ (`src/rng.rs`).

## Usage

```rust
use dealer_core::{DealGenerator, Position};

// Create generator with seed
let mut generator = DealGenerator::new(1);

// Generate a deal
let deal = generator.generate();

// Access hands
let north = deal.hand(Position::North);
println!("North has {} HCP", north.hcp());
println!("North shape: {}", north.shape());

// Analyze hand
if north.is_balanced() && north.hcp() >= 15 {
    println!("North can open 1NT");
}
```

## Example

Run the example to see deal generation in action:

```bash
cargo run --example generate_deal
```

## Testing

All core functionality is tested:

```bash
cargo test
```

Tests include:
- Card index conversion (0-51 mapping)
- HCP calculation
- Suit length counting
- Hand distribution and shape
- Deal generation determinism
- All 52 cards properly distributed

## Compatibility

The deal generator uses xoshiro256++, so a seed reproduces a dealer3 run and
**not** a dealer.exe one. The port of GNU `random()` that once did reproduce
dealer.exe's deals was removed in 0.5.0 and lives in
[dealer-legacy-shuffle](https://github.com/bridge-craftwork/dealer-legacy-shuffle).

## License

Apache-2.0
