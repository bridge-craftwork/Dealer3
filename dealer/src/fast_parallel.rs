//! Fast parallel deal generation using stateless independent deals.
//!
//! This module provides a simplified parallel architecture where:
//! - Supervisor generates seeds (trivially fast)
//! - Workers generate deals from seeds (fully independent, no state sharing)
//!
//! This is much more efficient than the legacy parallel module because:
//! 1. No shuffle state dependency between deals
//! 2. Seeds are just u64 values (8 bytes vs ~300 bytes work state)
//! 3. No shared configuration needed (each worker can generate independently)

use dealer_core::{
    generate_deal_from_seed, generate_deal_from_seed_no_predeal, Deal, FastDealConfig,
    FastDealGenerator, SwapMode,
};
use rayon::prelude::*;
use std::collections::VecDeque;
use std::sync::Arc;

/// Work unit for fast parallel generation - just a seed and serial number.
///
/// One unit is one *shuffle*. Without swapping that is also one deal; with
/// `-2` or `-3` it is the two or six deals that shuffle is arranged into.
#[derive(Clone, Copy)]
pub struct FastWorkUnit {
    /// Serial number for ordering results
    pub serial_number: u64,
    /// Seed for generating this deal
    pub seed: u64,
}

/// Completed work from a fast worker.
pub struct FastCompletedWork {
    /// Serial number for ordering
    pub serial_number: u64,
    /// The generated deal
    pub deal: Deal,
    /// Whether the filter passed
    pub passed: bool,
}

/// Configuration for fast parallel execution.
#[derive(Clone, Default)]
pub struct FastParallelConfig {
    /// Number of worker threads (0 = auto-detect)
    pub num_threads: usize,
}

/// Fast supervisor for parallel deal generation.
///
/// This supervisor is much simpler than the legacy one - it just generates seeds
/// and dispatches them to workers. No complex state management needed.
pub struct FastSupervisor {
    /// The seed generator
    generator: FastDealGenerator,
    /// Predeal configuration (shared via Arc if non-empty)
    predeal_config: Option<Arc<FastDealConfig>>,
    /// Next shuffle serial number to assign
    next_serial: u64,
    /// How many deals each shuffle is arranged into.
    swap: SwapMode,
    /// Deals a shuffle produced beyond what the last batch asked for.
    ///
    /// A batch size is not generally a multiple of the swap width, and a
    /// shuffle's variants have to stay together and in order. Holding the
    /// remainder over is what lets `process_batch` return *exactly* what it was
    /// asked for, so `-g` still stops on the deal it should.
    pending: VecDeque<(u64, Deal)>,
}

impl FastSupervisor {
    /// Create a new fast supervisor.
    pub fn new(seed: u64, parallel_config: FastParallelConfig) -> Self {
        // Configure rayon thread pool if custom thread count specified
        if parallel_config.num_threads > 0 {
            rayon::ThreadPoolBuilder::new()
                .num_threads(parallel_config.num_threads)
                .build_global()
                .ok(); // Ignore error if pool already initialized
        }

        Self {
            generator: FastDealGenerator::new(seed),
            predeal_config: None,
            next_serial: 0,
            swap: SwapMode::None,
            pending: VecDeque::new(),
        }
    }

    /// Create a new fast supervisor with predeal configuration.
    pub fn with_predeal(
        seed: u64,
        predeal_config: FastDealConfig,
        parallel_config: FastParallelConfig,
    ) -> Self {
        if parallel_config.num_threads > 0 {
            rayon::ThreadPoolBuilder::new()
                .num_threads(parallel_config.num_threads)
                .build_global()
                .ok();
        }

        Self {
            generator: FastDealGenerator::with_config(seed, FastDealConfig::new()),
            predeal_config: Some(Arc::new(predeal_config)),
            next_serial: 0,
            swap: SwapMode::None,
            pending: VecDeque::new(),
        }
    }

    /// Arrange every shuffle into the deals `swap` asks for.
    ///
    /// Set this before the first batch: it changes what a serial number means.
    pub fn with_swapping(mut self, swap: SwapMode) -> Self {
        self.swap = swap;
        self
    }

    /// Generate a batch of work units (just seeds).
    fn generate_batch(&mut self, count: usize) -> Vec<FastWorkUnit> {
        let mut units = Vec::with_capacity(count);

        for _ in 0..count {
            units.push(FastWorkUnit {
                serial_number: self.next_serial,
                seed: self.generator.next_seed(),
            });
            self.next_serial += 1;
        }

        units
    }

    /// Every deal one batch of shuffles produces, in order.
    ///
    /// Generation stays parallel: a shuffle and its variants are worked out
    /// together on one worker, since the variants are only a rearrangement of
    /// the hands the shuffle already dealt.
    fn deals_from(&self, units: Vec<FastWorkUnit>) -> Vec<(u64, Deal)> {
        let swap = self.swap;
        let width = swap.deals_per_shuffle() as u64;
        let predeal = self.predeal_config.clone();
        let mut deals: Vec<(u64, Deal)> = units
            .into_par_iter()
            .flat_map_iter(move |unit| {
                let base = match predeal {
                    Some(ref config) => generate_deal_from_seed(unit.seed, config),
                    None => generate_deal_from_seed_no_predeal(unit.seed),
                };
                let first = unit.serial_number * width;
                (0..width)
                    .map(move |variant| (first + variant, swap.apply(&base, variant as usize)))
            })
            .collect();
        deals.sort_by_key(|(serial, _)| *serial);
        deals
    }

