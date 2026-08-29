//! A levelled run, from a script to its deals.
//!
//! One entry point — [`level_and_generate`] — that characterizes a scenario,
//! works out the keep rate for each of its categories, and deals the levelled
//! copy. Both front ends call it, so there is one levelling rather than two
//! that agree by inspection.
//!
//! # What the caller does not see
//!
//! That a levelled run makes two passes, and that the second is a filter over
//! the first. Both passes walk the same stream from the same seed; the levelled
//! scenario is the characterized one with the keeps added; and `rnd()` seeds
//! from the deal rather than from a running stream. So every deal the second
//! pass can produce is one the first already dealt, and they are kept — by the
//! eight-byte handle that reproduces them, not as deals — and re-run instead of
//! dealt again.
//!
//! None of that is a caller's business, which is the point: it was, once, and
//! the browser got it while the command line went without.
//!
//! # What the caller does supply
//!
//! Three things, through [`RunHost`], and they are the only real differences
//! between a terminal and a page: how to evaluate a batch of deals, when to
//! stop, and what to do with a deal that was produced.

use crate::{MeasureStop, RunAccumulator, RunError, Stats};
use dealer_core::{
    generate_deal_from_seed, generate_deal_from_seed_no_predeal, Deal, FastDealConfig,
    FastDealGenerator, SwapMode,
};

/// Which pass a progress report belongs to.
///
/// A levelled run deals the scenario twice, and without this one bar would
/// appear to finish and start over.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Working out what the scenario does — how often each category comes up —
    /// which is what the keeps are computed from.
    Characterizing,
    /// Producing the deals that were asked for, in a run that was not levelled.
    Dealing,
    /// The same, for a levelled run: whatever the characterizing pass did not
    /// already deal. Usually nothing, since a levelled run is a filter over
    /// deals that pass has seen.
    AdditionalDealing,
}

impl Phase {
    /// How the phase is named to a reader.
    pub fn name(self) -> &'static str {
        match self {
            Phase::Characterizing => "characterizing",
            Phase::Dealing => "dealing",
            Phase::AdditionalDealing => "additional dealing",
        }
    }
}

/// Where a run's deals come from.
pub enum Deals {
    /// Shuffled from the seed, which is every ordinary run.
    Shuffled {
        predeal: FastDealConfig,
        /// How many deals each shuffle is arranged into, for `-2` and `-3`.
        swap: SwapMode,
    },
    /// Supplied, as `--input-deals` supplies them.
    Given(Vec<Deal>),
}

/// What a levelled run needs to know.
pub struct LevelRunOptions {
    pub seed: u32,
    /// Deals to produce, which sizes the second pass only.
    pub produce: usize,
    /// Deals to deal, across both passes together.
    pub max_generate: usize,
    /// Weight per levelling category. `None` takes the script's own
    /// `HandType_X_Share` declarations, which default to an even mix.
    pub target: Option<Vec<f64>>,
    /// Deals dealt per deal kept, above which exactness is relaxed.
    pub budget: Option<f64>,
    /// Fewest sightings of a category the run will divide by.
    pub min_sample: usize,
    /// Ceiling on what the characterizing pass may produce.
    pub measure_cap: usize,
    pub deals: Deals,
}

/// What a levelled run did.
pub struct LevelRunReport {
    /// The scenario that was actually dealt, for a caller that wants to keep it.
    pub script: String,
    pub plans: Vec<dealer_level::LevelPlan>,
    /// 1 unless a budget relaxed the target.
    pub lambda: f64,
    /// The share of qualifying deals the keeps let through.
    pub acceptance: f64,
    /// Matching deals per deal dealt, before the keeps.
    pub base_rate: f64,
    pub warnings: Vec<String>,
    /// What the characterizing pass measured, which is what the keeps divide by.
    pub measured: dealer_level::Measurement,
    /// Its hand types and their counts, which is what `natural` means.
    pub natural_hand_types: Vec<(String, usize)>,
    /// How they crossed the levelling categories, empty unless the two
    /// decompositions differ.
    pub natural_joint: Vec<Vec<usize>>,
    /// The producing pass's hand types and counts, which is what was delivered.
    pub hand_types: Vec<(String, usize)>,
    /// Its `average` and `frequency` results.
    pub stats: Stats,
    pub produced: usize,
    /// Deals dealt while characterizing, which is nearly all of the work.
    pub characterized: usize,
    /// Deals the producing pass had to deal for itself. Usually none.
    pub additional: usize,
    /// True if the deal budget ran out before `produce` was satisfied.
    pub hit_limit: bool,
}

