//! A deal that arrives with its double-dummy table is not solved again.
//!
//! This cannot be checked by looking at the output. The solver agrees with the
//! table, so a run that ignored every table would print exactly the same
//! numbers and only take longer — which is how the engine came to warm every
//! matched deal on the pool, tables and all, without a test noticing.
//!
//! So the assertion is on `dealer_dds::searches()`, the counter kept for this:
//! the same script over the same deals, once with their tables and once
//! without, must agree on every number and differ on how much work it took.

use dealer_core::Deal;
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
    let deals = deals();

    // Solved straight through `bridge_solver`, not through `dealer_dds`, so
    // these searches leave nothing in the memo. That matters: the memo would
    // serve the second run too, and both runs would report no searches whether
    // or not the tables were read.
    let tables: Vec<bridge_types::DdTable> = deals
        .iter()
        .map(|deal| dealer_dds::bridge_solver::solve_dd_table(&deal.into()))
        .collect();

    // Tables first, while nothing is remembered yet: any search here is a
    // search this run did not need.
    let (with, searched_with) = run(deals
        .iter()
        .cloned()
        .zip(tables)
        .map(|(d, t)| (d, Some(t)))
        .collect());
    let (without, searched_without) = run(deals.iter().cloned().map(|d| (d, None)).collect());

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
