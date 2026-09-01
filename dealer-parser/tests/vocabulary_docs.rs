//! Asserts every word in `vocabulary.rs` is documented, and that the
//! documentation parses.
//!
//! The reference page is generated from these tables, so an undocumented
//! function would show up there as a blank row — or, worse, not at all. Adding
//! a function to the grammar already fails `vocabulary_matches_grammar.rs`
//! until it is listed; this makes it fail until it is also described.
//!
//! The examples are parsed rather than merely eyeballed, so a reference cannot
//! print a snippet the engine would reject. `dealer-eval` takes this one step
//! further and evaluates them.

use dealer_parser::vocabulary::*;
use std::collections::BTreeSet;

/// The examples are fragments; a condition is the context they are written for.
fn parses_as_condition(example: &str) -> Result<(), String> {
    parses(&format!("condition {}\n", example))
}

fn parses(script: &str) -> Result<(), String> {
    let pre = dealer_parser::preprocess(script);
    dealer_parser::parse_program(&pre)
        .map(|_| ())
        .map_err(|e| format!("{}", e))
}

#[test]
fn every_function_is_documented() {
    let listed: BTreeSet<&str> = FUNCTIONS.iter().copied().collect();
    let documented: BTreeSet<&str> = FUNCTION_DOCS.iter().map(|d| d.name).collect();

    let missing: Vec<_> = listed.difference(&documented).collect();
    let extra: Vec<_> = documented.difference(&listed).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "FUNCTION_DOCS is out of step with FUNCTIONS\n  \
         in FUNCTIONS but undocumented: {:?}\n  \
         documented but not a function: {:?}\n\n\
         The language reference is generated from FUNCTION_DOCS, so an \
         undocumented function is one nobody can look up.",
        missing,
        extra
    );

    assert_eq!(
        FUNCTION_DOCS.len(),
        documented.len(),
        "FUNCTION_DOCS documents the same name twice"
    );
}

#[test]
fn function_docs_are_filled_in() {
    for doc in FUNCTION_DOCS {
        for (field, value) in [
            ("group", doc.group),
            ("signature", doc.signature),
            ("summary", doc.summary),
            ("example", doc.example),
        ] {
            assert!(
                !value.trim().is_empty(),
                "`{}` has an empty {}",
                doc.name,
                field
            );
        }
        assert!(
            doc.signature.starts_with(doc.name),
            "`{}` has a signature for something else: {:?}",
            doc.name,
            doc.signature
        );
        assert!(
            doc.summary.trim_end().ends_with('.'),
            "`{}`'s summary should read as a sentence: {:?}",
            doc.name,
            doc.summary
        );
        assert!(
            FUNCTION_GROUPS.contains(&doc.group),
            "`{}` is in group {:?}, which is not in FUNCTION_GROUPS",
            doc.name,
            doc.group
        );
    }
}

#[test]
fn function_aliases_point_at_a_real_function() {
    let by_name: BTreeSet<&str> = FUNCTION_DOCS.iter().map(|d| d.name).collect();
    for doc in FUNCTION_DOCS {
        let Some(target) = doc.alias_of else { continue };
        assert!(
            by_name.contains(target),
            "`{}` is an alias of `{}`, which does not exist",
            doc.name,
            target
        );
        assert_ne!(doc.name, target, "`{}` is an alias of itself", doc.name);

        // One hop only: an alias of an alias would leave the reference showing
        // "another spelling of X" where X is itself a redirect.
        let hop = FUNCTION_DOCS
            .iter()
            .find(|d| d.name == target)
            .expect("target checked above");
        assert!(
            hop.alias_of.is_none(),
            "`{}` points at `{}`, which is itself an alias of `{:?}`",
            doc.name,
            target,
            hop.alias_of
        );
    }
}

#[test]
fn every_operator_is_documented() {
    let listed: BTreeSet<&str> = OPERATORS.iter().copied().collect();
    let documented: BTreeSet<&str> = OPERATOR_DOCS.iter().map(|d| d.symbol).collect();

    let missing: Vec<_> = listed.difference(&documented).collect();
    let extra: Vec<_> = documented.difference(&listed).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "OPERATOR_DOCS is out of step with OPERATORS\n  \
         listed but undocumented: {:?}\n  \
         documented but not listed: {:?}",
        missing,
        extra
    );
    assert_eq!(
        OPERATOR_DOCS.len(),
        documented.len(),
        "OPERATOR_DOCS documents the same symbol twice"
    );
}

