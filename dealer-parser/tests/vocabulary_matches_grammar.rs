//! Asserts `vocabulary.rs` agrees with `grammar.pest`.
//!
//! The vocabulary lists exist so editors can highlight and complete the same
//! words the parser accepts. That is only true if they stay in step with the
//! grammar, so this extracts the word lists straight out of the PEG source and
//! compares them. Adding a function to the grammar without listing it here
//! fails the build.

use std::collections::BTreeSet;

const GRAMMAR: &str = include_str!("../src/grammar.pest");

/// Pull the body of a named rule out of the grammar source.
fn rule_body(name: &str) -> String {
    let start = GRAMMAR
        .find(&format!("{} = ", name))
        .unwrap_or_else(|| panic!("rule `{}` not found in grammar.pest", name));
    let rest = &GRAMMAR[start..];
    let open = rest.find('{').expect("rule has no body");
    let mut depth = 0usize;
    for (i, c) in rest[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return rest[open..open + i + 1].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("unterminated body for rule `{}`", name);
}

/// String literals in a rule body, both `"x"` and case-insensitive `^"x"`.
fn literals(body: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let bytes: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '"' {
            let mut j = i + 1;
            let mut s = String::new();
            while j < bytes.len() && bytes[j] != '"' {
                s.push(bytes[j]);
                j += 1;
            }
            // Keep only word literals. Character-class members from negative
            // lookaheads (e.g. `!(ASCII_ALPHANUMERIC | "_")`) are not vocabulary.
            if !s.is_empty()
                && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && s.chars().all(|c| c.is_ascii_alphanumeric())
            {
                out.insert(s);
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

fn as_set(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn compare(what: &str, from_grammar: BTreeSet<String>, from_vocab: BTreeSet<String>) {
    let missing: Vec<_> = from_grammar.difference(&from_vocab).collect();
    let extra: Vec<_> = from_vocab.difference(&from_grammar).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{} is out of step with grammar.pest\n  \
         in grammar but not in vocabulary.rs: {:?}\n  \
         in vocabulary.rs but not in grammar: {:?}\n\n\
         Editors take their word lists from vocabulary.rs, so a mismatch means \
         highlighting and completion disagree with the parser.",
        what,
        missing,
        extra
    );
}

#[test]
fn functions_match_grammar() {
    compare(
        "FUNCTIONS",
        literals(&rule_body("function_name")),
        as_set(dealer_parser::vocabulary::FUNCTIONS),
    );
}

#[test]
fn actions_match_grammar() {
    // `action_type` and the standalone `print_stmt` must list the same actions.
    let from_action = literals(&rule_body("action_type"));
    let from_print = literals(&rule_body("print_stmt"));
    assert_eq!(
        from_action, from_print,
        "action_type and print_stmt list different actions"
    );
    compare(
        "ACTIONS",
        from_action,
        as_set(dealer_parser::vocabulary::ACTIONS),
    );
}

#[test]
fn vulnerabilities_match_grammar() {
    compare(
        "VULNERABILITIES",
        literals(&rule_body("vulnerability")),
        as_set(dealer_parser::vocabulary::VULNERABILITIES),
    );
}

#[test]
fn positions_match_grammar() {
    compare(
        "POSITIONS",
        literals(&rule_body("position")),
        as_set(dealer_parser::vocabulary::POSITIONS),
    );
}

#[test]
fn statement_keywords_are_all_in_the_grammar() {
    // Statement keywords appear across many `*_stmt` rules, so rather than
    // enumerate the rules, check each keyword occurs as a literal somewhere.
    for kw in dealer_parser::vocabulary::STATEMENT_KEYWORDS {
        assert!(
            GRAMMAR.contains(&format!("^\"{}\"", kw)),
            "statement keyword `{}` does not appear in grammar.pest",
            kw
        );
    }
}

#[test]
fn logical_words_are_all_in_the_grammar() {
    for w in dealer_parser::vocabulary::LOGICAL_WORDS {
        assert!(
            GRAMMAR.contains(&format!("^\"{}\"", w)),
            "logical word `{}` does not appear in grammar.pest",
            w
        );
    }
}

/// A statement keyword must not be reachable as a bare variable reference.
///
/// Without this, a malformed statement backtracks into `expr` and parses as
/// bare identifiers the evaluator ignores — so `dealer soudfsfd` was accepted
/// silently, where dealer.exe reports "line 1: syntax error".
#[test]
fn malformed_statements_are_rejected() {
    let cases = [
        "dealer soudfsfd\n",
        "dealer\n",
        "vulnerable xyz\n",
        "produce abc\n",
        "generate abc\n",
        "predeal north XX\n",
        "condition\n",
    ];
    for script in cases {
        let pre = dealer_parser::preprocess(script);
        assert!(
            dealer_parser::parse_program(&pre).is_err(),
            "should have been rejected: {:?}",
            script
        );
    }
}

/// The fix must not reject anything valid.
#[test]
fn well_formed_statements_still_parse() {
    let cases = [
        "dealer south\ncondition hcp(north) >= 1\n",
        "vulnerable ns\ncondition hcp(north) >= 1\n",
        "produce 5\ncondition hcp(north) >= 1\n",
        "generate 100\ncondition hcp(north) >= 1\n",
        "predeal north SAKQ\ncondition hcp(south) >= 1\n",
        "condition hcp(north) >= 1\n",
        // A variable may still be named after a function or a position.
        "spadeFit = spades(north) + spades(south)\ncondition spadeFit >= 8\n",
        "n = hcp(north)\ncondition n >= 10\n",
    ];
    for script in cases {
        let pre = dealer_parser::preprocess(script);
        assert!(
            dealer_parser::parse_program(&pre).is_ok(),
            "should have parsed: {:?}\n{:?}",
            script,
            dealer_parser::parse_program(&pre).err()
        );
    }
}

/// The words dealer3 does not implement must be reserved in the grammar, so
/// they fail loudly rather than being read as variables.
///
/// This is the pairing that makes `NOT_SUPPORTED` more than a note: the list
/// the language reference prints and the rule the parser enforces are the same
/// list. Implementing one of these words means taking it out of both.
#[test]
fn not_supported_words_are_reserved_in_the_grammar() {
    compare(
        "NOT_SUPPORTED",
        literals(&rule_body("reserved_unsupported")),
        dealer_parser::vocabulary::NOT_SUPPORTED
            .iter()
            .map(|e| e.name.to_string())
            .collect(),
    );
}

/// `reserved_unsupported` is an ordered choice followed by a word boundary, and
/// PEG does not retry the choice once one branch has matched. So a word that is
/// a prefix of a later word would shadow it: `notrump` ahead of `notrumps` would
/// match seven letters of `notrumps`, then fail the boundary, then give up.
#[test]
fn reserved_words_are_ordered_longest_first() {
    let body = rule_body("reserved_unsupported");
    let order: Vec<&str> = body
        .match_indices('"')
        .filter(|(i, _)| body[..*i].ends_with('^'))
        .map(|(i, _)| {
            let rest = &body[i + 1..];
            &rest[..rest.find('"').expect("unterminated literal")]
        })
        .collect();

    for (i, word) in order.iter().enumerate() {
        for later in &order[i + 1..] {
            assert!(
                !later.starts_with(word),
                "`{}` is listed before `{}`, which it is a prefix of — the shorter one wins and \
                 the longer becomes unreachable",
                word,
                later
            );
        }
    }
}
