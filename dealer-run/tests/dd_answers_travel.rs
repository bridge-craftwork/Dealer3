//! Double-dummy answers travel with their deal, and are not searched twice.
//!
//! Neither half of this can be checked by looking at output. An answer is a
//! property of the deal, so a run that threw away every answer it had and
//! solved again would print exactly the same numbers and only take longer.
//! That is how the engine came to warm every matched deal on the pool, tables
//! and all, without a test noticing.
//!
//! So both assertions here are on `dealer_dds::searches()`, the counter kept
//! for this.

use dealer_core::Deal;
use std::sync::{Mutex, MutexGuard};

/// `dealer_dds::searches()` is one counter for the process, and cargo runs
/// these tests on threads of the same process — so a bracket around one test
/// counts another's searches too. Every test here takes this first.
///
/// Found the hard way: adding a third test made an existing one fail while
/// each passed alone.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn alone() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}
use dealer_run::{Deals, Produced, RunHost, RunOptions};

/// Keeps what `printrpt` rendered, which is where the trick counts appear.
#[derive(Default)]
struct Rows(Vec<String>);

impl RunHost for Rows {
    fn produced(&mut self, produced: &Produced) -> Result<(), String> {
        self.0.extend(produced.rows()?.printed);
        Ok(())
    }
}

/// Every double-dummy path a script can take: the table one, a single cell, and
/// the par score that needs the whole table.
const SCRIPT: &str = "condition 1\n\
                      action printrpt(trix(deal), dds(north, notrump), par(north))\n";

/// A condition that solves, and an action that asks the same question again.
///
/// The condition runs on a worker thread and the action on the main one, which
/// is the case a global store used to cover. `>= 0` is true of every deal, so
/// this measures the searching and nothing else.
const ASKED_TWICE: &str = "condition tricks(north, notrump) >= 0\n\
                           action average \"d\" dds(north, notrump)\n";

fn deals() -> Vec<Deal> {
    let config = dealer_core::FastDealConfig::new();
    (0..4)
        .map(|seed| dealer_core::generate_deal_from_seed(seed, &config))
        .collect()
}

fn run(deals: Vec<dealer_run::run::SolvedDeal>) -> (Vec<String>, usize) {
    let before = dealer_dds::searches();
    let mut rows = Rows::default();
    dealer_run::run(
        SCRIPT,
        RunOptions {
            vulnerability: dealer_core::Vulnerability::None,
            seed: 1,
            produce: usize::MAX,
            max_generate: usize::MAX,
            deals: Deals::Given(deals),
            leveling: None,
            round_robin: false,
            threads: 1,
            batch: 0,
            params: Default::default(),
        },
        &mut rows,
    )
    .expect("run");
    (rows.0, dealer_dds::searches() - before)
}

#[test]
fn a_supplied_table_is_read_rather_than_solved() {
    let _alone = alone();
    let deals = deals();

    // Solved straight through `bridge_solver`, so these searches are not
    // attributed to either run below and leave nothing behind on this thread's
    // analysis slot.
    let tables: Vec<bridge_types::DdTable> = deals
        .iter()
        .map(|deal| dealer_dds::bridge_solver::solve_dd_table(&deal.into()))
        .collect();

    let (with, searched_with) = run(deals
        .iter()
        .cloned()
        .zip(tables)
        .map(|(deal, table)| (deal, dealer_dds::DealTricks::from_table(&table)))
        .collect());
    let (without, searched_without) = run(deals
        .iter()
        .cloned()
        .map(|deal| (deal, dealer_dds::DealTricks::nothing()))
        .collect());

    assert_eq!(
        without, with,
        "a supplied table must give the same answers the solver would"
    );
    assert_eq!(
        searched_with, 0,
        "deals arriving with tables should need no searches at all, but ran {searched_with}"
    );
    assert!(
        searched_without > 0,
        "the run without tables should have had to search for its answers"
    );
}

/// What a worker searched for during the condition reaches the action, even
/// though the two run on different threads with a whole batch in between.
///
/// This is what the global store existed for, and the reason retiring it needed
/// a test rather than a build. Each deal holds one answer; asking for it twice
/// must cost one search, not two.
#[test]
fn an_answer_found_testing_a_deal_is_not_searched_for_again() {
    let _alone = alone();
    const DEALS: usize = 16;

    let before = dealer_dds::searches();
    let mut rows = Rows::default();
    let report = dealer_run::run(
        ASKED_TWICE,
        RunOptions {
            vulnerability: dealer_core::Vulnerability::None,
            seed: 11,
            produce: usize::MAX,
            max_generate: DEALS,
            deals: Deals::Shuffled {
                predeal: dealer_core::FastDealConfig::new(),
                swap: dealer_core::SwapMode::None,
            },
            leveling: None,
            round_robin: false,
            // More than one, so the condition really does run somewhere else.
            threads: 4,
            batch: 0,
            params: Default::default(),
        },
        &mut rows,
    )
    .expect("run");
    let searched = dealer_dds::searches() - before;

    assert_eq!(report.produced, DEALS, "every deal should have matched");
    assert_eq!(
        searched,
        DEALS,
        "one search a deal: {searched} means the answer was found {} times over",
        searched as f64 / DEALS as f64
    );
}

/// A run asking for a few deals must not solve a batch of them.
///
/// The batch is at least 1024 deals and 200 a thread, which is right when a
/// deal costs a microsecond and absurd when it costs twenty milliseconds: a
/// `produce 5` used to solve two thousand four hundred boards to hand back
/// five. Nothing in the output says so — the five are correct either way — so
/// the witness is the search counter.
#[test]
fn a_small_run_does_not_solve_a_whole_batch() {
    let _alone = alone();
    struct Sink;
    impl RunHost for Sink {
        fn produced(&mut self, _: &Produced) -> Result<(), String> {
            Ok(())
        }
    }

    // The action solves, so every deal taken needs one answer and no deal
    // beyond the target needs any.
    let script = "condition 1\naction average \"t\" tricks(north, notrump)\n";
    let before = dealer_dds::searches();
    let report = dealer_run::run(
        script,
        RunOptions {
            vulnerability: dealer_core::Vulnerability::None,
            seed: 1,
            produce: 4,
            max_generate: 1_000_000,
            deals: Deals::Shuffled {
                predeal: dealer_core::FastDealConfig::new(),
                swap: dealer_core::SwapMode::None,
            },
            leveling: None,
            round_robin: false,
            threads: 1,
            batch: 0,
            params: Default::default(),
        },
        &mut Sink,
    )
    .expect("run");
    let searched = dealer_dds::searches() - before;

    assert_eq!(
        report.produced, 4,
        "the run should still produce what it was asked for"
    );
    assert!(
        searched <= 16,
        "asking for 4 deals searched {searched} times — a batch's worth rather \
         than a run's"
    );
}
