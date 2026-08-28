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

use dealer_core::{Deal, FastDealConfig, FastDealGenerator, Position};
use dealer_eval::{
    eval, eval_with_context_and_counts, extract_constraint, extract_point_counts,
    extract_variables, EvalContext,
};
use dealer_parser::vocabulary;
use dealer_parser::{EsTerm, Expr, Statement, VulnerabilityType};
use dealer_pbn::{format_oneline, format_printall, format_printpbn, PbnBoard, Vulnerability};
use serde::Serialize;
use std::collections::HashMap;
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
#[derive(Serialize)]
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
    /// The rarest type's count in the measuring pass, and what that is worth as
    /// a relative error — the precision of the whole levelling rests on it.
    rarest: String,
    rarest_seen: usize,
    /// Wall-clock seconds spent measuring, which is the pass the reader did not
    /// ask for and cannot otherwise account for.
    measure_seconds: f64,
    /// Anything worth saying out loud that does not make the levelling wrong —
    /// a measurement thinner than the goal, above all.
    warnings: Vec<String>,
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

    let shares_of = |run: &RunOutcome| -> Vec<HandTypeShare> {
        run.hand_type_names
            .iter()
            .zip(&run.hand_type_counts)
            .map(|(name, count)| {
                let share = *count as f64 / run.produced.max(1) as f64;
                HandTypeShare {
                    name: name.clone(),
                    natural: share,
                    planned: share,
                    delivered: share,
                    produced: *count,
                    out_of: run.produced,
                }
            })
            .collect()
    };

    let (run, leveling) = if auto_level {
        // The target mix comes out of the script, exactly as it does on the
        // command line: `HandType_22_24_Share = 3` and nothing else. That is
        // why the page needs no control for it — a scenario carries its own
        // intended mix, and the two front ends cannot drift apart.
        let program = dealer_parser::parse_program(
            &dealer_parser::preprocess_all(script, &Default::default())
                .map_err(|e| JsError::new(&e))?,
        )
        .map_err(|e| JsError::new(&format!("Parse error: {}", e)))?;
        let weights = dealer_level::hand_type_shares(&program).map_err(|e| JsError::new(&e))?;

        let opts = dealer_level::LevelOptions {
            target: &weights,
            budget: None,
            seed,
            // A browser has no patience for the command line's 500 sightings of
            // the rarest type, and refusing outright would teach nothing. The
            // count it managed comes back instead, so the page can say how well
            // the keeps are pinned down.
            min_sample: MIN_BROWSER_SAMPLE,
            probe_produce: PROBE_PRODUCE.min(max_generate),
            measure_cap: max_generate,
        };
        // What the second pass may cost. A page blocks while it deals, so the
        // clock is the real limit here rather than a deal count — the command
        // line can spend a minute on a scenario the browser has to answer in
        // seconds, and the same request would mean very different waits.
        let measure_deadline = now_ms() + MEASURE_BUDGET_MS;
        let measured_per_ms = std::cell::Cell::new(0.0f64);
        // Reported apart from the run itself. Levelling deals a scenario twice
        // — once to find out what it does, once to do it — and a single total
        // makes the second look slow when most of the wait was the first.
        let measure_ms = std::cell::Cell::new(0.0f64);
        let measured_passes = std::cell::Cell::new(0u32);
        let (run, report) = dealer_level::level_and_run(
            script,
            &opts,
            // Measuring: counts only. These deals exist to be characterised and
            // thrown away, so none of them is rendered.
            |script, measure_produce| {
                // Clamped to what the clock allows, judged from how fast the
                // probe went. Returning fewer than asked for is expected: the
                // levelling reads the counts that come back, not the request.
                let left = (measure_deadline - now_ms()).max(0.0);
                let rate = measured_per_ms.get();
                let affordable = if rate > 0.0 {
                    ((left * rate) as usize).max(1)
                } else {
                    measure_produce
                };
                let asked = measure_produce.min(affordable);

                let started = now_ms();
                let outcome =
                    run_script(
                        script,
                        seed,
                        asked,
                        max_generate,
                        format,
                        false,
                        false,
                        &progress,
                        // The first call is the probe; anything after it is
                        // the real measurement, whose size the probe decided.
                        if measured_passes.get() == 0 {
                            Phase::Probe
                        } else {
                            Phase::Measuring
                        },
                    )
                    .map_err(|e| error_text(&e))?;
                let spent = (now_ms() - started).max(1.0);
                measured_per_ms.set(outcome.produced as f64 / spent);
                measure_ms.set(measure_ms.get() + spent);
                measured_passes.set(measured_passes.get() + 1);
                Ok(measurement(&outcome))
            },
            // Producing: the deals the page will show, interleaved.
            |script| {
                let outcome =
                    run_script(
                        script,
                        seed,
                        produce,
                        max_generate,
                        format,
                        true,
                        true,
                        &progress,
                        Phase::Dealing,
                    )
                    .map_err(|e| error_text(&e))?;
                let measured = measurement(&outcome);
                Ok((outcome, measured))
            },
        )
        .map_err(|e| JsError::new(&e))?;

        let rarest = report
            .plans
            .iter()
            .min_by(|a, b| a.natural.total_cmp(&b.natural));
        let leveling = LevelingResult {
            script: report.script,
            shares: report
                .shares
                .iter()
                .map(|s| HandTypeShare {
                    name: s.name.clone(),
                    natural: s.natural,
                    planned: s.planned,
                    delivered: s.delivered,
                    produced: s.produced,
                    out_of: s.out_of,
                })
                .collect(),
            exactness: report.lambda,
            acceptance: report.acceptance,
            cost: report.cost,
            measured: report.measured,
            rarest: rarest.map(|p| p.name.clone()).unwrap_or_default(),
            rarest_seen: rarest.map(|p| p.seen).unwrap_or(0),
            measure_seconds: measure_ms.get() / 1000.0,
            warnings: report.warnings.clone(),
        };
        (run, Some(leveling))
    } else {
        let run = run_script(
            script,
            seed,
            produce,
            max_generate,
            format,
            true,
            false,
            &progress,
            Phase::Dealing,
        )?;
        (run, None)
    };

    let hand_types = match &leveling {
        Some(leveled) => leveled
            .shares
            .iter()
            .map(|s| HandTypeShare {
                name: s.name.clone(),
                natural: s.natural,
                planned: s.planned,
                delivered: s.delivered,
                produced: s.produced,
                out_of: s.out_of,
            })
            .collect(),
        None => shares_of(&run),
    };

    let result = GenerateResult {
        deals: run.deals,
        deal_types: run.deal_types,
        generated: run.generated,
        produced: run.produced,
        hit_limit: run.hit_limit,
        averages: run.averages,
        frequencies: run.frequencies,
        printes: run.printes,
        hand_types,
        leveling,
        seconds: (now_ms() - started) / 1000.0,
    };
    serde_json::to_string(&result).map_err(|e| JsError::new(&e.to_string()))
}

