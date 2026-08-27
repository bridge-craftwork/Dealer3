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

use dealer_core::{FastDealConfig, FastDealGenerator, Position};
use dealer_eval::{
    eval, eval_with_context_and_counts, extract_constraint, extract_point_counts,
    extract_variables, EvalContext,
};
use dealer_parser::vocabulary;
use dealer_parser::{EsTerm, Expr, Statement, VulnerabilityType};
use dealer_pbn::{format_oneline, format_printall, format_printpbn, Vulnerability};
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

    fn render(self, deal: &dealer_core::Deal, index: usize, ctx: &OutputContext) -> String {
        match self {
            Format::OneLine => format_oneline(deal).trim_end().to_string(),
            Format::PrintAll => format_printall(deal, index),
            // Board numbers, dealer and vulnerability all belong in the PBN
            // tags; a file without them is far less useful to whatever opens it.
            Format::Pbn => format_printpbn(
                deal,
                index,
                ctx.dealer,
                ctx.vulnerability,
                None,
                Some(ctx.seed),
                None,
            ),
        }
    }
}

/// One `average "label" expr` result.
#[derive(Serialize)]
struct AverageResult {
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
/// `max_generate` bounds the work: a browser tab has no Ctrl-C, so a selective
/// filter must not be able to hang it. Callers should surface `hit_limit` rather
/// than silently showing a short result.
#[wasm_bindgen]
pub fn generate(
    script: &str,
    seed: u32,
    produce: usize,
    max_generate: usize,
    format: &str,
) -> Result<String, JsError> {
    let format = Format::parse(format)?;
    let started = now_ms();

    let preprocessed = dealer_parser::preprocess(script);
    let program = dealer_parser::parse_program(&preprocessed)
        .map_err(|e| JsError::new(&format!("Parse error: {}", e)))?;

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
    let mut deals = Vec::new();
    let mut printes_output = String::new();
    let mut generated = 0usize;
    let mut produced = 0usize;

    while produced < produce && generated < max_generate {
        let deal = generator.next_deal();
        generated += 1;

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
        if !printes_specs.is_empty() && deals.len() < MAX_RETURNED_DEALS {
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
        if deals.len() < MAX_RETURNED_DEALS {
            deals.push(format.render(&deal, produced, &output));
        }
        produced += 1;
    }

    let averages = averages
        .into_iter()
        .map(|(label, _, sum, count)| AverageResult {
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

    let result = GenerateResult {
        hit_limit: produced < produce && generated >= max_generate,
        printes: printes_output,
        produced,
        deals,
        generated,
        averages,
        frequencies,
        seconds: (now_ms() - started) / 1000.0,
    };
    serde_json::to_string(&result).map_err(|e| JsError::new(&e.to_string()))
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
    let preprocessed = dealer_parser::preprocess(script);
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
