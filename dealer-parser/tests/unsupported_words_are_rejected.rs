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
        // Any unknown name, not only the words we happen to list.
        "condition nosuchfunction(north) >= 1\n",
        "condition hcpx(north) >= 1\n",
        "condition kingz(north) >= 1\n",
        // A position is not callable either.
        "condition north(x) >= 1\n",
        // A space before the bracket changes nothing.
        "condition nosuchfunction (north) >= 5\n",
    ];
    for script in cases {
        assert!(!parses(script), "should have been rejected: {:?}", script);
    }
}

/// `notrump` used to be read as an unset variable, which is 0, which is clubs —
/// so the script asked about the wrong strain and got a believable answer. It is
/// now the number 4, which is what dealer.exe's own grammar resolves it to.
#[test]
fn notrump_is_the_number_four() {
    use dealer_parser::{Expr, Statement};

    let by_word = parse("condition tricks(north, notrumps) >= 0\n");
    let by_number = parse("condition tricks(north, 4) >= 0\n");
    assert_eq!(
        by_word, by_number,
        "`notrumps` must parse to exactly what `4` parses to"
    );
    assert_eq!(parse("condition notrump\n"), parse("condition notrumps\n"));

    // And it really is the literal, not something that merely compares equal.
    match &parse("condition notrump\n").statements[0] {
        Statement::Condition(Expr::Literal(n)) => assert_eq!(*n, 4),
        other => panic!("expected the literal 4, got {:?}", other),
    }
}

fn parse(script: &str) -> dealer_parser::Program {
    let pre = dealer_parser::preprocess(script);
    dealer_parser::parse_program(&pre)
        .unwrap_or_else(|e| panic!("should have parsed {script:?}: {e}"))
}

/// `pointcount` and `altcount` are implemented now, but their bad forms must
/// still fail where the script is written rather than at evaluation time, so the
/// editor underlines them.
#[test]
fn the_point_count_statements_validate_their_arguments() {
    // Valid.
    assert!(parses("pointcount 6 4 2 1\ncondition hcp(north) >= 20\n"));
    assert!(parses("altcount 2 1 1 1\ncondition 1\n"));
    assert!(parses(
        "pointcount 1 1 1 1 1 1 1 1 1 1 1 1 1\ncondition 1\n"
    ));
    assert!(parses("altcount 11 6 4 2 1\ncondition 1\n"));

    // Fourteen values: one more than there are ranks, which the original also
    // rejects with "too many pointcount values".
    assert!(!parses(
        "pointcount 1 1 1 1 1 1 1 1 1 1 1 1 1 1\ncondition 1\n"
    ));

    // Out of range. dealer.exe accepts these and writes past its own table;
    // dealer3 refuses instead.
    assert!(!parses("altcount 12 1 1 1\ncondition 1\n"));
    assert!(!parses("altcount 99 1\ncondition 1\n"));
    assert!(!parses("altcount -1 1\ncondition 1\n"));

    // A count with no values at all is a syntax error, as in the original.
    assert!(!parses("pointcount\ncondition 1\n"));
    assert!(!parses("altcount 2\ncondition 1\n"));
}

/// The error text has to name the limit, since "out of range" alone would leave
/// a reader guessing whether `altcount 2` or `altcount 0` sets `pt0`.
#[test]
fn the_altcount_range_error_explains_the_numbering() {
    let pre = dealer_parser::preprocess("altcount 12 1\ncondition 1\n");
    let error = dealer_parser::parse_program(&pre)
        .expect_err("altcount 12 should be refused")
        .to_string();
    assert!(error.contains("0 to 11"), "unhelpful message: {}", error);
    assert!(error.contains("pt0"), "unhelpful message: {}", error);
}

/// Nothing that worked before may break. These are the cases most likely to
/// collide with the new reserved words; the whole 1,076-script corpus was
/// diffed separately and was unchanged.
#[test]
fn ordinary_scripts_still_parse() {
    let cases = [
        "condition hcp(north) >= 12\n",
        "condition controls(north) >= 5 && aces(north) >= 1\n",
        // The spellings added alongside this change.
        "condition control(north) >= 5 && ace(north) >= 1\n",
        "condition hcps(north) >= 12 && king(north) >= 1\n",
        "condition trick(south, notrump) >= 9\n",
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
