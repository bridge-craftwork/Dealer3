//! The random number `rnd()` returns.
//!
//! The original draws from the same generator it shuffles with, so a script
//! calling `rnd()` changes the deals it sees. That is an artefact of having one
//! generator, not something a script relies on, and dealer3 cannot copy it
//! anyway: deals here are stateless functions of their own seed, worked out on
//! whatever thread is free, so a shared stream would make output depend on
//! thread scheduling.
//!
//! So `rnd()` gets its own stream, seeded from the deal it is being asked
//! about. That keeps the two properties worth having: the same seed gives the
//! same answers, whatever `-R` or `--batch-size` say, and different deals get
//! different numbers. `--rnd-seed` shifts the whole stream for a script that
//! wants a different draw from the same deals.

use dealer_core::{Deal, Position};
use std::sync::atomic::{AtomicU64, Ordering};

/// Mixed into every deal's seed by `--rnd-seed`.
///
/// A process-wide setting because it is a command-line one: it is read once
/// per deal, never written after startup.
static SEED: AtomicU64 = AtomicU64::new(0);

/// Set the value `--rnd-seed` supplies. Call once, before generating.
pub fn set_seed(seed: u64) {
    SEED.store(seed, Ordering::Relaxed);
}

/// The seed `rnd()` should use for `deal`.
///
/// Built from which cards each hand holds, so it is exactly as reproducible as
/// the deal itself and does not need the deal's own seed plumbed through the
/// evaluator.
pub fn seed_for(deal: &Deal) -> u64 {
    let mut mixed = SEED.load(Ordering::Relaxed);
    for position in Position::ALL {
        let mut hand = 0u64;
        for card in deal.hand(position).cards() {
            hand |= 1 << card.to_index();
        }
        mixed = splitmix64(mixed ^ hand);
    }
    mixed
}

/// The mixing function xoshiro's own authors recommend for seeding it.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dealer_core::FastDealGenerator;

    fn deals() -> Vec<Deal> {
        let mut generator = FastDealGenerator::new(20260827);
        (0..64).map(|_| generator.next_deal()).collect()
    }

    #[test]
    fn the_same_deal_always_gets_the_same_seed() {
        let deal = &deals()[0];
        assert_eq!(seed_for(deal), seed_for(deal));
        assert_eq!(seed_for(deal), seed_for(&deal.clone()));
    }

    #[test]
    fn different_deals_get_different_seeds() {
        let seeds: std::collections::BTreeSet<u64> = deals().iter().map(seed_for).collect();
        assert_eq!(seeds.len(), 64, "seeds collided across 64 deals");
    }

    /// The seed has to depend on *where* the cards are, not just which are in
    /// play — otherwise every arrangement of one shuffle, which is what `-3`
    /// produces, would draw the same numbers.
    #[test]
    fn moving_a_hand_changes_the_seed() {
        let deal = &deals()[0];
        let mut swapped = deal.clone();
        let east = swapped.hand(Position::East).clone();
        *swapped.hand_mut(Position::East) = swapped.hand(Position::West).clone();
        *swapped.hand_mut(Position::West) = east;
        assert_ne!(seed_for(deal), seed_for(&swapped));
    }
}