impl LevelRunReport {
    /// Deals dealt per deal kept, over both passes.
    pub fn cost(&self) -> f64 {
        1.0 / (self.base_rate * self.acceptance)
    }

    /// Every deal the run looked at.
    pub fn generated(&self) -> usize {
        self.characterized + self.additional
    }
}

/// What a front end supplies to a run: how to evaluate, when to stop, and where
/// produced deals go.
///
/// Deliberately small. Everything else about a levelled run is the same
/// wherever it happens, and belongs to the engine.
pub trait RunHost {
    /// Which of these deals the condition accepts.
    ///
    /// The default evaluates them in order. The command line overrides it to
    /// spread the batch across threads, which is the only reason this is the
    /// caller's business at all — the deals and their order are not affected.
    fn filter(&self, deals: &[Deal], test: &(dyn Fn(&Deal) -> bool + Sync)) -> Vec<bool> {
        deals.iter().map(test).collect()
    }

    /// Called as a pass goes, to report progress and to ask whether to stop.
    ///
    /// A page answers to a clock and a Cancel button; a terminal to `--timeout`
    /// and a progress meter. Returning true ends the pass where it stands,
    /// which is never an error: a characterizing pass reports what it managed
    /// and a producing pass returns what it has.
    fn should_stop(
        &mut self,
        _phase: Phase,
        _produced: usize,
        _generated: usize,
        _target: usize,
    ) -> bool {
        false
    }

    /// A deal the producing pass is handing over.
    fn produced(&mut self, deal: &Produced) -> Result<(), String>;

    /// How many deals to work on at a time. Larger amortises a thread pool;
    /// smaller answers a clock sooner.
    fn batch(&self) -> usize {
        4096
    }
}

/// A deal the producing pass has produced.
///
/// Carries the levelled scenario's variables as well as the deal, because a
/// front end has its own per-deal work — `printes` and `printrpt` write what
/// the script asked for — and that needs the program the engine parsed, not the
/// one the caller was holding. Contexts are built here rather than handed over
/// for the reason they are built where they are everywhere else: where their
/// boundaries fall is what a `rnd()` in one draws against a `rnd()` in another.
pub struct Produced<'a> {
    pub deal: &'a Deal,
    /// Index into the report's `hand_types`, or `None` where the scenario's
    /// categories do not cover every deal it produces.
    pub hand_type: Option<usize>,
    variables: &'a dealer_eval::Variables<'a>,
    point_counts: Option<&'a dealer_eval::PointCounts>,
}

impl<'a> Produced<'a> {
    /// A fresh context over this deal, for the caller's own per-deal work.
    pub fn context(&self) -> dealer_eval::EvalContext<'_> {
        dealer_eval::EvalContext::with_counts(self.deal, self.variables, self.point_counts)
    }
}

/// What reproduces one deal.
///
/// Eight bytes and a variant, because a deal is a pure function of the shuffle
/// seed that made it and which arrangement of that shuffle it is — so keeping
/// this keeps the deal, and remaking it costs a shuffle rather than a walk back
/// through the stream. Keeping the `Deal` would be four heap allocations
/// apiece: about 25 MB for a hundred thousand of them against 900 KB here.
#[derive(Clone, Copy)]
enum Handle {
    Shuffled {
        seed: u64,
        variant: u8,
    },
    /// An index into the supplied deals.
    Given(usize),
}

