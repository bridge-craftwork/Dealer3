//! WebAssembly bindings for dealer3.
//!
//! Exposes the engine to a browser: parse a script, generate deals, filter them.
//! The CLI's browser-hostile parts — file I/O, stdin, `process::exit`,
//! `SystemTime::now()`, rayon — stay in the `dealer` binary and are not reachable
//! from here.
//!
//! # Threading
//!
//! Single-threaded. Shared memory in wasm needs `SharedArrayBuffer`, which needs
//! COOP/COEP headers. Deal generation is stateless per seed, so a threaded build
//! would produce identical output, just faster — see `docs/WASM.md`.
//!
//! # Determinism
//!
//! Output is byte-identical to the native binary for the same seed and script,
//! so the Tier 2 regression hashes pin this build too.

mod library;

use dealer_core::{Deal, FastDealConfig, Position};
use dealer_parser::vocabulary;
use dealer_parser::{Statement, VulnerabilityType};
use dealer_pbn::{format_oneline, format_printall, format_printpbn, PbnBoard, Vulnerability};
use dealer_run::{Deals, LevelingOptions, Phase, Produced, RunHost, RunOptions};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// Upper bound on deals returned to the caller. A script may ask for tens of
/// thousands of deals to build a histogram; serialising them all would blow up
/// the JSON for no benefit, since a page cannot show them either. Statistics are
/// still accumulated over every matching deal.
const MAX_RETURNED_DEALS: usize = 500;

#[wasm_bindgen(start)]
pub fn start() {
    // Turn Rust panics into readable console errors rather than "unreachable".
    console_error_panic_hook::set_once();
}

/// How deals are rendered back to the caller.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    OneLine,
    PrintAll,
    /// Full PBN, suitable for saving and opening elsewhere.
    Pbn,
    /// No deals at all: the statistics, and nothing else.
    ///
    /// A run gathering numbers — the HCP-against-tricks cross-tabulation is the
    /// case that asked for this — never looks at a hand. Every other format
    /// holds deals as they are produced and renders each to a string, which is
    /// then serialised, posted across from the worker, parsed again and laid
    /// out as bridge diagrams nobody reads. This one holds none of them.
    ///
    /// The command line spells it `-f none`, and means the same thing. That is
    /// a new *value* for `-f` rather than a remapped switch — `-f` is dealer3's
    /// own, since the original picks a format with an `action` statement — so
    /// the compatibility rule is untouched and the two front ends agree.
    None,
}

impl Format {
    fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "oneline" | "printoneline" => Ok(Format::OneLine),
            "printall" | "all" => Ok(Format::PrintAll),
            "pbn" | "printpbn" => Ok(Format::Pbn),
            "none" => Ok(Format::None),
            other => Err(format!(
                "Unknown format '{}'. Use 'oneline', 'printall', 'pbn' or 'none'.",
                other
            )),
        }
    }

    /// Whether a produced deal is worth holding on to at all.
    ///
    /// The one thing that separates `None` from the rest, and it is asked
    /// before a deal is cloned rather than after it has been rendered: holding
    /// the deal is most of the cost and rendering it is the remainder.
    fn collects_deals(self) -> bool {
        self != Format::None
    }

    fn render(
        self,
        deal: &dealer_core::Deal,
        index: usize,
        ctx: &OutputContext,
        hand_type: Option<&str>,
    ) -> String {
        match self {
            Format::OneLine => format_oneline(deal).trim_end().to_string(),
            Format::PrintAll => format_printall(deal, index),
            // Board numbers, dealer, vulnerability and the hand type all belong
            // in the PBN tags; a file without them is far less useful to
            // whatever opens it, and a set saved from the page should say the
            // same things as one saved from the command line.
            Format::Pbn => format_printpbn(
                deal,
                &PbnBoard {
                    board_number: index,
                    dealer: ctx.dealer,
                    vulnerability: ctx.vulnerability,
                    seed: Some(ctx.seed),
                    hand_type,
                    ..Default::default()
                },
            ),
            // Never reached: nothing is held under `None`, so there is nothing
            // to render. Empty rather than a panic, so a mistake here would
            // cost a missing deal rather than a dead page.
            Format::None => String::new(),
        }
    }
}

/// One `average "label" expr` result.
#[derive(Serialize)]
struct AverageResult {
    /// True when the expression is about a `HandType_*` variable, so the page
    /// can show it in the hand-type table rather than twice.
    is_hand_type: bool,
    label: Option<String>,
    /// Mean over matching deals, or 0 when nothing matched.
    value: f64,
    /// Deals contributing, so a caller can show "over N deals" or grey out an
    /// average computed from too small a sample.
    count: usize,
}

/// One bucket of a frequency histogram.
#[derive(Serialize)]
struct FrequencyBin {
    value: i32,
    count: usize,
}

/// One `frequency "label" (expr, min, max)` result.
///
/// Returned as data rather than the CLI's ASCII table so the page can draw a
/// real chart. `below` and `above` correspond to the CLI's `Low` and `High`
/// rows: values outside a declared range, which would otherwise vanish.
#[derive(Serialize)]
struct FrequencyResult {
    label: Option<String>,
    /// Declared range, if the script gave one.
    min: Option<i32>,
    max: Option<i32>,
    /// Contiguous buckets across the range, zero-filled — the caller can plot
    /// these directly without filling gaps itself.
    bins: Vec<FrequencyBin>,
    /// Counts falling outside a declared range.
    below: usize,
    above: usize,
    /// Every observation, including `below` and `above`.
    total: usize,
    /// The two-dimensional form's cross-tabulation, when the script gave a
    /// second expression and range. `None` for an ordinary `frequency`.
    ///
    /// Carried even though the page does not draw it yet: without it a
    /// two-dimensional statement would come back looking like a one-dimensional
    /// one — the first expression's marginal, correct but silently short of
    /// what was asked for.
    grid: Option<FrequencyGridResult>,
}

/// A two-dimensional `frequency` as a grid, with both axes' ranges.
///
/// Rows are the first expression, columns the second. Both axes run
/// low-outliers, each value in the range, then high-outliers, so a row is
/// `max2 - min2 + 3` long — the same shape the CLI prints.
#[derive(Serialize)]
struct FrequencyGridResult {
    min1: i32,
    max1: i32,
    min2: i32,
    max2: i32,
    counts: Vec<Vec<usize>>,
}

/// What nature offered against what the levelled run delivered, per hand type.
/// The page draws its bars straight from these.
#[derive(Serialize, Clone)]
struct HandTypeShare {
    name: String,
    natural: f64,
    /// The share the keeps deliver in the long run, which is what the generated
    /// scenario's own text says. Equal to `delivered` when nothing was levelled.
    planned: f64,
    /// Its share of this run, which over a short set is lumpy however even the
    /// keeps are.
    delivered: f64,
    produced: usize,
    out_of: usize,
    /// How many deals of this type a round robin owed it. Against `produced` it
    /// says whether the type came up short. `None` for any other run.
    wanted: Option<usize>,
}

/// The levelling, as numbers. No prose: how it reads is the page's business.
#[derive(Serialize)]
struct LevelingResult {
    /// The scenario that actually ran, for the page to show beside the one that
    /// was written.
    script: String,
    shares: Vec<HandTypeShare>,
    /// 1 unless a budget relaxed the target.
    exactness: f64,
    /// The share of qualifying deals the keeps let through.
    acceptance: f64,
    /// Deals dealt per deal kept.
    cost: f64,
    /// How many deals the keeps were measured over.
    measured: usize,
    /// The rarest type's count, and what that is worth as a relative error.
    /// The precision of the whole levelling rests on it, and this is the number
    /// to read: a keep is `mix / natural`, so an error here is baked into the
    /// delivered mix rather than averaging out.
    rarest: String,
    rarest_seen: usize,
    /// Relative standard error on that rate — 0.022 at the 2,000 sightings
    /// characterizing aims at. Absent when the type was never seen, which is a
    /// levelling that could not be computed rather than one that is merely thin.
    rarest_error: Option<f64>,
    /// Wall-clock seconds spent measuring, which is the pass the reader did not
    /// ask for and cannot otherwise account for.
    measure_seconds: f64,
    /// Anything worth saying out loud that does not make the levelling wrong —
    /// a measurement thinner than the goal, above all.
    warnings: Vec<String>,
    /// Deals dealt while characterizing, which is nearly all of a levelled
    /// run's work and almost none of what it returns.
    characterized: usize,
}

#[derive(Serialize)]
struct GenerateResult {
    deals: Vec<String>,
    /// Whether the format asked for renders deals at all.
    ///
    /// False only under `none`, and it travels with the result rather than
    /// being read off whatever the format control says now: changing the
    /// dropdown without pressing Run again must not make the page describe the
    /// result on screen as something it is not. An empty `deals` cannot say
    /// this on its own — a run that matched nothing has one too.
    renders_deals: bool,
    /// Deals examined, including those the filter rejected.
    generated: usize,
    /// Deals that matched.
    produced: usize,
    /// True if `max_generate` was reached before `produce` was satisfied, so the
    /// caller can distinguish "no more matches" from "ran out of budget".
    hit_limit: bool,
    /// Results of the script's `average` statements, in declaration order.
    averages: Vec<AverageResult>,
    /// Results of the script's `frequency` statements, in declaration order.
    frequencies: Vec<FrequencyResult>,
    /// Wall-clock seconds spent generating, matching the CLI's "Time needed".
    seconds: f64,
    /// Everything the script's `printes` statements wrote, exactly as the CLI
    /// would have written it to a terminal. Empty when the script has none.
    printes: String,
    /// The hand type each returned deal matched, parallel to `deals`.
    deal_types: Vec<Option<String>>,
    /// The script's hand types and their shares of this run. Present whether or
    /// not it was levelled; without levelling `natural` and `delivered` agree.
    hand_types: Vec<HandTypeShare>,
    /// Present only when the run was levelled.
    leveling: Option<LevelingResult>,
    /// Present only when the run was dealt round robin.
    round_robin: Option<RoundRobinResult>,
    /// What the supplied deals turned out to be. Present only for
    /// [`generate_from_deals`]; a run that shuffles reads nothing.
    input: Option<InputSummary>,
}