/// How many deals to measure a levelling over in a browser.
///
/// The rarest hand type sets the precision of the whole thing, so this is the
/// number that matters. Ten thousand is a second or two and pins a type of a
/// few percent to within a point; the page reports where it actually landed.
/// Reports how far a run has got, for a caller that can paint it.
///
/// Throttled by the clock rather than by a deal count: a bare `hcp` condition
/// and one calling `tricks()` differ by orders of magnitude in how long a deal
/// takes, so any fixed count is either a flood of messages or a long silence.
struct Progress {
    to: Option<js_sys::Function>,
    /// When the last report went out, so they arrive at a readable rate.
    last_ms: std::cell::Cell<f64>,
}

/// Which pass a report belongs to.
///
/// A levelled run deals the scenario up to three times, and without this the
/// one bar would appear to finish and start over.
#[derive(Clone, Copy)]
enum Phase {
    /// Finding out how rare the rarest hand type is.
    Probe,
    /// Measuring it properly, now that we know how much that takes.
    Measuring,
    /// Producing the deals that were actually asked for.
    Dealing,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Probe => "probe",
            Phase::Measuring => "measuring",
            Phase::Dealing => "dealing",
        }
    }
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
    fn report(&self, phase: Phase, produced: usize, generated: usize, target: usize, force: bool) {
        let Some(to) = &self.to else { return };
        let now = now_ms();
        if !force && now - self.last_ms.get() < PROGRESS_EVERY_MS {
            return;
        }
        self.last_ms.set(now);

        let message = format!(
            r#"{{"phase":"{}","produced":{},"generated":{},"target":{}}}"#,
            phase.name(),
            produced,
            generated,
            target
        );
        // A caller that throws is not worth stopping the run for: the deals are
        // the point and the bar is decoration.
        let _ = to.call1(&wasm_bindgen::JsValue::NULL, &message.into());
    }
}

