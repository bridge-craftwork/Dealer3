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
        Statement::CsvReport(terms) => terms
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
