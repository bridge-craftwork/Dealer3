//! A run, from a script to its deals.
//!
//! One entry point — [`run`] — that deals a scenario and hands over what
//! matches, levelling it first when asked. Both front ends call it, so there is
//! one generate loop rather than two that agree by inspection.
//!
//! Everything about *dealing* is here: the stream, predeal, swapping, the
//! condition, the categories, the statistics, and the levelling. A front end
//! parses arguments or paints a page; it does not deal.
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

/// What a run needs to know.
pub struct RunOptions {
    pub seed: u32,
    /// Deals to produce. Zero levels a scenario without dealing from it, which
    /// is what a caller writing the levelled copy out and stopping there wants.
    pub produce: usize,
    /// Deals to deal, across every pass the run makes.
    pub max_generate: usize,
    pub deals: Deals,
    /// Level the scenario's categories before dealing. Absent for an ordinary
    /// run, which deals the script as written.
    pub leveling: Option<LevelingOptions>,
    /// Threads to deal and test on. 0 asks the machine what it has, 1 stays on
    /// this one. Ignored without the `parallel` feature, which is how a build
    /// with no threads to spawn — wasm32, today — gets the same answers more
    /// slowly rather than not at all.
    ///
    /// It cannot change what comes out: seeds are drawn in order on one thread,
    /// and the deals they make are collected back in the order they were drawn.
    pub threads: usize,
    /// Deals to work on at a time. 0 sizes it from `threads`. Larger amortises
    /// the hand-off; smaller answers a clock sooner.
    pub batch: usize,
    /// What `$1` and friends stand for, from the command line's `--param`.
    /// Applied here rather than by the caller because a levelled run
    /// preprocesses twice — the scenario, then its levelled copy — and a script
    /// half-substituted the second time would not parse.
    pub params: dealer_parser::ScriptParams,
}

/// What levelling a scenario needs to know.
pub struct LevelingOptions {
    /// Weight per levelling category. `None` takes the script's own
    /// `HandType_X_Share` declarations, which default to an even mix.
    pub target: Option<Vec<f64>>,
    /// Deals dealt per deal kept, above which exactness is relaxed.
    pub budget: Option<f64>,
    /// Fewest sightings of a category the run will divide by.
    pub min_sample: usize,
    /// Ceiling on what the characterizing pass may produce.
    pub measure_cap: usize,
}

/// What a run did.
pub struct RunReport {
    pub produced: usize,
    /// Every deal the run looked at, across every pass it made.
    pub generated: usize,
    /// True if the deal budget ran out before `produce` was satisfied, which is
    /// not the same as there being no more matches.
    pub hit_limit: bool,
    /// The script's hand types and how many produced deals matched each.
    pub hand_types: Vec<(String, usize)>,
    /// Its `average` and `frequency` results.
    pub stats: Stats,
    /// Present only when the run was levelled.
    pub leveling: Option<LevelingReport>,
}

/// What levelling a scenario came to.
pub struct LevelingReport {
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
    /// Deals dealt while characterizing, which is nearly all of a levelled
    /// run's work and almost none of what it returns.
    pub characterized: usize,
    /// Deals the producing pass had to deal for itself. Usually none: a
    /// levelled run is a filter over deals the characterizing pass already
    /// dealt, so they are re-used rather than dealt again.
    pub additional: usize,
}

impl LevelingReport {
    /// Deals dealt per deal kept.
    pub fn cost(&self) -> f64 {
        1.0 / (self.base_rate * self.acceptance)
    }
}

/// What a front end supplies to a run: when to stop, and where produced deals
/// go.
///
/// Deliberately small, and deliberately free of anything about *generating*.
/// Dealing, testing and how many threads to do it on are the same job wherever
/// it happens, so they are the engine's — a caller that had to supply threading
/// would be as fast as it happened to bother being.
pub trait RunHost {
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

