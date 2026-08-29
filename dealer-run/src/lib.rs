//! The body of a generate loop, shared by the two front ends.
//!
//! The command line and the browser each need their own loop: one draws deals
//! through a rayon batch or a PBN file and writes to a terminal, the other
//! answers to a JavaScript clock and hands deals back to a page. Those are real
//! differences. What sits *inside* the loop is not — classifying a deal against
//! the script's `HandType_*` and `LevelType_*` variables, accumulating its
//! `average` and `frequency` statements, and deciding when a measuring run has
//! learnt enough — and that had been written twice, comment for comment, in
//! `dealer/src/main.rs` and `wasm/src/lib.rs`.
//!
//! Twice meant drift. `mentions_hand_type` tagging reached only the browser;
//! the terminal prints the offending deal when two hand types overlap and the
//! browser does not; the rule for when a measuring run has enough was a single
//! pass with an early stop on one side and a probe that sized a second pass on
//! the other. The last of those is the one that mattered — it decides how
//! precise a levelling is, and a levelling that is measured badly is wrong
//! quietly and permanently.
//!
//! So the loop stays with the front end and its body moves here.
//!
//! # What this deliberately does not do
//!
//! It does not merge the [`EvalContext`]s a matching deal gets. Each block —
//! the condition, the `average`s, the classification — starting a fresh
//! context is what gives a `rnd()` in one a different draw from a `rnd()` in
//! another, which is what the original does, evaluating them as separate
//! calls. Sharing one context would be faster and would change the numbers.
//!
//! So where the boundaries fall is compatibility, not preference, and not a
//! thing for two front ends to each decide: [`RunAccumulator::observe`] builds
//! its own. The condition and `printes` stay with the caller, which builds its
//! own for those.

pub mod leveled;
pub use leveled::{
    level_and_generate, Deals, LevelRunOptions, LevelRunReport, Phase, Produced, RunHost,
};

use dealer_core::Deal;
use dealer_eval::{eval, EvalContext, PointCounts, Variables};
use dealer_parser::{Expr, Program, Statement};
use std::collections::BTreeMap;
use std::ops::Bound;

/// When a measuring run has nothing left to learn.
///
/// A keep is `mix / natural`, so the precision of a levelling is the precision
/// of the rarest category's measured rate and nothing else. The rule is
/// therefore a floor on the *rarest* count rather than a total: a run that has
/// produced a million deals has still learnt nothing about a category it has
/// seen twice.
#[derive(Debug, Clone, Copy)]
pub struct MeasureStop {
    goal: usize,
}

impl MeasureStop {
    /// Stop once every category has been seen `goal` times.
    pub fn new(goal: usize) -> Self {
        MeasureStop { goal }
    }

    /// The rule the command line has always used, at [`dealer_level::MEASURE_GOAL`].
    pub fn standard() -> Self {
        MeasureStop::new(dealer_level::MEASURE_GOAL)
    }

    /// Has every category been seen enough times to divide by?
    ///
    /// False for an empty decomposition: a script naming no categories is not
    /// measuring, and answering "yes, immediately" would stop a run that had
    /// learnt nothing.
    pub fn satisfied(&self, counts: &[usize]) -> bool {
        !counts.is_empty() && counts.iter().all(|n| *n >= self.goal)
    }
}

/// Which categories a produced deal fell into.
///
/// Both are indices into the accumulator's own ordering, and both are `None`
/// when the script declares no such variables — or, for a script whose types do
/// not cover every deal it produces, when this one matched none of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Matched {
    /// Index into [`RunAccumulator::hand_type_labels`].
    pub hand_type: Option<usize>,
    /// Index into [`RunAccumulator::leveling_labels`], which is the hand types
    /// unless the script declares `LevelType_` variables of its own.
    pub level_type: Option<usize>,
}