/// The stream a run draws from, and the only thing that knows how to go back to
/// a deal it has already dealt.
struct Source {
    seed: u32,
    deals: Deals,
    generator: FastDealGenerator,
    /// Deals drawn so far, which is the position `Retained::through` counts in.
    position: usize,
    /// Deals a shuffle produced beyond what the last batch wanted. A shuffle's
    /// arrangements have to stay together and in order, and a batch size is not
    /// generally a multiple of the swap width.
    pending: std::collections::VecDeque<(Handle, Deal)>,
}

impl Source {
    fn new(deals: Deals, seed: u32) -> Self {
        let generator = match &deals {
            Deals::Shuffled { predeal, .. } => {
                FastDealGenerator::with_config(seed as u64, predeal.clone())
            }
            Deals::Given(_) => FastDealGenerator::new(seed as u64),
        };
        Source {
            seed,
            deals,
            generator,
            position: 0,
            pending: std::collections::VecDeque::new(),
        }
    }

    /// Whether there can be any more deals at all.
    fn exhausted(&self) -> bool {
        match &self.deals {
            Deals::Given(all) => self.position >= all.len() && self.pending.is_empty(),
            Deals::Shuffled { .. } => false,
        }
    }

    /// The next `want` deals, fewer only if the supply ran out.
    fn next_batch(&mut self, want: usize) -> Vec<(Handle, Deal)> {
        let mut batch: Vec<(Handle, Deal)> = Vec::with_capacity(want);
        while batch.len() < want {
            if let Some(held) = self.pending.pop_front() {
                batch.push(held);
                self.position += 1;
                continue;
            }
            match &self.deals {
                Deals::Given(all) => {
                    if self.position >= all.len() {
                        break;
                    }
                    batch.push((Handle::Given(self.position), all[self.position].clone()));
                    self.position += 1;
                }
                Deals::Shuffled { swap, .. } => {
                    let seed = self.generator.next_seed();
                    let base = self.shuffle(seed);
                    for variant in 0..swap.deals_per_shuffle() {
                        self.pending.push_back((
                            Handle::Shuffled {
                                seed,
                                variant: variant as u8,
                            },
                            swap.apply(&base, variant),
                        ));
                    }
                }
            }
        }
        batch
    }

    fn shuffle(&self, seed: u64) -> Deal {
        if self.generator.has_predeal() {
            generate_deal_from_seed(seed, self.generator.config())
        } else {
            generate_deal_from_seed_no_predeal(seed)
        }
    }

    /// The deals these handles stand for.
    fn reproduce(&self, handles: &[Handle]) -> Vec<Deal> {
        handles
            .iter()
            .map(|handle| match handle {
                Handle::Shuffled { seed, variant } => match &self.deals {
                    Deals::Shuffled { swap, .. } => {
                        swap.apply(&self.shuffle(*seed), *variant as usize)
                    }
                    Deals::Given(_) => self.shuffle(*seed),
                },
                Handle::Given(index) => match &self.deals {
                    Deals::Given(all) => all[*index].clone(),
                    Deals::Shuffled { .. } => Deal::new(),
                },
            })
            .collect()
    }

    /// Wind to just past `position`, so a pass that has replayed everything up
    /// to there draws its next deal from the one after.
    ///
    /// Without this a second pass would start again at the first deal and
    /// produce every replayed deal a second time — which is invisible whenever
    /// the replay covers the whole run, and wrong the moment it does not.
    fn resume_after(&mut self, position: usize) {
        self.pending.clear();
        match &self.deals {
            Deals::Given(_) => self.position = position,
            Deals::Shuffled { swap, predeal } => {
                // Rebuilt rather than wound on: the characterizing pass left
                // the generator well past here, and a stream only runs forward.
                self.generator = FastDealGenerator::with_config(self.seed as u64, predeal.clone());
                let width = swap.deals_per_shuffle();
                // Whole shuffles only: a seed is one step of the generator with
                // no shuffle behind it, so skipping a million costs less than
                // dealing one. Any arrangements of the last shuffle that the
                // replay did not reach are dealt again, which is correct — they
                // were never produced.
                let shuffles = position / width;
                for _ in 0..shuffles {
                    self.generator.next_seed();
                }
                self.position = shuffles * width;
            }
        }
    }
}