const PROBE_PRODUCE: usize = 10_000;

/// How long the browser will go on measuring after the probe.
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

/// A `JsError` has no readable message on the Rust side, so errors crossing
/// into the engine's closures carry their text instead.
fn error_text(error: &JsError) -> String {
    let value: wasm_bindgen::JsValue = error.clone().into();
    value
        .as_string()
        .or_else(|| js_sys::Reflect::get(&value, &"message".into()).ok()?.as_string())
        .unwrap_or_else(|| "the run failed".to_string())
}

/// The counts a levelling needs, taken from a run.
fn measurement(run: &RunOutcome) -> dealer_level::Measurement {
    dealer_level::Measurement {
        produced: run.produced,
        generated: run.generated,
        names: run.hand_type_names.clone(),
        counts: run.hand_type_counts.clone(),
    }
}

/// One pass over the deals: what a run produces, before any levelling.
struct RunOutcome {
    deals: Vec<String>,
    /// The hand type each returned deal matched, parallel to `deals`.
    deal_types: Vec<Option<String>>,
    generated: usize,
    produced: usize,
    averages: Vec<AverageResult>,
    frequencies: Vec<FrequencyResult>,
    printes: String,
    hit_limit: bool,
    /// The script's `HandType_*` variables, in declaration order.
    hand_type_names: Vec<String>,
    /// How many produced deals matched each, parallel to `hand_type_names`.
    hand_type_counts: Vec<usize>,
}

