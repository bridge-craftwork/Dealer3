//! Fast, stateless deal generation for parallel execution.
//!
//! This module provides deal generation that is independent between deals,
//! enabling full parallelization. Each deal is generated from a single u64 seed
//! using xoshiro256++ and a stateless Fisher-Yates shuffle.
//!
//! Each deal starts from a fresh sorted deck and is derived solely from its own
//! seed, so deals are independent of one another and of the order in which they
//! are generated.
//!
//! # Predeal Support
//!
//! Predeal is supported using a two-phase approach (similar to Penguin dealer):
//! 1. Place predealt cards in their designated positions
//! 2. Fisher-Yates shuffle only the remaining cards into remaining slots

use crate::rng::Xoshiro256PlusPlus;
use crate::{Card, Deal, Position};

/// Configuration for fast deal generation, including predeal settings.
///
/// This struct is cheap to clone and can be shared across threads.
#[derive(Clone, Debug, Default)]
pub struct FastDealConfig {
    /// Predealt cards for each position (up to 13 per position).
    /// Cards are stored as indices 0-51.
    predeal: [Vec<u8>; 4],
}

impl FastDealConfig {
    /// Create a new configuration with no predeal.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add predealt cards for a position.
    ///
    /// Returns an error if:
    /// - More than 13 cards would be dealt to a position
    /// - A card is predealt twice (to any position)
    pub fn predeal(&mut self, position: Position, cards: &[Card]) -> Result<(), String> {
        let pos_idx = position as usize;

        for &card in cards {
            let card_idx = card.to_index();

            // Check if card already predealt
            for (p, predealt) in self.predeal.iter().enumerate() {
                if predealt.contains(&card_idx) {
                    return Err(format!(
                        "Card {:?} already predealt to {}",
                        card,
                        Position::from_index(p).unwrap().to_char()
                    ));
                }
            }

            // Check position card count
            if self.predeal[pos_idx].len() >= 13 {
                return Err(format!(
                    "More than 13 cards for position {}",
                    position.to_char()
                ));
            }

            self.predeal[pos_idx].push(card_idx);
        }

        Ok(())
    }

    /// Get the number of predealt cards for a position.
    pub fn predeal_count(&self, position: Position) -> usize {
        self.predeal[position as usize].len()
    }

    /// Check if a card index is predealt.
    fn is_predealt(&self, card_idx: u8) -> bool {
        self.predeal.iter().any(|v| v.contains(&card_idx))
    }
}

/// Generate a deal from a seed using stateless Fisher-Yates shuffle.
///
/// This function is completely independent - the same seed always produces
/// the same deal, regardless of any other deals generated.
pub fn generate_deal_from_seed(seed: u64, config: &FastDealConfig) -> Deal {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);

    // Start with a sorted deck (cards 0-51)
    let mut deck: [u8; 52] = std::array::from_fn(|i| i as u8);

    // Collect non-predealt cards
    let mut available: Vec<u8> = (0..52u8).filter(|&c| !config.is_predealt(c)).collect();

    // Fisher-Yates shuffle the available cards
    let n = available.len();
    for i in (1..n).rev() {
        let j = rng.next_index((i + 1) as u32) as usize;
        available.swap(i, j);
    }

    // Build the deck: predealt cards first, then shuffled available cards
    let mut available_iter = available.into_iter();

    for pos in 0..4 {
        let predealt = &config.predeal[pos];
        let start_slot = pos * 13;

        // Place predealt cards first
        for (slot_offset, &card_idx) in predealt.iter().enumerate() {
            deck[start_slot + slot_offset] = card_idx;
        }

        // Fill remaining slots with shuffled available cards
        for slot_offset in predealt.len()..13 {
            if let Some(card_idx) = available_iter.next() {
                deck[start_slot + slot_offset] = card_idx;
            }
        }
    }

    // Distribute deck to hands
    deck_to_deal(&deck)
}

/// Generate a deal from a seed with no predeal (optimized path).
///
/// This is faster than the general case when there's no predeal.
#[inline]
pub fn generate_deal_from_seed_no_predeal(seed: u64) -> Deal {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);

    // Start with a sorted deck (cards 0-51)
    let mut deck: [u8; 52] = std::array::from_fn(|i| i as u8);

    // Standard Fisher-Yates shuffle
    for i in (1..52).rev() {
        let j = rng.next_index((i + 1) as u32) as usize;
        deck.swap(i, j);
    }

    deck_to_deal(&deck)
}

