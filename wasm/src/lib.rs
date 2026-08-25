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

use dealer_core::FastDealGenerator;
use dealer_eval::{eval_with_context, extract_constraint, extract_variables};
use dealer_parser::vocabulary;
use dealer_pbn::{format_oneline, format_printall};
use serde::Serialize;
use wasm_bindgen::prelude::*;

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
}

impl Format {
    fn parse(s: &str) -> Result<Self, JsError> {
        match s.to_ascii_lowercase().as_str() {
            "oneline" | "printoneline" => Ok(Format::OneLine),
            "printall" | "all" => Ok(Format::PrintAll),
            other => Err(JsError::new(&format!(
                "Unknown format '{}'. Use 'oneline' or 'printall'.",
                other
            ))),
        }
    }

    fn render(self, deal: &dealer_core::Deal, index: usize) -> String {
        match self {
            Format::OneLine => format_oneline(deal).trim_end().to_string(),
            Format::PrintAll => format_printall(deal, index),
        }
    }
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

    let preprocessed = dealer_parser::preprocess(script);
    let program = dealer_parser::parse_program(&preprocessed)
        .map_err(|e| JsError::new(&format!("Parse error: {}", e)))?;

    let variables = extract_variables(&program);
    let constraint = extract_constraint(&program);

    let mut generator = FastDealGenerator::new(seed as u64);
    let mut deals = Vec::new();
    let mut generated = 0usize;

    while deals.len() < produce && generated < max_generate {
        let deal = generator.next_deal();
        generated += 1;

        let matched = match constraint {
            Some(expr) => eval_with_context(expr, &variables, &deal)
                .map_err(|e| JsError::new(&format!("Evaluation error: {}", e)))?
                != 0,
            None => true,
        };

        if matched {
            deals.push(format.render(&deal, deals.len()));
        }
    }

    let result = GenerateResult {
        hit_limit: deals.len() < produce && generated >= max_generate,
        produced: deals.len(),
        deals,
        generated,
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
}

/// The language's full vocabulary, for editor completion and hover.
///
/// Comes from `dealer_parser::vocabulary`, which is itself checked against
/// `grammar.pest`, so an editor built on this cannot advertise a function the
/// parser does not accept.
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
    };
    serde_json::to_string(&info).unwrap_or_else(|_| "{}".to_string())
}

/// Engine version, so a page can show which build it is running.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