/// What went wrong while observing a deal.
///
/// Carries the deal for [`RunError::Overlap`] because that is the one a reader
/// cannot act on without seeing it: two category definitions written pages
/// apart overlap on a corner neither author had in mind, and the corner is what
/// has to be looked at. The command line prints it; the browser has no terminal
/// to print it to and says so in the message alone.
#[derive(Debug)]
pub enum RunError {
    /// Two categories of the same decomposition claimed one deal.
    Overlap {
        /// `Hand` or `Level`, for the message.
        kind: &'static str,
        first: String,
        second: String,
        deal: Box<Deal>,
    },
    /// An expression could not be evaluated.
    Eval {
        /// What was being evaluated, for the message: a category's name, or
        /// `average` / `frequency`.
        what: String,
        message: String,
    },
}

impl RunError {
    /// The deal that provoked this, when showing it would help.
    pub fn deal(&self) -> Option<&Deal> {
        match self {
            RunError::Overlap { deal, .. } => Some(deal),
            RunError::Eval { .. } => None,
        }
    }
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Overlap {
                kind,
                first,
                second,
                deal: _,
            } => write!(
                f,
                "a deal is both `{}` and `{}`. {} types have to partition the deals, \
                 so at most one may match.",
                first, second, kind
            ),
            RunError::Eval { what, message } => write!(f, "{}: {}", what, message),
        }
    }
}

impl std::error::Error for RunError {}

/// One `average` statement's result.
#[derive(Debug, Clone)]
pub struct AverageStat {
    pub label: Option<String>,
    pub value: f64,
    /// How many deals it was measured over, which is what says whether a rare
    /// category was sampled enough to trust.
    pub count: usize,
    /// Whether the expression mentions a `HandType_`, so a front end that draws
    /// shares can tell an average that is one from an average that is not.
    pub is_hand_type: bool,
}

/// One `frequency` statement's result, binned.
#[derive(Debug, Clone)]
pub struct FrequencyStat {
    pub label: Option<String>,
    /// The declared range. `Option` because the AST allows its absence, though
    /// the grammar does not: `frequency` requires a min and a max, so nothing
    /// the parser produces reaches the absent case today.
    pub min: Option<i32>,
    pub max: Option<i32>,
    /// `(value, count)` across the range, including empty bins.
    pub bins: Vec<(i32, usize)>,
    /// Observations outside a declared range.
    pub below: usize,
    pub above: usize,
    /// Every observation, `below` and `above` included.
    pub total: usize,
}

/// Everything a run gathered, once it is over.
#[derive(Debug, Clone)]
pub struct Stats {
    pub averages: Vec<AverageStat>,
    pub frequencies: Vec<FrequencyStat>,
}

/// One `average` statement, mid-run.
struct Running<'a> {
    label: Option<String>,
    expr: &'a Expr,
    is_hand_type: bool,
    sum: f64,
    count: usize,
}

/// One `frequency` statement, mid-run. Ordered rather than hashed so the tails
/// outside a declared range are a pair of `range` queries.
struct Histogram<'a> {
    label: Option<String>,
    expr: &'a Expr,
    counts: BTreeMap<i32, usize>,
    range: Option<(i32, i32)>,
}

/// The body of a generate loop: classify a produced deal and count it.
///
/// Built once per script, then fed every deal the condition accepted. It does
/// not deal, does not render and does not decide when to stop generating —
/// only when a *measuring* run has learnt enough, which it answers through
/// [`RunAccumulator::measure_satisfied`].
pub struct RunAccumulator<'a> {
    hand_type_names: Vec<&'a str>,
    hand_type_labels: Vec<String>,
    hand_type_counts: Vec<usize>,

    /// The levelling decomposition's variables, empty when it is the hand types.
    level_type_names: Vec<&'a str>,
    /// Its labels, which are the hand types' when it declares none of its own.
    leveling_labels: Vec<String>,
    leveling_counts: Vec<usize>,
    /// Which prefix the levelling decomposition came from, for `Measurement`.
    leveling_prefix: &'static str,

    /// `joint[hand type][levelling category]`, empty unless the two differ.
    joint: Vec<Vec<usize>>,

    averages: Vec<Running<'a>>,
    frequencies: Vec<Histogram<'a>>,

    produced: usize,
    stop: MeasureStop,
}