/// Handles of matching deals, and how far into the stream they account for.
///
/// Bounded, and deliberately so: if the bound cuts the set short the producing
/// pass deals the rest itself. **The budget can never make a result wrong, only
/// fail to save time.**
struct Retained {
    handles: Vec<Handle>,
    budget: usize,
    through: usize,
}

impl Retained {
    fn new(budget: usize) -> Self {
        Retained {
            handles: Vec::new(),
            budget,
            through: 0,
        }
    }

    /// Offer a matching deal, kept if there is room. `position` is how many
    /// deals the stream had drawn, this one included.
    fn offer(&mut self, handle: Handle, position: usize) {
        if self.handles.len() < self.budget {
            self.handles.push(handle);
            self.through = position;
        }
    }
}

/// What one pass over the deals came to.
struct Pass {
    produced: usize,
    /// Deals drawn from the stream. Replayed deals are not among them: they
    /// were drawn, and counted, by the pass that kept them.
    generated: usize,
    measurement: dealer_level::Measurement,
    hand_types: Vec<(String, usize)>,
    joint: Vec<Vec<usize>>,
    stats: Stats,
    retained: Retained,
    hit_limit: bool,
}

/// Everything one pass varies.
struct PassOptions<'a> {
    phase: Phase,
    /// Deals to produce. A characterizing pass is stopped by `until_measured`
    /// long before this, which is only its ceiling.
    produce: usize,
    max_generate: usize,
    /// Stop as soon as every levelling category is worth dividing by.
    until_measured: bool,
    /// How many matching deals to keep for a later pass.
    retain: usize,
    /// Deals to re-run before drawing any new ones.
    replay: &'a [Handle],
    /// How far into the stream `replay` accounts for.
    resume: usize,
    /// Whether produced deals go to the host. A characterizing pass's deals
    /// exist to be counted and thrown away.
    emit: bool,
}