/// What arrived, when the caller supplied the deals rather than a seed.
///
/// The command line writes this to stderr and a page has no stderr, so it comes
/// back with the results instead. Without it a run over a library that arrived
/// short, or one whose records were mostly unreadable, looks exactly like a run
/// over all of it — fewer deals produced, nothing said. `read` against what the
/// caller believes it handed over is the check worth making, and it is the only
/// thing that catches a truncated download.
#[derive(Serialize)]
struct InputSummary {
    /// Which reader handled the bytes: `"zrd"`, `"pbn"` or `"lines"`.
    format: String,
    /// Deals read, which is every deal the run had to work with. The filter can
    /// only reduce this.
    read: usize,
    /// Deals that arrived with a double-dummy table and so are not solved
    /// again: `tricks()` over one of these costs nothing.
    solved: usize,
    /// Deals nobody has solved yet, which are solved on demand.
    unsolved: usize,
    /// Section separators, which are not deals. A library divides its sections
    /// with a record giving one seat sixteen cards.
    separators: usize,
    /// Records that could not be read, each with its reason, at most
    /// [`MAX_REPORTED_SKIPS`] of them. The deals around them were still read.
    skipped: Vec<String>,
    /// How many were skipped altogether, which `skipped` may not list in full.
    skipped_count: usize,
    /// Worth saying, but not a failure — chiefly a file whose name disagrees
    /// with what is inside it.
    notes: Vec<String>,
}

/// Skipped records named individually before the rest are merely counted. A
/// library whose every record was unreadable would otherwise hand a page
/// millions of strings it can show none of.
const MAX_REPORTED_SKIPS: usize = 10;

impl InputSummary {
    /// The reader's report, as the page sees it.
    ///
    /// `read` is passed in rather than added up from the report: it is how many
    /// deals the run was actually handed, which is the number a caller checks
    /// against what it sent.
    fn new(report: &dealer_run::deal_input::InputReport, read: usize) -> Self {
        Self {
            format: report.format.to_string(),
            read,
            solved: report.solved,
            unsolved: report.unsolved,
            separators: report.separators,
            skipped: report
                .skipped
                .iter()
                .take(MAX_REPORTED_SKIPS)
                .cloned()
                .collect(),
            skipped_count: report.skipped.len(),
            notes: report.notes.clone(),
        }
    }
}

/// Where a run's deals come from: the one thing the two entry points differ in,
/// and deliberately the only thing.
enum DealSource {
    /// Shuffled from the seed, which is every ordinary run.
    Shuffled,
    /// Supplied by the caller, already decoded by `dealer-run`'s reader — the
    /// same reader `--input-deals` goes through, so a file read in a tab and
    /// the same file read at a terminal cannot come to different conclusions
    /// about what is in it.
    Supplied {
        deals: Vec<dealer_run::run::SolvedDeal>,
        report: dealer_run::deal_input::InputReport,
    },
}

/// How a round robin was shaped, for a page that has to word it. The counts
/// per type are already in `hand_types`.
#[derive(Serialize)]
struct RoundRobinResult {
    /// Complete rounds the run dealt.
    rounds: usize,
    /// Deals in the partial round at the end, if any.
    remainder: usize,
    /// Whether every type appears once per round, which reads differently from
    /// a scenario weighting them with `HandType_X_Share`.
    even: bool,
}

/// Script settings that affect output but not generation.
struct OutputContext {
    dealer: Option<Position>,
    vulnerability: Option<Vulnerability>,
    seed: u32,
}

/// Wall-clock milliseconds. `std::time::SystemTime::now()` panics on
/// wasm32-unknown-unknown, so read the clock through JS instead.
///
/// Off wasm there is no JS to read: a `js_sys` import panics with "cannot call
/// wasm-bindgen imported functions on non-wasm targets", which would put every
/// one of these entry points out of reach of an ordinary `cargo test`. The
/// clock is the only thing standing in the way of running them there, and what
/// it reads has no effect on which deals come out.
fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }
}

/// Generate deals matching `script`, returning JSON.
///
/// With `auto_level`, the engine levels the scenario first: it measures how
/// often each `HandType_*` comes up, works out a keep rate for each, and runs
/// the levelled copy — two passes, both of them the engine's, so the browser
/// and the command line agree on what a levelling is and when to refuse one.
/// The deals then come back interleaved, walking through the types rather than
/// meeting them as they fall.
///
/// `max_generate` bounds the work: a browser tab has no Ctrl-C, so a selective
/// filter must not be able to hang it. Callers should surface `hit_limit`
/// rather than silently showing a short result.
/// Start a pool of `threads` web workers for the engine to deal on.
///
/// Only present in a threaded build (`./build.sh threaded`), and it must be
/// awaited before `generate` if a run is to use more than this thread. Needs
/// the page served with COOP and COEP — `SharedArrayBuffer` does not exist
/// without them — and the caller built for it.
///
/// Not calling it is not an error: the engine falls back to one thread and
/// deals exactly the same deals, which is the property that makes any of this
/// safe.
///
/// **The site does not ship a threaded build**, but that is a deployment
/// decision rather than a performance one: a second build to produce, and
/// COOP/COEP headers to serve.
///
/// It used to be a performance one. Threads made the browser slower — 4M deals
/// in six seconds on one against 290K on twelve — because a `Deal` was four
/// `Vec<Card>` allocations and wasm's dlmalloc serialises them, so every worker
/// queued on the same lock. `Hand` is an inline `[Card; 13]` now, and the shape
/// reversed: measured 2026-09-05, about 4x on twelve threads, and no cost at
/// one. `build.sh` carries the numbers.
#[cfg(feature = "parallel")]
#[wasm_bindgen]
pub fn start_threads(threads: usize) -> js_sys::Promise {
    THREADS.with(|t| t.set(threads.max(1)));
    wasm_bindgen_rayon::init_thread_pool(threads.max(1))
}

thread_local! {
    /// How many workers the caller started, which is what a run may use.
    static THREADS: std::cell::Cell<usize> = const { std::cell::Cell::new(1) };
}

/// Threads this build and this page can actually deal on.
fn threads_available() -> usize {
    #[cfg(feature = "parallel")]
    {
        THREADS.with(|t| t.get())
    }
    #[cfg(not(feature = "parallel"))]
    {
        1
    }
}

/// How long characterizing runs when a caller does not say, in seconds.
///
/// Exported so a page can show the number in a field rather than keeping a copy
/// of it: two defaults that were meant to be the same one drift, and the drift
/// shows up as a page whose control disagrees with the engine it drives.
#[wasm_bindgen]
pub fn measure_budget_seconds() -> f64 {
    MEASURE_BUDGET_MS / 1000.0
}

/// Whether this build can use more than one thread at all, so a page can tell
/// the difference between "not built for it" and "the browser refused".
#[wasm_bindgen]
pub fn supports_threads() -> bool {
    cfg!(feature = "parallel")
}

/// A page's side of a run: it holds deals, collects what the script printed,
/// paints a bar and answers a clock.
///
/// Everything else — the stream, the condition, the categories, the levelling
/// and the deals it re-uses rather than deals twice — belongs to `dealer-run`,
/// which is why this is short.
struct Page<'a> {
    progress: &'a Progress,
    /// When the characterizing pass must stop, whatever it has managed.
    deadline: f64,
    /// Deals the characterizing pass may take. `usize::MAX` when the page is
    /// shuffling its own, where the clock above is the only limit; the run's
    /// own budget when the deals were supplied, since a finite pile bounds
    /// itself whatever the clock says.
    measure_generate: usize,
    /// Deals to hand back, capped: a large `produce` used to gather statistics
    /// does not have to ship every deal to JS. Left empty altogether under
    /// `Format::None`, which is the whole point of that format.
    held: Vec<(Option<usize>, Deal)>,
    /// Whether the chosen format has any use for a deal. False only under
    /// `Format::None`, and then nothing is cloned, rendered or shipped.
    collects_deals: bool,
    /// Produced deals whose output has been taken, which is what the cap counts.
    ///
    /// Not `held.len()`: under `Format::None` nothing lands in `held`, and the
    /// cap still has to hold for `printed` — otherwise a script with a
    /// `printes` statement would build one string per deal over the whole run.
    kept: usize,
    /// Everything the script's `printes` and `printrpt` statements wrote,
    /// capped alongside the deals so the two stay in step.
    printed: String,
    /// True once the clock stopped a pass, so the page can say so.
    ran_out: bool,
    /// Wall-clock seconds spent characterizing, which is the pass the reader
    /// did not ask for and cannot otherwise account for. Timed here because
    /// the engine has no clock — the one it answers to is this one.
    characterizing_started: f64,
    characterizing_seconds: f64,
}