impl<'a> RunAccumulator<'a> {
    /// Read a script's categories and statistics statements.
    ///
    /// Fails only on a script whose `HandType_X_Share` declarations do not
    /// resolve — refused here rather than after a run, so a mixed-up set of
    /// shares costs nothing.
    pub fn new(program: &'a Program, stop: MeasureStop) -> Result<Self, String> {
        let hand_type_names = dealer_level::hand_types(program);
        let hand_type_labels: Vec<String> = hand_type_names
            .iter()
            .map(|n| dealer_level::hand_type_label(n).to_string())
            .collect();
        let level_type_names = dealer_level::level_types(program);
        let leveling = dealer_level::leveling_types(program)?;

        let mut averages = Vec::new();
        let mut frequencies = Vec::new();
        for statement in &program.statements {
            if let Statement::Action {
                averages: avg_specs,
                frequencies: freq_specs,
                ..
            } = statement
            {
                for a in avg_specs {
                    averages.push(Running {
                        label: a.label.clone(),
                        expr: &a.expr,
                        is_hand_type: dealer_level::mentions_hand_type(&a.expr),
                        sum: 0.0,
                        count: 0,
                    });
                }
                for f in freq_specs {
                    frequencies.push(Histogram {
                        label: f.label.clone(),
                        expr: &f.expr,
                        counts: BTreeMap::new(),
                        range: f.range,
                    });
                }
            }
        }

        let hand_type_counts = vec![0; hand_type_names.len()];
        let leveling_counts = vec![0; leveling.labels.len()];
        let joint = if level_type_names.is_empty() {
            Vec::new()
        } else {
            vec![vec![0; leveling.labels.len()]; hand_type_names.len()]
        };

        Ok(RunAccumulator {
            hand_type_names,
            hand_type_labels,
            hand_type_counts,
            level_type_names,
            leveling_labels: leveling.labels,
            leveling_counts,
            leveling_prefix: leveling.prefix,
            joint,
            averages,
            frequencies,
            produced: 0,
            stop,
        })
    }

