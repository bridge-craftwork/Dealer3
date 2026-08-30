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
}

impl Format {
    fn parse(s: &str) -> Result<Self, JsError> {
        match s.to_ascii_lowercase().as_str() {
            "oneline" | "printoneline" => Ok(Format::OneLine),
            "printall" | "all" => Ok(Format::PrintAll),
            "pbn" | "printpbn" => Ok(Format::Pbn),
            other => Err(JsError::new(&format!(
                "Unknown format '{}'. Use 'oneline', 'printall' or 'pbn'.",
                other
            ))),
        }
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
}

/// Script settings that affect output but not generation.
struct OutputContext {
    dealer: Option<Position>,
    vulnerability: Option<Vulnerability>,
    seed: u32,
}

/// Wall-clock milliseconds. `std::time::SystemTime::now()` panics on
/// wasm32-unknown-unknown, so read the clock through JS instead.
fn now_ms() -> f64 {
    js_sys::Date::now()
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
/// **The site does not ship a threaded build**, because more threads currently
/// make it slower rather than faster — see `build.sh`. This is the groundwork
/// and the proof that the engine's side is right, not a switch to flip.
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
    /// Deals it may take, which is the other thing that can stop it short.
    max_generate: usize,
    /// Deals to hand back, capped: a large `produce` used to gather statistics
    /// does not have to ship every deal to JS.
    held: Vec<(Option<usize>, Deal)>,
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
    /// Rough on purpose, and it firms up within the first moment. A bar drawn
    /// against 2,000 that ends at 61 looks broken; the same bar with the mark
    /// at 63 says the run is doing what it can and will not reach the goal,
    /// which is the thing worth knowing.
    fn reachable(&self, seen: usize, generated: usize, goal: usize) -> usize {
        if seen == 0 || generated == 0 {
            return goal;
        }
        let by_deals = seen as f64 * self.max_generate as f64 / generated as f64;
        let spent = (now_ms() - self.characterizing_started).max(1.0);
        let budget = (self.deadline - self.characterizing_started).max(1.0);
        let by_clock = seen as f64 * budget / spent;
        (by_deals.min(by_clock).round() as usize).clamp(seen.max(1), goal)
    }
}

impl RunHost for Page<'_> {
    fn should_stop(
        &mut self,
        phase: Phase,
        produced: usize,
        generated: usize,
        target: usize,
    ) -> bool {
        let expected = if phase == Phase::Characterizing {
            self.reachable(produced, generated, target)
        } else {
            target
        };
        self.progress
            .report(phase, produced, generated, target, expected, false);
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
        if self.held.len() >= MAX_RETURNED_DEALS {
            return Ok(());
        }
        for row in deal.rows()?.printed {
            self.printed.push(' ');
            self.printed.push_str(&row);
            self.printed.push('\n');
        }
        self.printed.push_str(&deal.printes()?);
        self.held.push((deal.hand_type, deal.deal.clone()));
        Ok(())
    }
}

/// Generate deals from a script, as JSON.
///
/// With `auto_level`, the engine characterizes the scenario first — how often
/// each `HandType_*` comes up — works out a keep rate for each and deals the
/// levelled copy. Both passes are the engine's business; what comes back is
/// the deals and the numbers behind them.
#[wasm_bindgen]
pub fn generate(
    script: &str,
    seed: u32,
    produce: usize,
    max_generate: usize,
    format: &str,
    auto_level: bool,
    on_progress: Option<js_sys::Function>,
) -> Result<String, JsError> {
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

    let preprocessed =
        dealer_parser::preprocess_all(script, &Default::default()).map_err(|e| JsError::new(&e))?;
    let program = dealer_parser::parse_program(&preprocessed)
        .map_err(|e| JsError::new(&format!("Parse error: {}", e)))?;

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
            Statement::Predeal { position, cards } => predeal
                .predeal(*position, cards)
                .map_err(|e| JsError::new(&format!("Predeal error: {}", e)))?,
            // `print` is a paginated hand record with form feeds, written for a
            // line printer. There is nowhere for that to go on a page, and
            // quietly dropping it would leave a script looking as though it had
            // run.
            Statement::Action { print_hands, .. } if !print_hands.is_empty() => {
                return Err(JsError::new(
                    "print(...) writes a paginated hand record for a printer and is not \
                     available in the browser",
                ))
            }
            _ => {}
        }
    }

    let mut page = Page {
        progress: &progress,
        deadline: started + MEASURE_BUDGET_MS,
        max_generate,
        held: Vec::new(),
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
            deals: Deals::Shuffled {
                predeal,
                swap: dealer_core::SwapMode::None,
            },
            // Whatever the caller started a pool with, and one if it did not
            // — see `start_threads`. A thread count cannot change what comes
            // out, only how long it takes, so a page that cannot spawn any
            // gets the same deals more slowly.
            threads: threads_available(),
            batch: 0,
            params: Default::default(),
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
                measure_cap: max_generate,
            }),
        },
        &mut page,
    )
    .map_err(|e| JsError::new(&e.to_string()))?;

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
            })
            .collect(),
        None => report
            .hand_types
            .iter()
            .map(|(name, count)| {
                let share = *count as f64 / report.produced.max(1) as f64;
                HandTypeShare {
                    name: name.clone(),
                    natural: share,
                    planned: share,
                    delivered: share,
                    produced: *count,
                    out_of: report.produced,
                }
            })
            .collect(),
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
            })
            .collect(),
        printes: page.printed,
        hand_types,
        leveling,
        seconds: (now_ms() - started) / 1000.0,
    };
    serde_json::to_string(&result).map_err(|e| JsError::new(&e.to_string()))
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

/// How long the browser will go on characterizing a scenario.
///
/// A page blocks while it deals, so this is a clock rather than a deal count —
/// which is also what lets one number serve every scenario. Falling short of
/// the goal is not an error: the count reached comes back and the panel says
/// what it was.
const MEASURE_BUDGET_MS: f64 = 6_000.0;

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
pub fn check_script(script: &str) -> String {
    let preprocessed = match dealer_parser::preprocess_all(script, &Default::default()) {
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
/// `hand_type_prefix` and `level_type_prefix` are matched with regard to case,
/// as `dealer_level` matches them; `share_suffix` is not, as `dealer_level`
/// does not. A highlighter that follows suit shows an author the difference.
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

/// Engine version, so a page can show which build it is running.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
