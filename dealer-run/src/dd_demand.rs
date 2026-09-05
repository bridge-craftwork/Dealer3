//! What double-dummy work a script's `action` will ask for, per produced deal.
//!
//! The point is to know it *before* the deals arrive, so the batch can be
//! solved on the worker pool instead of one cell at a time on the main thread.
//!
//! Why this is worth doing
//! ----------------------
//!
//! Workers deal and test the condition; the main thread evaluates the action.
//! A condition that calls `tricks()` therefore already solves in parallel —
//! measured at 4.5 cores busy — while the same call in an action solves on one
//! core with the pool idle. Same work, same `-R`, and on one measurement 12.1s
//! against 1.9s.
//!
//! Nothing needs to move to fix that. `dealer_dds` shares a solved cell across
//! threads the moment it is worked out, precisely so "a worker thread may only
//! see one deal calling `tricks()` in a whole batch, and the main thread needs
//! the answer". So the batch's deals are solved on the pool first, and the main
//! thread's evaluation then finds every answer already there. Evaluation order,
//! `rnd()` streams and output all stay exactly as they were.
//!
//! Why the demand is analysed rather than just solving whole tables
//! ---------------------------------------------------------------
//!
//! A table is twenty searches and a single `tricks()` is one. `action average
//! "x" tricks(south, spades)` asked for as a table would be twenty times the
//! work — so the cells are read off the script where they can be, and only a
//! genuinely table-shaped demand (`par`, `trix`, or an argument that is not a
//! literal) asks for all twenty.

use dealer_core::Position;
use dealer_dds::Denomination;
use dealer_parser::{CsvTerm, EsTerm, Expr, Function, Program, Statement};
use std::collections::HashSet;

/// The double-dummy cells one deal's action will need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DdDemand {
    /// No action expression reaches the solver. Nothing to warm.
    None,
    /// These (denomination, declarer) cells, and no others.
    Cells(Vec<(Denomination, Position)>),
    /// All twenty. `par` needs the whole table by definition, and so does a
    /// `tricks()` whose arguments cannot be read off the script.
    Table,
}

impl DdDemand {
    fn add_cell(&mut self, cell: (Denomination, Position)) {
        match self {
            DdDemand::Table => {}
            DdDemand::None => *self = DdDemand::Cells(vec![cell]),
            DdDemand::Cells(cells) => {
                if !cells.contains(&cell) {
                    cells.push(cell);
                }
            }
        }
    }

    fn widen_to_table(&mut self) {
        *self = DdDemand::Table;
    }

    /// Solve what this demand names, so a later evaluation finds it remembered.
    ///
    /// Errors are not reported: this is a cache warm, and the real evaluation
    /// that follows will raise anything genuinely wrong with its own message
    /// and position. Failing quietly here costs a solve, not an answer.
    pub fn warm(&self, deal: &dealer_core::Deal) {
        match self {
            DdDemand::None => {}
            DdDemand::Table => {
                dealer_dds::table(deal);
            }
            DdDemand::Cells(cells) => {
                for (denomination, declarer) in cells {
                    dealer_dds::tricks(deal, *denomination, *declarer);
                }
            }
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, DdDemand::None)
    }
}

/// Read the double-dummy demand off a program's `action` statements.
///
/// The condition is deliberately not looked at: it is evaluated by the workers
/// already, so anything it asks for is solved in parallel and remembered before
/// the action ever runs. Warming it again would be pure waste.
pub fn of_program(program: &Program) -> DdDemand {
    let mut demand = DdDemand::None;
    for statement in &program.statements {
        if let Statement::Action {
            averages,
            frequencies,
            printes,
            print_reports,
            ..
        } = statement
        {
            for spec in averages {
                walk(&spec.expr, program, &mut demand, &mut HashSet::new());
            }
            for spec in frequencies {
                walk(&spec.expr, program, &mut demand, &mut HashSet::new());
                if let Some((second, _)) = &spec.second {
                    walk(second, program, &mut demand, &mut HashSet::new());
                }
            }
            for terms in printes {
                for term in terms {
                    if let EsTerm::Expression(expr) = term {
                        walk(expr, program, &mut demand, &mut HashSet::new());
                    }
                }
            }
            for terms in print_reports {
                walk_csv(terms, program, &mut demand);
            }
        }
        // `csvrpt` as a bare statement, which DealerV2_4's scripts also write.
        if let Statement::CsvReport(terms) = statement {
            walk_csv(terms, program, &mut demand);
        }
    }
    demand
}

fn walk_csv(terms: &[CsvTerm], program: &Program, demand: &mut DdDemand) {
    for term in terms {
        match term {
            CsvTerm::Expression(expr) => walk(expr, program, demand, &mut HashSet::new()),
            // `trix` is the whole table for every seat it names.
            CsvTerm::Trix(_) => demand.widen_to_table(),
            _ => {}
        }
    }
}

/// Walk one expression, following variables.
///
/// `seen` guards against a variable that refers to itself, directly or through
/// others. The evaluator has its own answer for that; this only has to avoid
/// looping while looking.
fn walk(expr: &Expr, program: &Program, demand: &mut DdDemand, seen: &mut HashSet<String>) {
    match expr {
        Expr::FunctionCall { func, args } => {
            match func {
                // Every spelling of the single-cell question.
                Function::Tricks => match cell_of(args) {
                    Some(cell) => demand.add_cell(cell),
                    // Arguments that are not literals cannot be read here, and
                    // guessing would warm the wrong cell and solve the right
                    // one later anyway. The whole table is the honest answer.
                    None => demand.widen_to_table(),
                },
                // Par is derived from all twenty.
                Function::Par => demand.widen_to_table(),
                _ => {}
            }
            for arg in args {
                walk(arg, program, demand, seen);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            walk(left, program, demand, seen);
            walk(right, program, demand, seen);
        }
        Expr::UnaryOp { expr, .. } => walk(expr, program, demand, seen),
        Expr::Ternary {
            condition,
            true_expr,
            false_expr,
        } => {
            walk(condition, program, demand, seen);
            walk(true_expr, program, demand, seen);
            walk(false_expr, program, demand, seen);
        }
        Expr::Variable(name) => {
            if !seen.insert(name.clone()) {
                return;
            }
            for statement in &program.statements {
                if let Statement::Assignment {
                    name: assigned,
                    expr,
                } = statement
                {
                    if assigned == name {
                        walk(expr, program, demand, seen);
                    }
                }
            }
        }
        _ => {}
    }
}

/// The (denomination, declarer) a `tricks(position, denomination)` names, when
/// both are written as literals — which is how scripts write them.
fn cell_of(args: &[Expr]) -> Option<(Denomination, Position)> {
    if args.len() != 2 {
        return None;
    }
    let declarer = match &args[0] {
        Expr::Position(position) => *position,
        _ => return None,
    };
    let denomination = match &args[1] {
        Expr::Suit(suit) => Denomination::from_suit(*suit),
        Expr::Literal(n) => Denomination::from_index(*n)?,
        _ => return None,
    };
    Some((denomination, declarer))
}