impl Page<'_> {
    /// How far this pass is likely to get, in sightings of the scarcest
    /// category — which is what its bar is counting.
    ///
    /// Two limits can stop characterizing short of the goal, and both are the
    /// page's: the clock, and the deals it is allowed. Whichever arrives first
    /// sets the ceiling, and the rate so far projects it — `seen` sightings in
    /// this much of the budget will be about `seen / spent` in all of it.
    ///
    /// For a page shuffling its own deals there is only the clock, since
    /// `measure_generate` is then unbounded: `Max generate` bounds the run and
    /// stopped bounding this pass, which is what it was never asked to do.
    ///
    /// Rough on purpose, and it firms up within the first moment. A bar drawn
    /// against 2,000 that ends at 61 looks broken; the same bar with the mark
    /// at 63 says the run is doing what it can and will not reach the goal,
    /// which is the thing worth knowing.
    fn reachable(&self, seen: usize, generated: usize, goal: usize) -> usize {
        if seen == 0 || generated == 0 {
            return goal;
        }
        let by_deals = seen as f64 * self.measure_generate as f64 / generated as f64;
        let spent = (now_ms() - self.characterizing_started).max(1.0);
        let budget = (self.deadline - self.characterizing_started).max(1.0);
        let by_clock = seen as f64 * budget / spent;
        (by_deals.min(by_clock).round() as usize).clamp(seen.max(1), goal)
    }

    /// Offer the bar a report, throttled by [`Progress`] itself.
    ///
    /// Shared by the two hooks that paint: the offer to stop that ends a batch,
    /// and the reports the engine makes from inside one. They must draw the
    /// same bar, so they go through the same place.
    fn paint(&self, phase: Phase, produced: usize, generated: usize, target: usize) {
        let expected = if phase == Phase::Characterizing {
            self.reachable(produced, generated, target)
        } else {
            target
        };
        self.progress
            .report(phase, produced, generated, target, expected, false);
    }
}

impl RunHost for Page<'_> {
    /// Paint the bar, and nothing else.
    ///
    /// The engine offers this from inside a batch — between the chunks a
    /// solving pass builds its deals in, while it solves the ones that matched,
    /// and as it hands them over. That is the whole of what a script calling
    /// `tricks()` on shuffled deals needs to stop looking hung: a batch is at
    /// least 1024 deals, and at 23 ms a deal the offer to stop that ends one
    /// arrives half a minute late (#83).
    ///
    /// The clock that stops a characterizing pass is deliberately not read
    /// here. Stopping is `should_stop`'s, still once a batch, so a levelled run
    /// stops exactly where it did.
    fn progress(&mut self, phase: Phase, produced: usize, generated: usize, target: usize) {
        self.paint(phase, produced, generated, target);
    }

    fn should_stop(
        &mut self,
        phase: Phase,
        produced: usize,
        generated: usize,
        target: usize,
    ) -> bool {
        self.paint(phase, produced, generated, target);
        if phase == Phase::Characterizing && now_ms() >= self.deadline {
            self.ran_out = true;
            return true;
        }
        false
    }

    fn pass_finished(&mut self, phase: Phase, produced: usize, generated: usize, target: usize) {
        if phase == Phase::Characterizing {
            self.characterizing_seconds = (now_ms() - self.characterizing_started) / 1000.0;
        }
        // A finished pass reached exactly what it reached, so that is the mark
        // too — a bar arriving at its own expectation rather than stopping
        // somewhere short of a figure it was never going to meet.
        self.progress
            .report(phase, produced, generated, target, produced.max(1), true);
    }

    fn produced(&mut self, deal: &Produced) -> Result<(), String> {
        if self.kept >= MAX_RETURNED_DEALS {
            return Ok(());
        }
        self.kept += 1;
        // What the script itself wrote, whatever the format. `printes` and
        // `printrpt` are statements the script ran, not a rendering of a deal,
        // so a format that shows no hands must not quietly turn them off. Both
        // cost nothing at all when the script declares none, which is nearly
        // every script.
        for row in deal.rows()?.printed {
            self.printed.push(' ');
            self.printed.push_str(&row);
            self.printed.push('\n');
        }
        self.printed.push_str(&deal.printes()?);
        // The deal itself, which `Format::None` has no use for: no clone here,
        // no render after the run, and nothing to serialise across to the page.
        if self.collects_deals {
            self.held.push((deal.hand_type, deal.deal.clone()));
        }
        Ok(())
    }
}

/// Generate deals from a script, as JSON.
///
/// With `auto_level`, the engine characterizes the scenario first — how often
/// each `HandType_*` comes up — works out a keep rate for each and deals the
/// levelled copy. Both passes are the engine's business; what comes back is
/// the deals and the numbers behind them.
///
/// With `round_robin`, it divides `produce` among the `HandType_*` variables —
/// one of each per round, any remainder going to whichever types turn up next,
/// one apiece — instead of taking deals as they come. Nothing is measured, so
/// nothing can be measured wrong: a levelled set of twenty is four of each on
/// average and 6/1/5/4/4 without anything having gone wrong, where a round is
/// four. Refused alongside `auto_level`, which asks for the same thing the
/// other way.
/// `measure_seconds` is how long characterizing may take, and it is the only
/// thing that stops it: `max_generate` bounds the run that was asked for, not
/// the measuring that pays for it. Left out — `undefined` or `null` — it takes
/// [`MEASURE_BUDGET_MS`]. The command line spells the same thing
/// `--level-timeout`.
// The argument list is the JS calling convention: wasm_bindgen exports these
// positionally, and folding them into a settings object would move the naming
// out of the type system and into a hand-written cast on both sides.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen]
pub fn generate(
    script: &str,
    seed: u32,
    produce: usize,
    max_generate: usize,
    format: &str,
    auto_level: bool,
    round_robin: bool,
    params: Vec<String>,
    measure_seconds: Option<f64>,
    on_progress: Option<js_sys::Function>,
) -> Result<String, JsError> {
    run_script(
        script,
        seed,
        produce,
        max_generate,
        format,
        auto_level,
        round_robin,
        &params,
        measure_seconds,
        on_progress,
        DealSource::Shuffled,
    )
    .map_err(|e| JsError::new(&e))
}

/// Run `script` over deals the caller supplies, rather than dealing any.
///
/// Everything else — `auto_level`, `round_robin`, `params`, `format` and the
/// JSON that comes back — is [`generate`]'s and behaves as it does there. Where
/// the deals come from is the only difference between the two.
///
/// `deals` is the file's bytes — a `Uint8Array`, which is what a `fetch()`
/// gives after `arrayBuffer()`. **The browser is the HTTP client**: nothing
/// here fetches, opens or names a file, which is why one entry point serves a
/// download, a drag-and-drop and a file input alike.
///
/// The format is decided by what the bytes are, not what they were called — a
/// Pavlicek `.zrd` library, PBN, or the one-line and printall layouts — through
/// the same reader `--input-deals` uses at the terminal. A library's records
/// and PBN's `[DoubleDummyTricks]` bring their double-dummy tables with them, so
/// a script calling `tricks()` over a solved file solves nothing.
///
/// What came back is in `input`, and **a caller should look at it**: `read`
/// against the number of deals it believes it sent is what tells a run over a
/// truncated download from a run over all of it. Neither `produced` nor
/// `hit_limit` can say that — a run that exhausts the deals it was given has
/// not hit its budget, so it stops short and looks like success.
///
/// `seed` no longer decides which deals appear, since they are given, but it is
/// still what `rnd()` draws from and what orders an interleaved set — so it is
/// asked for, exactly as `generate` asks for it.
///
/// `predeal` is refused rather than ignored: it arranges cards into deals this
/// program shuffles, and there is nothing for it to do to deals that arrived
/// already dealt. The command line refuses the same combination.
///
/// `measure_seconds` bounds characterizing as it does in [`generate`], but here
/// the deals bound it too: a supplied pile is finite, so both passes share it
/// and whichever limit arrives first stops the pass.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen]
pub fn generate_from_deals(
    script: &str,
    deals: &[u8],
    seed: u32,
    produce: usize,
    max_generate: usize,
    format: &str,
    auto_level: bool,
    round_robin: bool,
    params: Vec<String>,
    measure_seconds: Option<f64>,
    on_progress: Option<js_sys::Function>,
) -> Result<String, JsError> {
    // One decoder, two front ends. Anything read here that the command line
    // would not read the same way is a bug in one of them, not a difference
    // between a page and a terminal.
    // The whole file, in order. A page that wants to start somewhere else —
    // the seed picking a position in a large library, which is what #68 asks
    // for — will pass a window of its own; that is a decision about what the
    // page offers, not one to make while wiring two branches together.
    let (supplied, report) =
        dealer_run::deals_from_bytes(deals, dealer_run::deal_input::Window::all())
            .map_err(|e| JsError::new(&e))?;
    run_script(
        script,
        seed,
        produce,
        max_generate,
        format,
        auto_level,
        round_robin,
        &params,
        measure_seconds,
        on_progress,
        DealSource::Supplied {
            deals: supplied,
            report,
        },
    )
    .map_err(|e| JsError::new(&e))
}