fn run_pass(
    script: &str,
    source: &mut Source,
    host: &mut dyn RunHost,
    opts: PassOptions,
) -> Result<Pass, String> {
    let preprocessed = dealer_parser::preprocess_all(script, &Default::default())?;
    let program =
        dealer_parser::parse_program(&preprocessed).map_err(|e| format!("Parse error: {}", e))?;
    let variables = dealer_eval::extract_variables(&program);
    let constraint = dealer_eval::extract_constraint(&program);
    let point_counts = dealer_eval::extract_point_counts(&program)
        .map_err(|e| format!("Point count error: {}", e))?;
    let point_counts = point_counts.as_ref();
    let mut accumulator = RunAccumulator::new(&program, MeasureStop::standard())?;

    let test = |deal: &Deal| match constraint {
        Some(expr) => {
            dealer_eval::eval_with_context_and_counts(expr, &variables, deal, point_counts)
                .map(|value| value != 0)
                .unwrap_or(false)
        }
        None => true,
    };

    let mut retained = Retained::new(opts.retain);
    let mut produced = 0usize;
    let mut generated = 0usize;
    let mut replayed = 0usize;
    let mut resumed = opts.replay.is_empty();
    let batch_size = host.batch().max(1);

    'passing: while produced < opts.produce {
        // Whatever an earlier pass kept, before anything new.
        let (handles, deals, from_stream) = if replayed < opts.replay.len() {
            let take = batch_size.min(opts.replay.len() - replayed);
            let handles = &opts.replay[replayed..replayed + take];
            replayed += take;
            (handles.to_vec(), source.reproduce(handles), false)
        } else {
            if !resumed {
                source.resume_after(opts.resume);
                resumed = true;
            }
            if generated >= opts.max_generate || source.exhausted() {
                break;
            }
            let want = batch_size.min(opts.max_generate - generated);
            let batch = source.next_batch(want);
            if batch.is_empty() {
                break;
            }
            let (handles, deals): (Vec<_>, Vec<_>) = batch.into_iter().unzip();
            (handles, deals, true)
        };

        // Where the last of these sits in the stream, so a kept deal's position
        // is known without threading one through every deal.
        let batch_end = source.position;
        let matched = host.filter(&deals, &test);
        for (index, deal) in deals.iter().enumerate() {
            if from_stream {
                generated += 1;
            }
            if !matched[index] {
                continue;
            }
            let hand_type = accumulator
                .observe(deal, &variables, point_counts)
                .map_err(|e: RunError| e.to_string())?
                .hand_type;
            if from_stream {
                retained.offer(handles[index], batch_end - (deals.len() - 1 - index));
            }
            if opts.emit {
                host.produced(&Produced {
                    deal,
                    hand_type,
                    variables: &variables,
                    point_counts,
                })?;
            }
            produced += 1;
            // Every category seen enough times to divide by, which is the whole
            // of what a characterizing pass is for.
            if opts.until_measured && accumulator.measure_satisfied() {
                break 'passing;
            }
            if produced >= opts.produce {
                break 'passing;
            }
        }
        if host.should_stop(
            opts.phase,
            if opts.until_measured {
                accumulator.rarest_measured()
            } else {
                produced
            },
            generated,
            if opts.until_measured {
                dealer_level::MEASURE_GOAL
            } else {
                opts.produce
            },
        ) {
            break;
        }
    }
    host.should_stop(
        opts.phase,
        if opts.until_measured {
            accumulator.rarest_measured()
        } else {
            produced
        },
        generated,
        if opts.until_measured {
            dealer_level::MEASURE_GOAL
        } else {
            opts.produce
        },
    );

    let measurement = accumulator.measurement(generated);
    let hand_types: Vec<(String, usize)> = accumulator
        .hand_type_labels()
        .iter()
        .cloned()
        .zip(accumulator.hand_type_counts().iter().copied())
        .collect();
    let joint = measurement.joint.clone();
    Ok(Pass {
        hit_limit: produced < opts.produce && generated >= opts.max_generate,
        produced,
        generated,
        measurement,
        hand_types,
        joint,
        stats: accumulator.finish(),
        retained,
    })
}