    /// Observe one deal the condition accepted.
    ///
    /// Contexts are built here rather than passed in: where their boundaries
    /// fall is what a `rnd()` in an `average` draws against a `rnd()` in a hand
    /// type, and that is compatibility rather than preference. Drawn exactly
    /// where both front ends drew them — one shared by the `average`s and the
    /// `frequency`s, one for the hand types, one for the levelling types — and
    /// each built only when something needs it.
    pub fn observe<'d>(
        &mut self,
        deal: &'d Deal,
        variables: &'d Variables<'d>,
        counts: Option<&'d PointCounts>,
    ) -> Result<Matched, RunError> {
        if !self.averages.is_empty() || !self.frequencies.is_empty() {
            let ctx = EvalContext::with_counts(deal, variables, counts);
            for average in self.averages.iter_mut() {
                let value = eval(average.expr, &ctx).map_err(|e| RunError::Eval {
                    what: "Average evaluation error".to_string(),
                    message: e.to_string(),
                })?;
                average.sum += value as f64;
                average.count += 1;
            }
            for frequency in self.frequencies.iter_mut() {
                let value = eval(frequency.expr, &ctx).map_err(|e| RunError::Eval {
                    what: "Frequency evaluation error".to_string(),
                    message: e.to_string(),
                })?;
                *frequency.counts.entry(value).or_insert(0) += 1;
            }
        }

        let hand_type = if self.hand_type_names.is_empty() {
            None
        } else {
            let ctx = EvalContext::with_counts(deal, variables, counts);
            pick(&self.hand_type_names, &ctx, deal, "Hand")?
        };
        if let Some(i) = hand_type {
            self.hand_type_counts[i] += 1;
        }

        // The levelling decomposition, which is usually the hand types — and
        // then their counts serve for both, rather than classifying twice.
        let level_type = if self.level_type_names.is_empty() {
            hand_type
        } else {
            let ctx = EvalContext::with_counts(deal, variables, counts);
            pick(&self.level_type_names, &ctx, deal, "Level")?
        };
        if let Some(i) = level_type {
            self.leveling_counts[i] += 1;
            if let (Some(g), false) = (hand_type, self.joint.is_empty()) {
                self.joint[g][i] += 1;
            }
        }

        self.produced += 1;
        Ok(Matched {
            hand_type,
            level_type,
        })
    }

    /// Has the levelling decomposition been seen enough times to divide by?
    ///
    /// The whole of what a measuring run is for. A front end checks it once per
    /// produced deal and stops when it turns true.
    pub fn measure_satisfied(&self) -> bool {
        self.stop.satisfied(&self.leveling_counts)
    }

    /// How many times the rarest levelling category has been seen.
    ///
    /// What the precision of a levelling rests on, and so the honest measure of
    /// how far a characterizing pass has got: a run that has produced a million
    /// deals is no further along than its scarcest category says it is.
    pub fn rarest_measured(&self) -> usize {
        self.leveling_counts.iter().copied().min().unwrap_or(0)
    }

    /// Deals observed, which is the run's produced count.
    pub fn produced(&self) -> usize {
        self.produced
    }

    /// The script's hand types, in declaration order, without their prefix.
    pub fn hand_type_labels(&self) -> &[String] {
        &self.hand_type_labels
    }

    /// How many produced deals matched each, parallel to the labels.
    pub fn hand_type_counts(&self) -> &[usize] {
        &self.hand_type_counts
    }

    /// The levelling decomposition's labels, which are the hand types' unless
    /// the script declares `LevelType_` variables of its own.
    pub fn leveling_labels(&self) -> &[String] {
        &self.leveling_labels
    }

    /// How many produced deals matched each, parallel to those labels.
    pub fn leveling_counts(&self) -> &[usize] {
        &self.leveling_counts
    }

    /// What was measured, in the form the levelling arithmetic reads.
    ///
    /// `generated` is the caller's: this counts what the condition accepted and
    /// has no idea how many deals it was offered.
    pub fn measurement(&self, generated: usize) -> dealer_level::Measurement {
        dealer_level::Measurement {
            produced: self.produced,
            generated,
            names: self.leveling_labels.clone(),
            counts: self.leveling_counts.clone(),
            prefix: self.leveling_prefix,
            groups: if self.joint.is_empty() {
                Vec::new()
            } else {
                self.hand_type_labels.clone()
            },
            joint: self.joint.clone(),
        }
    }

    /// The `average` and `frequency` results, binned.
    pub fn finish(self) -> Stats {
        let averages = self
            .averages
            .into_iter()
            .map(|a| AverageStat {
                label: a.label,
                value: if a.count > 0 {
                    a.sum / a.count as f64
                } else {
                    0.0
                },
                count: a.count,
                is_hand_type: a.is_hand_type,
            })
            .collect();

        let frequencies = self
            .frequencies
            .into_iter()
            .map(|f| {
                let histogram = f.counts;
                let total: usize = histogram.values().sum();
                let (lo, hi) = match f.range {
                    Some((lo, hi)) => (lo, hi),
                    None => (
                        histogram.keys().copied().min().unwrap_or(0),
                        histogram.keys().copied().max().unwrap_or(0),
                    ),
                };
                FrequencyStat {
                    label: f.label,
                    min: f.range.map(|(lo, _)| lo),
                    max: f.range.map(|(_, hi)| hi),
                    bins: (lo..=hi)
                        .map(|v| (v, histogram.get(&v).copied().unwrap_or(0)))
                        .collect(),
                    // Excluded rather than `hi + 1`, which overflows on a
                    // range ending at `i32::MAX`.
                    below: histogram.range(..lo).map(|(_, v)| v).sum(),
                    above: histogram
                        .range((Bound::Excluded(hi), Bound::Unbounded))
                        .map(|(_, v)| v)
                        .sum(),
                    total,
                }
            })
            .collect();

        Stats {
            averages,
            frequencies,
        }
    }
}