/// Convert a shuffled deck array to a Deal.
#[inline]
/// Build a `Deal` from a shuffled deck.
///
/// The hands come out in deal order, not sorted. Sorting here cost about
/// 340ns of the roughly 530ns it took to produce a deal -- two thirds of all
/// generation -- and it was paid on every deal generated, while a run
/// typically keeps well under 1% of them. Nothing needed it:
///
///   - The counting functions (`hcp`, `top4`, `suit_length`, `controls`, and
///     the rest) are sums and filters, so order cannot reach them.
///   - The rank-position ones (`losers_in_suit`, `suit_quality`, `cccc`) each
///     sort their own local copy first.
///   - Every output formatter -- PBN, printall, printoneline, compact -- sorts
///     the suit it is about to print.
///   - `dealer_dds`'s memo key is a bitmask.
///
/// Sort a hand explicitly with [`Hand::sort`] or [`Hand::sorted`] where order
/// is wanted.
fn deck_to_deal(deck: &[u8; 52]) -> Deal {
    let mut deal = Deal::new();

    for (slot, &card_idx) in deck.iter().enumerate() {
        let card = Card::from_index(card_idx).unwrap();
        let position = Position::from_index(slot / 13).unwrap();
        deal.hand_mut(position).add_card(card);
    }

    deal
}

/// Fast deal generator that produces seeds for parallel workers.
///
/// The supervisor uses this to generate a sequence of seeds, which workers
/// then use to generate deals independently.
#[derive(Clone)]
pub struct FastDealGenerator {
    /// Master RNG for generating deal seeds
    master_rng: Xoshiro256PlusPlus,
    /// Predeal configuration
    config: FastDealConfig,
    /// Number of deals generated so far
    generated: u64,
}

impl FastDealGenerator {
    /// Create a new generator with the given seed.
    pub fn new(seed: u64) -> Self {
        Self {
            master_rng: Xoshiro256PlusPlus::seed_from_u64(seed),
            config: FastDealConfig::new(),
            generated: 0,
        }
    }

    /// Create a new generator with the given seed and predeal config.
    pub fn with_config(seed: u64, config: FastDealConfig) -> Self {
        Self {
            master_rng: Xoshiro256PlusPlus::seed_from_u64(seed),
            config,
            generated: 0,
        }
    }

    /// Generate the next deal seed.
    #[inline]
    pub fn next_seed(&mut self) -> u64 {
        self.generated += 1;
        self.master_rng.next_u64()
    }

    /// Generate a batch of seeds.
    pub fn next_seeds(&mut self, count: usize) -> Vec<u64> {
        (0..count).map(|_| self.next_seed()).collect()
    }

    /// Generate the next deal directly (for single-threaded use).
    pub fn next_deal(&mut self) -> Deal {
        let seed = self.next_seed();
        if self.has_predeal() {
            generate_deal_from_seed(seed, &self.config)
        } else {
            generate_deal_from_seed_no_predeal(seed)
        }
    }

    /// Check if there's any predeal configured.
    pub fn has_predeal(&self) -> bool {
        self.config.predeal.iter().any(|v| !v.is_empty())
    }

    /// Get the predeal configuration.
    pub fn config(&self) -> &FastDealConfig {
        &self.config
    }

    /// Get the number of deals generated so far.
    pub fn generated_count(&self) -> u64 {
        self.generated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rank, Suit};

    #[test]
    fn test_generate_deal_deterministic() {
        // Same seed should always produce same deal
        let deal1 = generate_deal_from_seed_no_predeal(42);
        let deal2 = generate_deal_from_seed_no_predeal(42);
        assert_eq!(deal1, deal2);
    }

    #[test]
    fn test_generate_deal_different_seeds() {
        // Different seeds should produce different deals
        let deal1 = generate_deal_from_seed_no_predeal(1);
        let deal2 = generate_deal_from_seed_no_predeal(2);
        assert_ne!(deal1, deal2);
    }

    #[test]
    fn test_generate_deal_valid() {
        // Each deal should have valid structure
        for seed in 0..100 {
            let deal = generate_deal_from_seed_no_predeal(seed);

            // Each hand has 13 cards
            assert_eq!(deal.north.len(), 13);
            assert_eq!(deal.east.len(), 13);
            assert_eq!(deal.south.len(), 13);
            assert_eq!(deal.west.len(), 13);

            // Total HCP is 40
            let total_hcp = deal.north.hcp() + deal.east.hcp() + deal.south.hcp() + deal.west.hcp();
            assert_eq!(total_hcp, 40);

            // All 52 cards are present
            let mut all_cards: Vec<u8> = Vec::new();
            for pos in Position::ALL {
                for card in deal.hand(pos).cards() {
                    all_cards.push(card.to_index());
                }
            }
            all_cards.sort();
            let expected: Vec<u8> = (0..52).collect();
            assert_eq!(all_cards, expected);
        }
    }

    #[test]
    fn test_predeal_basic() {
        let mut config = FastDealConfig::new();
        config
            .predeal(
                Position::North,
                &[
                    Card::new(Suit::Spades, Rank::Ace),
                    Card::new(Suit::Spades, Rank::King),
                ],
            )
            .unwrap();

        let deal = generate_deal_from_seed(42, &config);

        // North should have AS and KS
        let north = deal.hand(Position::North);
        assert!(north.cards().contains(&Card::new(Suit::Spades, Rank::Ace)));
        assert!(north.cards().contains(&Card::new(Suit::Spades, Rank::King)));
        assert_eq!(north.len(), 13);
    }