    /// A pass is over, with its final numbers.
    ///
    /// Separate from [`RunHost::should_stop`] because a caller that throttles
    /// its reports needs one it will not throttle away: a bar frozen at 76% as
    /// the next phase starts reads as something having gone wrong.
    fn pass_finished(
        &mut self,
        _phase: Phase,
        _produced: usize,
        _generated: usize,
        _target: usize,
    ) {
    }

    /// A deal the producing pass is handing over.
    fn produced(&mut self, deal: &Produced) -> Result<(), String>;
}

/// The engine's thread pool, and how it maps work over a batch.
///
/// A pool of its own rather than rayon's global one: a global can only be
/// configured once per process, so a second run in the same process would
/// silently keep the first one's thread count.
struct Workers {
    #[cfg(feature = "parallel")]
    pool: Option<rayon::ThreadPool>,
}

impl Workers {
    fn new(_threads: usize) -> Self {
        #[cfg(feature = "parallel")]
        // One thread is this one, so there is nothing to hand off to.
        let pool = if _threads == 1 {
            None
        } else {
            rayon::ThreadPoolBuilder::new()
                .num_threads(_threads)
                .build()
                .ok()
        };
        Workers {
            #[cfg(feature = "parallel")]
            pool,
        }
    }

    /// Build and test every deal in a batch.
    ///
    /// Both together, because which of the two costs more is a property of the
    /// script and not of the engine: a shuffle is about a microsecond, while a
    /// condition ranges from a table lookup to a double-dummy solve. Splitting
    /// them would parallelise whichever half we guessed at.
    ///
    /// Collected by index, so the deals come back in the order their seeds were
    /// drawn however many threads worked on them.
    fn build_and_test(
        &self,
        count: usize,
        build: &(dyn Fn(usize) -> Deal + Sync),
        test: &(dyn Fn(&Deal) -> bool + Sync),
    ) -> Vec<(Deal, bool)> {
        let one = |index: usize| {
            let deal = build(index);
            let passed = test(&deal);
            (deal, passed)
        };
        #[cfg(feature = "parallel")]
        if let Some(pool) = &self.pool {
            use rayon::prelude::*;
            return pool.install(|| (0..count).into_par_iter().map(one).collect());
        }
        (0..count).map(one).collect()
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
    reports: &'a Reports,
}

/// What a script asked to be written out per produced deal.
#[derive(Default)]
struct Reports {
    printes: Vec<Vec<dealer_parser::EsTerm>>,
    printrpt: Vec<Vec<dealer_parser::CsvTerm>>,
    csvrpt: Vec<Vec<dealer_parser::CsvTerm>>,
}

/// The rows a script's `printrpt` and `csvrpt` statements make of one deal.
///
/// Together because they differ only in where the row goes — same terms, same
/// quoting, same commas — and because the original evaluates them as one call,
/// so a `rnd()` in either draws from the same stream.
pub struct Rows {
    /// One per `printrpt`, which the original writes to the terminal.
    pub printed: Vec<String>,
    /// One per `csvrpt`, which the original writes to a file.
    pub csv: Vec<String>,
}

impl Produced<'_> {
    /// A fresh context over this deal, for a caller's own per-deal work.
    ///
    /// Fresh rather than shared for the reason contexts are built where they
    /// are everywhere else: where their boundaries fall is what a `rnd()` in
    /// one draws against a `rnd()` in another.
    pub fn context(&self) -> dealer_eval::EvalContext<'_> {
        dealer_eval::EvalContext::with_counts(self.deal, self.variables, self.point_counts)
    }

    /// What the script's `printes` statements say for this deal.
    ///
    /// Nothing between terms and no line ending unless the script asked for
    /// one, as the original writes it. Empty when the script has none.
    pub fn printes(&self) -> Result<String, String> {
        if self.reports.printes.is_empty() {
            return Ok(String::new());
        }
        let ctx = self.context();
        let mut out = String::new();
        for terms in &self.reports.printes {
            for term in terms {
                match term {
                    dealer_parser::EsTerm::String(text) => out.push_str(text),
                    dealer_parser::EsTerm::Newline => out.push('\n'),
                    dealer_parser::EsTerm::Expression(expr) => {
                        let value = dealer_eval::eval(expr, &ctx)
                            .map_err(|e| format!("printes evaluation error: {}", e))?;
                        out.push_str(&value.to_string());
                    }
                }
            }
        }
        Ok(out)
    }

    /// The report rows this deal makes, for the caller to put where they go.
    pub fn rows(&self) -> Result<Rows, String> {
        if self.reports.printrpt.is_empty() && self.reports.csvrpt.is_empty() {
            return Ok(Rows {
                printed: Vec::new(),
                csv: Vec::new(),
            });
        }
        let ctx = self.context();
        let render = |terms: &[dealer_parser::CsvTerm]| report_row(terms, self.deal, &ctx);
        Ok(Rows {
            printed: self
                .reports
                .printrpt
                .iter()
                .map(|terms| render(terms))
                .collect::<Result<_, _>>()?,
            csv: self
                .reports
                .csvrpt
                .iter()
                .map(|terms| render(terms))
                .collect::<Result<_, _>>()?,
        })
    }
}