/// Characterize a scenario, level it, and deal the result.
///
/// The whole of a levelled run: `script` goes in, deals come out through
/// [`RunHost::produced`], and the report carries the scenario that was actually
/// dealt for a caller that wants to keep it.
///
/// With `produce` at zero the second pass is skipped — the levelling is worked
/// out and reported and nothing is dealt from it, which is what
/// `--write-leveled` wants.
pub fn level_and_generate(
    script: &str,
    opts: LevelRunOptions,
    host: &mut dyn RunHost,
) -> Result<LevelRunReport, String> {
    // The scenario with a levelling block in it: the same text the keeps will
    // be written into, so the two cannot describe different scenarios.
    let prepared = dealer_level::insert_leveling_block(script)?;
    dealer_level::check_leveling_source(&prepared)?;

    let mut source = Source::new(opts.deals, opts.seed);
    let characterizing = run_pass(
        &prepared,
        &mut source,
        host,
        PassOptions {
            phase: Phase::Characterizing,
            produce: opts.measure_cap,
            max_generate: opts.max_generate,
            until_measured: true,
            // Everything it matches, up to what a run could conceivably want.
            // Not a tuning knob a caller should have to think about: too low
            // only costs the producing pass some dealing.
            retain: RETAIN_DEALS,
            replay: &[],
            resume: 0,
            emit: false,
        },
    )?;

    let program = dealer_parser::parse_program(&dealer_parser::preprocess_all(
        &prepared,
        &Default::default(),
    )?)
    .map_err(|e| format!("Parse error: {}", e))?;
    let weights = match opts.target {
        Some(ref target) => target.clone(),
        None => dealer_level::leveling_types(&program)?.shares,
    };
    let leveled = dealer_level::level_from(
        &prepared,
        &characterizing.measurement,
        &weights,
        opts.budget,
        opts.seed,
        opts.min_sample,
    )?;

    // The deals asked for. A levelled scenario is the one just characterized
    // with the keeps added, so every deal it can produce is one that pass
    // already dealt: they are re-run from their handles rather than dealt
    // again, and only a run wanting more than was kept deals anything itself.
    let producing = if opts.produce == 0 {
        None
    } else {
        Some(run_pass(
            &leveled.script,
            &mut source,
            host,
            PassOptions {
                phase: Phase::AdditionalDealing,
                produce: opts.produce,
                // Both passes deal from one budget.
                max_generate: opts.max_generate.saturating_sub(characterizing.generated),
                until_measured: false,
                retain: 0,
                replay: &characterizing.retained.handles,
                resume: characterizing.retained.through,
                emit: true,
            },
        )?)
    };

    Ok(LevelRunReport {
        script: leveled.script,
        plans: leveled.plans,
        lambda: leveled.lambda,
        acceptance: leveled.acceptance,
        base_rate: leveled.base_rate,
        warnings: leveled.warnings,
        natural_hand_types: characterizing.hand_types.clone(),
        natural_joint: characterizing.joint.clone(),
        measured: characterizing.measurement,
        characterized: characterizing.generated,
        hand_types: producing
            .as_ref()
            .map(|p| p.hand_types.clone())
            .unwrap_or_else(|| characterizing.hand_types.clone()),
        stats: match producing {
            Some(ref p) => p.stats.clone(),
            None => characterizing.stats,
        },
        produced: producing.as_ref().map(|p| p.produced).unwrap_or(0),
        additional: producing.as_ref().map(|p| p.generated).unwrap_or(0),
        hit_limit: producing.as_ref().map(|p| p.hit_limit).unwrap_or(false),
    })
}

/// How many of the characterizing pass's deals to keep.
///
/// Eight bytes and change apiece, so this is a few MB at the very worst and
/// typically far less. High enough that the producing pass almost never has to
/// deal anything itself; when it does, it simply does.
const RETAIN_DEALS: usize = 1_000_000;

#[cfg(test)]
mod tests {
    use super::*;

    const LADDER: &str = "\
HandType_Weak = hcp(north) <= 10
HandType_Middling = hcp(north) >= 11 and hcp(north) <= 15
HandType_Strong = hcp(north) >= 16
condition 1
";

    /// A front end that keeps what it is given and nothing else.
    #[derive(Default)]
    struct Collector {
        deals: Vec<(Deal, Option<usize>)>,
        /// Phases seen, in order, so a test can say which passes ran.
        phases: Vec<Phase>,
        batch: usize,
    }

    impl RunHost for Collector {
        fn should_stop(&mut self, phase: Phase, _: usize, _: usize, _: usize) -> bool {
            if self.phases.last() != Some(&phase) {
                self.phases.push(phase);
            }
            false
        }
        fn produced(&mut self, produced: &Produced) -> Result<(), String> {
            self.deals.push((produced.deal.clone(), produced.hand_type));
            Ok(())
        }
        fn batch(&self) -> usize {
            if self.batch == 0 {
                4096
            } else {
                self.batch
            }
        }
    }

    fn options(produce: usize) -> LevelRunOptions {
        LevelRunOptions {
            seed: 20260829,
            produce,
            max_generate: 5_000_000,
            target: None,
            budget: None,
            min_sample: 50,
            measure_cap: 2_000_000,
            deals: Deals::Shuffled {
                predeal: FastDealConfig::new(),
                swap: SwapMode::None,
            },
        }
    }