    #[test]
    fn test_predeal_deterministic() {
        let mut config = FastDealConfig::new();
        config
            .predeal(Position::South, &[Card::new(Suit::Hearts, Rank::Queen)])
            .unwrap();

        let deal1 = generate_deal_from_seed(123, &config);
        let deal2 = generate_deal_from_seed(123, &config);
        assert_eq!(deal1, deal2);
    }

    #[test]
    fn test_predeal_multiple_positions() {
        let mut config = FastDealConfig::new();
        config
            .predeal(Position::North, &[Card::new(Suit::Spades, Rank::Ace)])
            .unwrap();
        config
            .predeal(Position::South, &[Card::new(Suit::Hearts, Rank::Ace)])
            .unwrap();

        let deal = generate_deal_from_seed(99, &config);

        assert!(deal
            .hand(Position::North)
            .cards()
            .contains(&Card::new(Suit::Spades, Rank::Ace)));
        assert!(deal
            .hand(Position::South)
            .cards()
            .contains(&Card::new(Suit::Hearts, Rank::Ace)));
    }

    #[test]
    fn test_predeal_full_hand() {
        // Predeal all 13 spades to North
        let mut config = FastDealConfig::new();
        let all_spades: Vec<Card> = (0..13)
            .map(|i| Card::from_index(39 + i).unwrap()) // Spades are 39-51
            .collect();
        config.predeal(Position::North, &all_spades).unwrap();

        let deal = generate_deal_from_seed(777, &config);

        let north = deal.hand(Position::North);
        assert_eq!(north.suit_length(Suit::Spades), 13);
        assert_eq!(north.len(), 13);
    }

    #[test]
    fn test_predeal_duplicate_error() {
        let mut config = FastDealConfig::new();
        config
            .predeal(Position::North, &[Card::new(Suit::Spades, Rank::Ace)])
            .unwrap();

        // Try to predeal same card to South
        let result = config.predeal(Position::South, &[Card::new(Suit::Spades, Rank::Ace)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already predealt"));
    }

    #[test]
    fn test_predeal_too_many_error() {
        let mut config = FastDealConfig::new();

        // Try to predeal 14 cards
        let cards: Vec<Card> = (0..14).map(|i| Card::from_index(i).unwrap()).collect();
        let result = config.predeal(Position::North, &cards);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("More than 13"));
    }

    /// Predeal must not leave a card placed twice or dropped. `test_generate_deal_valid`
    /// checks the full 52 without predeal; this covers the case where predealt cards are
    /// seeded into the deck first, which is where a double-placement could hide.
    #[test]
    fn test_predeal_deal_still_has_all_52_cards() {
        let mut config = FastDealConfig::new();
        config
            .predeal(
                Position::North,
                &[
                    Card::new(Suit::Spades, Rank::Ace),
                    Card::new(Suit::Hearts, Rank::Ace),
                ],
            )
            .unwrap();

        let mut gen = FastDealGenerator::with_config(42, config);
        let deal = gen.next_deal();

        let mut indices: Vec<u8> = Position::ALL
            .iter()
            .flat_map(|pos| deal.hand(*pos).cards().iter().map(|c| c.to_index()))
            .collect();
        assert_eq!(indices.len(), 52);
        indices.sort();
        assert_eq!(indices, (0..52).collect::<Vec<u8>>());
    }

    #[test]
    fn test_fast_generator_sequence() {
        let mut gen1 = FastDealGenerator::new(42);
        let mut gen2 = FastDealGenerator::new(42);

        // Should produce same sequence of deals
        for _ in 0..10 {
            assert_eq!(gen1.next_deal(), gen2.next_deal());
        }
    }

    #[test]
    fn test_fast_generator_seeds() {
        let mut gen = FastDealGenerator::new(123);

        let seeds = gen.next_seeds(100);
        assert_eq!(seeds.len(), 100);

        // Seeds should be unique (extremely unlikely to have duplicates)
        let unique: std::collections::HashSet<_> = seeds.iter().collect();
        assert_eq!(unique.len(), 100);
    }

    #[test]
    fn test_fast_generator_with_predeal() {
        let mut config = FastDealConfig::new();
        config
            .predeal(Position::West, &[Card::new(Suit::Diamonds, Rank::Jack)])
            .unwrap();

        let mut gen = FastDealGenerator::with_config(555, config);

        for _ in 0..10 {
            let deal = gen.next_deal();
            assert!(deal
                .hand(Position::West)
                .cards()
                .contains(&Card::new(Suit::Diamonds, Rank::Jack)));
        }
    }
}
