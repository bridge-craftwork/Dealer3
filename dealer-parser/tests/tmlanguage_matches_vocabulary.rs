//! Asserts the shipped TextMate grammar covers the language's full vocabulary.
//!
//! `syntaxes/dlr.tmLanguage.json` drives syntax highlighting in both the VS Code
//! extension and the web editor. It is generated from `vocabulary.rs` by
//! `scripts/generate-tmlanguage.py`; this test is what stops it silently falling
//! behind. Before it existed the grammar was missing 19 functions and the
//! `csvrpt` keyword, and listed two functions that do not exist.

use dealer_parser::vocabulary::*;

const TMLANGUAGE: &str = include_str!("../syntaxes/dlr.tmLanguage.json");

/// Every alternation body in the grammar's `match` patterns, concatenated.
/// Crude, but enough to answer "does this word appear as a highlighted term".
fn highlighted_words() -> Vec<String> {
    let mut words = Vec::new();
    for line in TMLANGUAGE.lines() {
        let Some(start) = line.find("\\\\b(") else {
            continue;
        };
        let rest = &line[start + 4..];
        let Some(end) = rest.find(')') else { continue };
        for w in rest[..end].split('|') {
            let w = w.trim();
            if !w.is_empty() {
                words.push(w.to_string());
            }
        }
    }
    words
}

fn assert_all_covered(what: &str, expected: &[&str]) {
    let words = highlighted_words();
    let missing: Vec<&&str> = expected
        .iter()
        .filter(|w| !words.contains(&w.to_string()))
        .collect();
    assert!(
        missing.is_empty(),
        "{} not highlighted by syntaxes/dlr.tmLanguage.json: {:?}\n\n\
         Regenerate it with:\n  python3 scripts/generate-tmlanguage.py\n\n\
         The grammar is shared with the Practice-Bidding-Scenarios VS Code \
         extension, so a gap here is a gap in both editors.",
        what,
        missing
    );
}

#[test]
fn all_functions_are_highlighted() {
    assert_all_covered("functions", FUNCTIONS);
}

#[test]
fn all_statement_keywords_are_highlighted() {
    assert_all_covered("statement keywords", STATEMENT_KEYWORDS);
}

#[test]
fn all_actions_are_highlighted() {
    assert_all_covered("actions", ACTIONS);
}

#[test]
fn all_logical_words_are_highlighted() {
    assert_all_covered("logical words", LOGICAL_WORDS);
}

#[test]
fn no_phantom_functions() {
    // The reverse direction: the grammar must not advertise functions the parser
    // does not accept. `control` and `imp` were listed for years and are not real.
    let real = all_reserved();
    let words = highlighted_words();
    let suspect: Vec<&String> = words
        .iter()
        .filter(|w| w.chars().all(|c| c.is_ascii_lowercase()) && w.len() > 2)
        .filter(|w| !real.contains(&w.as_str()))
        .collect();
    assert!(
        suspect.is_empty(),
        "syntaxes/dlr.tmLanguage.json highlights words the parser does not accept: {:?}",
        suspect
    );
}