    fn run(produce: usize) -> (Collector, LevelRunReport) {
        let mut host = Collector::default();
        let report = level_and_generate(LADDER, options(produce), &mut host).expect("run");
        (host, report)
    }

    #[test]
    fn a_levelled_run_delivers_the_mix_it_was_asked_for() {
        let (host, report) = run(300);
        assert_eq!(host.deals.len(), 300);
        assert_eq!(report.produced, 300);
        assert_eq!(report.hand_types.len(), 3);

        // Natural is lopsided; levelled is not. An even three-way split is a
        // third each, and 300 deals is enough to see that within a few points.
        let natural: Vec<f64> = report
            .natural_hand_types
            .iter()
            .map(|(_, n)| *n as f64 / report.measured.produced as f64)
            .collect();
        assert!(
            natural.iter().any(|s| *s < 0.2) && natural.iter().any(|s| *s > 0.4),
            "the bands should be far from even to start with: {:?}",
            natural
        );
        for (name, count) in &report.hand_types {
            let share = *count as f64 / 300.0;
            assert!(
                (share - 1.0 / 3.0).abs() < 0.06,
                "`{}` came out at {:.3} of a run levelled toward a third each",
                name,
                share
            );
        }
    }

    /// The characterizing pass stops on the scarcest category, whatever else it
    /// has seen — and its deals are counted, not returned.
    #[test]
    fn characterizing_measures_to_the_goal_and_returns_nothing() {
        let (host, report) = run(50);
        assert_eq!(
            host.deals.len(),
            50,
            "only the producing pass hands deals over"
        );
        assert!(
            report.measured.produced > 10_000,
            "measuring a band this rare takes far more than the 50 asked for: {}",
            report.measured.produced
        );
        let rarest = report.measured.counts.iter().copied().min().unwrap_or(0);
        assert!(
            rarest >= dealer_level::MEASURE_GOAL,
            "stopped with the rarest category at {} of {}",
            rarest,
            dealer_level::MEASURE_GOAL
        );
        assert_eq!(report.warnings, Vec::<String>::new());
    }

    /// The cache, seen only by its effect: the producing pass deals nothing of
    /// its own, because the characterizing pass had already dealt everything it
    /// needed.
    #[test]
    fn the_producing_pass_deals_nothing_it_does_not_have_to() {
        let (_, report) = run(300);
        assert_eq!(
            report.additional, 0,
            "every produced deal should come from what characterizing already dealt"
        );
        assert_eq!(report.generated(), report.characterized);
    }

    /// And with a batch small enough to make the replay span many of them, the
    /// answer is the same — the seam between replaying and dealing is not
    /// allowed to change what comes out.
    #[test]
    fn the_deals_do_not_depend_on_the_batch_size() {
        let mut big = Collector::default();
        let a = level_and_generate(LADDER, options(120), &mut big).expect("run");
        let mut small = Collector {
            batch: 7,
            ..Default::default()
        };
        let b = level_and_generate(LADDER, options(120), &mut small).expect("run");

        assert_eq!(
            a.script, b.script,
            "the same measurement, so the same keeps"
        );
        assert_eq!(
            big.deals.len(),
            small.deals.len(),
            "and the same number of deals"
        );
        assert!(
            big.deals.iter().zip(&small.deals).all(|(x, y)| x == y),
            "the same deals in the same order"
        );
    }

    /// `produce` at zero levels without dealing, which is what a caller writing
    /// the scenario out and stopping there wants.
    #[test]
    fn levelling_without_dealing() {
        let (host, report) = run(0);
        assert!(host.deals.is_empty());
        assert_eq!(report.produced, 0);
        assert_eq!(report.additional, 0);
        assert!(report.script.contains("### BEGIN GENERATED LEVELING ###"));
        assert!(
            report.script.contains("roll"),
            "a levelled scenario keeps by a roll of the dice"
        );
        assert!(report.plans.iter().all(|p| p.keep > 0.0 && p.keep <= 1.0));
    }
}