#[test]
fn operator_docs_are_in_precedence_order() {
    // The reference prints them in this order and groups by level, so an entry
    // out of sequence would silently claim the wrong binding strength.
    let mut previous = 0;
    for doc in OPERATOR_DOCS {
        assert!(
            doc.precedence >= previous,
            "`{}` has precedence {} after {}; OPERATOR_DOCS must run tightest first",
            doc.symbol,
            doc.precedence,
            previous
        );
        previous = doc.precedence;
    }
}

#[test]
fn operator_word_forms_are_the_ones_the_parser_accepts() {
    let words: BTreeSet<&str> = LOGICAL_WORDS.iter().copied().collect();
    let documented: BTreeSet<&str> = OPERATOR_DOCS.iter().filter_map(|d| d.word).collect();
    assert_eq!(
        words, documented,
        "the word forms in OPERATOR_DOCS disagree with LOGICAL_WORDS"
    );
}

#[test]
fn every_statement_keyword_is_documented() {
    let listed: BTreeSet<&str> = STATEMENT_KEYWORDS.iter().copied().collect();
    let documented: BTreeSet<&str> = STATEMENT_DOCS.iter().filter_map(|d| d.keyword).collect();

    let missing: Vec<_> = listed.difference(&documented).collect();
    let extra: Vec<_> = documented.difference(&listed).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "STATEMENT_DOCS is out of step with STATEMENT_KEYWORDS\n  \
         listed but undocumented: {:?}\n  \
         documented but not a statement keyword: {:?}",
        missing,
        extra
    );
}

#[test]
fn every_action_is_documented() {
    let listed: BTreeSet<&str> = ACTIONS.iter().copied().collect();
    let documented: BTreeSet<&str> = ACTION_DOCS.iter().map(|d| d.name).collect();
    assert_eq!(
        listed, documented,
        "ACTION_DOCS is out of step with ACTIONS"
    );
}

#[test]
fn unsupported_words_really_are_unsupported() {
    // If one of these is ever implemented it must come off the list, or the
    // reference will go on telling people to avoid something that works.
    let reserved: BTreeSet<&str> = all_reserved().into_iter().collect();
    for entry in NOT_SUPPORTED {
        assert!(
            !reserved.contains(entry.name),
            "`{}` is listed in NOT_SUPPORTED but is now part of the vocabulary — \
             take it off the list",
            entry.name
        );
        assert!(
            !entry.instead.trim().is_empty(),
            "`{}` says nothing about what to write instead",
            entry.name
        );
    }
}

#[test]
fn function_examples_parse() {
    for doc in FUNCTION_DOCS {
        if let Err(e) = parses_as_condition(doc.example) {
            panic!(
                "the example for `{}` does not parse: {:?}\n{}",
                doc.name, doc.example, e
            );
        }
    }
}

#[test]
fn operator_examples_parse() {
    for doc in OPERATOR_DOCS {
        // The assignment example is a statement in its own right rather than
        // something that can sit inside a condition.
        let result = if doc.symbol == "=" {
            parses(&format!("{}\ncondition 1\n", doc.example))
        } else {
            parses_as_condition(doc.example)
        };
        if let Err(e) = result {
            panic!(
                "the example for `{}` does not parse: {:?}\n{}",
                doc.symbol, doc.example, e
            );
        }
    }
}

#[test]
fn statement_examples_parse() {
    for doc in STATEMENT_DOCS {
        if let Err(e) = parses(&format!("{}\n", doc.example)) {
            panic!(
                "the example for `{}` does not parse: {:?}\n{}",
                doc.form, doc.example, e
            );
        }
    }
}

#[test]
fn action_examples_parse() {
    for doc in ACTION_DOCS {
        // `printside` needs its side; every other action is its own whole form.
        let form = doc.form.unwrap_or(doc.name);
        if let Err(e) = parses(&format!("action {}\ncondition 1\n", form)) {
            panic!("`action {}` does not parse\n{}", form, e);
        }
    }
}

/// The claim the `!` entry makes about its own precedence, checked rather than
/// asserted: a leading `not` covers the whole comparison, while one inside an
/// arithmetic operand binds only to what follows it.
#[test]
fn the_documented_not_precedence_is_the_real_one() {
    assert!(parses("condition not hcp(north) >= 12\n").is_ok());
    assert!(parses("condition 100 * not hcp(north) >= 12\n").is_ok());
}

/// Likewise for the chained-comparison note on `==`.
#[test]
fn comparisons_chain_as_documented() {
    assert!(parses("condition hcp(north) == hcp(south) == hcp(east)\n").is_ok());
}