/// The body both entry points share: everything except where the deals came
/// from.
///
/// Speaks `String` rather than `JsError` so that it can be called from an
/// ordinary test. `JsError::new` reaches for JavaScript's `Error`, which does
/// not exist off wasm — a failure raised in here would abort the test process
/// instead of being an error a test can assert on, and the failures are
/// precisely what wants asserting.
#[allow(clippy::too_many_arguments)]
fn run_script(
    script: &str,
    seed: u32,
    produce: usize,
    max_generate: usize,
    format: &str,
    auto_level: bool,
    round_robin: bool,
    params: &[String],
    measure_seconds: Option<f64>,
    on_progress: Option<js_sys::Function>,
    source: DealSource,
) -> Result<String, String> {
    let format = Format::parse(format)?;
    let started = now_ms();

    // Progress, for a caller that can paint it — which means a worker, since
    // nothing repaints while this runs on the main thread.
    //
    // Time-based rather than every N deals: how long a deal takes varies by
    // orders of magnitude between a bare `hcp` condition and one calling
    // `tricks()`, so a fixed deal count is either a flood or a silence. The
    // phase travels with it because a levelled run deals the scenario more
    // than once and a single bar would appear to restart.
    let progress = Progress::new(on_progress);

    let params = script_params_from(params)?;
    let preprocessed = dealer_parser::preprocess_all(script, &params)?;
    let program =
        dealer_parser::parse_program(&preprocessed).map_err(|e| format!("Parse error: {}", e))?;

    // Split before the script is read, so the statements can be checked against
    // what the run is actually going to deal from.
    let (given, input) = match source {
        DealSource::Shuffled => (None, None),
        DealSource::Supplied { deals, report } => {
            let summary = InputSummary::new(&report, deals.len());
            (Some(deals), Some(summary))
        }
    };
    // Whether this run is dealing its own cards rather than reading cards it
    // was handed. It decides what may bound the characterizing pass, so it is
    // read here, before `given` is moved into the options below.
    let has_own_deals = given.is_none();

    // Settings that affect how a deal is labelled rather than which deals are
    // produced, and the predeal the run starts from.
    let mut output = OutputContext {
        dealer: None,
        vulnerability: None,
        seed,
    };
    let mut predeal = FastDealConfig::new();
    for statement in &program.statements {
        match statement {
            Statement::Dealer(pos) => output.dealer = Some(*pos),
            Statement::Vulnerable(v) => {
                output.vulnerability = Some(match v {
                    VulnerabilityType::None => Vulnerability::None,
                    VulnerabilityType::NS => Vulnerability::NS,
                    VulnerabilityType::EW => Vulnerability::EW,
                    VulnerabilityType::All => Vulnerability::All,
                })
            }
            // Predeal arranges cards into deals this program shuffles. Against
            // supplied deals there is nothing for it to do, and quietly
            // ignoring it would leave a script looking as though its predealt
            // cards were honoured. The command line refuses the same pair.
            Statement::Predeal { .. } if given.is_some() => {
                return Err(
                    "predeal arranges cards into deals this program shuffles, so it has \
                     nothing to do with deals supplied to it"
                        .to_string(),
                )
            }
            Statement::Predeal { position, cards } => predeal
                .predeal(*position, cards)
                .map_err(|e| format!("Predeal error: {}", e))?,
            // `print` is a paginated hand record with form feeds, written for a
            // line printer. There is nowhere for that to go on a page, and
            // quietly dropping it would leave a script looking as though it had
            // run.
            Statement::Action { print_hands, .. } if !print_hands.is_empty() => {
                return Err(
                    "print(...) writes a paginated hand record for a printer and is not \
                     available in the browser"
                        .to_string(),
                )
            }
            _ => {}
        }
    }

    // How long the reader is prepared to spend characterizing, in seconds, or
    // the default when they have not said. Clamped: a page that dealt for an
    // hour because a field held 3600 would be indistinguishable from one that
    // had hung, and a budget under a second cannot measure anything.
    let measure_budget_ms = measure_seconds
        .filter(|s| s.is_finite())
        .map(|s| (s * 1000.0).clamp(1_000.0, MAX_MEASURE_BUDGET_MS))
        .unwrap_or(MEASURE_BUDGET_MS);

    let mut page = Page {
        progress: &progress,
        deadline: started + measure_budget_ms,
        measure_generate: match has_own_deals {
            true => usize::MAX,
            false => max_generate,
        },
        held: Vec::new(),
        collects_deals: format.collects_deals(),
        kept: 0,
        printed: String::new(),
        ran_out: false,
        characterizing_started: started,
        characterizing_seconds: 0.0,
    };
    let report = dealer_run::run(
        script,
        RunOptions {
            seed,
            produce,
            max_generate,
            // What `par()` is told: the `vulnerable` statement if the script
            // has one, and neither side otherwise. Same rule as the terminal.
            vulnerability: match output.vulnerability {
                Some(Vulnerability::NS) => dealer_core::Vulnerability::NorthSouth,
                Some(Vulnerability::EW) => dealer_core::Vulnerability::EastWest,
                Some(Vulnerability::All) => dealer_core::Vulnerability::Both,
                Some(Vulnerability::None) | None => dealer_core::Vulnerability::None,
            },
            // The supplied deals, or the shuffle. Moved rather than cloned:
            // a library is large, and copying it to choose between two arms
            // would double the peak the page has to hold.
            deals: match given {
                Some(deals) => Deals::Given(deals),
                None => Deals::Shuffled {
                    predeal,
                    swap: dealer_core::SwapMode::None,
                },
            },
            // Whatever the caller started a pool with, and one if it did not
            // — see `start_threads`. A thread count cannot change what comes
            // out, only how long it takes, so a page that cannot spawn any
            // gets the same deals more slowly.
            threads: threads_available(),
            batch: 0,
            params: params.clone(),
            round_robin,
            leveling: auto_level.then_some(LevelingOptions {
                // The target mix comes out of the script, exactly as it does on
                // the command line: `HandType_22_24_Share = 3` and nothing
                // else. That is why the page needs no control for it — a
                // scenario carries its own intended mix, and the two front ends
                // cannot drift apart.
                target: None,
                budget: None,
                // A browser has no patience for the command line's 500
                // sightings of the rarest type, and refusing outright would
                // teach nothing. The count it managed comes back instead, so
                // the page can say how well the keeps are pinned down.
                min_sample: MIN_BROWSER_SAMPLE,
                // Matching deals the pass may produce, as `--level-measure`
                // means at the terminal and with its default. A ceiling, not a
                // target: measuring stops as soon as the rarest category is
                // worth dividing by, and on the scenarios that need levelling
                // the clock below arrives long before this does.
                measure_cap: MEASURE_PRODUCE_CAP,
                // Characterizing gets its own deal allowance, so `Max generate`
                // bounds the run the reader asked for and nothing else. It had
                // been doing both jobs, and the deal cap was cutting the
                // measuring short with seconds of the budget still unspent.
                //
                // `usize::MAX` when this page shuffles its own deals: nothing
                // but the clock above stops the pass, which is what "spend up
                // to N seconds working this out" means. Supplied deals are a
                // finite pile rather than a tap, so there the run's budget is
                // the honest bound and both passes share it, exactly as the
                // command line does.
                measure_deals: match has_own_deals {
                    true => dealer_run::MeasureDeals::Own(usize::MAX),
                    false => dealer_run::MeasureDeals::Shared,
                },
            }),
        },
        &mut page,
    )
    .map_err(|e| e.to_string())?;

    // Interleaved, a set walks through the categories rather than meeting them
    // as they fall. Numbered by where they land, so a reader that sorts on the
    // board number cannot quietly undo the ordering.
    let labels: Vec<String> = report.hand_types.iter().map(|(n, _)| n.clone()).collect();
    let order: Vec<usize> = if report.leveling.is_some() && !labels.is_empty() {
        let mut buckets: Vec<(Option<String>, Vec<usize>)> = Vec::new();
        for (index, (matched, _)) in page.held.iter().enumerate() {
            let label = matched.map(|i| labels[i].clone());
            match buckets.iter_mut().find(|(name, _)| *name == label) {
                Some((_, deals)) => deals.push(index),
                None => buckets.push((label, vec![index])),
            }
        }
        let names: Vec<&str> = labels.iter().map(String::as_str).collect();
        dealer_level::interleave(&names, buckets, seed as u64)
    } else {
        (0..page.held.len()).collect()
    };
    let mut deals = Vec::with_capacity(order.len());
    let mut deal_types = Vec::with_capacity(order.len());
    for (position, index) in order.into_iter().enumerate() {
        let (matched, deal) = &page.held[index];
        let label = matched.map(|i| labels[i].as_str());
        deals.push(format.render(deal, position, &output, label));
        deal_types.push(label.map(str::to_string));
    }

    let hand_types: Vec<HandTypeShare> = match &report.leveling {
        Some(levelling) => report
            .hand_types
            .iter()
            .enumerate()
            .map(|(i, (name, count))| HandTypeShare {
                name: name.clone(),
                // What the characterizing pass saw, which is what `natural`
                // means. The producing run has already had the keeps applied.
                natural: levelling
                    .natural_hand_types
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, n)| *n as f64 / levelling.measured.produced.max(1) as f64)
                    .unwrap_or(0.0),
                planned: planned_share(levelling, i),
                delivered: *count as f64 / report.produced.max(1) as f64,
                produced: *count,
                out_of: report.produced,
                // A levelled run can be dealt round robin too — the levelling
                // measures the scenario, the round decides which of its deals
                // reach the caller — so this is read the same way in both
                // branches.
                wanted: report.round_robin.as_ref().map(|p| p.owed(i).max(*count)),
            })
            .collect(),
        None => {
            // Dealing a round robin, `planned` is the even split that was asked
            // for rather than the mix that turned up — and unless the deals ran
            // out, the two are the same number. That is the whole point of it,
            // and the page draws the pair without needing to know which mode it
            // is in.
            let plan = report.round_robin.as_ref();
            report
                .hand_types
                .iter()
                .enumerate()
                .map(|(i, (name, count))| {
                    let share = *count as f64 / report.produced.max(1) as f64;
                    HandTypeShare {
                        name: name.clone(),
                        natural: share,
                        planned: match plan {
                            Some(p) => p.per_round[i] as f64 / p.round_size().max(1) as f64,
                            None => share,
                        },
                        delivered: share,
                        produced: *count,
                        out_of: report.produced,
                        // What the complete rounds owed it — or, once that is
                        // met, what it actually took. A type holding part of
                        // the partial round was owed that much, and saying so
                        // keeps the pair a fraction a reader can trust: "5 of
                        // 5" and "4 of 4", never "5 of 4", which looks like an
                        // error rather than a partial round.
                        wanted: plan.map(|p| p.owed(i).max(*count)),
                    }
                })
                .collect()
        }
    };

    let leveling = report.leveling.as_ref().map(|levelling| {
        let rarest = levelling.rarest();
        LevelingResult {
            script: levelling.script.clone(),
            shares: hand_types.clone(),
            exactness: levelling.lambda,
            acceptance: levelling.acceptance,
            cost: levelling.cost(),
            measured: levelling.measured.produced,
            rarest: rarest.map(|p| p.name.clone()).unwrap_or_default(),
            rarest_seen: rarest.map(|p| p.seen).unwrap_or(0),
            rarest_error: Some(levelling.precision()).filter(|e| e.is_finite()),
            measure_seconds: page.characterizing_seconds,
            warnings: levelling.warnings.clone(),
            characterized: levelling.characterized,
        }
    });

    let result = GenerateResult {
        deals,
        renders_deals: format.collects_deals(),
        deal_types,
        generated: report.generated,
        produced: report.produced,
        hit_limit: report.hit_limit,
        averages: report
            .stats
            .averages
            .iter()
            .map(|a| AverageResult {
                is_hand_type: a.is_hand_type,
                label: a.label.clone(),
                value: a.value,
                count: a.count,
            })
            .collect(),
        frequencies: report
            .stats
            .frequencies
            .iter()
            .map(|f| FrequencyResult {
                label: f.label.clone(),
                min: f.min,
                max: f.max,
                bins: f
                    .bins
                    .iter()
                    .map(|(value, count)| FrequencyBin {
                        value: *value,
                        count: *count,
                    })
                    .collect(),
                below: f.below,
                above: f.above,
                total: f.total,
                grid: f.grid.as_ref().map(|g| FrequencyGridResult {
                    min1: g.min1,
                    max1: g.max1,
                    min2: g.min2,
                    max2: g.max2,
                    counts: g.counts.clone(),
                }),
            })
            .collect(),
        printes: page.printed,
        hand_types,
        leveling,
        round_robin: report.round_robin.as_ref().map(|p| RoundRobinResult {
            rounds: p.rounds,
            remainder: p.remainder,
            even: p.even(),
        }),
        seconds: (now_ms() - started) / 1000.0,
        input,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// A hand type's share of a levelled run once the keeps are applied.
///
/// Its own natural rate cannot say this when the scenario levels on a separate
/// `LevelType_` decomposition: what a hand type delivers then depends on how
/// its deals were spread across the levelling categories.
fn planned_share(levelling: &dealer_run::LevelingReport, index: usize) -> f64 {
    let keeps: Vec<f64> = levelling.plans.iter().map(|p| p.keep).collect();
    if levelling.natural_joint.is_empty() {
        return levelling
            .plans
            .get(index)
            .map(|p| p.mix)
            .unwrap_or_default();
    }
    dealer_level::group_mix(&levelling.natural_joint, &keeps)
        .get(index)
        .copied()
        .unwrap_or(0.0)
}

struct Progress {
    to: Option<js_sys::Function>,
    /// When the last report went out, so they arrive at a readable rate.
    last_ms: std::cell::Cell<f64>,
}

/// Shortest gap between reports. Fast enough to look live, slow enough that
/// posting them is never the expensive part.
const PROGRESS_EVERY_MS: f64 = 100.0;

impl Progress {
    fn new(to: Option<js_sys::Function>) -> Self {
        Self {
            to,
            last_ms: std::cell::Cell::new(0.0),
        }
    }

    /// Report, unless one went out too recently. `force` overrides that, for
    /// the end of a phase — otherwise a bar can stop short of its own total.
    #[allow(clippy::too_many_arguments)]
    fn report(
        &self,
        phase: Phase,
        produced: usize,
        generated: usize,
        target: usize,
        // How far this pass is expected to get, which is `target` unless a
        // limit will stop it short.
        expected: usize,
        force: bool,
    ) {
        let Some(to) = &self.to else { return };
        let now = now_ms();
        if !force && now - self.last_ms.get() < PROGRESS_EVERY_MS {
            return;
        }
        self.last_ms.set(now);

        let message = format!(
            r#"{{"phase":"{}","produced":{},"generated":{},"target":{},"expected":{}}}"#,
            phase.name(),
            produced,
            generated,
            target,
            expected
        );
        // A caller that throws is not worth stopping the run for: the deals are
        // the point and the bar is decoration.
        let _ = to.call1(&wasm_bindgen::JsValue::NULL, &message.into());
    }
}

/// How long the browser will go on characterizing a scenario, unless the caller
/// says otherwise.
///
/// A page blocks while it deals, so this is a clock rather than a deal count —
/// which is also what lets one number serve every scenario. Falling short of
/// the goal is not an error: the count reached comes back and the panel says
/// what it was.
///
/// Twenty seconds, not the six it was. Six was chosen when the deal cap was
/// stopping the pass first anyway, so the clock rarely got to say anything;
/// with the cap gone and threads under it, six seconds of a scenario the weight
/// of Jacoby 2NT still measures its rarest type barely a hundred times, which is
/// a ±10% rate to divide by. This is the default rather than the limit — the
/// page offers a field, and the command line has `--level-timeout`.
const MEASURE_BUDGET_MS: f64 = 20_000.0;

/// Matching deals the characterizing pass may produce before it gives up.
///
/// `--level-measure`'s default, so the two front ends stop for the same reasons
/// and at the same places. It is the loosest of the three limits on that pass —
/// the rarest category being measured well enough stops it first on a scenario
/// worth levelling, and the clock stops it first on one that is not.
const MEASURE_PRODUCE_CAP: usize = 2_000_000;

/// The longest a caller may ask for.
///
/// The page blocks a worker for the whole of it and can only stop by being
/// terminated, so a mistyped field must not be able to buy an afternoon of it.
const MAX_MEASURE_BUDGET_MS: f64 = 300_000.0;

/// Fewest sightings of a type the browser will divide by.
///
/// The command line refuses under 500, which is the right bar for a build step
/// that can simply be told to measure over more. A page that refused would
/// teach nothing, so it goes ahead from 50 and says how well the keeps are
/// pinned down.
const MIN_BROWSER_SAMPLE: usize = 50;

/// The counts a levelling needs, taken from a run.
/// Where a name is first used, so an undefined-name report can point at it.
///
/// The parser does not carry positions into the AST, and the check that finds
/// these names works on the AST — so the line is recovered by looking. A whole
/// word, outside comments, in the script as the editor holds it rather than the
/// preprocessed text, so the position lines up with what is on screen.
///
/// Returns `None` rather than guessing when it cannot find one; a report with
/// no position is better than one pointing at the wrong line.
fn locate_name(script: &str, name: &str) -> Option<(usize, usize)> {
    let mut in_block_comment = false;
    for (row, raw) in script.lines().enumerate() {
        let mut line = raw;
        if in_block_comment {
            match line.find("*/") {
                Some(i) => {
                    line = &line[i + 2..];
                    in_block_comment = false;
                }
                None => continue,
            }
        }
        // Drop a trailing line comment, and anything a block comment opens.
        let mut visible = line;
        if let Some(i) = visible.find("/*") {
            in_block_comment = true;
            visible = &visible[..i];
        }
        for marker in ["#", "//"] {
            if let Some(i) = visible.find(marker) {
                visible = &visible[..i];
            }
        }

        let bytes = visible.as_bytes();
        let mut at = 0;
        while let Some(i) = visible[at..].find(name) {
            let start = at + i;
            let end = start + name.len();
            let before_ok = start == 0 || !is_name_byte(bytes[start - 1]);
            let after_ok = end >= bytes.len() || !is_name_byte(bytes[end]);
            if before_ok && after_ok {
                // Columns are 1-based, as pest reports them.
                let col = visible[..start].chars().count() + 1;
                let offset = raw.len() - line.len();
                return Some((row + 1, col + offset));
            }
            at = end;
        }
    }
    None
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[derive(Serialize)]
struct CheckResult {
    ok: bool,
    /// Present only when `ok` is false.
    error: Option<String>,
    line: Option<usize>,
    column: Option<usize>,
}

/// Validate a script without generating, for live editor diagnostics.
///
/// Returns JSON rather than throwing, so an editor can call it on every
/// keystroke without exception handling. The line and column come from the
/// parser itself, so squiggles agree with the engine by construction.
#[wasm_bindgen]
pub fn check_script(script: &str, params: Vec<String>) -> String {
    let params = match script_params_from(&params) {
        Ok(params) => params,
        Err(message) => {
            return serde_json::to_string(&CheckResult {
                ok: false,
                error: Some(message),
                line: None,
                column: None,
            })
            .unwrap_or_default()
        }
    };
    let preprocessed = match dealer_parser::preprocess_all(script, &params) {
        Ok(text) => text,
        // The editor squiggles this the same as a parse error, which is what it
        // is from the writer's point of view.
        Err(message) => {
            return serde_json::to_string(&CheckResult {
                ok: false,
                error: Some(message),
                line: None,
                column: None,
            })
            .unwrap_or_default()
        }
    };
    let result = match dealer_parser::parse_program(&preprocessed) {
        // A misspelled name is not a syntax error — a bare expression is a legal
        // statement, so `not x4` where the variable is `HandType_x4` parses and
        // is quietly discarded. The command line has always reported these;
        // without the same check here the browser was the front end that said
        // nothing, and a script whose hand types silently never match is exactly
        // where that costs the most.
        Ok(program) => match dealer_parser::undefined_variables(&program).as_slice() {
            [] => CheckResult {
                ok: true,
                error: None,
                line: None,
                column: None,
            },
            names => {
                let (line, column) = match locate_name(script, &names[0]) {
                    Some((l, c)) => (Some(l), Some(c)),
                    None => (None, None),
                };
                CheckResult {
                    ok: false,
                    error: Some(format!(
                        "{} used but never defined: {}",
                        if names.len() == 1 {
                            "a name is"
                        } else {
                            "names are"
                        },
                        names.join(", ")
                    )),
                    line,
                    column,
                }
            }
        },
        Err(e) => {
            let text = format!("{}", e);
            let (line, column) = parse_position(&text);
            CheckResult {
                ok: false,
                error: Some(text),
                line,
                column,
            }
        }
    };
    serde_json::to_string(&result).unwrap_or_else(|_| {
        r#"{"ok":false,"error":"could not serialise result","line":null,"column":null}"#.to_string()
    })
}

/// Pull `line:col` out of a pest error, which renders as ` --> 3:17`.
fn parse_position(msg: &str) -> (Option<usize>, Option<usize>) {
    let Some(idx) = msg.find("--> ") else {
        return (None, None);
    };
    let rest = &msg[idx + 4..];
    let end = rest.find('\n').unwrap_or(rest.len());
    let mut parts = rest[..end].trim().split(':');
    (
        parts.next().and_then(|s| s.trim().parse().ok()),
        parts.next().and_then(|s| s.trim().parse().ok()),
    )
}

/// The documentation tables, mirrored for serialisation.
///
/// `dealer-parser` deliberately has no serde dependency — it is the parser, and
/// the editors and pages that read its vocabulary are downstream of it. So the
/// shapes are restated here and copied field by field, which costs a dozen
/// lines once and keeps serde out of the parser.
mod docs {
    use dealer_parser::vocabulary;
    use serde::Serialize;

    #[derive(Serialize)]
    pub struct FunctionDoc {
        pub name: &'static str,
        pub group: &'static str,
        pub signature: &'static str,
        pub summary: &'static str,
        pub example: &'static str,
        pub alias_of: Option<&'static str>,
        pub note: Option<&'static str>,
    }

    #[derive(Serialize)]
    pub struct OperatorDoc {
        pub symbol: &'static str,
        pub word: Option<&'static str>,
        pub precedence: u8,
        pub summary: &'static str,
        pub example: &'static str,
        pub note: Option<&'static str>,
    }

    #[derive(Serialize)]
    pub struct StatementDoc {
        pub keyword: Option<&'static str>,
        pub form: &'static str,
        pub summary: &'static str,
        pub example: &'static str,
        pub note: Option<&'static str>,
    }

    #[derive(Serialize)]
    pub struct ActionDoc {
        pub name: &'static str,
        pub summary: &'static str,
        pub note: Option<&'static str>,
    }

    #[derive(Serialize)]
    pub struct NotSupported {
        pub name: &'static str,
        pub instead: &'static str,
    }

    pub fn functions() -> Vec<FunctionDoc> {
        vocabulary::FUNCTION_DOCS
            .iter()
            .map(|d| FunctionDoc {
                name: d.name,
                group: d.group,
                signature: d.signature,
                summary: d.summary,
                example: d.example,
                alias_of: d.alias_of,
                note: d.note,
            })
            .collect()
    }

    pub fn operators() -> Vec<OperatorDoc> {
        vocabulary::OPERATOR_DOCS
            .iter()
            .map(|d| OperatorDoc {
                symbol: d.symbol,
                word: d.word,
                precedence: d.precedence,
                summary: d.summary,
                example: d.example,
                note: d.note,
            })
            .collect()
    }

    pub fn statements() -> Vec<StatementDoc> {
        vocabulary::STATEMENT_DOCS
            .iter()
            .map(|d| StatementDoc {
                keyword: d.keyword,
                form: d.form,
                summary: d.summary,
                example: d.example,
                note: d.note,
            })
            .collect()
    }

    pub fn actions() -> Vec<ActionDoc> {
        vocabulary::ACTION_DOCS
            .iter()
            .map(|d| ActionDoc {
                name: d.name,
                summary: d.summary,
                note: d.note,
            })
            .collect()
    }

    pub fn not_supported() -> Vec<NotSupported> {
        vocabulary::NOT_SUPPORTED
            .iter()
            .map(|d| NotSupported {
                name: d.name,
                instead: d.instead,
            })
            .collect()
    }
}

/// The names the levelling machinery acts on, rather than words the grammar
/// knows.
///
/// They are a convention over ordinary variables — that is the point of them,
/// since a script using them still parses on BBO — which leaves an editor no
/// way to tell `HandType_12` from a name the author chose. So the engine hands
/// its own constants out, for the same reason it hands out its vocabulary:
/// a second copy in JavaScript would be a second copy to go stale.
///
/// Prefixes and the share suffix are all matched without regard to case, as
/// `dealer_level` matches them: the prefix is a magic word, and a name that
/// differs only in case is not a name anybody meant to be different. The
/// variable itself stays case-sensitive to refer to, as it is in dealer.exe,
/// but that is a fact about the name rather than about the convention.
#[derive(Serialize)]
struct LevelingNames {
    hand_type_prefix: &'static str,
    level_type_prefix: &'static str,
    share_suffix: &'static str,
    verdicts: Vec<&'static str>,
    no_leveling: &'static str,
    block_begin: &'static str,
    block_end: &'static str,
    stamp: &'static str,
}

#[derive(Serialize)]
struct LanguageInfo {
    functions: Vec<&'static str>,
    statement_keywords: Vec<&'static str>,
    actions: Vec<&'static str>,
    positions: Vec<&'static str>,
    vulnerabilities: Vec<&'static str>,
    logical_words: Vec<&'static str>,
    other_keywords: Vec<&'static str>,
    operators: Vec<&'static str>,

    // The same vocabulary, described. The language reference page is rendered
    // from these, so it cannot list a function the parser rejects or miss one it
    // accepts — the guarantee the editor's highlighting already relies on.
    function_groups: Vec<&'static str>,
    function_docs: Vec<docs::FunctionDoc>,
    operator_docs: Vec<docs::OperatorDoc>,
    statement_docs: Vec<docs::StatementDoc>,
    action_docs: Vec<docs::ActionDoc>,
    not_supported: Vec<docs::NotSupported>,

    // Not vocabulary — the levelling conventions, which the editor colours so
    // an author can see which names the engine reads.
    leveling: LevelingNames,
}

/// The language's full vocabulary, for editor completion and hover, and its
/// documentation, for the language reference page.
///
/// Comes from `dealer_parser::vocabulary`, which is itself checked against
/// `grammar.pest`, so an editor built on this cannot advertise a function the
/// parser does not accept — and a reference page built on it cannot describe
/// a language other than the one that runs.
#[wasm_bindgen]
pub fn language_info() -> String {
    let info = LanguageInfo {
        functions: vocabulary::FUNCTIONS.to_vec(),
        statement_keywords: vocabulary::STATEMENT_KEYWORDS.to_vec(),
        actions: vocabulary::ACTIONS.to_vec(),
        positions: vocabulary::POSITIONS.to_vec(),
        vulnerabilities: vocabulary::VULNERABILITIES.to_vec(),
        logical_words: vocabulary::LOGICAL_WORDS.to_vec(),
        other_keywords: vocabulary::OTHER_KEYWORDS.to_vec(),
        operators: vocabulary::OPERATORS.to_vec(),

        function_groups: vocabulary::FUNCTION_GROUPS.to_vec(),
        function_docs: docs::functions(),
        operator_docs: docs::operators(),
        statement_docs: docs::statements(),
        action_docs: docs::actions(),
        not_supported: docs::not_supported(),

        leveling: LevelingNames {
            hand_type_prefix: dealer_level::HAND_TYPE_PREFIX,
            level_type_prefix: dealer_level::LEVEL_TYPE_PREFIX,
            share_suffix: dealer_level::SHARE_SUFFIX,
            verdicts: dealer_level::VERDICTS.to_vec(),
            no_leveling: dealer_level::NO_LEVELING,
            block_begin: dealer_level::LEVEL_BEGIN,
            block_end: dealer_level::LEVEL_END,
            stamp: dealer_level::LEVEL_STAMP,
        },
    };
    serde_json::to_string(&info).unwrap_or_else(|_| "{}".to_string())
}

/// Read the page's parameter fields, which arrive in `--param`'s own spelling.
///
/// The same `N=TEXT` the command line takes, so there is one place that decides
/// what a parameter spec is, and a value copied out of a browser field pastes
/// straight into a terminal.
fn script_params_from(specs: &[String]) -> Result<dealer_parser::ScriptParams, String> {
    let mut params = dealer_parser::ScriptParams::default();
    for spec in specs {
        // A field left empty is not a value; the script's own default, or the
        // error, should stand.
        if spec
            .split_once('=')
            .is_none_or(|(_, value)| value.is_empty())
        {
            continue;
        }
        params.set(spec)?;
    }
    Ok(params)
}

#[derive(Serialize)]
struct ParamInfo {
    index: usize,
    default: Option<String>,
    description: Option<String>,
    declared_on: Option<usize>,
    used_on: Option<usize>,
}

#[derive(Serialize)]
struct ParamsResult {
    ok: bool,
    /// Present only when `ok` is false: a malformed `# param` line.
    error: Option<String>,
    params: Vec<ParamInfo>,
}

/// What a script says about its own `$0`-`$9`, for the page to ask with.
///
/// Without this the browser could find the `$n` occurrences and have nothing to
/// label them with and no sensible starting value — so a parameterised scenario
/// simply failed to parse, naming a line the reader could not act on.
///
/// Returns JSON rather than throwing, for the same reason `check_script` does:
/// this runs as the script is edited.
#[wasm_bindgen]
pub fn script_params(script: &str) -> String {
    let result = match dealer_parser::script_parameters(script) {
        Ok(wanted) => ParamsResult {
            ok: true,
            error: None,
            params: wanted
                .into_iter()
                .map(|p| ParamInfo {
                    index: p.index,
                    default: p.default,
                    description: p.description,
                    declared_on: p.declared_on,
                    used_on: p.used_on,
                })
                .collect(),
        },
        Err(message) => ParamsResult {
            ok: false,
            error: Some(message),
            params: Vec::new(),
        },
    };
    serde_json::to_string(&result).unwrap_or_default()
}

/// Whether anything in `script` can reach the double-dummy solver.
///
/// `true` when a `tricks`, `dds`, `par` or `trix` — under any of its spellings,
/// in a condition, an action or a variable either of them reads — is actually
/// evaluated. The question a page asks with it is whether solved deals are
/// worth fetching: a script that never asks a double-dummy question gains
/// nothing from a library that arrives with the answers, and should not be
/// paying a download for it.
///
/// `undefined` when the script does not parse, which is not the same as no. A
/// page must not tell someone their script wants no double-dummy work when it
/// has not been able to read the script at all.
///
/// Answered by the same [`dealer_run::dd_demand::touches_solver`] the run uses
/// to decide whether to carry answers with its deals, rather than by looking
/// for the words: `t = tricks(north, notrump)` mentions one and `x = t` does
/// not, and a comment mentioning `par` is not a call.
#[wasm_bindgen]
pub fn script_uses_double_dummy(script: &str, params: Vec<String>) -> Option<bool> {
    let params = script_params_from(&params).ok()?;
    let preprocessed = dealer_parser::preprocess_all(script, &params).ok()?;
    let program = dealer_parser::parse_program(&preprocessed).ok()?;
    Some(dealer_run::dd_demand::touches_solver(&program))
}

/// Engine version, so a page can show which build it is running.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ten records of Pavlicek's library, every one of them solved. The same
    /// fixture `dealer-run` reads, so a front end that read it differently
    /// would show up here rather than in a browser.
    const LIBRARY: &[u8] = include_bytes!("../../dealer-run/tests/fixtures/rpdd_10First.zrd");

    /// North's hand in the fixture's first record. Pins that the deals returned
    /// are the file's: a shuffle that happened to produce ten deals would
    /// satisfy every count in these tests and fail this.
    const FIRST_NORTH: &str = "J873.J42.Q65.KT2";

    /// Two deals in the one-line layout, which carries no tables.
    const TWO_ONELINE: &str = concat!(
        "n J873.J42.Q65.KT2 e AT652.A976.AJ82. s Q4.85.KT9.A87643 w K9.KQT3.743.QJ95\n",
        "n QJ3.A93.K4.KT875 e A98.QJT2.97653.2 s T76.K65.AQJ8.J43 w K542.874.T2.AQ96\n",
    );

    /// Run a script over supplied bytes, as the page would.
    fn over(deals: &[u8], script: &str) -> Result<serde_json::Value, String> {
        over_as(deals, script, "oneline")
    }

    /// The same, in a named format — which is the only thing `none` changes.
    fn over_as(deals: &[u8], script: &str, format: &str) -> Result<serde_json::Value, String> {
        let (supplied, report) =
            dealer_run::deals_from_bytes(deals, dealer_run::deal_input::Window::all())?;
        let json = run_script(
            script,
            1,
            1000,
            1_000_000,
            format,
            false,
            false,
            &[],
            None,
            None,
            DealSource::Supplied {
                deals: supplied,
                report,
            },
        )?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }

    /// The `input` block, which every supplied run must carry.
    fn input(result: &serde_json::Value) -> &serde_json::Value {
        let report = &result["input"];
        assert!(!report.is_null(), "a supplied run must report what it read");
        report
    }

    #[test]
    fn the_deals_that_run_are_the_deals_that_were_supplied() {
        let result = over(LIBRARY, "condition 1\n").expect("a library should run");

        assert_eq!(result["generated"], 10, "the fixture holds ten records");
        assert_eq!(result["produced"], 10, "nothing filters them out");
        assert_eq!(
            result["deals"].as_array().map(Vec::len),
            Some(10),
            "every one of them should come back"
        );
        // Not merely ten deals: the file's ten. A shuffle would pass every
        // count above and fail here.
        assert!(
            result["deals"][0]
                .as_str()
                .is_some_and(|deal| deal.contains(FIRST_NORTH)),
            "the first deal should be the file's first record, not a shuffle: {}",
            result["deals"][0]
        );
    }

    #[test]
    fn the_report_says_what_arrived() {
        let result = over(LIBRARY, "condition 1\n").expect("a library should run");
        let report = input(&result);

        assert_eq!(report["format"], "zrd");
        assert_eq!(report["read"], 10);
        assert_eq!(
            report["solved"], 10,
            "every record of this fixture carries its table"
        );
        assert_eq!(report["unsolved"], 0);
        assert_eq!(report["separators"], 0);
        assert_eq!(report["skipped_count"], 0);
        assert_eq!(report["skipped"].as_array().map(Vec::len), Some(0));
        assert_eq!(report["notes"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn a_short_read_is_visible_even_though_the_run_looks_like_a_success() {
        // Half a library — what a download cut off on a record boundary
        // leaves. The run produces five deals and reports no limit hit, so the
        // count read is the only thing that can say the other five never
        // arrived.
        let half = &LIBRARY[..LIBRARY.len() / 2];
        let result = over(half, "condition 1\n").expect("half a library is still a library");

        assert_eq!(result["produced"], 5);
        assert_eq!(
            result["hit_limit"], false,
            "it ran out of deals, not budget"
        );
        assert_eq!(input(&result)["read"], 5, "the count is the only warning");
    }

    #[test]
    fn a_filter_still_applies_to_supplied_deals() {
        // Ground truth from the fixture itself rather than a number written
        // here: what is under test is that the condition is applied to these
        // deals, and a hand-copied count would agree just as well with a filter
        // that had stopped being applied at all.
        let (deals, _) =
            dealer_run::deals_from_bytes(LIBRARY, dealer_run::deal_input::Window::all())
                .expect("read the fixture");
        let expected = deals
            .iter()
            .filter(|(deal, _)| deal.hand(Position::North).hcp() >= 13)
            .count();
        assert!(
            expected > 0 && expected < deals.len(),
            "this test is pointless unless the filter both keeps and rejects: {}",
            expected
        );

        let result = over(LIBRARY, "condition hcp(north) >= 13\n").expect("run over the fixture");
        assert_eq!(result["produced"], expected);
        assert_eq!(result["generated"], 10, "all ten were examined");
        assert_eq!(input(&result)["read"], 10);
    }

    #[test]
    fn text_deals_arrive_unsolved_and_are_counted_as_such() {
        let result = over(TWO_ONELINE.as_bytes(), "condition 1\n").expect("one-line should run");
        let report = input(&result);

        assert_eq!(report["format"], "lines");
        assert_eq!(report["read"], 2);
        assert_eq!(report["solved"], 0, "this layout carries no tables");
        assert_eq!(report["unsolved"], 2);
        assert_eq!(result["produced"], 2);
    }

    #[test]
    fn a_pbn_table_comes_through_as_solved_and_a_board_without_one_does_not() {
        // Two boards, one of them analysed. Read is neither `solved` nor
        // `unsolved` here, which is the point: a run has to be told how many
        // deals it got, and neither count is that number.
        let pbn = "[Board \"1\"]\n\
                   [Deal \"N:QJ3.A93.K4.KT875 A98.QJT2.97653.2 T76.K65.AQJ8.J43 K542.874.T2.AQ96\"]\n\
                   [DoubleDummyTricks \"87879878793555345564\"]\n\
                   \n\
                   [Board \"2\"]\n\
                   [Deal \"N:J873.J42.Q65.KT2 AT652.A976.AJ82. Q4.85.KT9.A87643 K9.KQT3.743.QJ95\"]\n";

        let result = over(pbn.as_bytes(), "condition 1\n").expect("PBN should run");
        let report = input(&result);

        assert_eq!(report["format"], "pbn");
        assert_eq!(report["read"], 2);
        assert_eq!(report["solved"], 1, "the table travelled with its own deal");
        assert_eq!(report["unsolved"], 1, "and not with the other one");
        assert_eq!(result["produced"], 2);
    }

    #[test]
    fn a_deal_that_could_not_be_read_is_named_rather_than_silently_dropped() {
        // The second board is a card short. It is worse than an unreadable one:
        // it would otherwise run, and report statistics over a twelve-card
        // hand, without a word.
        let pbn = "[Board \"1\"]\n\
                   [Deal \"N:QJ3.A93.K4.KT875 A98.QJT2.97653.2 T76.K65.AQJ8.J43 K542.874.T2.AQ96\"]\n\
                   \n\
                   [Board \"2\"]\n\
                   [Deal \"N:J87.J42.Q65.KT2 AT652.A976.AJ82. Q4.85.KT9.A87643 K9.KQT3.743.QJ95\"]\n";

        let result = over(pbn.as_bytes(), "condition 1\n").expect("the good board still runs");
        let report = input(&result);

        assert_eq!(report["read"], 1, "only one board was whole");
        assert_eq!(result["produced"], 1);
        assert_eq!(report["skipped_count"], 1);
        assert_eq!(
            report["skipped"].as_array().map(Vec::len),
            Some(1),
            "and the reason should come with it: {}",
            report["skipped"]
        );
    }

    #[test]
    fn bytes_that_are_neither_format_are_an_error_that_says_so() {
        // A library cut off part way through a record, which is what a
        // connection dropped mid-download leaves.
        let ragged = &LIBRARY[..LIBRARY.len() - 5];
        let error = over(ragged, "condition 1\n").expect_err("this is not readable");
        assert!(
            error.contains("neither a Pavlicek library nor text"),
            "the error should name what was ruled out: {}",
            error
        );
    }

    #[test]
    fn predeal_is_refused_against_supplied_deals() {
        let error = over(LIBRARY, "predeal north SAKQ\ncondition 1\n")
            .expect_err("predeal has nothing to do with deals that arrived dealt");
        assert!(
            error.contains("predeal"),
            "the error should say which statement is the problem: {}",
            error
        );
    }

    #[test]
    fn predeal_still_works_when_the_deals_are_shuffled() {
        // The refusal above has to be about supplied deals rather than about
        // predeal, which the browser has always honoured.
        let json = run_script(
            "predeal north SAKQ\ncondition 1\n",
            1,
            3,
            100_000,
            "oneline",
            false,
            false,
            &[],
            None,
            None,
            DealSource::Shuffled,
        )
        .expect("a shuffled run predeals as it always did");
        let result: serde_json::Value =
            serde_json::from_str(&json).expect("the result should be JSON");
        assert_eq!(result["produced"], 3);
        assert!(
            result["input"].is_null(),
            "a run that read nothing has nothing to report: {}",
            result["input"]
        );
    }

    #[test]
    fn statistics_are_accumulated_over_supplied_deals() {
        // The rest of the machinery has to reach supplied deals too, not just
        // the list of them: an `average` over a file is most of why one is
        // read.
        let (deals, _) =
            dealer_run::deals_from_bytes(LIBRARY, dealer_run::deal_input::Window::all())
                .expect("read the fixture");
        let expected: f64 = deals
            .iter()
            .map(|(deal, _)| deal.hand(Position::North).hcp() as f64)
            .sum::<f64>()
            / deals.len() as f64;

        let result = over(
            LIBRARY,
            "condition 1\naction printoneline, average \"N HCP\" hcp(north)\n",
        )
        .expect("run over the fixture");
        let average = result["averages"][0]["value"]
            .as_f64()
            .expect("an average should come back");
        assert!(
            (average - expected).abs() < 1e-9,
            "average over the file's deals: got {}, expected {}",
            average,
            expected
        );
        assert_eq!(result["averages"][0]["count"], 10);
    }

    /// A script that asks for every kind of statistic there is, so that "the
    /// numbers are untouched" is a claim about all of them.
    const EVERY_STATISTIC: &str = "condition 1\n\
         action printoneline,\n\
         average \"N HCP\" hcp(north),\n\
         frequency \"N HCP\" (hcp(north), 0, 20)\n";

    #[test]
    fn none_keeps_every_statistic_and_not_one_deal() {
        let shown = over_as(LIBRARY, EVERY_STATISTIC, "oneline").expect("one line runs");
        let quiet = over_as(LIBRARY, EVERY_STATISTIC, "none").expect("none runs");

        // The run itself is untouched: the same deals looked at, the same deals
        // matched, and the same numbers over them. `none` decides what is
        // written out, never what is generated or measured.
        assert_eq!(quiet["generated"], shown["generated"]);
        assert_eq!(quiet["produced"], shown["produced"]);
        assert_eq!(quiet["averages"], shown["averages"]);
        assert_eq!(quiet["frequencies"], shown["frequencies"]);
        assert_eq!(quiet["hand_types"], shown["hand_types"]);
        // The page has no stderr, so the input report is the only thing that
        // says what a run over a library actually read. It must survive too.
        assert_eq!(quiet["input"], shown["input"]);

        // And nothing whatever is held or rendered.
        assert_eq!(shown["deals"].as_array().map(Vec::len), Some(10));
        assert_eq!(
            quiet["deals"].as_array().map(Vec::len),
            Some(0),
            "none holds no deals"
        );
        assert_eq!(quiet["deal_types"].as_array().map(Vec::len), Some(0));

        // Said in the result rather than inferred from an empty list: a run
        // that matched nothing has an empty list too, and the page has to tell
        // the two apart to say the right thing about either.
        assert_eq!(shown["renders_deals"], true);
        assert_eq!(quiet["renders_deals"], false);
    }

    #[test]
    fn none_still_writes_what_the_script_itself_printed() {
        // `printes` is a statement the script ran, not a rendering of a deal.
        // A format that shows no hands must not quietly stop it, or a script
        // whose whole output is its own `printes` line would come back blank.
        let script = "condition 1\naction printes(\"N=\", hcp(north), \\n)\n";
        let quiet = over_as(LIBRARY, script, "none").expect("none runs");
        let shown = over_as(LIBRARY, script, "oneline").expect("one line runs");

        assert_eq!(quiet["printes"], shown["printes"]);
        assert!(
            quiet["printes"]
                .as_str()
                .is_some_and(|s| s.lines().count() == 10),
            "one line per deal, as the terminal would have written it: {}",
            quiet["printes"]
        );
    }

    #[test]
    fn an_unknown_format_names_the_ones_that_exist() {
        let error = over_as(LIBRARY, "condition 1\n", "hands").expect_err("no such format");
        for offered in ["oneline", "printall", "pbn", "none"] {
            assert!(
                error.contains(offered),
                "the refusal should offer {offered}: {error}"
            );
        }
    }

    /// What a page asks before offering to fetch a solved-deal library: does
    /// this script ask a double-dummy question at all?
    ///
    /// Read off the parsed program rather than the text, which is the whole
    /// reason it is answered here and not in JavaScript — the last two cases
    /// are ones a search for the words gets wrong in both directions.
    #[test]
    fn a_script_says_whether_it_asks_a_double_dummy_question() {
        let cases: [(&str, Option<bool>, &str); 7] = [
            (
                "condition hcp(north) >= 20\n",
                Some(false),
                "nothing here reaches the solver",
            ),
            (
                "condition tricks(north, notrump) >= 9\n",
                Some(true),
                "a condition that solves",
            ),
            (
                "condition 1\naction printoneline, average \"par\" par(north)\n",
                Some(true),
                "an action that solves",
            ),
            (
                "condition 1\naction printoneline, average \"t\" dds(north, notrump)\n",
                Some(true),
                "dds under its own spelling",
            ),
            (
                "t = tricks(south, hearts)\ncondition t >= 10\n",
                Some(true),
                "through a variable",
            ),
            (
                "# par and tricks are what this script does not do\ncondition hcp(north) >= 4\n",
                Some(false),
                "the words in a comment are not calls",
            ),
            (
                "condition hcp(north) >=\n",
                None,
                "an unreadable script is not a no",
            ),
        ];
        for (script, expected, why) in cases {
            assert_eq!(
                script_uses_double_dummy(script, Vec::new()),
                expected,
                "{why}: {script:?}"
            );
        }
    }
}
