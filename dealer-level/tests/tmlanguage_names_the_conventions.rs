//! Asserts the shipped TextMate grammar knows the levelling conventions.
//!
//! `HandType_` and the rest are ordinary variables to the grammar — that is the
//! point of them, since a script using them still parses on BBO — so nothing
//! but the highlighter can tell an author which names the engine acts on. The
//! grammar is generated from the constants below by
//! `scripts/generate-tmlanguage.py`; this is what stops it falling behind them,
//! the way the word lists fell nineteen functions behind the parser before
//! `tests/tmlanguage_matches_vocabulary.rs` existed.
//!
//! It lives here rather than beside that test because `dealer-parser` cannot
//! see `dealer-level`: the dependency runs the other way.

use dealer_level::*;

const TMLANGUAGE: &str = include_str!("../../dealer-parser/syntaxes/dlr.tmLanguage.json");

fn assert_named(what: &str, text: &str) {
    assert!(
        TMLANGUAGE.contains(text),
        "the {} `{}` is not in dealer-parser/syntaxes/dlr.tmLanguage.json, so the VS Code \
         extension will not colour it.\n\nRegenerate the grammar with:\n  \
         python3 scripts/generate-tmlanguage.py --also-update-vscode",
        what,
        text
    );
}

#[test]
fn the_two_decompositions_are_highlighted() {
    assert_named("hand type prefix", HAND_TYPE_PREFIX);
    assert_named("level type prefix", LEVEL_TYPE_PREFIX);
    assert_named("share suffix", SHARE_SUFFIX);
}

#[test]
fn the_verdict_and_its_placeholder_are_highlighted() {
    for verdict in VERDICTS {
        assert_named("verdict", verdict);
    }
    assert_named("placeholder verdict", NO_LEVELING);
}

#[test]
fn the_generated_block_is_highlighted() {
    // Comments, all three of them — which is what lets a levelled scenario run
    // on BBO, and why nothing else would tell them from an author's aside.
    assert_named("block marker", LEVEL_BEGIN);
    assert_named("block marker", LEVEL_END);
    assert_named("stamp", LEVEL_STAMP);
}
