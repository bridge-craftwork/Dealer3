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

    /// Whether `known` already holds everything this demand would search for.
    ///
    /// A deal that arrived from a solved file answers yes to any demand; a deal
    /// whose condition happened to ask the one question the action asks answers
    /// yes too. Either way there is nothing to warm, and warming anyway is
    /// invisible except on the clock.
    pub fn satisfied_by(&self, known: &dealer_dds::DealTricks) -> bool {
        match self {
            DdDemand::None => true,
            DdDemand::Table => known.table().is_some(),
            DdDemand::Cells(cells) => cells
                .iter()
                .all(|(denomination, declarer)| known.get(*denomination, *declarer).is_some()),
        }
    }

    /// Solve what this demand names, so a later evaluation finds it remembered.
    ///
    /// Errors are not reported: this is a cache warm, and the real evaluation
    /// that follows will raise anything genuinely wrong with its own message
    /// and position. Failing quietly here costs a solve, not an answer.
    /// Nothing known is passed in throughout: warming exists for deals nobody
    /// has solved. A deal that already knows the answers is skipped by
    /// [`DdDemand::satisfied_by`] before reaching here.
    pub fn warm(&self, deal: &dealer_core::Deal) {
        match self {
            DdDemand::None => {}
            DdDemand::Table => {
                // Solving is the point here; the table it hands back is for
                // callers that want one. What this leaves on the thread is
                // collected by `Workers::warm_each` and carried back with the
                // deal.
                dealer_dds::solve_table(deal);
            }
            DdDemand::Cells(cells) => {
                for (denomination, declarer) in cells {
                    dealer_dds::tricks(&dealer_dds::NOTHING_KNOWN, deal, *denomination, *declarer);
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
/// already, and what they work out comes back with the deal, so the action
/// finds it in hand. Warming it again would be pure waste — and the run does
/// not even try, because [`DdDemand::satisfied_by`] sees the answers are
/// already there.
/// Whether anything anywhere in `program` can reach the solver.
///
/// Wider than [`of_program`], which asks only what a produced deal's *action*
/// will need. A `tricks()` in the condition reaches the solver too, and a run
/// whose condition solves has answers worth carrying even though its action
/// asks for nothing.
///
/// Used to keep the carrying off the hot path entirely: for the great majority
/// of scripts, which never mention double-dummy, there is nothing to carry and
/// this says so once for the whole run rather than forty bytes a deal.
/// Does testing a deal against this program's condition reach the solver?
///
/// Narrower than [`touches_solver`], and it answers a different question: not
/// "will this run solve" but "does each deal cost a search *before* we know
/// whether it matched". A `tricks()` in an action is paid only by deals that
/// matched, and can be warmed on the pool in whatever quantity is wanted; a
/// `tricks()` in the condition is paid by every deal built, which is what makes
/// the size of a batch matter.
pub fn condition_touches_solver(program: &Program) -> bool {
    let mut demand = DdDemand::None;
    if let Some(constraint) = dealer_eval::extract_constraint(program) {
        walk(constraint, program, &mut demand, &mut HashSet::new());
    }
    !demand.is_none()
}

pub fn touches_solver(program: &Program) -> bool {
    let mut demand = DdDemand::None;
    if let Some(constraint) = dealer_eval::extract_constraint(program) {
        walk(constraint, program, &mut demand, &mut HashSet::new());
    }
    !demand.is_none() || !of_program(program).is_none()
}

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