/// One `printrpt` or `csvrpt` list, as a comma-separated row.
///
/// One renderer because DealerV2_4's two statements differ only in where the
/// row goes.
fn report_row(
    terms: &[dealer_parser::CsvTerm],
    deal: &Deal,
    ctx: &dealer_eval::EvalContext,
) -> Result<String, String> {
    use dealer_core::Position;
    use dealer_parser::{CsvTerm, Side};
    use dealer_pbn::format_hand_pbn;
    let mut parts: Vec<String> = Vec::new();
    for term in terms {
        match term {
            CsvTerm::Expression(expr) => {
                let value = dealer_eval::eval(expr, ctx)
                    .map_err(|e| format!("Report evaluation error: {}", e))?;
                parts.push(value.to_string());
            }
            CsvTerm::String(text) => parts.push(format!("'{}'", text)),
            CsvTerm::Compass(pos) => parts.push(format_hand_pbn(deal.hand(*pos))),
            CsvTerm::Side(side) => {
                let (a, b) = match side {
                    Side::NS => (Position::North, Position::South),
                    Side::EW => (Position::East, Position::West),
                };
                parts.push(format!(
                    "{} {}",
                    format_hand_pbn(deal.hand(a)),
                    format_hand_pbn(deal.hand(b))
                ));
            }
            CsvTerm::Deal => parts.push(format!(
                "{} {} {} {}",
                format_hand_pbn(deal.hand(Position::North)),
                format_hand_pbn(deal.hand(Position::East)),
                format_hand_pbn(deal.hand(Position::South)),
                format_hand_pbn(deal.hand(Position::West))
            )),
        }
    }
    Ok(parts.join(","))
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
    /// Arrangements a shuffle produced beyond what the last batch wanted. A
    /// shuffle's arrangements have to stay together and in order, and a batch
    /// size is not generally a multiple of the swap width.
    pending: std::collections::VecDeque<Handle>,
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

    /// Handles for the next `want` deals, fewer only if the supply ran out.
    ///
    /// Cheap and serial — a seed is one step of the generator — so that the
    /// expensive part, turning a seed into a deal, is left for whatever threads
    /// there are. Drawing them here in order is also what keeps a run's output
    /// independent of how many threads that turns out to be.
    fn next_handles(&mut self, want: usize) -> Vec<Handle> {
        let mut batch: Vec<Handle> = Vec::with_capacity(want);
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
                    batch.push(Handle::Given(self.position));
                    self.position += 1;
                }
                Deals::Shuffled { swap, .. } => {
                    let seed = self.generator.next_seed();
                    for variant in 0..swap.deals_per_shuffle() {
                        self.pending.push_back(Handle::Shuffled {
                            seed,
                            variant: variant as u8,
                        });
                    }
                }
            }
        }
        batch
    }

    /// The deal a handle stands for.
    fn build(&self, handle: Handle) -> Deal {
        match handle {
            Handle::Shuffled { seed, variant } => match &self.deals {
                Deals::Shuffled { swap, .. } => swap.apply(&self.shuffle(seed), variant as usize),
                Deals::Given(_) => self.shuffle(seed),
            },
            Handle::Given(index) => match &self.deals {
                Deals::Given(all) => all[index].clone(),
                Deals::Shuffled { .. } => Deal::new(),
            },
        }
    }

    fn shuffle(&self, seed: u64) -> Deal {
        if self.generator.has_predeal() {
            generate_deal_from_seed(seed, self.generator.config())
        } else {
            generate_deal_from_seed_no_predeal(seed)
        }
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
    params: &'a dealer_parser::ScriptParams,
    /// Threads to deal and test on, and how many deals to hand them at a time.
    threads: usize,
    batch: usize,
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
) -> Result<Pass, RunError> {
    let preprocessed = dealer_parser::preprocess_all(script, opts.params)?;
    let program =
        dealer_parser::parse_program(&preprocessed).map_err(|e| format!("Parse error: {}", e))?;
    let variables = dealer_eval::extract_variables(&program);
    let constraint = dealer_eval::extract_constraint(&program);
    let point_counts = dealer_eval::extract_point_counts(&program)
        .map_err(|e| format!("Point count error: {}", e))?;
    let point_counts = point_counts.as_ref();
    let mut accumulator = RunAccumulator::new(&program, MeasureStop::standard())?;

    let mut reports = Reports::default();
    for statement in &program.statements {
        match statement {
            dealer_parser::Statement::PrintReport(terms) => reports.printrpt.push(terms.clone()),
            dealer_parser::Statement::CsvReport(terms) => reports.csvrpt.push(terms.clone()),
            dealer_parser::Statement::Action {
                printes,
                print_reports,
                ..
            } => {
                reports.printes.extend(printes.iter().cloned());
                reports.printrpt.extend(print_reports.iter().cloned());
            }
            _ => {}
        }
    }

    let test = |deal: &Deal| match constraint {
        Some(expr) => {
            dealer_eval::eval_with_context_and_counts(expr, &variables, deal, point_counts)
                .map(|value| value != 0)
                .unwrap_or(false)
        }
        None => true,
    };

    let workers = Workers::new(opts.threads);
    let mut retained = Retained::new(opts.retain);
    let mut produced = 0usize;
    let mut generated = 0usize;
    let mut replayed = 0usize;
    let mut resumed = opts.replay.is_empty();
    let batch_size = opts.batch;

    'passing: while produced < opts.produce {
        // Whatever an earlier pass kept, before anything new.
        let (handles, from_stream) = if replayed < opts.replay.len() {
            let take = batch_size.min(opts.replay.len() - replayed);
            let handles = opts.replay[replayed..replayed + take].to_vec();
            replayed += take;
            (handles, false)
        } else {
            if !resumed {
                source.resume_after(opts.resume);
                resumed = true;
            }
            if generated >= opts.max_generate || source.exhausted() {
                break;
            }
            let want = batch_size.min(opts.max_generate - generated);
            let handles = source.next_handles(want);
            if handles.is_empty() {
                break;
            }
            (handles, true)
        };

        // The expensive half, on whatever threads there are: making each deal
        // and asking the condition about it. Which of the two costs more is the
        // script's business — a shuffle is about a microsecond, a `tricks()`
        // condition is ten milliseconds — so they travel together.
        let built =
            workers.build_and_test(handles.len(), &|index| source.build(handles[index]), &test);

        // Where the last of these sits in the stream, so a kept deal's position
        // is known without threading one through every deal.
        let batch_end = source.position;
        for (index, (deal, matched)) in built.iter().enumerate() {
            if from_stream {
                generated += 1;
            }
            if !matched {
                continue;
            }
            let hand_type = accumulator
                .observe(deal, &variables, point_counts)?
                .hand_type;
            if from_stream {
                retained.offer(handles[index], batch_end - (built.len() - 1 - index));
            }
            if opts.emit {
                host.produced(&Produced {
                    deal,
                    hand_type,
                    variables: &variables,
                    point_counts,
                    reports: &reports,
                })
                .map_err(RunError::Failed)?;
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
    host.pass_finished(
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

/// Deal a scenario, levelling it first when asked.
///
/// Deals reach the caller through [`RunHost::produced`]; what the run came to
/// is the report. With `opts.leveling` set the scenario is characterized first
/// — how often each of its categories comes up — the keep rate for each worked
/// out, and the levelled copy dealt; the report then carries that copy, for a
/// caller that wants to keep it.
///
/// With `produce` at zero nothing is dealt from the levelling, which is what a
/// caller writing the scenario out and stopping there wants.
pub fn run(script: &str, opts: RunOptions, host: &mut dyn RunHost) -> Result<RunReport, RunError> {
    let threads = resolve_threads(opts.threads);
    // Enough work per hand-off that the hand-off is not the expensive part, and
    // little enough that a caller answering to a clock is asked often.
    let batch = if opts.batch == 0 {
        (200 * threads).clamp(1024, 65_536)
    } else {
        opts.batch
    };

    let Some(leveling) = opts.leveling else {
        // An ordinary run: one pass, the script as written, every match handed
        // over as it comes.
        let mut source = Source::new(opts.deals, opts.seed);
        let pass = run_pass(
            script,
            &mut source,
            host,
            PassOptions {
                phase: Phase::Dealing,
                params: &opts.params,
                threads,
                batch,
                produce: opts.produce,
                max_generate: opts.max_generate,
                until_measured: false,
                retain: 0,
                replay: &[],
                resume: 0,
                emit: true,
            },
        )?;
        return Ok(RunReport {
            produced: pass.produced,
            generated: pass.generated,
            hit_limit: pass.hit_limit,
            hand_types: pass.hand_types,
            stats: pass.stats,
            leveling: None,
        });
    };

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
            params: &opts.params,
            threads,
            batch,
            produce: leveling.measure_cap,
            max_generate: opts.max_generate,
            until_measured: true,
            // Everything it matches, up to what a run could conceivably want.
            // Not a knob a caller should have to think about: too low only
            // costs the producing pass some dealing.
            retain: RETAIN_DEALS,
            replay: &[],
            resume: 0,
            emit: false,
        },
    )?;

    let program =
        dealer_parser::parse_program(&dealer_parser::preprocess_all(&prepared, &opts.params)?)
            .map_err(|e| format!("Parse error: {}", e))?;
    let weights = match leveling.target {
        Some(ref target) => target.clone(),
        None => dealer_level::leveling_types(&program)?.shares,
    };
    let leveled = dealer_level::level_from(
        &prepared,
        &characterizing.measurement,
        &weights,
        leveling.budget,
        opts.seed,
        leveling.min_sample,
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
                params: &opts.params,
                threads,
                batch,
                produce: opts.produce,
                // Every pass deals from one budget.
                max_generate: opts.max_generate.saturating_sub(characterizing.generated),
                until_measured: false,
                retain: 0,
                replay: &characterizing.retained.handles,
                resume: characterizing.retained.through,
                emit: true,
            },
        )?)
    };

    Ok(RunReport {
        produced: producing.as_ref().map(|p| p.produced).unwrap_or(0),
        generated: characterizing.generated + producing.as_ref().map(|p| p.generated).unwrap_or(0),
        hit_limit: producing.as_ref().map(|p| p.hit_limit).unwrap_or(false),
        hand_types: producing
            .as_ref()
            .map(|p| p.hand_types.clone())
            .unwrap_or_else(|| characterizing.hand_types.clone()),
        stats: match producing {
            Some(ref p) => p.stats.clone(),
            None => characterizing.stats,
        },
        leveling: Some(LevelingReport {
            script: leveled.script,
            plans: leveled.plans,
            lambda: leveled.lambda,
            acceptance: leveled.acceptance,
            base_rate: leveled.base_rate,
            warnings: leveled.warnings,
            natural_hand_types: characterizing.hand_types,
            natural_joint: characterizing.joint,
            measured: characterizing.measurement,
            characterized: characterizing.generated,
            additional: producing.as_ref().map(|p| p.generated).unwrap_or(0),
        }),
    })
}

