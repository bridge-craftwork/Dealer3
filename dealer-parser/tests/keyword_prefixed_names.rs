//! A variable may be named after a keyword, as long as the name is longer.
//!
//! `conditionMet`, `actionList`, `printewFoo`: each begins with a word that
//! introduces a statement, and each is an ordinary identifier that dealer.exe
//! accepts, because its lexer takes the longest token. dealer3's statement
//! rules used to match the keyword without checking what followed, so they ate
//! the front of the name — issue #12.
//!
//! Half of that was loud, a parse error on the leftover `= 1`. The other half
//! was not, and that is the half worth testing: `action` and the `print*`
//! statements take no required argument, so `actionList = 1` parsed as an empty
//! `action` followed by an assignment to `List`. The script then ran to its
//! generate limit, matched nothing and exited 0.
//!
//! The names are built from `vocabulary`, so a keyword added later is covered
//! without anyone remembering to add it here.

use dealer_parser::vocabulary::{ACTIONS, STATEMENT_KEYWORDS};
use dealer_parser::{parse_program, preprocess, Expr, Statement};

/// Every word a statement can begin with.
fn leading_words() -> Vec<&'static str> {
    STATEMENT_KEYWORDS
        .iter()
        .chain(ACTIONS.iter())
        .copied()
        .collect()
}

fn parse(script: &str) -> Vec<Statement> {
    let pre = preprocess(script);
    parse_program(&pre)
        .unwrap_or_else(|e| panic!("should have parsed {script:?}:\n{e}"))
        .statements
}

fn rejects(script: &str) {
    let pre = preprocess(script);
    assert!(
        parse_program(&pre).is_err(),
        "should have been rejected: {script:?}"
    );
}

#[test]
fn a_name_beginning_with_a_keyword_is_assigned_under_its_whole_name() {
    for word in leading_words() {
        let name = format!("{word}Value");
        let statements = parse(&format!("{name} = 1\n"));
        assert_eq!(
            statements.len(),
            1,
            "`{name} = 1` should be one statement, got {statements:?}"
        );
        match &statements[0] {
            Statement::Assignment { name: assigned, .. } => assert_eq!(assigned, &name),
            other => panic!("`{name} = 1` parsed as {other:?}"),
        }
    }
}

#[test]
fn such_a_name_can_then_be_referred_to() {
    for word in leading_words() {
        let name = format!("{word}Value");
        let statements = parse(&format!("{name} = 1\ncondition {name}\n"));
        assert_eq!(statements.len(), 2, "for `{name}`: {statements:?}");
        match &statements[1] {
            Statement::Condition(Expr::Variable(referenced)) => assert_eq!(referenced, &name),
            other => panic!("`condition {name}` parsed as {other:?}"),
        }
    }
}

/// The form without an assignment, which is a bare expression and the script's
/// constraint. Reordering the grammar's alternatives would fix the assignment
/// above and leave this one silently wrong, so it is tested separately.
#[test]
fn such_a_name_works_as_a_bare_final_expression() {
    for word in leading_words() {
        let name = format!("{word}Value");
        let statements = parse(&format!("{name} = 1\n{name}\n"));
        assert_eq!(statements.len(), 2, "for `{name}`: {statements:?}");
        match &statements[1] {
            Statement::Expression(Expr::Variable(referenced)) => assert_eq!(referenced, &name),
            other => panic!("bare `{name}` parsed as {other:?}"),
        }
    }
}

/// Digits count as part of a name too: `produce5` is an identifier, not
/// `produce` followed by the number 5.
#[test]
fn a_digit_after_a_keyword_is_part_of_the_name() {
    for name in ["produce5", "generate10", "pointcount4", "altcount2"] {
        let statements = parse(&format!("{name} = 1\n"));
        match statements.as_slice() {
            [Statement::Assignment { name: assigned, .. }] => assert_eq!(assigned, name),
            other => panic!("`{name} = 1` parsed as {other:?}"),
        }
    }
}

/// The argument rules matter as much as the keyword: `dealerN` would otherwise
/// be `dealer N`, and `vulnerableNS` would be `vulnerable NS`.
#[test]
fn an_argument_word_after_a_keyword_is_part_of_the_name() {
    for name in [
        "dealerN",
        "dealerNorth",
        "dealerSeat",
        "vulnerableNS",
        "vulnerableAll",
        "vulnerableNone",
        "predealNorth",
    ] {
        let statements = parse(&format!("{name} = 1\n"));
        match statements.as_slice() {
            [Statement::Assignment { name: assigned, .. }] => assert_eq!(assigned, name),
            other => panic!("`{name} = 1` parsed as {other:?}"),
        }
    }
}

#[test]
fn the_statements_themselves_still_parse() {
    let statements = parse(concat!(
        "predeal north SAKQ\n",
        "dealer south\n",
        "vulnerable NS\n",
        "produce 3\n",
        "generate 500\n",
        "pointcount 4 3 2 1\n",
        "altcount 2 2 1 1 1\n",
        "condition hcp(north) >= 10\n",
        "action printoneline, average \"a\" hcp(north), frequency \"f\" (hcp(north), 0, 20)\n",
    ));
    assert!(matches!(statements[0], Statement::Predeal { .. }));
    assert!(matches!(statements[1], Statement::Dealer(_)));
    assert!(matches!(statements[2], Statement::Vulnerable(_)));
    assert!(matches!(statements[3], Statement::Produce(3)));
    assert!(matches!(statements[4], Statement::Generate(500)));
    assert!(matches!(statements[5], Statement::PointCount(_)));
    assert!(matches!(statements[6], Statement::AltCount { .. }));
    assert!(matches!(statements[7], Statement::Condition(_)));
    assert!(matches!(statements[8], Statement::Action { .. }));

    // The standalone forms too, which have their own rules.
    for script in [
        "printall\n",
        "printew\n",
        "printpbn\n",
        "printcompact\n",
        "printoneline\n",
        "average \"a\" hcp(north)\n",
        "frequency \"f\" (hcp(north), 0, 20)\n",
        "csvrpt(hcp(north))\n",
    ] {
        assert_eq!(parse(script).len(), 1, "{script:?}");
    }
}

/// The values a statement takes need the same boundary as its keyword.
/// `vulnerable ewer` used to be read as `vulnerable ew` with a stray `er` after
/// it; dealer.exe calls it a syntax error, and so should we.
#[test]
fn a_statements_argument_must_end_where_the_word_ends() {
    for script in [
        "vulnerable ewer\n",
        "vulnerable nsomething\n",
        "vulnerable allx\n",
        "vulnerable noneish\n",
    ] {
        rejects(script);
    }
    for script in [
        "vulnerable ew\n",
        "vulnerable ns\n",
        "vulnerable all\n",
        "vulnerable none\n",
    ] {
        assert_eq!(parse(script).len(), 1, "{script:?}");
    }
}

/// The guards must not soften the loud rejection of a malformed statement,
/// which is what a keyword followed by nonsense still is.
#[test]
fn a_malformed_statement_is_still_rejected() {
    for script in [
        "dealer soudfsfd\n",
        "vulnerable maybe\n",
        "produce lots\n",
        "generate plenty\n",
        "predeal north\n",
    ] {
        rejects(script);
    }
}
