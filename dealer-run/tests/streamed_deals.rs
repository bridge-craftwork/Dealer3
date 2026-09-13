//! Deals read as the run asks for them give the run a list would (#21).
//!
//! A streamed source cannot go back, so a levelled run's second pass is handed
//! the matches the first one kept and carries on from where it stopped, instead
//! of rewinding the way a list or a seed can. None of that may show in what comes
//! out. So every test here runs the same deals twice — once handed over whole,
//! once pulled one at a time — and demands the same deals and the same numbers.

use std::cell::Cell;
use std::rc::Rc;

use dealer_core::Deal;
use dealer_run::run::SolvedDeal;
use dealer_run::{
    DealStream, Deals, LevelingOptions, MeasureDeals, Produced, RunHost, RunOptions, RunReport,
};

/// Deals handed over one at a time, counting how many the run asked for.
struct Pulled {
    deals: std::vec::IntoIter<SolvedDeal>,
    pulled: Rc<Cell<usize>>,
    /// Fail when asked for this deal, counting from zero.
    fail_at: Option<usize>,
}

impl DealStream for Pulled {
    fn next_deal(&mut self) -> Result<Option<SolvedDeal>, String> {
        if self.fail_at == Some(self.pulled.get()) {
            return Err("the library stopped answering".to_string());
        }
        let next = self.deals.next();
        if next.is_some() {
            self.pulled.set(self.pulled.get() + 1);
        }
        Ok(next)
    }

    fn carries_tables(&self) -> bool {
        false
    }

    fn report(&self) -> dealer_run::deal_input::InputReport {
        dealer_run::deal_input::InputReport {
            format: "test",
            unsolved: self.pulled.get(),
            ..Default::default()
        }
    }
}

/// Keeps every deal the run produced, in order.
#[derive(Default)]
struct Kept(Vec<Deal>);

impl RunHost for Kept {
    fn produced(&mut self, produced: &Produced) -> Result<(), String> {
        self.0.push(produced.deal.clone());
        Ok(())
    }
}

/// `count` shuffled deals, the same ones every call.
fn deals(count: u64) -> Vec<SolvedDeal> {
    let config = dealer_core::FastDealConfig::new();
    (0..count)
        .map(|seed| {
            (
                dealer_core::generate_deal_from_seed(seed, &config),
                dealer_dds::DealTricks::nothing(),
            )
        })
        .collect()
}

fn options(deals: Deals, produce: usize, threads: usize) -> RunOptions {
    RunOptions {
        vulnerability: dealer_core::Vulnerability::None,
        seed: 1,
        produce,
        max_generate: usize::MAX,
        deals,
        leveling: None,
        round_robin: false,
        threads,
        batch: 0,
        params: Default::default(),
    }
}

fn run(script: &str, opts: RunOptions) -> (Vec<Deal>, RunReport) {
    let mut kept = Kept::default();
    let report = dealer_run::run(script, opts, &mut kept).expect("run");
    (kept.0, report)
}

fn streamed(all: Vec<SolvedDeal>) -> (Deals, Rc<Cell<usize>>) {
    let pulled = Rc::new(Cell::new(0));
    let stream = Pulled {
        deals: all.into_iter(),
        pulled: Rc::clone(&pulled),
        fail_at: None,
    };
    (Deals::Streamed(Box::new(stream)), pulled)
}

/// Everything a run reports that a reader could compare, bar the input report,
/// which only a streamed run has.
fn outcome(report: &RunReport) -> String {
    format!(
        "produced {} generated {} hit_limit {} types {:?} stats {:?}",
        report.produced, report.generated, report.hit_limit, report.hand_types, report.stats
    )
}

const SELECTIVE: &str = "condition hcp(north) >= 17 && shape(north, any 5332)\n\
                         action average \"N\" hcp(north), frequency \"S\" (hcp(south), 0, 20)\n";

const LADDER: &str = "\
HandType_Weak = hcp(north) <= 9
HandType_Middling = hcp(north) >= 10 and hcp(north) <= 14
HandType_Strong = hcp(north) >= 15
condition hcp(south) >= 8
action average \"S\" hcp(south)
";

#[test]
fn a_plain_run_over_a_stream_is_the_run_over_the_list() {
    let all = deals(60_000);
    for threads in [1, 4] {
        let (from_list, listed) = run(SELECTIVE, options(Deals::Given(all.clone()), 150, threads));
        let (stream, _) = streamed(all.clone());
        let (from_stream, streamed_report) = run(SELECTIVE, options(stream, 150, threads));

        assert_eq!(
            from_stream, from_list,
            "{threads} thread(s): a stream produced different deals from the list"
        );
        assert_eq!(
            outcome(&streamed_report),
            outcome(&listed),
            "{threads} thread(s)"
        );
        assert!(
            from_list.len() == 150,
            "the filter should find 150 in 60,000 deals, or nothing was compared"
        );
    }
}

#[test]
fn a_levelled_run_over_a_stream_is_the_levelled_run_over_the_list() {
    // Enough deals that characterizing stops on the match it needed, part-way
    // through a batch, and the producing pass has to pick up inside that batch.
    let all = deals(120_000);
    let levelled = |deals| RunOptions {
        leveling: Some(LevelingOptions {
            target: None,
            budget: None,
            min_sample: 50,
            measure_cap: 1_000_000,
            measure_deals: MeasureDeals::Shared,
        }),
        ..options(deals, 600, 2)
    };

    let (from_list, listed) = run(LADDER, levelled(Deals::Given(all.clone())));
    let (stream, pulled) = streamed(all.clone());
    let (from_stream, streamed_report) = run(LADDER, levelled(stream));

    assert_eq!(
        listed.produced, 600,
        "the levelled run should have delivered"
    );
    assert_eq!(
        from_stream, from_list,
        "a levelled run over a stream produced different deals from the list"
    );
    assert_eq!(outcome(&streamed_report), outcome(&listed));
    assert!(
        pulled.get() < all.len(),
        "the stream was read to the end ({} deals) for a run that needed fewer",
        pulled.get()
    );
}

#[test]
fn a_stream_is_read_no_further_than_the_run_needs() {
    let (stream, pulled) = streamed(deals(100_000));
    let (produced, report) = run("condition 1\n", options(stream, 5, 1));

    assert_eq!(produced.len(), 5);
    // One batch, at most: deals are drawn a batch at a time, and a run wanting
    // five has no reason to draw a second.
    assert!(
        pulled.get() <= 1024,
        "a run wanting 5 deals read {} of them",
        pulled.get()
    );
    assert_eq!(
        report.input.map(|input| input.unsolved),
        Some(pulled.get()),
        "the run should report what the stream read"
    );
}

#[test]
fn a_stream_that_fails_ends_the_run_with_its_reason() {
    let stream = Pulled {
        deals: deals(5_000).into_iter(),
        pulled: Rc::new(Cell::new(0)),
        fail_at: Some(3_000),
    };
    let error = dealer_run::run(
        "condition 0\n",
        options(Deals::Streamed(Box::new(stream)), 1, 1),
        &mut Kept::default(),
    )
    .err()
    .expect("a stream that fails should not end in a quiet success");
    assert!(
        error.to_string().contains("stopped answering"),
        "the stream's own reason should reach the caller: {error}"
    );
}

#[test]
fn deals_handed_over_whole_report_no_input() {
    let (_, report) = run("condition 1\n", options(Deals::Given(deals(10)), 5, 1));
    assert!(
        report.input.is_none(),
        "the caller of a list already has its report"
    );
}
