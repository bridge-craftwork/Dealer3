//! The language's vocabulary, in one place.
//!
//! Editors need the same word lists the parser recognises: syntax highlighting,
//! completion, and hover all depend on them. Keeping a second copy in a
//! TextMate grammar or an editor plugin means it drifts — and it did. Before
//! this module existed, `dlr.tmLanguage.json` was missing 19 functions
//! (`tens`, `jacks`, `queens`, `kings`, `aces`, `top2`..`top5`, `pt0`..`pt9`),
//! missing the `csvrpt` keyword, and listing two functions that do not exist
//! (`control`, `imp`).
//!
//! These lists are the source of truth. `tests/vocabulary_matches_grammar.rs`
//! asserts they agree with `grammar.pest`, and
//! `tests/tmlanguage_matches_vocabulary.rs` asserts the shipped TextMate grammar
//! agrees with them, so a new function cannot be added to the parser without the
//! editors noticing.

/// Functions callable in an expression, e.g. `hcp(north)`.
pub const FUNCTIONS: &[&str] = &[
    // Hand evaluation
    "hcp", "controls", "losers", "loser", "quality", "cccc",
    // Shape and specific cards
    "shape", "hascard",
    // Suit lengths — plural and singular forms are both accepted
    "spades", "hearts", "diamonds", "clubs", "spade", "heart", "diamond", "club",
    // Named point counts
    "tens", "jacks", "queens", "kings", "aces", "top2", "top3", "top4", "top5", "c13",
    // Indexed point counts
    "pt0", "pt1", "pt2", "pt3", "pt4", "pt5", "pt6", "pt7", "pt8", "pt9",
    // Double-dummy and scoring
    "tricks", "score", "imps",
];

/// Statement keywords that introduce a directive.
pub const STATEMENT_KEYWORDS: &[&str] = &[
    "condition",
    "produce",
    "generate",
    "action",
    "dealer",
    "vulnerable",
    "predeal",
    "csvrpt",
    "average",
    "frequency",
];

/// Output actions, valid inside `action` or standalone.
pub const ACTIONS: &[&str] = &[
    "printall",
    "printew",
    "printpbn",
    "printcompact",
    "printoneline",
];

/// Compass positions. Single letters are accepted but may be shadowed by a
/// variable of the same name, which the evaluator allows deliberately.
pub const POSITIONS: &[&str] = &["north", "south", "east", "west", "n", "s", "e", "w"];

/// Vulnerability settings.
pub const VULNERABILITIES: &[&str] = &["none", "ns", "ew", "all"];

/// Word-form logical operators, alternatives to `&&`, `||` and `!`.
pub const LOGICAL_WORDS: &[&str] = &["and", "or", "not"];

/// Other reserved words: `any` introduces a shape pattern, `deal` is a csvrpt term.
pub const OTHER_KEYWORDS: &[&str] = &["any", "deal"];

/// Symbolic operators, longest-first so a tokenizer matching in order does not
/// split `>=` into `>` and `=`.
pub const OPERATORS: &[&str] = &[
    "==", "!=", ">=", "<=", "&&", "||", ">", "<", "!", "?", ":", "+", "-", "*", "/", "%", "=",
];

/// Every reserved word, for "is this an identifier or a keyword" checks.
pub fn all_reserved() -> Vec<&'static str> {
    let mut v = Vec::new();
    for list in [
        FUNCTIONS,
        STATEMENT_KEYWORDS,
        ACTIONS,
        POSITIONS,
        VULNERABILITIES,
        LOGICAL_WORDS,
        OTHER_KEYWORDS,
    ] {
        v.extend_from_slice(list);
    }
    v.sort_unstable();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_duplicates_within_a_list() {
        for (name, list) in [
            ("FUNCTIONS", FUNCTIONS),
            ("STATEMENT_KEYWORDS", STATEMENT_KEYWORDS),
            ("ACTIONS", ACTIONS),
            ("POSITIONS", POSITIONS),
            ("VULNERABILITIES", VULNERABILITIES),
            ("LOGICAL_WORDS", LOGICAL_WORDS),
            ("OTHER_KEYWORDS", OTHER_KEYWORDS),
            ("OPERATORS", OPERATORS),
        ] {
            let mut seen = list.to_vec();
            seen.sort_unstable();
            let before = seen.len();
            seen.dedup();
            assert_eq!(before, seen.len(), "{} contains duplicates", name);
        }
    }

    #[test]
    fn operators_are_longest_first() {
        // A tokenizer trying these in order must not match `>` before `>=`.
        for (i, op) in OPERATORS.iter().enumerate() {
            for longer in &OPERATORS[i + 1..] {
                assert!(
                    !longer.starts_with(op),
                    "`{}` precedes `{}`, which it is a prefix of",
                    op,
                    longer
                );
            }
        }
    }

    #[test]
    fn all_reserved_covers_every_list() {
        let all = all_reserved();
        for f in FUNCTIONS {
            assert!(all.contains(f), "{} missing from all_reserved()", f);
        }
        for k in STATEMENT_KEYWORDS {
            assert!(all.contains(k), "{} missing from all_reserved()", k);
        }
    }
}
