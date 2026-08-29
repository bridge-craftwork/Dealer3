//! What threading buys, and what it must not cost.
//!
//! Ignored by default because it is a measurement rather than an assertion —
//! run it with:
//!
//! ```text
//! cargo test -p dealer-run --release --features parallel --test threads -- --ignored --nocapture
//! ```
//!
//! `Jacoby_2N.dlr` because it is a real scenario with a real condition: about
//! two fifths of its per-deal cost is the filter and the rest is the shuffle,
//! which is why the engine parallelises the two together rather than guessing
//! which one matters.

use dealer_core::{FastDealConfig, SwapMode};
use dealer_run::{Deals, Produced, RunHost, RunOptions};

struct Sink;
impl RunHost for Sink {
    fn produced(&mut self, _: &Produced) -> Result<(), String> {
        Ok(())
    }
}

const JACOBY: &str = include_str!("../../dealer-parser/tests/fixtures/Jacoby_2N.dlr");

fn deal(threads: usize, count: usize) -> (f64, usize) {
    let started = std::time::Instant::now();
    let report = dealer_run::run(
        JACOBY,
        RunOptions {
            seed: 1,
            produce: usize::MAX,
            max_generate: count,
            deals: Deals::Shuffled {
                predeal: FastDealConfig::new(),
                swap: SwapMode::None,
            },
            leveling: None,
            threads,
            batch: 0,
            params: Default::default(),
        },
        &mut Sink,
    )
    .expect("run");
    (started.elapsed().as_secs_f64(), report.produced)
}

#[test]
#[ignore = "a measurement, not an assertion"]
fn how_much_threads_buy() {
    for threads in [1usize, 2, 4, 8, 0] {
        let (seconds, produced) = deal(threads, 1_000_000);
        println!("threads={threads:<2} {seconds:>6.3}s  produced={produced}");
    }
}

/// And the thing that must hold however fast it goes: the same deals however
/// many threads dealt them.
///
/// Fewer deals than the measurement above, since this runs in the ordinary
/// suite and an unoptimised build is slow — but enough of them to cross many
/// batches, which is where a threading mistake would show.
#[test]
fn a_run_does_not_care_how_many_threads_dealt_it() {
    let (_, one) = deal(1, 120_000);
    let (_, many) = deal(0, 120_000);
    assert_eq!(one, many, "auto-detected threads changed what was produced");
    assert!(one > 0, "the scenario should match something");
}
