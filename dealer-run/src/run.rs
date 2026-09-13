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
use dealer_eval::{EvalError, VarCache};

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
    /// Supplied, as `--input-deals` supplies them, each with the double-dummy
    /// table it arrived with.
    ///
    /// The table travels with its deal rather than going into a shared store:
    /// its lifetime is then the deal's, a deal the filter rejects takes its
    /// table with it, and how many deals a file holds stops mattering.
    Given(Vec<SolvedDeal>),
}

/// One supplied deal and whatever double-dummy answers it arrived with.
///
/// Empty answers are the ordinary case — most formats carry no analysis — and
/// mean only "nobody has solved this", which is what a dealt deal looks like
/// too.
pub type SolvedDeal = (Deal, dealer_dds::DealTricks);

/// One deal, whether it matched, and what testing it worked out along the way.
struct Tested {
    deal: Deal,
    passed: Result<bool, EvalError>,
    known: dealer_dds::DealTricks,
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
    /// Divide `produce` among the scenario's hand types instead of taking deals
    /// as they come: one of each per round, with any remainder going to
    /// whichever types turn up next, one apiece.
    ///
    /// `produce` still says how many. This only says how they are chosen, which
    /// is why it is a flag and not a second count.
    ///
    /// Nothing is measured, so nothing can be measured wrong — which is the
    /// point of it. A levelling divides by a rate it estimated, and a rate
    /// estimated 20% high delivers a mix 20% off for good, however many deals
    /// are produced afterwards. A round robin is exact by construction.
    ///
    /// Refused alongside `leveling`: applying keeps as well would throw away
    /// rare deals the round then has to wait for again.
    pub round_robin: bool,
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
    /// Which side is vulnerable, for the words that ask — `par()` today.
    ///
    /// The run's own setting, not the rotation `printpbn` applies when a script
    /// names none: a value that changed with a board's position would make
    /// `par()` answer differently for the same cards, which is not what a
    /// script asking about par means.
    pub vulnerability: dealer_core::Vulnerability,
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
    /// Where the characterizing pass's deals come out of.
    pub measure_deals: MeasureDeals,
}

/// Which deals a characterizing pass is allowed to look at.
///
/// Characterizing and producing are two passes with two different questions,
/// and a caller that wants to bound them separately has to be able to say so.
/// A front end where one number does both jobs can only be set for one of them.
pub enum MeasureDeals {
    /// Out of the run's `max_generate`: what characterizing deals, the
    /// producing pass no longer can. The command line's model, where `-g`
    /// bounds the whole run and `--level-timeout` bounds the pass.
    Shared,
    /// Its own allowance, leaving `max_generate` entirely to the producing
    /// pass.
    ///
    /// `usize::MAX` puts no deal ceiling on it at all, which leaves the host's
    /// clock — [`RunHost::should_stop`] — as the only thing that stops it. That
    /// is what the browser asks for: a reader there says how many seconds to
    /// spend characterizing, and the deal limit beside it is about the run.
    Own(usize),
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
    /// The categories the keeps were computed over, and how many produced deals
    /// matched each. Empty unless the script declares `LevelType_` variables:
    /// without them the levelling categories are the hand types above.
    ///
    /// This is what says a levelled run delivered its mix, when the two
    /// decompositions differ — `hand_types` then counts something else.
    pub level_types: Vec<(String, usize)>,
    /// What a round-robin run was aiming at, when it was one: the count every
    /// hand type is owed, and how many deals were left over for a partial round
    /// at the end. Against `hand_types` it says which types came up short.
    pub round_robin: Option<dealer_level::RoundRobinPlan>,
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

    /// The rarest category, which is what the whole levelling's precision rests
    /// on: every other one was seen more often and is better known.
    pub fn rarest(&self) -> Option<&dealer_level::LevelPlan> {
        self.plans
            .iter()
            .min_by(|a, b| a.natural.total_cmp(&b.natural))
    }

    /// How well the rarest category's rate is known, as a relative standard
    /// error — 0.022 for the 2,000 sightings characterizing aims at.
    ///
    /// The number worth reading, and the reason a sighting count is reported at
    /// all. A keep is `mix / natural`, so this error passes straight into the
    /// delivered mix and does not average out over a longer run: dealing more
    /// afterwards converges on the wrong number rather than scattering around
    /// the right one.
    ///
    /// Infinite when the rarest was never seen, which is a levelling that could
    /// not be computed rather than one that is merely thin.
    pub fn precision(&self) -> f64 {
        let Some(rarest) = self.rarest() else {
            return f64::INFINITY;
        };
        if rarest.seen == 0 || self.measured.produced == 0 {
            return f64::INFINITY;
        }
        (rarest.natural * (1.0 - rarest.natural) / self.measured.produced as f64).sqrt()
            / rarest.natural
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

    /// How far a pass has got, offered from inside a batch. It cannot stop
    /// anything.
    ///
    /// [`RunHost::should_stop`] is offered once per batch, and a batch is at
    /// least 1024 deals. That is often enough while a deal costs a few hundred
    /// nanoseconds and useless when it costs twenty-three milliseconds: one
    /// batch of a script that wants a double-dummy table is tens of seconds, so
    /// a page that paints from `should_stop` alone shows nothing at all and
    /// reads as hung rather than busy (#83).
    ///
    /// Reporting-only on purpose. Offering `should_stop` more often would let a
    /// run end where it previously would not — and a characterizing pass that
    /// stops sooner measures less, which changes the keeps a levelled run deals
    /// with. A report has no such power, so where it is offered is free to
    /// follow where the time goes.
    ///
    /// It carries exactly what `should_stop` carries, so a host already
    /// painting a bar has only to paint it from here as well. The numbers mean
    /// what they mean there: for a characterizing pass, sightings of the
    /// scarcest category against the goal; otherwise produced deals against
    /// what was asked for.
    ///
    /// `generated` is the one that differs, and only ever upwards: it counts
    /// the batch in hand as it is dealt and tested, where a pass's own count
    /// only counts the deals it went on to walk. A pass that reaches its
    /// `produce` half way through a batch dealt the whole of it, and a report
    /// that shrank back to the walked count would be a meter running backwards.
    fn progress(&mut self, _phase: Phase, _produced: usize, _generated: usize, _target: usize) {}

    /// A deal the producing pass is handing over.
    fn produced(&mut self, deal: &Produced) -> Result<(), String>;
}

/// What one batch of dealing and testing is: how many deals, how many of them
/// at a time, and how many variables a deal's scratch may have to hold.
///
/// Three numbers rather than three arguments, and the third is the odd one:
/// it is not about the work but about the scratch that does it — zero means no
/// script variable is ever cached, so nothing in a batch ever allocates and
/// rayon is left to split it however it likes. See [`Workers::build_and_test`].
#[derive(Clone, Copy)]
struct Batch {
    count: usize,
    chunk: usize,
    variables: usize,
}

/// The engine's threads, and how it maps work over a batch.
///
/// A pool of its own where one can be had: rayon's global pool can only be
/// configured once per process, so a second run would silently keep the first
/// one's thread count.
///
/// On the web it cannot. Building a pool spawns threads, and on wasm that needs
/// a spawn hook which `wasm-bindgen-rayon` installs for the global pool alone —
/// a private pool simply fails to build. So a build that cannot have its own
/// falls back to the global one rather than to running serially, which is what
/// it did at first, silently, while looking like it had threads.
struct Workers {
    #[cfg(feature = "parallel")]
    pool: Option<rayon::ThreadPool>,
    /// Whether to use rayon's global pool, because a private one was wanted and
    /// could not be built.
    #[cfg(feature = "parallel")]
    global: bool,
}

impl Workers {
    fn new(_threads: usize) -> Self {
        // One thread is this one, so there is nothing to hand off to.
        #[cfg(feature = "parallel")]
        let wanted = _threads > 1;
        #[cfg(feature = "parallel")]
        let pool = wanted.then(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(_threads)
                .build()
                .ok()
        });
        Workers {
            #[cfg(feature = "parallel")]
            pool: pool.flatten(),
            #[cfg(feature = "parallel")]
            global: wanted,
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
    /// Each deal comes back with whatever double-dummy answers testing it
    /// worked out, harvested on the thread that did the work. That is how a
    /// `tricks()` in a condition reaches the action that asks the same
    /// question later on the main thread: the answer travels with its deal
    /// rather than being looked up in a store keyed by cards (#61).
    ///
    /// `test` is handed the scratch its context is built in, one per worker
    /// rather than one per deal. A context that makes its own allocates the
    /// moment a script mentions a variable, and on wasm — one dlmalloc behind
    /// one lock — that allocation is what turned twelve threads into a
    /// slow-down (#86). Emptied here, between the deal that was tested and the
    /// next one, which is what a deal boundary means.
    fn build_and_test<'v>(
        &self,
        start: usize,
        count: usize,
        variables: usize,
        build: &(dyn Fn(usize) -> Deal + Sync),
        test: &(dyn Fn(usize, &Deal, &VarCache<'v>) -> Result<bool, EvalError> + Sync),
        harvest: bool,
    ) -> Vec<Tested> {
        let one = |cache: &mut VarCache<'v>, offset: usize| {
            let index = start + offset;
            let deal = build(index);
            let passed = test(index, &deal, cache.start_deal());
            let known = if harvest {
                dealer_dds::learned(&deal)
            } else {
                dealer_dds::DealTricks::nothing()
            };
            Tested {
                deal,
                passed,
                known,
            }
        };
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            // `map_init` rather than `map`: rayon calls the initialiser once
            // per piece of work it splits off, not once per item, so a piece
            // builds one scratch and every deal in it re-uses that.
            //
            // With `with_min_len` to say how small a piece may get, and that is
            // not a detail. Rayon splits adaptively: every steal lets it halve
            // again, and deals are cheap enough that twelve workers steal
            // constantly — measured on wasm, an unbounded split cut a batch of
            // 2,400 into pieces of one or two, so a scratch was built about as
            // often as a context used to be and the allocation came straight
            // back: a one-variable script ran at 2.5M deals/s on twelve threads
            // with no floor and 8.1M with one. Eight pieces per worker is
            // enough to keep them fed — a batch's deals all cost the same, and
            // the one pass whose deals do not comes through here 32 at a time,
            // where this works out at 1.
            let floor = || {
                if variables == 0 {
                    // Nothing is ever put in the scratch, so it never
                    // allocates, so there is nothing to spread — and rayon's
                    // own splitting balances a batch better than any floor. A
                    // bare condition measured 29% slower on twelve wasm threads
                    // with a floor it had no use for.
                    return 1;
                }
                (count / (rayon::current_num_threads() * 8)).max(1)
            };
            if let Some(pool) = &self.pool {
                return pool.install(|| {
                    (0..count)
                        .into_par_iter()
                        .with_min_len(floor())
                        .map_init(VarCache::new, one)
                        .collect()
                });
            }
            if self.global {
                return (0..count)
                    .into_par_iter()
                    .with_min_len(floor())
                    .map_init(VarCache::new, one)
                    .collect();
            }
        }
        let mut cache = VarCache::new();
        (0..count).map(|offset| one(&mut cache, offset)).collect()
    }

