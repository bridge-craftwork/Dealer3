//! Names a program uses but never defines.
//!
//! A misspelled word is not a syntax error here, because a bare expression is a
//! legal statement — the original's grammar has `def: expr`, and every practice
//! scenario in the wild relies on it to write a condition without the keyword.
//! So `dealr west` parses: two statements, a variable reference and a compass,
//! both discarded.
//!
//! dealer.exe does not let that pass. It reports `line 1: unknown variable`,
//! because it resolves every name as it reduces rather than only the ones that
//! end up being evaluated. dealer3 evaluates the condition and the action
//! expressions, so a name in a statement that turns out to be neither is never
//! looked up and never complained about.
//!
//! Hence this pass, run once after parsing: walk every expression in the
//! program and report any variable reference with no assignment behind it.
//! Positions, suits and cards are their own kinds of expression, so a bare
//! `west` is not a name in need of defining — only `Expr::Variable` is.

use crate::ast::{Expr, Program, Statement};
use std::collections::BTreeSet;

/// Every name referenced but never assigned, in the order first referenced.
///
/// Defined-anywhere rather than defined-above: the original is a single pass
/// and so insists on the latter, but nothing in dealer3 needs the ordering and
/// tightening it would refuse scripts that work today for no gain in typos
/// caught.
pub fn undefined_variables(program: &Program) -> Vec<String> {
    let mut defined: BTreeSet<&str> = BTreeSet::new();
    for statement in &program.statements {
        if let Statement::Assignment { name, .. } = statement {
            defined.insert(name.as_str());
        }
    }

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut found = Vec::new();
    let mut note = |name: &str, found: &mut Vec<String>| {
        if !defined.contains(name) && seen.insert(name.to_string()) {
            found.push(name.to_string());
        }
    };

    for statement in &program.statements {
        for expr in statement_exprs(statement) {
            walk(expr, &mut note, &mut found);
        }
    }
    found
}

/// Seats left standing on their own as statements.
///
/// A bare expression is a legal statement, so a compass with nothing done to it
/// parses and is discarded — which is how `predeal north SAKQ south` reads once
/// the `predeal` has taken every seat that came with holdings. dealer.exe
/// answers `syntax error` there, because a seat is its own token in its grammar
/// and `predealarg: COMPASS holdings` requires the holdings.
///
/// Reported after `undefined_variables` rather than at parse time, and this is
/// the reason: `dealr west` is *also* a bare compass, and what is wrong with it
/// is the misspelled `dealer`, not the seat. Naming the seat first would bury
/// the useful half.
pub fn dangling_seats(program: &Program) -> Vec<String> {
    program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Expression(Expr::Position(seat)) => Some(seat.to_string().to_lowercase()),
            _ => None,
        })
        .collect()
}

/// Every expression a statement holds, whether or not the run will evaluate it.
fn statement_exprs(statement: &Statement) -> Vec<&Expr> {
    match statement {
        Statement::Assignment { expr, .. } => vec![expr],
        Statement::Expression(expr) | Statement::Condition(expr) => vec![expr],
        Statement::Action {
            averages,
            frequencies,
            printes,
            ..
        } => {
            let mut out: Vec<&Expr> = averages.iter().map(|a| &a.expr).collect();
            out.extend(frequencies.iter().map(|f| &f.expr));
            for terms in printes {
                out.extend(terms.iter().filter_map(|term| match term {
                    crate::ast::EsTerm::Expression(expr) => Some(expr),
                    _ => None,
                }));
            }
            out
        }
        Statement::CsvReport(terms) | Statement::PrintReport(terms) => terms
            .iter()
            .filter_map(|term| match term {
                crate::ast::CsvTerm::Expression(expr) => Some(expr),
                _ => None,
            })
            .collect(),
        Statement::Produce(_)
        | Statement::Generate(_)
        | Statement::Dealer(_)
        | Statement::Vulnerable(_)
        | Statement::Title(_)
        | Statement::Seed(_)
        | Statement::Predeal { .. }
        | Statement::PointCount(_)
        | Statement::AltCount { .. } => Vec::new(),
    }
}

fn walk<F>(expr: &Expr, note: &mut F, found: &mut Vec<String>)
where
    F: FnMut(&str, &mut Vec<String>),
{
    match expr {
        Expr::Variable(name) => note(name, found),
        Expr::BinaryOp { left, right, .. } => {
            walk(left, note, found);
            walk(right, note, found);
        }
        Expr::UnaryOp { expr, .. } => walk(expr, note, found),
        Expr::Ternary {
            condition,
            true_expr,
            false_expr,
        } => {
            walk(condition, note, found);
            walk(true_expr, note, found);
            walk(false_expr, note, found);
        }
        Expr::FunctionCall { args, .. } => {
            for arg in args {
                walk(arg, note, found);
            }
        }
        Expr::Literal(_)
        | Expr::Position(_)
        | Expr::ShapePattern(_)
        | Expr::Card(_)
        | Expr::Suit(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> Vec<String> {
        let program = crate::parse_program(&crate::preprocess(source)).expect("parses");
        undefined_variables(&program)
    }

    fn seats(source: &str) -> Vec<String> {
        let program = crate::parse_program(&crate::preprocess(source)).expect("parses");
        dangling_seats(&program)
    }

    /// The way a multi-seat `predeal` gets mistyped: the last seat given no
    /// holdings falls out of the statement and reads as a bare compass.
    #[test]
    fn a_predeal_seat_without_holdings_is_left_dangling() {
        assert_eq!(seats("predeal north SAKQ south"), vec!["south"]);
    }

    /// A seat that was given holdings is part of the `predeal` and not dangling.
    #[test]
    fn a_predeal_that_names_its_holdings_leaves_nothing_dangling() {
        assert!(seats("predeal north SAKQ south SJ32").is_empty());
        assert!(seats("predeal north S,HAKQ south SJ32").is_empty());
    }

    /// `dealr west` is a bare seat too, and there the misspelling is the thing
    /// worth reporting — which is why the caller asks for the names first.
    #[test]
    fn a_misspelled_keyword_leaves_a_seat_dangling_as_well() {
        assert_eq!(names("dealr west"), vec!["dealr"]);
        assert_eq!(seats("dealr west"), vec!["west"]);
    }

    #[test]
    fn a_misspelled_statement_keyword_is_caught() {
        // The case this exists for. `dealer west` is a statement; `dealr west`
        // is a variable reference and a compass, and dealer.exe answers
        // `line 1: unknown variable`.
        assert_eq!(names("dealr west\nhcp(north) > 10\n"), ["dealr"]);
    }

    #[test]
    fn a_name_defined_anywhere_is_defined() {
        assert!(names("opener = hcp(north) >= 15\ncondition opener\n").is_empty());
        // Including below its use, which the original would refuse and we do not.
        assert!(names("condition opener\nopener = hcp(north) >= 15\n").is_empty());
    }

    #[test]
    fn compasses_and_suits_are_not_names() {
        assert!(names("condition hcp(west) > 10 and spades(north) == 5\n").is_empty());
    }

    #[test]
    fn it_looks_inside_actions_and_assignments() {
        assert_eq!(
            names("x = mistyped + 1\ncondition x > 0\naction average \"a\" alsowrong\n"),
            // First referenced, not alphabetical: the order is where to look.
            ["mistyped", "alsowrong"]
        );
    }

    #[test]
    fn each_name_is_reported_once() {
        assert_eq!(names("condition wrong > 1 and wrong < 5\n"), ["wrong"]);
    }
}