/// Run a script once.
///
/// `max_generate` bounds the work: a browser tab has no Ctrl-C, so a selective
/// filter must not be able to hang it.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn run_script(
    script: &str,
    seed: u32,
    produce: usize,
    max_generate: usize,
    format: Format,
    keep_deals: bool,
    interleave: bool,
    progress: &Progress,
    phase: Phase,
) -> Result<RunOutcome, JsError> {

    let preprocessed = dealer_parser::preprocess_all(script, &Default::default()).map_err(|e| JsError::new(&e))?;
    let program = dealer_parser::parse_program(&preprocessed)
        .map_err(|e| JsError::new(&format!("Parse error: {}", e)))?;

    // The script's categories of hand. A naming convention rather than syntax,
    // so a script using it still parses on BBO.
    let hand_type_names = dealer_level::hand_types(&program);
    let hand_type_labels: Vec<String> = hand_type_names
        .iter()
        .map(|n| dealer_level::hand_type_label(n).to_string())
        .collect();
    let mut hand_type_counts = vec![0usize; hand_type_names.len()];
    let mut deal_types: Vec<Option<String>> = Vec::new();

    let variables = extract_variables(&program);
    let constraint = extract_constraint(&program);
    let point_counts = extract_point_counts(&program)
        .map_err(|e| JsError::new(&format!("Point count error: {}", e)))?;
    let point_counts = point_counts.as_ref();

    // `average` and `frequency` accumulate over matching deals only, mirroring
    // the CLI. Collected up front so the per-deal loop stays a tight walk.
    let mut averages: Vec<(Option<String>, Expr, f64, usize)> = Vec::new();
    let mut freqs: Vec<(
        Option<String>,
        Expr,
        HashMap<i32, usize>,
        Option<(i32, i32)>,
    )> = Vec::new();
    let mut printes_specs: Vec<Vec<EsTerm>> = Vec::new();
    for statement in &program.statements {
        if let Statement::Action {
            averages: avg_specs,
            frequencies: freq_specs,
            printes,
            print_hands,
            ..
        } = statement
        {
            printes_specs.extend(printes.iter().cloned());
            // `print` is a paginated hand record with form feeds, written for a
            // line printer. There is nowhere for that to go on a page, and
            // quietly dropping it would leave a script looking as though it had
            // run.
            if !print_hands.is_empty() {
                return Err(JsError::new(
                    "print(...) writes a paginated hand record for a printer and is not \
                     available in the browser",
                ));
            }
            for a in avg_specs {
                averages.push((a.label.clone(), a.expr.clone(), 0.0, 0));
            }
            for f in freq_specs {
                freqs.push((f.label.clone(), f.expr.clone(), HashMap::new(), f.range));
            }
        }
    }
    let collecting_stats = !averages.is_empty() || !freqs.is_empty();

    // `dealer` and `vulnerable` statements do not affect which deals are
    // produced, only how they are labelled in PBN output.
    let mut output = OutputContext {
        dealer: None,
        vulnerability: None,
        seed,
    };
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
            _ => {}
        }
    }

    // Predeal fixes cards before shuffling. Missing this silently produced deals
    // that ignored the script's `predeal` lines, which verify.mjs caught by
    // diffing against the CLI.
    let mut predeal_config = FastDealConfig::new();
    let mut has_predeal = false;
    for statement in &program.statements {
        if let Statement::Predeal { position, cards } = statement {
            predeal_config
                .predeal(*position, cards)
                .map_err(|e| JsError::new(&format!("Predeal error: {}", e)))?;
            has_predeal = true;
        }
    }
    debug_assert!(
        !has_predeal
            || [
                Position::North,
                Position::East,
                Position::South,
                Position::West
            ]
            .iter()
            .any(|p| predeal_config.predeal_count(*p) > 0)
    );

    let mut generator = if has_predeal {
        FastDealGenerator::with_config(seed as u64, predeal_config)
    } else {
        FastDealGenerator::new(seed as u64)
    };
    // Held rather than rendered as they come: a board's number belongs to where
    // it lands, and interleaving does not decide that until every deal is in.
    let mut held: Vec<(Option<usize>, Deal)> = Vec::new();
    let mut printes_output = String::new();
    let mut generated = 0usize;
    let mut produced = 0usize;

    while produced < produce && generated < max_generate {
        let deal = generator.next_deal();
        generated += 1;
        progress.report(phase, produced, generated, produce, false);

        let matched = match constraint {
            Some(expr) => {
                eval_with_context_and_counts(expr, &variables, &deal, point_counts)
                    .map_err(|e| JsError::new(&format!("Evaluation error: {}", e)))?
                    != 0
            }
            None => true,
        };
        if !matched {
            continue;
        }

        if collecting_stats {
            let ctx = EvalContext::with_counts(&deal, &variables, point_counts);
            for (_, expr, sum, count) in averages.iter_mut() {
                let v = eval(expr, &ctx)
                    .map_err(|e| JsError::new(&format!("Average evaluation error: {}", e)))?;
                *sum += v as f64;
                *count += 1;
            }
            for (_, expr, histogram, _) in freqs.iter_mut() {
                let v = eval(expr, &ctx)
                    .map_err(|e| JsError::new(&format!("Frequency evaluation error: {}", e)))?;
                *histogram.entry(v).or_insert(0) += 1;
            }
        }

        // `printes` writes to a terminal in the CLI; here it is collected and
        // handed back for the page to show. Capped alongside the deals for the
        // same reason, and by the same count, so the two stay in step.
        if !printes_specs.is_empty() && held.len() < MAX_RETURNED_DEALS {
            let ctx = EvalContext::with_counts(&deal, &variables, point_counts);
            for terms in &printes_specs {
                for term in terms {
                    match term {
                        EsTerm::String(text) => printes_output.push_str(text),
                        EsTerm::Newline => printes_output.push('\n'),
                        EsTerm::Expression(expr) => {
                            let value = eval(expr, &ctx).map_err(|e| {
                                JsError::new(&format!("printes evaluation error: {}", e))
                            })?;
                            printes_output.push_str(&value.to_string());
                        }
                    }
                }
            }
        }

        // Which category of hand this is. Two matching is refused, as in the
        // CLI: the types are meant to partition the deals, and a tag that
        // silently picked the first would leave a set wrong about what it holds.
        let mut matched_type: Option<usize> = None;
        if !hand_type_names.is_empty() {
            let ctx = EvalContext::with_counts(&deal, &variables, point_counts);
            for (i, name) in hand_type_names.iter().enumerate() {
                let value = eval(&Expr::Variable(name.to_string()), &ctx).map_err(|e| {
                    JsError::new(&format!("Hand type `{}` could not be evaluated: {}", name, e))
                })?;
                if value != 0 {
                    if let Some(first) = matched_type {
                        return Err(JsError::new(&format!(
                            "A deal is both `{}` and `{}`. Hand types have to partition the \
                             deals, so at most one may match.",
                            hand_type_names[first], name
                        )));
                    }
                    matched_type = Some(i);
                }
            }
            if let Some(i) = matched_type {
                hand_type_counts[i] += 1;
            }
        }

        // Deals are capped independently of `produced` so a large `produce` used
        // purely to gather statistics does not have to ship every deal to JS.
        if keep_deals && held.len() < MAX_RETURNED_DEALS {
            held.push((matched_type, deal.clone()));
        }
        produced += 1;
    }

    let averages = averages
        .into_iter()
        .map(|(label, expr, sum, count)| AverageResult {
            is_hand_type: dealer_level::mentions_hand_type(&expr),
            label,
            value: if count > 0 { sum / count as f64 } else { 0.0 },
            count,
        })
        .collect();

    let frequencies = freqs
        .into_iter()
        .map(|(label, _, histogram, range)| {
            let total: usize = histogram.values().sum();
            let (lo, hi) = match range {
                Some((lo, hi)) => (lo, hi),
                None => (
                    histogram.keys().copied().min().unwrap_or(0),
                    histogram.keys().copied().max().unwrap_or(0),
                ),
            };
            let bins = (lo..=hi)
                .map(|value| FrequencyBin {
                    value,
                    count: histogram.get(&value).copied().unwrap_or(0),
                })
                .collect();
            let below = histogram
                .iter()
                .filter(|(&k, _)| k < lo)
                .map(|(_, &v)| v)
                .sum();
            let above = histogram
                .iter()
                .filter(|(&k, _)| k > hi)
                .map(|(_, &v)| v)
                .sum();
            FrequencyResult {
                label,
                min: range.map(|(lo, _)| lo),
                max: range.map(|(_, hi)| hi),
                bins,
                below,
                above,
                total,
            }
        })
        .collect();

    // Interleaved, a set walks through the categories rather than meeting them
    // as they fall. Numbered by where they land, so a reader that sorts on the
    // board number cannot quietly undo the ordering.
    let order: Vec<usize> = if interleave && !hand_type_labels.is_empty() {
        let mut buckets: Vec<(Option<String>, Vec<usize>)> = Vec::new();
        for (index, (matched, _)) in held.iter().enumerate() {
            let label = matched.map(|i| hand_type_labels[i].clone());
            match buckets.iter_mut().find(|(name, _)| *name == label) {
                Some((_, deals)) => deals.push(index),
                None => buckets.push((label, vec![index])),
            }
        }
        let labels: Vec<&str> = hand_type_labels.iter().map(String::as_str).collect();
        dealer_level::interleave(&labels, buckets, seed as u64)
    } else {
        (0..held.len()).collect()
    };
    let mut deals = Vec::with_capacity(order.len());
    for (position, index) in order.into_iter().enumerate() {
        let (matched, deal) = &held[index];
        let label = matched.map(|i| hand_type_labels[i].as_str());
        deals.push(format.render(deal, position, &output, label));
        deal_types.push(label.map(str::to_string));
    }

    // Forced, so a bar reaches its own total. The throttle otherwise leaves the
    // last hundred milliseconds of a phase unreported, and a bar frozen at 76%
    // as the next one starts reads as something having gone wrong.
    progress.report(phase, produced, generated, produce, true);

    Ok(RunOutcome {
        hit_limit: produced < produce && generated >= max_generate,
        printes: printes_output,
        produced,
        deals,
        deal_types,
        generated,
        averages,
        frequencies,
        // The labels, not the variable names: everything downstream — the
        // keeps, the `{{level-mix}}` markers, the badge on a board — is written
        // in terms of `12_14` rather than `HandType_12_14`.
        hand_type_names: hand_type_labels.clone(),
        hand_type_counts,
    })
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
        Ok(_) => CheckResult {
            ok: true,
            error: None,
            line: None,
            column: None,
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
    };
    serde_json::to_string(&info).unwrap_or_else(|_| "{}".to_string())
}

/// Engine version, so a page can show which build it is running.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