/// Deals kept from a characterizing pass, by the seed that makes them.
///
/// Eight bytes each, and no allocation: a deal is a pure function of one `u64`
/// (`dealer_core::generate_deal_from_seed`), so keeping the seed keeps the deal
/// and regenerating it needs no replay of the stream. Keeping the `Deal` itself
/// would be four heap allocations apiece — about 25 MB for a hundred thousand
/// of them, against 800 KB here.
///
/// Bounded, and deliberately so. The point of keeping them is that a levelled
/// run is a filter over what the characterizing pass already dealt, so those
/// deals need not be dealt again. If the bound cuts the set short the producing
/// pass simply deals the rest itself. **The budget can never make a result
/// wrong, only fail to save time** — which is what makes it a number worth
/// tuning later rather than getting right first.
///
/// That last promise is what [`Retained::through`] is for, and it is not free.
/// The seeds alone say which deals were kept, not where they sat in the stream,
/// and a pass that resumed by starting a fresh generator would deal the replayed
/// deals a second time — the same deals, produced twice. So the position of the
/// last kept seed travels with them: everything up to it was examined and every
/// match in it was kept, so the next pass may begin immediately after.
#[derive(Debug, Default)]
pub struct Retained {
    seeds: Vec<u64>,
    budget: usize,
    through: usize,
}

impl Retained {
    /// Keep at most `budget` seeds. Zero keeps none.
    pub fn new(budget: usize) -> Self {
        Retained {
            seeds: Vec::new(),
            budget,
            through: 0,
        }
    }

    /// Offer a matching deal's seed, kept if there is room.
    ///
    /// `position` is how many deals the generator had drawn, this one included.
    pub fn offer(&mut self, seed: u64, position: usize) {
        if self.seeds.len() < self.budget {
            self.seeds.push(seed);
            self.through = position;
        }
    }

    /// The seeds, in the order the deals came.
    pub fn seeds(&self) -> &[u64] {
        &self.seeds
    }

    /// How far into the stream these seeds account for.
    ///
    /// Every deal up to here was examined and every match among them kept, so a
    /// pass replaying [`Retained::seeds`] has covered exactly this much and must
    /// draw its next deal from `through + 1` — not from the start, which would
    /// produce the replayed deals all over again.
    pub fn through(&self) -> usize {
        self.through
    }

    /// Whether the budget stopped it keeping everything it was offered.
    pub fn full(&self) -> bool {
        self.seeds.len() >= self.budget
    }
}

