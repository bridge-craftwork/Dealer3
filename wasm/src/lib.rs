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
use dealer_parser::{EsTerm, Statement, VulnerabilityType};
use dealer_pbn::{
    format_hand_pbn, format_oneline, format_printall, format_printpbn, PbnBoard, Vulnerability,
};
use dealer_run::{MeasureStop, Retained, RunAccumulator};
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
        let weights = dealer_level::leveling_types(&program)
            .map_err(|e| JsError::new(&e))?
            .shares;

        // The scenario with a levelling block in it, which is what gets
        // characterized: the same text the keeps will be written into, so the
        // two cannot describe different scenarios.
        let prepared = dealer_level::insert_leveling_block(script).map_err(|e| JsError::new(&e))?;
        dealer_level::check_leveling_source(&prepared).map_err(|e| JsError::new(&e))?;

        // What the characterizing pass may cost. A page blocks while it deals,
        // so the clock is the real limit here rather than a deal count — the
        // command line can spend a minute on a scenario the browser has to
        // answer in seconds, and the same request would mean very different
        // waits.
        let characterizing_started = now_ms();
        let characterized = run_script(
            Run {
                script: &prepared,
                seed,
                // Not a target: this pass stops when the rarest category is
                // worth dividing by, or when the clock or the deal cap says so.
                produce: usize::MAX,
                max_generate,
                format,
                keep_deals: false,
                interleave: false,
                phase: Phase::Characterizing,
                deadline: Some(characterizing_started + MEASURE_BUDGET_MS),
                replay: &[],
                retain: RETAIN_DEALS,
                skip: 0,
                until_measured: true,
            },
            &progress,
        )?;
        let measure_seconds = (now_ms() - characterizing_started) / 1000.0;

        let measured = measurement(&characterized);
        let natural_joint = joint_of(&characterized);
        let leveled = dealer_level::level_from(
            &prepared,
            &measured,
            &weights,
            None,
            seed,
            // A browser has no patience for the command line's 500 sightings of
            // the rarest type, and refusing outright would teach nothing. The
            // count it managed comes back instead, so the page can say how well
            // the keeps are pinned down.
            MIN_BROWSER_SAMPLE,
        )
        .map_err(|e| JsError::new(&e))?;

        // The deals the page will show. A levelled scenario is the one just
        // characterized with the keeps added, so every deal it can produce is
        // one that pass already dealt — they are re-run from their seeds rather
        // than dealt again, and only a run that wants more than was kept deals
        // anything itself.
        let run = run_script(
            Run {
                script: &leveled.script,
                seed,
                produce,
                // What the characterizing pass did not spend. Both passes walk
                // the same stream, so one budget covers the run.
                max_generate: max_generate.saturating_sub(characterized.generated),
                format,
                keep_deals: true,
                interleave: true,
                phase: Phase::AdditionalDealing,
                deadline: None,
                replay: characterized.retained.seeds(),
                retain: 0,
                skip: characterized.retained.through(),
                until_measured: false,
            },
            &progress,
        )?;

        let rarest = leveled
            .plans
            .iter()
            .min_by(|a, b| a.natural.total_cmp(&b.natural));
        // Per hand type, always: that is what the deals are grouped by and what
        // the reader recognises. Where the two decompositions are the same the
        // plans already say this; where they differ, `planned` comes from how
        // each hand type's deals crossed the levelling categories.
        //
        // It has to be the *characterizing* pass: the producing run has already
        // had the keeps applied, so weighting it by them again counts them
        // twice.
        let planned = dealer_level::group_mix(
            &natural_joint,
            &leveled.plans.iter().map(|p| p.keep).collect::<Vec<_>>(),
        );
        let shares: Vec<HandTypeShare> = run
            .hand_type_names
            .iter()
            .zip(&run.hand_type_counts)
            .enumerate()
            .map(|(i, (name, count))| HandTypeShare {
                name: name.clone(),
                natural: characterized
                    .hand_type_names
                    .iter()
                    .position(|n| n == name)
                    .map(|j| {
                        characterized.hand_type_counts[j] as f64
                            / characterized.produced.max(1) as f64
                    })
                    .unwrap_or(0.0),
                planned: planned.get(i).copied().unwrap_or(0.0),
                delivered: *count as f64 / run.produced.max(1) as f64,
                produced: *count,
                out_of: run.produced,
            })
            .collect();

        let leveling = LevelingResult {
            script: leveled.script,
            shares,
            exactness: leveled.lambda,
            acceptance: leveled.acceptance,
            cost: 1.0 / (leveled.base_rate * leveled.acceptance),
            measured: characterized.produced,
            rarest: rarest.map(|p| p.name.clone()).unwrap_or_default(),
            rarest_seen: rarest.map(|p| p.seen).unwrap_or(0),
            measure_seconds,
            warnings: leveled.warnings.clone(),
            characterized: characterized.generated,
        };
        // Both passes dealt from the one budget, so the count the page shows is
        // both of them. Reporting only the second said 1,701 where the run had
        // dealt 50,000 — and said it next to "levelled over 52,771 measured
        // deals", which is the same deals counted honestly.
        let run = RunOutcome {
            generated: characterized.generated + run.generated,
            ..run
        };
        (run, Some(leveling))
    } else {
        let run = run_script(
            Run {
                script,
                seed,
                produce,
                max_generate,
                format,
                keep_deals: true,
                interleave: false,
                phase: Phase::Dealing,
                deadline: None,
                replay: &[],
                retain: 0,
                skip: 0,
                until_measured: false,
            },
            &progress,
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
/// A levelled run deals the scenario twice, and without this the one bar would
/// appear to finish and start over.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
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
    fn name(self) -> &'static str {
        match self {
            Phase::Characterizing => "characterizing",
            Phase::Dealing => "dealing",
            Phase::AdditionalDealing => "additional dealing",
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

/// How many of the characterizing pass's deals to keep, by seed, for the
/// producing pass to replay.
///
/// Eight bytes each, so this is 8 MB at the very worst and typically a tenth of
/// that — a characterizing pass produces a hundred thousand or so before its
/// rarest category is pinned down. Set high enough that the producing pass
/// almost never has to deal anything itself; when it does, it simply does, and
/// the answer is the same either way.
const RETAIN_DEALS: usize = 1_000_000;

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

/// One `printrpt` row, without its leading space./// One `printrpt` row, without its leading space.
///
/// The same shape the command line writes for `csvrpt` and `printrpt`: strings
/// in single quotes, hands in PBN notation, everything else an integer, commas
/// between. Kept in step with the CLI's `report_row` by
/// `dealer/tests/print_report.rs`, which compares the two.
fn report_row(
    terms: &[dealer_parser::CsvTerm],
    deal: &Deal,
    ctx: &EvalContext,
) -> Result<String, JsError> {
    use dealer_parser::{CsvTerm, Side};
    let mut parts: Vec<String> = Vec::new();
    for term in terms {
        match term {
            CsvTerm::Expression(expr) => {
                let value = eval(expr, ctx)
                    .map_err(|e| JsError::new(&format!("printrpt evaluation error: {}", e)))?;
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

/// The run's hand types crossed with its levelling categories.
///
/// Empty when the two are the same decomposition, in which case a hand type's
/// planned share is simply its own plan and there is nothing to cross.
fn joint_of(run: &RunOutcome) -> Vec<Vec<usize>> {
    if run.level_type_names.is_empty() {
        // One category each, so the crossing is the identity: hand type `i`
        // draws all of its deals from levelling category `i`.
        (0..run.hand_type_names.len())
            .map(|i| {
                let mut row = vec![0usize; run.hand_type_names.len()];
                row[i] = run.hand_type_counts[i];
                row
            })
            .collect()
    } else {
        run.joint.clone()
    }
}

fn measurement(run: &RunOutcome) -> dealer_level::Measurement {
    // The levelling decomposition when the scenario declares one, the hand
    // types otherwise — the same rule the command line follows.
    if run.level_type_names.is_empty() {
        dealer_level::Measurement {
            produced: run.produced,
            generated: run.generated,
            names: run.hand_type_names.clone(),
            counts: run.hand_type_counts.clone(),
            prefix: dealer_level::HAND_TYPE_PREFIX,
            groups: Vec::new(),
            joint: Vec::new(),
        }
    } else {
        dealer_level::Measurement {
            produced: run.produced,
            generated: run.generated,
            names: run.level_type_names.clone(),
            counts: run.level_counts.clone(),
            prefix: dealer_level::LEVEL_TYPE_PREFIX,
            groups: run.hand_type_names.clone(),
            joint: run.joint.clone(),
        }
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
    /// The levelling decomposition's labels, empty unless the scenario declares
    /// `LevelType_` variables of its own.
    level_type_names: Vec<String>,
    /// How many produced deals matched each, parallel to `level_type_names`.
    level_counts: Vec<usize>,
    /// `joint[hand type][level type]` counts, empty unless the two differ.
    joint: Vec<Vec<usize>>,
    /// The seeds of matching deals, for a later pass to replay. Empty unless
    /// the run was asked to keep them.
    retained: Retained,
}

/// One pass over the deals.
///
/// A struct rather than a parameter list: a levelled run needs the same walk
/// with four things varied, and eleven positional arguments had already stopped
/// being readable at the call site.
struct Run<'a> {
    script: &'a str,
    seed: u32,
    /// Deals to produce. `usize::MAX` for a characterizing pass, which stops on
    /// [`Run::until_measured`] instead.
    produce: usize,
    /// Deals to *deal*, which bounds a browser tab that has no Ctrl-C. Replayed
    /// deals do not count against it: they were dealt, and counted, already.
    max_generate: usize,
    format: Format,
    keep_deals: bool,
    interleave: bool,
    phase: Phase,
    /// When to stop regardless of what has been produced. Set for the
    /// characterizing pass, which answers to a clock; `None` for the run the
    /// reader asked for, which answers to the deal count they gave.
    deadline: Option<f64>,
    /// Deals to re-examine before dealing any new ones, by their seeds.
    ///
    /// A levelled scenario is the characterizing pass's scenario with the keeps
    /// added, so every deal it can produce is one that pass already found. Those
    /// deals are therefore re-run here rather than dealt again — the same deals,
    /// since a deal is a pure function of its seed, and the whole levelled
    /// condition is evaluated over them so a scenario whose own condition calls
    /// `rnd()` gets the draws it would have got.
    replay: &'a [u64],
    /// How many matching deals' seeds to keep, for a later pass to replay.
    retain: usize,
    /// Deals the replayed seeds already account for, skipped before dealing
    /// anything new. Without it a pass whose replay ran out would start again
    /// at the first deal and produce every replayed deal a second time.
    skip: usize,
    /// Stop as soon as the levelling decomposition is sampled well enough to
    /// divide by — the only thing a characterizing pass is for.
    until_measured: bool,
}

fn run_script(run: Run, progress: &Progress) -> Result<RunOutcome, JsError> {
    let Run {
        script,
        seed,
        produce,
        max_generate,
        format,
        keep_deals,
        interleave,
        phase,
        deadline,
        replay,
        retain,
        skip,
        until_measured,
    } = run;
    let preprocessed =
        dealer_parser::preprocess_all(script, &Default::default()).map_err(|e| JsError::new(&e))?;
    let program = dealer_parser::parse_program(&preprocessed)
        .map_err(|e| JsError::new(&format!("Parse error: {}", e)))?;

    // Classification, counting and the `average`/`frequency` accumulation are
    // `dealer-run`'s, shared with the command line — including which
    // decomposition gets levelled (the `LevelType_` ones when the scenario
    // declares any, the hand types otherwise) and how many `EvalContext`s a
    // matching deal gets, which is what a `rnd()` in one draws against a
    // `rnd()` in another.
    let mut accumulator =
        RunAccumulator::new(&program, MeasureStop::standard()).map_err(|e| JsError::new(&e))?;
    // The labels, not the variable names: everything downstream — the keeps,
    // the `{{level-mix}}` markers, the badge on a board — is written in terms
    // of `12_14` rather than `HandType_12_14`.
    let hand_type_labels: Vec<String> = accumulator.hand_type_labels().to_vec();
    let level_type_labels: Vec<String> = dealer_level::level_types(&program)
        .iter()
        .map(|n| dealer_level::level_type_label(n).to_string())
        .collect();
    let mut deal_types: Vec<Option<String>> = Vec::new();

    let variables = extract_variables(&program);
    let constraint = extract_constraint(&program);
    let point_counts = extract_point_counts(&program)
        .map_err(|e| JsError::new(&format!("Point count error: {}", e)))?;
    let point_counts = point_counts.as_ref();

    let mut printes_specs: Vec<Vec<EsTerm>> = Vec::new();
    // `printrpt` writes to stdout, which here is the same Text view `printes`
    // reaches. `csvrpt` is not collected: that one writes a file, and a page
    // has nowhere to put it.
    let mut printrpt_specs: Vec<Vec<dealer_parser::CsvTerm>> = Vec::new();
    for statement in &program.statements {
        if let Statement::PrintReport(terms) = statement {
            printrpt_specs.push(terms.clone());
        }
        if let Statement::Action {
            printes,
            print_hands,
            print_reports,
            ..
        } = statement
        {
            printes_specs.extend(printes.iter().cloned());
            printrpt_specs.extend(print_reports.iter().cloned());
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
        }
    }

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
    let mut retained = Retained::new(retain);
    // Deals looked at, replayed and dealt alike, so the clock is read at a
    // steady rate whichever they are.
    let mut examined = 0usize;
    let mut replayed = 0usize;
    // How far the generator has been wound on, replayed deals included.
    let mut stepped = 0usize;

    loop {
        if produced >= produce {
            break;
        }
        // Whatever the characterizing pass kept, before anything new. These
        // deals cost a shuffle rather than a shuffle and a rejected condition,
        // and there are typically far more of them than a run needs.
        let deal_seed = if replayed < replay.len() {
            let seed = replay[replayed];
            replayed += 1;
            seed
        } else {
            // The replayed deals' share of the stream, stepped past the first
            // time a deal has to be dealt rather than replayed. A seed is one
            // step of the generator with no shuffle behind it, so skipping a
            // million costs less than dealing one.
            while stepped < skip {
                generator.next_seed();
                stepped += 1;
            }
            if generated >= max_generate {
                break;
            }
            generated += 1;
            stepped += 1;
            generator.next_seed()
        };
        let deal = if generator.has_predeal() {
            dealer_core::generate_deal_from_seed(deal_seed, generator.config())
        } else {
            dealer_core::generate_deal_from_seed_no_predeal(deal_seed)
        };
        examined += 1;
        // A characterizing pass is as far along as its scarcest category says,
        // so that is what it reports against — a bar that means something,
        // rather than a count with no denominator.
        if until_measured {
            progress.report(
                phase,
                accumulator.rarest_measured(),
                generated,
                dealer_level::MEASURE_GOAL,
                false,
            );
        } else {
            progress.report(phase, produced, generated, produce, false);
        }

        // Checked here because the clock is already being read for the report
        // above, and every few thousand deals rather than every one because a
        // selective condition rejects most of them without a `now_ms()` of its
        // own being worth it.
        if let Some(deadline) = deadline {
            if examined.is_multiple_of(4096) && now_ms() >= deadline {
                break;
            }
        }

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

        // The `average`s, the `frequency`s and both classifications, in the
        // order and the contexts the command line uses them in. Two categories
        // claiming one deal is refused here, not resolved: the types are meant
        // to partition the deals, and a tag that silently picked the first
        // would leave a set wrong about what it holds.
        let matched_type = accumulator
            .observe(&deal, &variables, point_counts)
            .map_err(|e| JsError::new(&e.to_string()))?
            .hand_type;
        retained.offer(deal_seed, generated);

        // `printes` writes to a terminal in the CLI; here it is collected and
        // handed back for the page to show. Capped alongside the deals for the
        // same reason, and by the same count, so the two stay in step.
        if !printrpt_specs.is_empty() && held.len() < MAX_RETURNED_DEALS {
            let ctx = EvalContext::with_counts(&deal, &variables, point_counts);
            for terms in &printrpt_specs {
                printes_output.push(' ');
                printes_output.push_str(&report_row(terms, &deal, &ctx)?);
                printes_output.push('\n');
            }
        }

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

        // Deals are capped independently of `produced` so a large `produce` used
        // purely to gather statistics does not have to ship every deal to JS.
        if keep_deals && held.len() < MAX_RETURNED_DEALS {
            held.push((matched_type, deal.clone()));
        }
        produced += 1;

        // Every category seen enough times to divide by, which is the whole of
        // what a characterizing pass is for. Checked after the deal that
        // finished the job rather than before the next one, so the pass ends on
        // the same deal however it was reached.
        if until_measured && accumulator.measure_satisfied() {
            break;
        }
    }

    // Forced, so a bar reaches its own total. The throttle otherwise leaves the
    // last hundred milliseconds of a phase unreported, and a bar frozen at 76%
    // as the next one starts reads as something having gone wrong.
    if until_measured {
        progress.report(
            phase,
            accumulator.rarest_measured(),
            generated,
            dealer_level::MEASURE_GOAL,
            true,
        );
    } else {
        progress.report(phase, produced, generated, produce, true);
    }

    // Taken before `finish` consumes the accumulator, which is what bins the
    // frequencies.
    let hand_type_counts = accumulator.hand_type_counts().to_vec();
    let level_counts = if level_type_labels.is_empty() {
        Vec::new()
    } else {
        accumulator.leveling_counts().to_vec()
    };
    let joint = accumulator.measurement(generated).joint;
    let stats = accumulator.finish();

    let averages = stats
        .averages
        .into_iter()
        .map(|a| AverageResult {
            is_hand_type: a.is_hand_type,
            label: a.label,
            value: a.value,
            count: a.count,
        })
        .collect();

    let frequencies = stats
        .frequencies
        .into_iter()
        .map(|f| FrequencyResult {
            label: f.label,
            min: f.min,
            max: f.max,
            bins: f
                .bins
                .into_iter()
                .map(|(value, count)| FrequencyBin { value, count })
                .collect(),
            below: f.below,
            above: f.above,
            total: f.total,
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

    Ok(RunOutcome {
        retained,
        hit_limit: produced < produce && generated >= max_generate,
        printes: printes_output,
        produced,
        deals,
        deal_types,
        generated,
        averages,
        frequencies,
        hand_type_names: hand_type_labels.clone(),
        hand_type_counts,
        level_type_names: level_type_labels,
        level_counts,
        joint,
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