    /// The same, offered to a caller in pieces of `chunk`, with `between`
    /// called after each piece that is not the last.
    ///
    /// `chunk >= count` is the ordinary case and does exactly what
    /// [`Workers::build_and_test`] does: one map, one vector, straight out of
    /// the pool, and the loop below never runs. `between` is never called.
    ///
    /// It lives here rather than in `run_pass` because of what it cost there.
    /// Written as `if solving { chunked } else { whole }` in the pass, both
    /// paths were live in the hottest function in the crate, and a run that
    /// never solved — which is nearly all of them — paid **2.4%** for the one
    /// it did not take. The same code with the chunked arm statically dead ran
    /// **1.9% faster** than the version before any of this, which is the size
    /// of what inlining and code layout were doing. One call site, always
    /// taken, is the fix; the branch that chooses is a `chunk` value rather
    /// than two paths through the caller.
    ///
    /// It cannot change what comes out: the work is a map from index to deal,
    /// collected in the order the indices were drawn, so where the pieces are
    /// cut changes when the threads are handed their work and nothing else.
    fn build_and_test_in_chunks<'v>(
        &self,
        batch: Batch,
        build: &(dyn Fn(usize) -> Deal + Sync),
        test: &(dyn Fn(usize, &Deal, &VarCache<'v>) -> Result<bool, EvalError> + Sync),
        harvest: bool,
        between: &mut dyn FnMut(usize),
    ) -> Vec<Tested> {
        let Batch {
            count,
            chunk,
            variables,
        } = batch;
        let mut built = self.build_and_test(0, chunk.min(count), variables, build, test, harvest);
        while built.len() < count {
            between(built.len());
            let from = built.len();
            built.extend(self.build_and_test(
                from,
                chunk.min(count - from),
                variables,
                build,
                test,
                harvest,
            ));
        }
        built
    }

    /// Run `warm` over every deal, on the pool.
    ///
    /// Used to solve a batch's produced deals before the main thread evaluates
    /// their action, so the evaluation finds every cell already worked out. See
    /// `crate::dd_demand`.
    ///
    /// Returns what each warm worked out, in the order the deals were given,
    /// so the caller can put it back with its deal. Nothing is left behind on
    /// the worker for the main thread to find, so warming whose results nobody
    /// collected would be work thrown away.
    fn warm_each(
        &self,
        deals: &[&Deal],
        warm: &(dyn Fn(&Deal) + Sync),
    ) -> Vec<dealer_dds::DealTricks> {
        let one = |deal: &&Deal| {
            warm(deal);
            dealer_dds::learned(deal)
        };
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            if let Some(pool) = &self.pool {
                return pool.install(|| deals.par_iter().map(one).collect());
            }
            if self.global {
                return deals.par_iter().map(one).collect();
            }
        }
        deals.iter().map(one).collect()
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
    /// Scratch for the contexts this deal's reports are evaluated in.
    ///
    /// One per produced deal rather than one per context: a produced deal is
    /// rare next to a generated one — a characterizing pass emits none at all —
    /// so this is not the allocation #86 is about, and holding it here means
    /// `printes` and `printrpt` over the same deal work a variable out once
    /// between them.
    cache: dealer_eval::VarCache<'a>,
    point_counts: Option<&'a dealer_eval::PointCounts>,
    reports: &'a Reports,
    vulnerability: dealer_core::Vulnerability,
    /// What is known about this deal's double-dummy results by the time it is
    /// produced: what it arrived with, plus everything the run worked out.
    dd_tricks: &'a dealer_dds::DealTricks,
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

impl<'a> Produced<'a> {
    /// This deal's complete double-dummy table, if it has one.
    ///
    /// What it arrived with, plus whatever the run worked out — the same
    /// `DealTricks` the script's own `tricks()` reads. `None` unless all
    /// twenty cells are known, and **nothing is solved to answer this**: an
    /// exporter asks "do we know already?", and a deal nobody asked about is
    /// not worth twenty searches to annotate.
    pub fn dd_table(&self) -> Option<dealer_dds::bridge_solver::DdTable> {
        self.dd_tricks.table()
    }

    /// A fresh context over this deal, for a caller's own per-deal work.
    ///
    /// Fresh rather than shared for the reason contexts are built where they
    /// are everywhere else: where their boundaries fall is what a `rnd()` in
    /// one draws against a `rnd()` in another. That is the `rnd()` stream,
    /// which is still one per context. The variable values are this deal's
    /// scratch, shared with every other context over the same deal — which
    /// cannot move a `rnd()`, since a variable that can reach one is never
    /// cached at all.
    pub fn context(&self) -> dealer_eval::EvalContext<'_, 'a> {
        dealer_eval::EvalContext::for_deal(
            self.deal,
            self.variables,
            self.point_counts,
            self.vulnerability,
            self.dd_tricks,
            &self.cache,
        )
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
            // Five columns a seat, in strain order C, D, H, S, NT. Pushed as
            // one part holding its own commas, so it lands in the row as
            // separate columns without the join needing to know.
            CsvTerm::Trix(seats) => {
                let known = ctx.dd_tricks();
                if known.table().is_none() {
                    // Solved denomination-outermost, for the cache sharing
                    // described on `dealer_dds::solve_table`, then read back per
                    // seat in the order the report wants. A deal that arrived
                    // with its table needs no warming: every read below is a
                    // lookup.
                    for denomination in dealer_dds::Denomination::ALL {
                        for seat in seats {
                            dealer_dds::tricks(known, deal, denomination, *seat);
                        }
                    }
                }
                for seat in seats {
                    let columns: Vec<String> = dealer_dds::Denomination::ALL
                        .iter()
                        .map(|denomination| {
                            dealer_dds::tricks(known, deal, *denomination, *seat).to_string()
                        })
                        .collect();
                    parts.push(columns.join(","));
                }
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

    /// What this handle's deal arrived knowing, if anything.
    ///
    /// Only a supplied deal can arrive knowing something: a shuffled deal has
    /// never been solved by anybody.
    /// Whether any supplied deal arrived already solved.
    ///
    /// Asked once a run rather than once a deal: it decides whether there is
    /// anything to carry at all.
    fn carries_tables(&self) -> bool {
        match &self.deals {
            Deals::Given(all) => all.iter().any(|(_, known)| !known.is_empty()),
            Deals::Shuffled { .. } => false,
        }
    }

    fn tricks(&self, handle: Handle) -> &dealer_dds::DealTricks {
        match (&self.deals, handle) {
            (Deals::Given(all), Handle::Given(index)) => &all[index].1,
            _ => &dealer_dds::NOTHING_KNOWN,
        }
    }

    /// The deal a handle stands for.
    fn build(&self, handle: Handle) -> Deal {
        match handle {
            Handle::Shuffled { seed, variant } => match &self.deals {
                Deals::Shuffled { swap, .. } => swap.apply(&self.shuffle(seed), variant as usize),
                Deals::Given(_) => self.shuffle(seed),
            },
            Handle::Given(index) => match &self.deals {
                Deals::Given(all) => all[index].0.clone(),
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
    /// Each handle with what was worked out about its deal, so a second pass
    /// does not search again for what the first pass already knows. Before
    /// #61 this came back from a global store; carrying it is the same saving
    /// without the store.
    handles: Vec<(Handle, dealer_dds::DealTricks)>,
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
    fn offer(&mut self, handle: Handle, known: dealer_dds::DealTricks, position: usize) {
        if self.handles.len() < self.budget {
            self.handles.push((handle, known));
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
    /// The levelling categories and their counts, empty unless the script
    /// declares a decomposition of its own.
    level_types: Vec<(String, usize)>,
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
    replay: &'a [(Handle, dealer_dds::DealTricks)],
    /// How far into the stream `replay` accounts for.
    resume: usize,
    /// Which side is vulnerable, handed to every expression the pass evaluates.
    vulnerability: dealer_core::Vulnerability,
    /// Whether produced deals go to the host. A characterizing pass's deals
    /// exist to be counted and thrown away.
    emit: bool,
    /// A round robin's shape, when the pass is dealing one. `produce` is
    /// unchanged by it, so the pass ends where it always did — when it has
    /// produced what was asked for.
    round_robin: Option<&'a dealer_level::RoundRobinPlan>,
}

/// One deal's answers out of a batch's, or nothing known.
///
/// The vector is empty for a run that can never reach the solver, which is what
/// keeps the carrying off the hot path — so "no entry" and "nothing known" have
/// to be the same answer.
fn known_at(known: &[dealer_dds::DealTricks], index: usize) -> &dealer_dds::DealTricks {
    known.get(index).unwrap_or(&dealer_dds::NOTHING_KNOWN)
}

/// How far a pass has got, and towards what.
///
/// A characterizing pass counts sightings of the scarcest category towards
/// [`dealer_level::MEASURE_GOAL`]; every other pass counts produced deals
/// towards what was asked for. Written once because four call sites now ask —
/// `should_stop`, `pass_finished` and the two [`RunHost::progress`] reports
/// inside a batch — and a bar jumps if any of them disagrees.
fn pass_numbers(
    until_measured: bool,
    accumulator: &RunAccumulator,
    produced: usize,
    produce: usize,
) -> (usize, usize) {
    if until_measured {
        (accumulator.rarest_measured(), dealer_level::MEASURE_GOAL)
    } else {
        (produced, produce)
    }
}

/// The condition, ready to be evaluated over a deal on any worker.
///
/// A function rather than a closure written where it is used, because the
/// scratch's lifetime has to be *named*. `VarCache` is invariant in the
/// lifetime of the names it caches, so a closure parameter written
/// `cache: &VarCache` binds that lifetime higher-ranked along with the
/// reference's — and a cache good for every lifetime is one whose script has to
/// live for `'static`. Naming it here binds it to the script and leaves only
/// the borrow itself per-call, which is what a worker lending its scratch one
/// deal at a time needs.
fn condition_tester<'v>(
    constraint: Option<&'v dealer_parser::Expr>,
    variables: &'v dealer_eval::Variables<'v>,
    point_counts: Option<&'v dealer_eval::PointCounts>,
    vulnerability: dealer_core::Vulnerability,
) -> impl Fn(&dealer_dds::DealTricks, &Deal, &VarCache<'v>) -> Result<bool, EvalError> + Sync {
    move |known: &dealer_dds::DealTricks, deal: &Deal, cache: &VarCache<'v>| match constraint {
        Some(expr) => {
            let ctx = dealer_eval::EvalContext::for_deal(
                deal,
                variables,
                point_counts,
                vulnerability,
                known,
                cache,
            );
            dealer_eval::eval(expr, &ctx).map(|value| value != 0)
        }
        None => Ok(true),
    }
}

/// Deals a solving pass builds, tests or warms between progress reports.
///
/// Chosen against the clock rather than the cache: a deal needing a
/// double-dummy table costs about 23 ms, so this is a couple of hundred
/// milliseconds of work spread over whatever threads there are — often enough
/// to look alive, rare enough that dispatching the chunk is never the expensive
/// part. It is only ever used by a pass that reaches the solver; an ordinary
/// pass takes its batch whole, as it always did.
const SOLVING_CHUNK: usize = 32;

/// Matching deals between progress reports, once a pass is solving.
///
/// The produced loop runs on this thread, and a script whose `average` or whose
/// action asks a double-dummy question the warm could not read off the script
/// solves here, one deal at a time. Counting to eight rather than reading a
/// clock keeps the report cheap: the host reads its own clock and throttles,
/// and is asked eight times less often than there are matching deals.
///
/// *Matching* deals, counted after the condition has had its say. Everything
/// expensive on this thread is downstream of the condition, so a deal it
/// rejected has nothing to report — and counting those instead put a compare
/// and a branch on the one path every single deal takes.
const SOLVING_REPORT_EVERY: usize = 8;

fn run_pass(
    script: &str,
    source: &mut Source,
    host: &mut dyn RunHost,
    opts: PassOptions,
) -> Result<Pass, RunError> {
    let preprocessed = dealer_parser::preprocess_all(script, opts.params)?;
    let program =
        dealer_parser::parse_program(&preprocessed).map_err(|e| format!("Parse error: {}", e))?;
    // Everything about the script that a deal cannot change, checked once here
    // rather than per deal. An argument count is the main one: the evaluator
    // raised it correctly before, but the condition discarded the error and
    // read it as "this deal does not match", so a miscounted call generated the
    // whole run, produced nothing, said nothing and exited 0 (#36).
    dealer_eval::check_program(&program).map_err(|(what, e)| RunError::Eval {
        what,
        message: e.to_string(),
    })?;

    let variables = dealer_eval::extract_variables(&program);
    let constraint = dealer_eval::extract_constraint(&program);
    let point_counts = dealer_eval::extract_point_counts(&program)
        .map_err(|e| format!("Point count error: {}", e))?;
    let point_counts = point_counts.as_ref();
    let mut accumulator =
        RunAccumulator::new(&program, MeasureStop::standard(), opts.vulnerability)?;
    // What the action will ask the solver for, per produced deal. Worked out
    // once: it is a property of the script, not of a deal.
    let dd_demand = crate::dd_demand::of_program(&program);
    // Whether any answer worth carrying can exist at all: the script reaches
    // the solver somewhere, or the deals arrived from a file that had already
    // been solved.
    let asks_solver = crate::dd_demand::touches_solver(&program);
    let arrives_solved = source.carries_tables();
    let dd_in_play = asks_solver || arrives_solved;
    // Whether this pass can actually be *slow*: it asks the solver something
    // and the deals did not arrive with the answers already in them. That is
    // what the progress reports below are gated on, and the gate is the whole
    // of how they are kept off the hot path — a plain deal is a couple of
    // hundred nanoseconds, so a run that never solves must pay one boolean per
    // batch and one predictable branch per deal, and no clock read anywhere.
    //
    // A file that carried tables for only some of its deals falls on the fast
    // side and reports once a batch, as before. That is the old behaviour for a
    // case that is not slow enough to need better.
    let solving = asks_solver && !arrives_solved;
    if let Some(plan) = opts.round_robin {
        accumulator = accumulator.with_round_robin(plan.clone());
    }

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

    // What is left after `check_program` is genuinely per-deal — a strain
    // computed at run time that lands outside 0 to 4, say. That is carried back
    // rather than discarded, and stops the run where it happens.
    // The context is built here rather than through
    // `dealer_eval::eval_with_context_and_counts` so that the run's
    // vulnerability reaches it. `par()` in a condition needs it, and the
    // convenience helper cannot supply one.
    // `index` is into the batch's handles, which is how a deal's table is found:
    // a supplied deal may have arrived already solved, and the condition should
    // read that rather than search for what the file already knew.
    // `cache` is the worker's scratch, emptied for this deal by
    // `Workers::build_and_test` — see [`dealer_eval::VarCache`] for why holding
    // one per worker rather than building one per context is the whole point.
    let test = condition_tester(constraint, &variables, point_counts, opts.vulnerability);

    let workers = Workers::new(opts.threads);
    // This thread's scratch for the contexts a produced deal is classified and
    // counted in. One for the pass, not one per deal: `observe` empties it for
    // each deal it is given, which is the only way to get at it. See
    // [`dealer_eval::VarCache`].
    let mut record_cache = VarCache::new();
    let mut retained = Retained::new(opts.retain);
    let mut produced = 0usize;
    let mut generated = 0usize;
    let mut replayed = 0usize;
    // Deals that passed the condition, across the pass. Only ever read to space
    // out progress reports, and only ever touched by a pass that solves.
    let mut matched_seen = 0usize;
    let mut resumed = opts.replay.is_empty();
    let batch_size = opts.batch;

    'passing: while produced < opts.produce {
        // Whatever an earlier pass kept, before anything new. A replayed deal
        // brings back what that pass worked out about it, so a levelled run
        // does not solve the same deal twice.
        let (handles, carried, from_stream) = if replayed < opts.replay.len() {
            let take = batch_size.min(opts.replay.len() - replayed);
            let slice = &opts.replay[replayed..replayed + take];
            replayed += take;
            let handles: Vec<Handle> = slice.iter().map(|(handle, _)| *handle).collect();
            let carried: Vec<dealer_dds::DealTricks> = if dd_in_play {
                slice.iter().map(|(_, known)| *known).collect()
            } else {
                Vec::new()
            };
            (handles, carried, false)
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
            // Kept beside the handles rather than paired with them: a handle is
            // sixteen bytes and this is forty, and a run that can never use it
            // should not pay to carry it past every deal it deals.
            let carried: Vec<dealer_dds::DealTricks> = if dd_in_play {
                handles
                    .iter()
                    .map(|handle| *source.tricks(*handle))
                    .collect()
            } else {
                Vec::new()
            };
            (handles, carried, true)
        };

        // The expensive half, on whatever threads there are: making each deal
        // and asking the condition about it. Which of the two costs more is the
        // script's business — a shuffle is about a microsecond, a `tricks()`
        // condition is ten milliseconds — so they travel together.
        //
        // Taken whole unless the pass is solving, and then a chunk at a time so
        // that a caller answering to a clock hears from the run inside a batch
        // rather than only at the end of one. Splitting the map cannot change
        // what comes out — it maps indices to deals and collects them in
        // order — only when the threads are handed their work.
        //
        // Two paths, and the ordinary one is the single call it always was: one
        // map, one vector, straight out of the worker pool with nothing copied
        // and nothing appended. Reporting through a batch belongs to the pass
        // that needs it, so it lives in `build_in_chunks` and this function is
        // no bigger on the path almost every run takes.
        // One call, always taken. Which of the two things it does is a `chunk`
        // value rather than two paths through this function — see
        // `Workers::build_and_test_in_chunks`, and the 2.4% that writing it the
        // other way cost a run that never solves.
        //
        // Nothing is observed while a batch is being built, so what the pass
        // has produced and what it is aiming at cannot move: worked out once,
        // here, rather than per report.
        let chunk = if solving {
            SOLVING_CHUNK
        } else {
            handles.len()
        };
        let (done, target) =
            pass_numbers(opts.until_measured, &accumulator, produced, opts.produce);
        // Copies, and they have to be. A closure captures what it reads by
        // reference, so reading `generated` from inside one takes its address —
        // and `generated` is incremented once per deal in the loop below, so
        // that alone moves the pass's hottest counter out of a register and
        // onto the stack for the whole function. Worth 1.7% of a script with a
        // substantial condition, and invisible in the source until you look for
        // it. These are read once each and never written, so the counter itself
        // stays where it belongs.
        let batch_generated = generated;
        let batch_from_stream = from_stream;
        let phase = opts.phase;
        let built = workers.build_and_test_in_chunks(
            Batch {
                count: handles.len(),
                chunk,
                variables: variables.len(),
            },
            &|index| source.build(handles[index]),
            &|index, deal, cache| test(known_at(&carried, index), deal, cache),
            dd_in_play,
            &mut |built_so_far| {
                let so_far = if batch_from_stream {
                    batch_generated + built_so_far
                } else {
                    batch_generated
                };
                host.progress(phase, done, so_far, target);
            },
        );

        // Deals this pass has dealt and tested, counting the batch in hand.
        // Held rather than read off `generated`, which is incremented as the
        // loop below walks the batch: a report that said "sixty-four dealt"
        // while the batch was being solved and "one dealt" a moment later, as
        // the walk began, would be a meter running backwards.
        let dealt = if from_stream {
            generated + built.len()
        } else {
            generated
        };

        // A supplied deal's own answers, plus whatever testing it worked out.
        // From here on this is the deal's double-dummy knowledge, and it is the
        // only place a `tricks()`, `dds()` or `par()` in this batch looks.
        //
        // Left empty for a run that cannot reach the solver, which is most of
        // them. Carrying costs forty bytes and a twenty-cell merge a deal, and
        // a deal costs a couple of hundred nanoseconds, so doing it for a
        // script that never mentions double-dummy was measurable — near 7% of
        // plain generation.
        let mut known: Vec<dealer_dds::DealTricks> = if dd_in_play {
            built
                .iter()
                .enumerate()
                .map(|(index, tested)| {
                    let mut known = *known_at(&carried, index);
                    known.merge(&tested.known);
                    known
                })
                .collect()
        } else {
            Vec::new()
        };

        // Where the last of these sits in the stream, so a kept deal's position
        // is known without threading one through every deal.
        let batch_end = source.position;

        // Solve this batch's matched deals on the pool, before the main thread
        // starts evaluating their actions one at a time. The workers are idle
        // at this point, so the loop below finds every cell already worked out.
        // Nothing else changes: the deals are still observed in order, and no
        // expression is evaluated here.
        //
        // A deal that already knows what the script will ask is left out. Its
        // answers are in hand — from a file that carried them, or from its own
        // condition — and warming it would search for something nobody would
        // read. That exclusion is invisible in the output, since the solver
        // agrees with what is already known, so it shows up only as time.
        if !dd_demand.is_none() {
            // Only as many as this pass can still use. A batch is at least
            // 1024 deals and 200 a thread, so a run asking for a hundred builds
            // two thousand four hundred — fine when a deal costs a microsecond,
            // and the difference between two seconds and twenty when each one
            // is a double-dummy search. The deals past the target are never
            // walked, so solving them buys nothing at all.
            //
            // Only a plain producing pass has a target to count against.
            // Characterizing runs until it has seen enough or the clock stops
            // it. And a round robin turns matches away when their round is
            // full, so `produced` is behind the number of matches this batch
            // will walk — capping by it would leave the rest to be solved one
            // at a time on this thread, which is slower than solving too many
            // on the pool.
            let counts_every_match = !opts.until_measured && opts.round_robin.is_none();
            let still_wanted = if counts_every_match {
                opts.produce.saturating_sub(produced)
            } else {
                usize::MAX
            };
            let wanted: Vec<usize> = built
                .iter()
                .enumerate()
                .filter(|(index, tested)| {
                    matches!(tested.passed, Ok(true)) && !dd_demand.satisfied_by(&known[*index])
                })
                .map(|(index, _)| index)
                .take(still_wanted)
                .collect();
            if wanted.len() > 1 {
                // A chunk at a time while the pass is solving, for the reason
                // the building above is: this is where a whole batch's
                // seventeen seconds go, and a bar that only moves afterwards
                // has not moved at all.
                let step = if solving { SOLVING_CHUNK } else { wanted.len() };
                for chunk in wanted.chunks(step.max(1)) {
                    let deals: Vec<&Deal> = chunk.iter().map(|index| &built[*index].deal).collect();
                    let warmed = workers.warm_each(&deals, &|deal| dd_demand.warm(deal));
                    for (index, warmed) in chunk.iter().zip(warmed) {
                        known[*index].merge(&warmed);
                    }
                    if solving {
                        let (done, target) =
                            pass_numbers(opts.until_measured, &accumulator, produced, opts.produce);
                        host.progress(opts.phase, done, dealt, target);
                    }
                }
            }
        }

        for (index, tested) in built.iter().enumerate() {
            let deal = &tested.deal;
            if from_stream {
                generated += 1;
            }
            let matched = tested.passed.as_ref().map_err(|e| RunError::Eval {
                what: "condition".to_string(),
                message: e.to_string(),
            })?;
            if !matched {
                continue;
            }
            let observed = accumulator.observe(
                deal,
                &variables,
                point_counts,
                known_at(&known, index),
                &mut record_cache,
            )?;
            // Where the main thread does its own solving: an `average` over
            // `tricks()`, or an action asking for a cell the warm could not
            // read off the script, is searched right here, one deal at a time.
            //
            // Below the early-out above, and counting deals that got this far
            // rather than the batch index, because everything expensive on this
            // thread is downstream of the condition. A deal the condition threw
            // away costs nothing to report on — and testing for it up there put
            // a compare and a branch on the one path every deal takes, and kept
            // `index` live where it had been dead. That was worth 1.4% of a
            // run whose condition matches nothing, which is the shape that
            // shows it.
            if solving {
                matched_seen += 1;
                if matched_seen.is_multiple_of(SOLVING_REPORT_EVERY) {
                    let (done, target) =
                        pass_numbers(opts.until_measured, &accumulator, produced, opts.produce);
                    host.progress(opts.phase, done, dealt, target);
                }
            }
            if from_stream {
                retained.offer(
                    handles[index],
                    *known_at(&known, index),
                    batch_end - (built.len() - 1 - index),
                );
            }
            // A deal whose hand type has had its share of the round. It cost
            // exactly what it would have cost anyway — the rarity is in the
            // dealing, not in the taking — and there is nothing to be saved by
            // noticing sooner.
            if !observed.taken {
                continue;
            }
            let hand_type = observed.matched.hand_type;
            if opts.emit {
                host.produced(&Produced {
                    deal,
                    hand_type,
                    variables: &variables,
                    cache: dealer_eval::VarCache::new(),
                    point_counts,
                    reports: &reports,
                    vulnerability: opts.vulnerability,
                    dd_tricks: known_at(&known, index),
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
        let (done, target) =
            pass_numbers(opts.until_measured, &accumulator, produced, opts.produce);
        if host.should_stop(opts.phase, done, generated, target) {
            break;
        }
    }
    let (done, target) = pass_numbers(opts.until_measured, &accumulator, produced, opts.produce);
    host.pass_finished(opts.phase, done, generated, target);

    let measurement = accumulator.measurement(generated);
    let hand_types: Vec<(String, usize)> = accumulator
        .hand_type_labels()
        .iter()
        .cloned()
        .zip(accumulator.hand_type_counts().iter().copied())
        .collect();
    // Empty when the levelling categories are the hand types: the same counts
    // under the same names, and a caller would only print them twice.
    let level_types: Vec<(String, usize)> = if accumulator.levels_on_level_types() {
        accumulator
            .leveling_labels()
            .iter()
            .cloned()
            .zip(accumulator.leveling_counts().iter().copied())
            .collect()
    } else {
        Vec::new()
    };
    let joint = measurement.joint.clone();
    Ok(Pass {
        hit_limit: produced < opts.produce && generated >= opts.max_generate,
        produced,
        generated,
        measurement,
        hand_types,
        level_types,
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
/// Does this script's condition reach the solver, so far as can be told before
/// the run proper parses it?
///
/// Only the batch size depends on this, and only as a choice between two
/// reasonable sizes — so a script that will not parse answers `false` and takes
/// the ordinary batch. The parse that matters happens below and reports the
/// error properly; failing here would report it twice, in the wrong words.
fn condition_solves(script: &str, params: &dealer_parser::ScriptParams) -> bool {
    dealer_parser::preprocess_all(script, params)
        .ok()
        .and_then(|text| dealer_parser::parse_program(&text).ok())
        .is_some_and(|program| crate::dd_demand::condition_touches_solver(&program))
}

pub fn run(script: &str, opts: RunOptions, host: &mut dyn RunHost) -> Result<RunReport, RunError> {
    let threads = resolve_threads(opts.threads);
    // Enough work per hand-off that the hand-off is not the expensive part, and
    // little enough that a caller answering to a clock is asked often.
    //
    // Both halves of that assume a deal is cheap. When the *condition* solves,
    // one is about five orders of magnitude dearer — hundreds of nanoseconds
    // becomes tens of milliseconds — and a batch sized for the cheap case
    // solves two thousand four hundred deals to answer `produce 5`. The
    // handles are drawn before any of them is tested, so the surplus cannot be
    // abandoned once enough have matched: that would skip deals the next batch
    // should have seen, and change which deals a seed produces.
    //
    // So the batch is drawn small instead. At tens of milliseconds a deal the
    // hand-off is free by comparison, and a selective filter simply takes more
    // batches — each one still spread across every thread.
    let batch = if opts.batch == 0 {
        if condition_solves(script, &opts.params) {
            (2 * threads).clamp(16, 256)
        } else {
            (200 * threads).clamp(1024, 65_536)
        }
    } else {
        opts.batch
    };

    // The shape of a round, if one was asked for. Worked out once, from the
    // scenario as written: a levelling adds `roll` and the keeps to a script
    // and leaves its `HandType_` declarations alone, so the rounds are the same
    // either way.
    //
    // It shapes the *producing* pass and only that. A characterizing pass is
    // measuring how often each type comes up, and a pass that stopped taking a
    // type once it had enough would measure its own filter.
    let round_robin = if opts.round_robin {
        Some(round_robin_for(script, opts.produce, &opts.params)?)
    } else {
        None
    };

    let Some(leveling) = opts.leveling else {
        // An ordinary run: one pass, the script as written, every match handed
        // over as it comes — or, dealing a round robin, every match whose hand
        // type still has room in the round.
        let produce = opts.produce;
        let mut source = Source::new(opts.deals, opts.seed);
        let pass = run_pass(
            script,
            &mut source,
            host,
            PassOptions {
                vulnerability: opts.vulnerability,
                phase: Phase::Dealing,
                params: &opts.params,
                threads,
                batch,
                produce,
                max_generate: opts.max_generate,
                until_measured: false,
                retain: 0,
                replay: &[],
                resume: 0,
                emit: true,
                round_robin: round_robin.as_ref(),
            },
        )?;
        return Ok(RunReport {
            produced: pass.produced,
            generated: pass.generated,
            hit_limit: pass.hit_limit,
            hand_types: pass.hand_types,
            level_types: pass.level_types,
            round_robin,
            stats: pass.stats,
            leveling: None,
        });
    };

    // The scenario with a levelling block in it: the same text the keeps will
    // be written into, so the two cannot describe different scenarios.
    let prepared = dealer_level::insert_leveling_block(script)?;
    dealer_level::check_leveling_source(&prepared)?;

    let mut source = Source::new(opts.deals, opts.seed);
    // Two limits, two passes, and which one pays for the measuring is the
    // caller's to say. `Shared` is the command line's arrangement — one budget
    // across the run — and `Own` hands `max_generate` to the producing pass
    // whole, so a caller bounding it is bounding the run it asked for and
    // nothing else.
    let measure_generate = match leveling.measure_deals {
        MeasureDeals::Shared => opts.max_generate,
        MeasureDeals::Own(deals) => deals,
    };
    let characterizing = run_pass(
        &prepared,
        &mut source,
        host,
        PassOptions {
            vulnerability: opts.vulnerability,
            phase: Phase::Characterizing,
            params: &opts.params,
            threads,
            batch,
            produce: leveling.measure_cap,
            max_generate: measure_generate,
            until_measured: true,
            // Everything it matches, up to what a run could conceivably want.
            // Not a knob a caller should have to think about: too low only
            // costs the producing pass some dealing.
            retain: RETAIN_DEALS,
            replay: &[],
            resume: 0,
            emit: false,
            round_robin: None,
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
                vulnerability: opts.vulnerability,
                phase: Phase::AdditionalDealing,
                params: &opts.params,
                threads,
                batch,
                produce: opts.produce,
                // Under `Shared`, every pass deals from one budget, so what
                // characterizing spent is gone. Under `Own` it had a budget of
                // its own and this one is untouched.
                max_generate: match leveling.measure_deals {
                    MeasureDeals::Shared => {
                        opts.max_generate.saturating_sub(characterizing.generated)
                    }
                    MeasureDeals::Own(_) => opts.max_generate,
                },
                until_measured: false,
                retain: 0,
                replay: &characterizing.retained.handles,
                resume: characterizing.retained.through,
                emit: true,
                round_robin: round_robin.as_ref(),
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
        level_types: producing
            .as_ref()
            .map(|p| p.level_types.clone())
            .unwrap_or_else(|| characterizing.level_types.clone()),
        round_robin,
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

/// How `produce` divides among the script's hand types.
///
/// Parsed here rather than in the pass so a scenario that cannot be dealt round
/// robin — one naming no hand types, or weighting them — is refused before a
/// card is dealt rather than after the run.
fn round_robin_for(
    script: &str,
    produce: usize,
    params: &dealer_parser::ScriptParams,
) -> Result<dealer_level::RoundRobinPlan, RunError> {
    let program = dealer_parser::parse_program(&dealer_parser::preprocess_all(script, params)?)
        .map_err(|e| format!("Parse error: {}", e))?;
    Ok(dealer_level::round_robin_plan(&program, produce)?)
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
            vulnerability: dealer_core::Vulnerability::None,
            round_robin: false,
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
                measure_deals: MeasureDeals::Shared,
            }),
        }
    }

    fn round_robin(script: &str, produce: usize, max_generate: usize) -> (Collector, RunReport) {
        let mut opts = options(produce, false);
        opts.round_robin = true;
        opts.max_generate = max_generate;
        let mut host = Collector::default();
        let report = super::run(script, opts, &mut host).expect("run");
        (host, report)
    }

    /// Where levelling is exact on average, a round robin is exact.
    #[test]
    fn produce_divides_evenly_among_the_hand_types() {
        let (host, report) = round_robin(LADDER, 36, 5_000_000);
        assert_eq!(report.produced, 36);
        assert_eq!(host.deals.len(), 36);
        for (name, count) in &report.hand_types {
            assert_eq!(*count, 12, "{name} came out at {count}");
        }
        assert!(!report.hit_limit);
    }

    /// The remainder goes to different types, one apiece. Two more deals over
    /// three types is two types with one extra, never one type with two.
    #[test]
    fn a_remainder_never_repeats_a_hand_type() {
        let (_, report) = round_robin(LADDER, 38, 5_000_000);
        assert_eq!(report.produced, 38);
        let counts: Vec<usize> = report.hand_types.iter().map(|(_, n)| *n).collect();
        assert_eq!(counts.iter().sum::<usize>(), 38);
        for count in &counts {
            assert!(
                (12..=13).contains(count),
                "a partial round repeated a type: {counts:?}"
            );
        }
        assert_eq!(counts.iter().filter(|n| **n == 13).count(), 2);
    }

    /// Fewer deals than types: all remainder, so every deal is a different
    /// type and none is dealt twice.
    #[test]
    fn fewer_deals_than_types_gives_one_each_of_some() {
        let (_, report) = round_robin(LADDER, 2, 5_000_000);
        assert_eq!(report.produced, 2);
        let counts: Vec<usize> = report.hand_types.iter().map(|(_, n)| *n).collect();
        assert_eq!(counts.iter().filter(|n| **n == 1).count(), 2);
        assert_eq!(counts.iter().filter(|n| **n == 0).count(), 1);
    }

    /// Run `script` levelled, saying where the characterizing pass's deals come
    /// from.
    fn leveled(
        script: &str,
        produce: usize,
        max_generate: usize,
        measure_deals: MeasureDeals,
    ) -> RunReport {
        let mut opts = options(produce, true);
        opts.max_generate = max_generate;
        if let Some(leveling) = opts.leveling.as_mut() {
            leveling.measure_deals = measure_deals;
        }
        let mut host = Collector::default();
        super::run(script, opts, &mut host).expect("run")
    }

    /// `SKEWED`'s rarest type is about one deal in three hundred, so measuring
    /// it well needs several hundred thousand deals — far more than this run is
    /// allowed. Shared, the run's budget stops the pass; the levelling is then
    /// computed from whatever it managed.
    #[test]
    fn a_shared_budget_lets_the_deal_limit_stop_characterizing() {
        let report = leveled(SKEWED, 5, 60_000, MeasureDeals::Shared);
        let leveling = report.leveling.expect("levelled");
        assert!(
            leveling.characterized <= 60_000,
            "characterizing dealt {} of a 60,000 budget it was supposed to share",
            leveling.characterized,
        );
        assert!(
            leveling.characterized + leveling.additional <= 60_000,
            "the run dealt {} against a 60,000 budget",
            leveling.characterized + leveling.additional,
        );
    }

    /// With an allowance of its own the same pass runs past that limit, because
    /// the limit was never about it. This is what lets a front end bound the run
    /// someone asked for without also bounding the measuring that pays for it —
    /// the browser, where `Max generate` was doing both jobs and cutting the
    /// measurement short.
    #[test]
    fn its_own_budget_lets_characterizing_run_past_the_deal_limit() {
        let report = leveled(SKEWED, 5, 60_000, MeasureDeals::Own(2_000_000));
        let leveling = report.leveling.expect("levelled");
        assert!(
            leveling.characterized > 60_000,
            "characterizing stopped at {}, so something still bounds it by the run's budget",
            leveling.characterized,
        );
        assert_eq!(report.produced, 5);
    }

    /// The same bands with the middle one asked for three times a round.
    const WEIGHTED: &str = "\
HandType_Weak = hcp(north) <= 10
HandType_Middling = hcp(north) >= 11 and hcp(north) <= 15
HandType_Strong = hcp(north) >= 16
HandType_Middling_Share = 3
condition 1
";

    /// A band rare enough that filling it is the whole cost of the run: about
    /// one deal in three hundred, against two types that arrive constantly.
    const SKEWED: &str = "\
HandType_Weak = hcp(north) <= 10
HandType_Middling = hcp(north) >= 11 and hcp(north) <= 21
HandType_Strong = hcp(north) >= 22
condition 1
";

    /// The rarest type binds and nothing else does: the common ones fill early
    /// and everything after is dealt and passed over.
    #[test]
    fn a_round_costs_what_the_rarest_type_costs() {
        let (_, report) = round_robin(SKEWED, 18, 5_000_000);
        assert_eq!(report.produced, 18);
        // Six `Weak` arrive in the first twenty deals or so. Six `Strong` do
        // not, and the round cannot close until they have.
        assert!(
            report.generated > 500,
            "18 deals taken from only {} dealt would mean the rare type was not being waited for",
            report.generated
        );
    }

    /// Not an error. A short set is still a set, and the report says what each
    /// type was owed so a caller can name the ones that fell short.
    #[test]
    fn a_round_that_cannot_be_filled_returns_what_it_managed() {
        let (host, report) = round_robin(SKEWED, 18, 300);
        assert!(report.hit_limit);
        assert_eq!(report.produced, host.deals.len());
        assert!(report.produced < 18, "{} was not short", report.produced);
        let plan = report.round_robin.expect("the round's shape");
        assert_eq!(plan.rounds, 6);
        assert!(
            (0..report.hand_types.len()).any(|i| report.hand_types[i].1 < plan.owed(i)),
            "nothing short in a run that hit its limit"
        );
        // Never over: a type that has had its share stops being taken.
        for (i, (name, got)) in report.hand_types.iter().enumerate() {
            assert!(
                *got <= plan.owed(i) + plan.per_round[i],
                "{name} took {got} against {} a round",
                plan.per_round[i]
            );
        }
    }

    /// A deal that is dealt and passed over must leave no trace. Otherwise the
    /// averages would describe the qualifying population rather than the set
    /// that was actually delivered.
    #[test]
    fn a_deal_passed_over_is_not_in_the_statistics() {
        let script = "\
HandType_Weak = hcp(north) <= 10
HandType_Middling = hcp(north) >= 11 and hcp(north) <= 15
HandType_Strong = hcp(north) >= 16
condition 1
action average \"strong\" 100 * HandType_Strong
";
        let (_, report) = round_robin(script, 36, 5_000_000);
        let average = &report.stats.averages[0];
        // A third of the delivered set is Strong, by construction; the
        // qualifying population is nothing like a third. And the average was
        // taken over the 36 deals delivered, not over every deal that matched.
        assert_eq!(average.count, 36);
        assert!(
            (average.value - 100.0 / 3.0).abs() < 1e-6,
            "the average saw deals the set did not: {}",
            average.value
        );
    }

    /// The point of a share: three of that type in every round, exactly.
    #[test]
    fn a_share_puts_that_many_of_a_type_in_every_round() {
        // A round is 1 + 3 + 1, so 25 deals is five complete rounds.
        let (_, report) = round_robin(WEIGHTED, 25, 5_000_000);
        assert_eq!(report.produced, 25);
        let counts: Vec<usize> = report.hand_types.iter().map(|(_, n)| *n).collect();
        assert_eq!(counts, vec![5, 15, 5]);
    }

    /// With a share in play the partial round is still a round: a type may take
    /// up to its own share again, and no more.
    #[test]
    fn a_weighted_remainder_never_exceeds_a_types_share() {
        let (_, report) = round_robin(WEIGHTED, 27, 5_000_000);
        assert_eq!(report.produced, 27);
        let plan = report.round_robin.expect("the round's shape");
        assert_eq!((plan.rounds, plan.remainder), (5, 2));
        for (i, (name, got)) in report.hand_types.iter().enumerate() {
            assert!(
                (plan.owed(i)..=plan.owed(i) + plan.per_round[i]).contains(got),
                "{name} took {got}, outside its round of {}",
                plan.per_round[i]
            );
        }
    }

    /// Two capabilities on the same declarations, not two answers to one
    /// question: the levelling measures the scenario and writes the copy, the
    /// round decides which of its deals reach the caller. The characterizing
    /// pass must still see the scenario unfiltered, or it would be measuring
    /// its own filter.
    #[test]
    fn a_levelling_and_a_round_robin_do_different_jobs() {
        let mut opts = options(36, true);
        opts.round_robin = true;
        let mut host = Collector::default();
        let report = super::run(LADDER, opts, &mut host).expect("run");

        // The round: exact, where the levelling alone is exact on average.
        assert_eq!(report.produced, 36);
        for (name, count) in &report.hand_types {
            assert_eq!(*count, 12, "{name} came out at {count}");
        }
        // And the levelling still happened, with its own numbers.
        let levelling = report.leveling.expect("the levelling");
        assert_eq!(levelling.plans.len(), 3);
        assert!(levelling
            .script
            .contains("### BEGIN GENERATED LEVELING ###"));
        // Measured naturally: the natural mix is lopsided, which is the whole
        // reason the keeps exist. A characterizing pass that had been dealt
        // round robin would have measured three equal thirds.
        let natural: Vec<usize> = levelling
            .natural_hand_types
            .iter()
            .map(|(_, n)| *n)
            .collect();
        assert!(
            natural.iter().max() > natural.iter().min(),
            "the probe measured its own filter: {natural:?}"
        );
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

    /// A host that listens to everything, and remembers what it heard.
    ///
    /// The same collector underneath, so a test can compare a run that was
    /// listened to against one that was not and be comparing the run rather
    /// than two different front ends.
    #[derive(Default)]
    struct Listener {
        collector: Collector,
        /// Every `progress` report, as (generated, produced).
        reports: Vec<(usize, usize)>,
        /// Where every `should_stop` offer fell, so a test can say a report
        /// arrived somewhere an offer to stop did not.
        stop_offers: Vec<usize>,
    }

    impl RunHost for Listener {
        fn should_stop(&mut self, _: Phase, _: usize, generated: usize, _: usize) -> bool {
            self.stop_offers.push(generated);
            false
        }
        fn progress(&mut self, _: Phase, produced: usize, generated: usize, _: usize) {
            self.reports.push((generated, produced));
        }
        fn pass_finished(&mut self, phase: Phase, a: usize, b: usize, c: usize) {
            self.collector.pass_finished(phase, a, b, c);
        }
        fn produced(&mut self, produced: &Produced) -> Result<(), String> {
            self.collector.produced(produced)
        }
    }

    /// A condition that solves, so every deal the pass tests costs a search.
    const SOLVING_CONDITION: &str = "condition tricks(north, notrump) >= 6\n";

    /// An action that solves, so every deal the pass *produces* costs one.
    const SOLVING_ACTION: &str = "condition 1\naction average \"nt\" tricks(north, notrump)\n";

    /// Small enough to finish in a test, big enough to span more than one chunk.
    fn solving_options(produce: usize) -> RunOptions {
        RunOptions {
            threads: 0,
            batch: SOLVING_CHUNK * 2,
            ..options(produce, false)
        }
    }

    /// The whole point of the hook: a pass that solves says something before
    /// its batch is over. `should_stop` is offered once a batch, and asking it
    /// more often was not the answer because it can end a run (#83).
    #[test]
    fn a_solving_condition_reports_inside_its_batch() {
        let mut host = Listener::default();
        super::run(SOLVING_CONDITION, solving_options(1), &mut host).expect("run");
        // The batch is two chunks, so the first report lands halfway through
        // it — before the batch has been tested, and long before the offer to
        // stop that used to be the only word a caller got.
        assert!(
            host.reports
                .iter()
                .any(|(generated, _)| *generated == SOLVING_CHUNK),
            "nothing reported part-way through the first batch: {:?}",
            host.reports
        );
        assert!(
            host.stop_offers
                .iter()
                .all(|offer| *offer >= SOLVING_CHUNK * 2),
            "an offer to stop arrived inside a batch: {:?}",
            host.stop_offers
        );
    }

    /// The same for the other place the time goes: solving a batch's produced
    /// deals on the pool, which is where a table-shaped action spends it.
    #[test]
    fn a_solving_action_reports_while_it_warms() {
        let batch = SOLVING_CHUNK * 4;
        let mut host = Listener::default();
        let opts = RunOptions {
            batch,
            ..solving_options(batch)
        };
        super::run(SOLVING_ACTION, opts, &mut host).expect("run");
        // Warming happens with the batch dealt and nothing produced yet, so its
        // reports are the ones saying every deal dealt and none produced. Four
        // chunks give four of them, and the produced loop's first report says
        // the same thing, so five. Warming the batch in one go would give that
        // one report and no more — which is what "the bar moves only once the
        // solving is over" looks like from here.
        let while_warming = host
            .reports
            .iter()
            .filter(|(generated, produced)| *generated == batch && *produced == 0)
            .count();
        assert!(
            host.reports
                .windows(2)
                .all(|pair| pair[1].0 >= pair[0].0 && pair[1].1 >= pair[0].1),
            "a report went backwards: {:?}",
            host.reports
        );
        assert!(
            while_warming >= 3,
            "the batch's solving went unreported until it was over: {:?}",
            host.reports
        );
    }

    /// A run that never reaches the solver is not reported on inside a batch at
    /// all — that gate is how this stays off the hot path. It still gets the
    /// one offer a batch it always had.
    #[test]
    fn a_plain_run_is_not_reported_on_inside_its_batch() {
        let mut host = Listener::default();
        super::run(LADDER, solving_options(40), &mut host).expect("run");
        assert!(
            host.reports.is_empty(),
            "a run that never solves paid for reports it could not need: {:?}",
            host.reports
        );
        assert_eq!(host.collector.deals.len(), 40, "and it still dealt");
    }

    /// Listening changes nothing: same deals, same counts, same statistics.
    /// That is the whole claim of a hook that cannot stop anything.
    #[test]
    fn listening_changes_nothing_about_what_a_run_produces() {
        for script in [LADDER, SOLVING_CONDITION, SOLVING_ACTION] {
            let mut deaf = Collector::default();
            let quiet = super::run(script, solving_options(20), &mut deaf).expect("run");
            let mut keen = Listener::default();
            let loud = super::run(script, solving_options(20), &mut keen).expect("run");
            assert_eq!(quiet.produced, loud.produced, "produced differed");
            assert_eq!(quiet.generated, loud.generated, "generated differed");
            assert_eq!(quiet.hand_types, loud.hand_types, "hand types differed");
            assert_eq!(
                format!("{:?}", quiet.stats),
                format!("{:?}", loud.stats),
                "statistics differed"
            );
            assert_eq!(
                deaf.deals.len(),
                keen.collector.deals.len(),
                "a different number of deals came out"
            );
            assert!(
                deaf.deals
                    .iter()
                    .zip(&keen.collector.deals)
                    .all(|(a, b)| a == b),
                "the deals or their order differed"
            );
            assert_eq!(deaf.phases, keen.collector.phases, "the passes differed");
        }
    }

    /// And for a levelled run, where a stop in the wrong place would change no
    /// deal but would change what the keeps were measured over.
    #[test]
    fn listening_changes_nothing_about_a_levelled_run() {
        let mut deaf = Collector::default();
        let quiet = super::run(LADDER, options(60, true), &mut deaf).expect("run");
        let mut keen = Listener::default();
        let loud = super::run(LADDER, options(60, true), &mut keen).expect("run");
        let (a, b) = (
            quiet.leveling.expect("levelled"),
            loud.leveling.expect("levelled"),
        );
        assert_eq!(a.script, b.script, "the levelled scenario differed");
        assert_eq!(
            a.measured.counts, b.measured.counts,
            "the measurement differed"
        );
        assert_eq!(a.characterized, b.characterized, "characterizing differed");
        assert!(deaf
            .deals
            .iter()
            .zip(&keen.collector.deals)
            .all(|(x, y)| x == y));
    }
}