    /// Process a batch of deals in parallel.
    ///
    /// `count` is a number of deals, not of shuffles: with `-3` a batch of ten
    /// is two shuffles' worth, and the two deals left over start the next
    /// batch. Returns results sorted by serial number.
    pub fn process_batch<F>(&mut self, count: usize, filter: F) -> Vec<FastCompletedWork>
    where
        F: Fn(&Deal) -> bool + Sync,
    {
        let mut deals: Vec<(u64, Deal)> = self.pending.drain(..).collect();
        let shortfall = count.saturating_sub(deals.len());
        if shortfall > 0 {
            let shuffles = shortfall.div_ceil(self.swap.deals_per_shuffle());
            let units = self.generate_batch(shuffles);
            deals.extend(self.deals_from(units));
        }
        if deals.len() > count {
            self.pending = deals.split_off(count).into();
        }

        let mut results: Vec<FastCompletedWork> = deals
            .into_par_iter()
            .map(|(serial_number, deal)| {
                let passed = filter(&deal);
                FastCompletedWork {
                    serial_number,
                    deal,
                    passed,
                }
            })
            .collect();

        // Sort by serial number for deterministic output
        results.sort_by_key(|w| w.serial_number);
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fast_supervisor_batch() {
        let config = FastParallelConfig { num_threads: 1 };
        let mut supervisor = FastSupervisor::new(42, config);

        let results = supervisor.process_batch(10, |_| true);

        assert_eq!(results.len(), 10);

        // Check serial numbers are in order
        for (i, result) in results.iter().enumerate() {
            assert_eq!(result.serial_number, i as u64);
            assert!(result.passed);
        }
    }

    #[test]
    fn test_fast_supervisor_deterministic() {
        let config = FastParallelConfig { num_threads: 4 };

        let mut sup1 = FastSupervisor::new(123, config.clone());
        let mut sup2 = FastSupervisor::new(123, config);

        let results1 = sup1.process_batch(50, |_| true);
        let results2 = sup2.process_batch(50, |_| true);

        assert_eq!(results1.len(), results2.len());
        for (r1, r2) in results1.iter().zip(results2.iter()) {
            assert_eq!(r1.serial_number, r2.serial_number);
            assert_eq!(r1.deal, r2.deal);
        }
    }

    #[test]
    fn test_fast_supervisor_filter() {
        let config = FastParallelConfig { num_threads: 2 };
        let mut supervisor = FastSupervisor::new(42, config);

        // Filter: North has >= 15 HCP
        let results = supervisor.process_batch(100, |deal| deal.north.hcp() >= 15);

        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.iter().filter(|r| !r.passed).count();

        assert!(passed > 0, "Expected some deals to pass");
        assert!(failed > 0, "Expected some deals to fail");
        assert_eq!(passed + failed, 100);
    }

    #[test]
    fn test_fast_supervisor_matches_sequential() {
        let seed = 999u64;

        // Sequential generation
        let mut seq_gen = FastDealGenerator::new(seed);
        let sequential_deals: Vec<Deal> = (0..20).map(|_| seq_gen.next_deal()).collect();

        // Parallel generation
        let config = FastParallelConfig { num_threads: 4 };
        let mut supervisor = FastSupervisor::new(seed, config);
        let parallel_results = supervisor.process_batch(20, |_| true);

        // Should match exactly
        for (i, (seq_deal, par_result)) in sequential_deals
            .iter()
            .zip(parallel_results.iter())
            .enumerate()
        {
            assert_eq!(
                seq_deal, &par_result.deal,
                "Deal {} differs between sequential and parallel",
                i
            );
        }
    }

    #[test]
    fn test_fast_work_unit_size() {
        // Work units should be tiny - just 16 bytes (serial + seed)
        let unit_size = std::mem::size_of::<FastWorkUnit>();
        assert_eq!(unit_size, 16, "FastWorkUnit should be exactly 16 bytes");
    }

    #[test]
    fn test_fast_supervisor_with_predeal() {
        use dealer_core::{Card, Position, Rank, Suit};

        let mut predeal = FastDealConfig::new();
        predeal
            .predeal(Position::North, &[Card::new(Suit::Spades, Rank::Ace)])
            .unwrap();

        let config = FastParallelConfig { num_threads: 2 };
        let mut supervisor = FastSupervisor::with_predeal(42, predeal, config);

        let results = supervisor.process_batch(20, |_| true);

        // All deals should have AS in North
        for result in &results {
            assert!(result
                .deal
                .hand(Position::North)
                .cards()
                .contains(&Card::new(Suit::Spades, Rank::Ace)));
        }
    }
}