/// Which of `names` this deal is, refusing two.
///
/// Two matching is refused rather than resolved: the categories are meant to
/// partition the deals, and a tag that silently picked the first would leave a
/// practice set quietly wrong about what it contains.
fn pick(
    names: &[&str],
    ctx: &EvalContext,
    deal: &Deal,
    kind: &'static str,
) -> Result<Option<usize>, RunError> {
    let mut matched: Option<usize> = None;
    for (i, name) in names.iter().enumerate() {
        let value =
            eval(&Expr::Variable((*name).to_string()), ctx).map_err(|e| RunError::Eval {
                what: format!("{} type `{}` could not be evaluated", kind, name),
                message: e.to_string(),
            })?;
        if value != 0 {
            if let Some(first) = matched {
                return Err(RunError::Overlap {
                    kind,
                    first: names[first].to_string(),
                    second: (*name).to_string(),
                    deal: Box::new(deal.clone()),
                });
            }
            matched = Some(i);
        }
    }
    Ok(matched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dealer_core::FastDealGenerator;
    use dealer_eval::extract_variables;

    /// Run `count` deals from a fixed seed through an accumulator, accepting
    /// every one. The condition is the front end's business, so the tests give
    /// it nothing to reject and watch what the body does with what it sees.
    fn observe_all(
        program: &Program,
        stop: MeasureStop,
        count: usize,
    ) -> (RunAccumulator<'_>, Vec<Matched>) {
        let variables = extract_variables(program);
        let mut acc = RunAccumulator::new(program, stop).expect("accumulator");
        let mut generator = FastDealGenerator::new(20260829);
        let mut matched = Vec::new();
        for _ in 0..count {
            let deal = generator.next_deal();
            matched.push(acc.observe(&deal, &variables, None).expect("observe"));
        }
        (acc, matched)
    }

    fn parse(source: &str) -> Program {
        dealer_parser::parse_program(source).expect("parse")
    }

    const LADDER: &str = "\
HandType_Weak = hcp(north) <= 9
HandType_Middling = hcp(north) >= 10 and hcp(north) <= 14
HandType_Strong = hcp(north) >= 15
condition 1
";

    #[test]
    fn nothing_to_divide_by_is_not_enough_measured() {
        let stop = MeasureStop::new(2);
        assert!(!stop.satisfied(&[]), "a script naming no categories");
        assert!(!stop.satisfied(&[5, 5, 1]), "one category still short");
        assert!(stop.satisfied(&[2, 9, 2]), "every category at the goal");
    }

    #[test]
    fn hand_types_are_counted_and_partition_what_was_produced() {
        let program = parse(LADDER);
        let (acc, matched) = observe_all(&program, MeasureStop::standard(), 500);

        assert_eq!(acc.produced(), 500);
        assert_eq!(acc.hand_type_labels(), ["Weak", "Middling", "Strong"]);
        assert_eq!(acc.hand_type_counts().iter().sum::<usize>(), 500);
        assert!(
            acc.hand_type_counts().iter().all(|n| *n > 0),
            "every band should show up over 500 deals: {:?}",
            acc.hand_type_counts()
        );
        // The indices handed back are the ones the counts are kept under.
        let mut tally = vec![0usize; 3];
        for m in &matched {
            tally[m.hand_type.expect("the bands cover every deal")] += 1;
        }
        assert_eq!(tally, acc.hand_type_counts());
    }

    #[test]
    fn a_deal_in_two_categories_is_refused_and_the_deal_comes_back() {
        // Deliberately overlapping: every deal with 15+ is also 10+.
        let program = parse(
            "HandType_Ten = hcp(north) >= 10\nHandType_Fifteen = hcp(north) >= 15\ncondition 1\n",
        );
        let variables = extract_variables(&program);
        let mut acc = RunAccumulator::new(&program, MeasureStop::standard()).expect("accumulator");
        let mut generator = FastDealGenerator::new(20260829);

        let error = loop {
            let deal = generator.next_deal();
            if let Err(e) = acc.observe(&deal, &variables, None) {
                break e;
            }
        };
        match &error {
            RunError::Overlap {
                kind,
                first,
                second,
                ..
            } => {
                assert_eq!(*kind, "Hand");
                assert_eq!(first, "HandType_Ten");
                assert_eq!(second, "HandType_Fifteen");
            }
            other => panic!("expected an overlap, got {:?}", other),
        }
        assert!(
            error.deal().is_some(),
            "the overlapping deal is what a reader has to look at"
        );
        assert!(error.to_string().contains("partition"));
    }

    #[test]
    fn measuring_stops_on_the_rarest_category_not_the_total() {
        // `Strong` is much the rarest of the three, so a goal of 20 is reached
        // long after the other two have passed it.
        let program = parse(LADDER);
        let variables = extract_variables(&program);
        let mut acc = RunAccumulator::new(&program, MeasureStop::new(20)).expect("accumulator");
        let mut generator = FastDealGenerator::new(20260829);
        while !acc.measure_satisfied() {
            let deal = generator.next_deal();
            acc.observe(&deal, &variables, None).expect("observe");
            assert!(
                acc.produced() < 10_000,
                "should have stopped long before now"
            );
        }
        assert!(
            acc.leveling_counts().iter().all(|n| *n >= 20),
            "stopped with a category short: {:?}",
            acc.leveling_counts()
        );
        assert_eq!(
            acc.leveling_counts().iter().filter(|n| **n == 20).count(),
            1,
            "exactly one category should be sitting on the goal — the rarest, \
             which is what stopped it: {:?}",
            acc.leveling_counts()
        );
    }

    #[test]
    fn a_declared_level_decomposition_is_counted_apart_from_the_hand_types() {
        let program = parse(
            "\
HandType_Weak = hcp(north) <= 11
HandType_Strong = hcp(north) >= 12
LevelType_Short = spades(north) <= 3
LevelType_Long = spades(north) >= 4
condition 1
",
        );
        let (acc, matched) = observe_all(&program, MeasureStop::standard(), 400);

        assert_eq!(acc.hand_type_labels(), ["Weak", "Strong"]);
        assert_eq!(acc.leveling_labels(), ["Short", "Long"]);
        assert_eq!(acc.hand_type_counts().iter().sum::<usize>(), 400);
        assert_eq!(acc.leveling_counts().iter().sum::<usize>(), 400);
        // Independent decompositions, so the two should disagree.
        assert_ne!(acc.hand_type_counts(), acc.leveling_counts());

        // The joint table is what says where the keeps leave each hand type,
        // so its rows have to add up to the hand types and its columns to the
        // levelling categories.
        let measured = acc.measurement(400);
        assert_eq!(measured.groups, ["Weak", "Strong"]);
        for (row, count) in measured.joint.iter().zip(acc.hand_type_counts()) {
            assert_eq!(row.iter().sum::<usize>(), *count);
        }
        for (i, count) in acc.leveling_counts().iter().enumerate() {
            assert_eq!(measured.joint.iter().map(|r| r[i]).sum::<usize>(), *count);
        }
        // Both classifications reach the caller.
        assert!(matched
            .iter()
            .all(|m| m.hand_type.is_some() && m.level_type.is_some()));
    }

    #[test]
    fn without_level_types_the_hand_types_serve_for_both() {
        let program = parse(LADDER);
        let (acc, matched) = observe_all(&program, MeasureStop::standard(), 200);
        assert_eq!(acc.leveling_labels(), acc.hand_type_labels());
        assert_eq!(acc.leveling_counts(), acc.hand_type_counts());
        assert!(
            acc.measurement(200).joint.is_empty(),
            "one decomposition needs no joint table"
        );
        assert!(matched.iter().all(|m| m.hand_type == m.level_type));
    }

    #[test]
    fn averages_and_frequencies_accumulate_over_what_was_observed() {
        let program = parse(
            "\
condition 1
action printall,
  average \"N HCP\" hcp(north),
  frequency \"NHCP\" (hcp(north), 10, 16)
",
        );
        let (acc, _) = observe_all(&program, MeasureStop::standard(), 1000);
        let stats = acc.finish();

        assert_eq!(stats.averages.len(), 1);
        let avg = &stats.averages[0];
        assert_eq!(avg.label.as_deref(), Some("N HCP"));
        assert_eq!(avg.count, 1000);
        assert!(!avg.is_hand_type);
        // A hand is a quarter of a 40-point pack.
        assert!(
            (avg.value - 10.0).abs() < 0.6,
            "mean hcp over 1000 deals was {}",
            avg.value
        );

        assert_eq!(stats.frequencies.len(), 1);
        let freq = &stats.frequencies[0];
        assert_eq!((freq.min, freq.max), (Some(10), Some(16)));
        assert_eq!(
            freq.bins.iter().map(|(v, _)| *v).collect::<Vec<_>>(),
            (10..=16).collect::<Vec<_>>()
        );
        assert_eq!(
            freq.total, 1000,
            "every observation is counted, in range or not"
        );
        let inside: usize = freq.bins.iter().map(|(_, c)| c).sum();
        assert_eq!(inside + freq.below + freq.above, freq.total);
        assert!(
            freq.below > 0 && freq.above > 0,
            "10..16 leaves both tails out"
        );
    }

    /// The invariant the replay rests on.
    ///
    /// Seeds alone say which deals were kept, not where they sat in the stream.
    /// A pass that replayed them and then started a fresh generator would deal
    /// the replayed deals over again — the bug this field exists to prevent,
    /// and one that hides completely at a budget large enough never to bind.
    #[test]
    fn retained_seeds_say_how_far_into_the_stream_they_reach() {
        let mut kept = Retained::new(3);
        // Matches at stream positions 5, 11 and 40, then one it has no room for.
        for (seed, position) in [(0xAA, 5), (0xBB, 11), (0xCC, 40), (0xDD, 77)] {
            kept.offer(seed, position);
        }
        assert_eq!(kept.seeds(), [0xAA, 0xBB, 0xCC]);
        assert!(kept.full());
        assert_eq!(
            kept.through(),
            40,
            "the position of the last seed kept, not of the last one offered — \
             resuming at 77 would skip the deals between"
        );
    }

    #[test]
    fn keeping_nothing_reaches_nowhere() {
        let mut kept = Retained::new(0);
        kept.offer(1, 9);
        assert!(kept.seeds().is_empty());
        assert!(kept.full(), "a budget of nothing is full from the start");
        assert_eq!(kept.through(), 0, "so a later pass must start from the top");
    }

    /// The boundary that keeps `rnd()` compatible with the original.
    ///
    /// The `average`s and the hand types are evaluated in separate contexts, so
    /// each starts the deal's `rnd()` stream afresh and a hand type's draw does
    /// not depend on how many `average`s were declared. Share one context and
    /// adding an unrelated `average` silently reclassifies deals — which is why
    /// this is asserted rather than left to the comment explaining it.
    ///
    /// One rnd-driven type, not two: the hand types share a context with each
    /// other, so a second would draw a different number from the same stream
    /// and the two could claim the same deal.
    #[test]
    fn an_average_calling_rnd_does_not_move_a_hand_type_calling_rnd() {
        let bands = "HandType_Coin = rnd(100) < 50\ncondition 1\n";
        let alone = parse(bands);
        let (without, _) = observe_all(&alone, MeasureStop::standard(), 400);

        let with_average = parse(&format!(
            "{}action printall,\n  average \"noise\" rnd(100)\n",
            bands
        ));
        let (with, _) = observe_all(&with_average, MeasureStop::standard(), 400);

        assert_eq!(
            without.hand_type_counts(),
            with.hand_type_counts(),
            "declaring an average that draws from rnd() moved the hand type's \
             own draw, so the two share a context when they must not"
        );
        // And the draws really are being made, or the test proves nothing.
        let heads = without.hand_type_counts()[0];
        assert!(
            heads > 0 && heads < 400,
            "a coin landed the same way 400 times: {}",
            heads
        );
    }

    #[test]
    fn an_average_over_a_hand_type_is_marked_as_one() {
        let program = parse(
            "\
HandType_Strong = hcp(north) >= 15
condition 1
action printall,
  average \"share\" 100 * HandType_Strong,
  average \"points\" hcp(north)
",
        );
        let (acc, _) = observe_all(&program, MeasureStop::standard(), 100);
        let stats = acc.finish();
        assert_eq!(
            stats
                .averages
                .iter()
                .map(|a| a.is_hand_type)
                .collect::<Vec<_>>(),
            [true, false],
            "only the first mentions a HandType_"
        );
    }
}
