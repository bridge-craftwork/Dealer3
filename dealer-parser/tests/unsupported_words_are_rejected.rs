//! The words dealer3 does not implement must be rejected, not misread.
//!
//! Before this, an unrecognised name was simply an identifier, and an
//! identifier is an ordinary expression — so a statement quietly turned into a
//! different statement. `condition control(north) >= 5` parsed as the bare
//! expression `control`, then `(north) >= 5`; the last expression in a script is
//! the condition, so the filter that ran was `north >= 5`, which is constant.
//! The program printed no deals, no error, and exited 0.
//!
//! The same shape with a threshold that happened to be true matched
//! *everything* instead. `tricks(north, notrumps)` was the worst of the set:
//! `notrumps` became an unset variable, the denomination argument takes a
//! number, and 0 means clubs — so the script asked about clubs and got a
//! believable answer.
//!
//! See issue #15. Implementing these words is a separate job; this only makes
//! them loud.

fn parses(script: &str) -> bool {
    let pre = dealer_parser::preprocess(script);
    dealer_parser::parse_program(&pre).is_ok()
}

#[test]
fn every_unsupported_word_is_rejected_in_expression_position() {
    for entry in dealer_parser::vocabulary::NOT_SUPPORTED {
        let script = format!("condition {} >= 1\n", entry.name);
        assert!(
            !parses(&script),
            "`{}` is listed as unsupported but still parses as a variable: {:?}",
            entry.name,
            script
        );
    }
}

#[test]
fn every_unsupported_word_is_rejected_as_a_variable_name() {
    // Rejecting at the assignment reports the problem where it is written,
    // rather than at the first use of the variable several lines later.
    for entry in dealer_parser::vocabulary::NOT_SUPPORTED {
        let script = format!("{} = 3\ncondition 1\n", entry.name);
        assert!(
            !parses(&script),
            "`{}` can still be used as a variable name: {:?}",
            entry.name,
            script
        );
    }
}

#[test]
fn calling_something_that_is_not_a_function_is_an_error() {
    let cases = [
        // The singular and plural spellings from the original's lexer.
        "condition control(north) >= 5\n",
        "condition hcps(north) >= 12\n",
        "condition ace(north) >= 1\n",
        "condition king(north) >= 1\n",
        "condition trick(north) >= 1\n",
        // Any other unknown name, not only the ones we happen to list.
        "condition nosuchfunction(north) >= 1\n",
        // A position is not callable either.
        "condition north(x) >= 1\n",
        // A space before the bracket changes nothing.
        "condition control (north) >= 5\n",
    ];
    for script in cases {
        assert!(!parses(script), "should have been rejected: {:?}", script);
    }
}

#[test]
fn notrump_is_rejected_rather_than_read_as_clubs() {
    assert!(!parses("condition tricks(north, notrumps) >= 0\n"));
    assert!(!parses("condition tricks(north, notrump) >= 0\n"));
    // The spellings that do work are untouched.
    assert!(parses("condition tricks(north, 4) >= 0\n"));
    assert!(parses("condition tricks(north, spades) >= 0\n"));
}

#[test]
fn the_point_count_statements_are_rejected_rather_than_ignored() {
    // These were the worst of the set: they parsed, were thrown away, and the
    // script silently ran on the scale the author was trying to replace. A run
    // of `pointcount 6 4 2 1` with `hcp(north) >= 20` produced exactly the hit
    // rate of the standard 4-3-2-1 scale.
    assert!(!parses("pointcount 6 4 2 1\ncondition hcp(north) >= 20\n"));
    assert!(!parses("altcount 0 1 1 1 1 1 1 1 1 1 1 1 1\ncondition 1\n"));
}

/// Nothing that worked before may break. These are the cases most likely to
/// collide with the new reserved words; the whole 1,076-script corpus was
/// diffed separately and was unchanged.
#[test]
fn ordinary_scripts_still_parse() {
    let cases = [
        "condition hcp(north) >= 12\n",
        "condition controls(north) >= 5 && aces(north) >= 1\n",
        "condition kings(north) >= 1 && queens(north) >= 1 && tens(north) >= 1\n",
        "condition top2(north, spades) == 2\n",
        // Names that merely begin with a reserved word.
        "aceCount = aces(north)\ncondition aceCount >= 2\n",
        "kingdom = kings(north)\ncondition kingdom >= 1\n",
        "printer = hcp(north)\ncondition printer >= 1\n",
        "tens_held = tens(north)\ncondition tens_held >= 1\n",
        // Brackets after an operator, which must not look like a call.
        "condition hcp(north) * (2 + 1) >= 30\n",
        "condition (hcp(north) + hcp(south)) >= 25\n",
        "fit = spades(north) + spades(south)\ncondition fit >= 8\n",
        // Single-letter variables shadowing positions.
        "n = hcp(north)\ncondition n >= 10\n",
        // The print actions, whose names start with the reserved `print`.
        "action printall\ncondition 1\n",
        "action printoneline\ncondition 1\n",
        "printpbn\ncondition 1\n",
    ];
    for script in cases {
        assert!(parses(script), "should have parsed: {:?}", script);
    }
}