const RETAIN_DEALS: usize = 1_000_000;

/// What `threads: 0` means on this machine.
///
/// One without the `parallel` feature, whatever was asked: a build with no
/// threads to spawn gets the same answers more slowly, which is exactly what it
/// means for the count not to change what comes out.
fn resolve_threads(asked: usize) -> usize {
    #[cfg(not(feature = "parallel"))]
    {
        let _ = asked;
        1
    }
    #[cfg(feature = "parallel")]
    if asked == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    } else {
        asked
    }
}

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
    }

    impl RunHost for Collector {
        fn should_stop(&mut self, _: Phase, _: usize, _: usize, _: usize) -> bool {
            false
        }
        fn pass_finished(&mut self, phase: Phase, _: usize, _: usize, _: usize) {
            self.phases.push(phase);
        }
        fn produced(&mut self, produced: &Produced) -> Result<(), String> {
            self.deals.push((produced.deal.clone(), produced.hand_type));
            Ok(())
        }
    }

    fn options(produce: usize, leveling: bool) -> RunOptions {
        RunOptions {
            seed: 20260829,
            produce,
            max_generate: 5_000_000,
            deals: Deals::Shuffled {
                predeal: FastDealConfig::new(),
                swap: SwapMode::None,
            },
            threads: 1,
            batch: 0,
            params: Default::default(),
            leveling: leveling.then_some(LevelingOptions {
                target: None,
                budget: None,
                min_sample: 50,
                measure_cap: 2_000_000,
            }),
        }
    }

    fn levelled(produce: usize) -> (Collector, RunReport) {
        let mut host = Collector::default();
        let report = super::run(LADDER, options(produce, true), &mut host).expect("run");
        (host, report)
    }

    #[test]
    fn a_levelled_run_delivers_the_mix_it_was_asked_for() {
        let (host, report) = levelled(300);
        assert_eq!(host.deals.len(), 300);
        assert_eq!(report.produced, 300);
        assert_eq!(report.hand_types.len(), 3);

        // Natural is lopsided; levelled is not. An even three-way split is a
        // third each, and 300 deals is enough to see that within a few points.
        let levelling = report.leveling.as_ref().expect("levelled");
        let natural: Vec<f64> = levelling
            .natural_hand_types
            .iter()
            .map(|(_, n)| *n as f64 / levelling.measured.produced as f64)
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
        let (host, report) = levelled(50);
        assert_eq!(
            host.deals.len(),
            50,
            "only the producing pass hands deals over"
        );
        let levelling = report.leveling.as_ref().expect("levelled");
        assert!(
            levelling.measured.produced > 10_000,
            "measuring a band this rare takes far more than the 50 asked for: {}",
            levelling.measured.produced
        );
        let rarest = levelling.measured.counts.iter().copied().min().unwrap_or(0);
        assert!(
            rarest >= dealer_level::MEASURE_GOAL,
            "stopped with the rarest category at {} of {}",
            rarest,
            dealer_level::MEASURE_GOAL
        );
        assert_eq!(levelling.warnings, Vec::<String>::new());
    }

    /// The cache, seen only by its effect: the producing pass deals nothing of
    /// its own, because the characterizing pass had already dealt everything it
    /// needed.
    #[test]
    fn the_producing_pass_deals_nothing_it_does_not_have_to() {
        let (_, report) = levelled(300);
        let levelling = report.leveling.as_ref().expect("levelled");
        assert_eq!(
            levelling.additional, 0,
            "every produced deal should come from what characterizing already dealt"
        );
        assert_eq!(report.generated, levelling.characterized);
    }

    /// And with a batch small enough to make the replay span many of them, the
    /// answer is the same — the seam between replaying and dealing is not
    /// allowed to change what comes out.
    #[test]
    fn the_deals_do_not_depend_on_the_batch_size() {
        let mut big = Collector::default();
        let a = super::run(LADDER, options(120, true), &mut big).expect("run");
        let mut small = Collector::default();
        let b = super::run(
            LADDER,
            RunOptions {
                batch: 7,
                ..options(120, true)
            },
            &mut small,
        )
        .expect("run");

        let (a, b) = (a.leveling.expect("levelled"), b.leveling.expect("levelled"));
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

    /// An ordinary run is the same call with no levelling: one pass, the script
    /// as written, and no report of a levelling that did not happen.
    #[test]
    fn a_plain_run_deals_the_script_as_written() {
        let mut host = Collector::default();
        let report = super::run(LADDER, options(80, false), &mut host).expect("run");
        assert!(report.leveling.is_none());
        assert_eq!(host.deals.len(), 80);
        assert_eq!(report.produced, 80);
        assert_eq!(host.phases, vec![Phase::Dealing]);
        assert!(
            report.generated >= 80,
            "a condition of 1 accepts every deal, so nothing is dealt twice"
        );
        // Nature's own mix, not an even one — nothing levelled it.
        let strong = report
            .hand_types
            .iter()
            .find(|(name, _)| name == "Strong")
            .map(|(_, n)| *n as f64 / 80.0)
            .expect("a Strong band");
        assert!(
            strong < 0.25,
            "16+ opposite nothing is rare, so it should be well under a third: {:.3}",
            strong
        );
    }

    /// The guarantee threading rests on.
    ///
    /// Seeds are drawn in order on one thread and the deals they make are
    /// collected back by index, so how many threads did the making cannot be
    /// read off the result. Without that a levelled scenario would depend on
    /// the machine that generated it, and the pair in `examples/` — regenerated
    /// and diffed by CI — would fail on any box but the one it was written on.
    ///
    /// Only meaningful with the `parallel` feature; without it every count
    /// resolves to one thread and the test passes trivially, which is itself
    /// worth asserting.
    #[test]
    fn thread_count_does_not_change_a_run() {
        let deals_at = |threads: usize| {
            let mut host = Collector::default();
            let report = super::run(
                LADDER,
                RunOptions {
                    threads,
                    ..options(150, true)
                },
                &mut host,
            )
            .expect("run");
            let levelling = report.leveling.expect("levelled");
            (host.deals, levelling.script, levelling.measured.counts)
        };
        let one = deals_at(1);
        for threads in [2, 4, 0] {
            let many = deals_at(threads);
            assert_eq!(one.1, many.1, "threads={} changed the levelling", threads);
            assert_eq!(one.2, many.2, "threads={} changed the measurement", threads);
            assert_eq!(
                one.0.len(),
                many.0.len(),
                "threads={} changed how many deals came out",
                threads
            );
            assert!(
                one.0.iter().zip(&many.0).all(|(a, b)| a == b),
                "threads={} changed the deals or their order",
                threads
            );
        }
    }

    /// `produce` at zero levels without dealing, which is what a caller writing
    /// the scenario out and stopping there wants.
    #[test]
    fn levelling_without_dealing() {
        let (host, report) = levelled(0);
        assert!(host.deals.is_empty());
        assert_eq!(report.produced, 0);
        let levelling = report.leveling.as_ref().expect("levelled");
        assert_eq!(levelling.additional, 0);
        assert!(levelling
            .script
            .contains("### BEGIN GENERATED LEVELING ###"));
        assert!(
            levelling.script.contains("roll"),
            "a levelled scenario keeps by a roll of the dice"
        );
        assert!(levelling
            .plans
            .iter()
            .all(|p| p.keep > 0.0 && p.keep <= 1.0));
    }
}
