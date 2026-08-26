pub mod counts;

pub use counts::{CountError, CountRow, PointCounts};

use dealer_core::{Card, Deal, Position, Suit};
use dealer_dds::{Denomination, DoubleDummySolver};
use dealer_parser::{BinaryOp, Expr, Function, Program, ShapePattern, Statement, UnaryOp};
use rustc_hash::FxHashMap;
use std::cell::RefCell;

/// IMP conversion table (from DealerV2_4)
/// Maps score differences to IMP values
/// Table[i] represents the minimum score difference for (i+1) IMPs
const IMP_TABLE: [i32; 24] = [
    10, 40, 80, 120, 160, 210, 260, 310, 360, 410, 490, 590, 740, 890, 1090, 1190, 1490, 1740,
    1990, 2240, 2490, 2990, 3490, 3990,
];

/// Convert score difference to IMPs
///
/// Uses standard IMP table:
/// - 0-9: 0 IMPs
/// - 10-39: 1 IMP
/// - 40-79: 2 IMPs
/// - ... up to 3990+: 24 IMPs
///
/// Sign is preserved (negative score difference returns negative IMPs)
fn score_to_imps(score_diff: i32) -> i32 {
    let abs_diff = score_diff.abs();

    if abs_diff == 0 {
        return 0;
    }

    // Find the IMP value by searching the table
    // Table[i] represents the minimum score for (i+1) IMPs
    let mut imps = 0;
    for (i, &threshold) in IMP_TABLE.iter().enumerate() {
        if abs_diff >= threshold {
            imps = (i + 1) as i32;
        } else {
            break;
        }
    }

    // Preserve sign
    if score_diff < 0 {
        -imps
    } else {
        imps
    }
}

/// Strain (denomination) for contract scoring
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strain {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
    NoTrump,
}

impl Strain {
    /// Returns true if this is a minor suit (clubs or diamonds)
    fn is_minor(&self) -> bool {
        matches!(self, Strain::Clubs | Strain::Diamonds)
    }
}

/// Doubled state of a contract
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doubled {
    Undoubled,
    Doubled,
    Redoubled,
}

/// A bridge contract
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contract {
    pub level: u8, // 1-7
    pub strain: Strain,
    pub doubled: Doubled,
}

impl Contract {
    /// Parse a contract string like "3n", "4s", "7nt", "3hx", "3hxx"
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.to_lowercase();
        let chars: Vec<char> = s.chars().collect();

        if chars.is_empty() {
            return None;
        }

        // First character must be level 1-7
        let level = chars[0].to_digit(10)? as u8;
        if !(1..=7).contains(&level) {
            return None;
        }

        if chars.len() < 2 {
            return None;
        }

        // Parse strain
        let (strain, rest_start) = if chars.len() >= 3 && chars[1] == 'n' && chars[2] == 't' {
            (Strain::NoTrump, 3)
        } else {
            let strain = match chars[1] {
                'c' => Strain::Clubs,
                'd' => Strain::Diamonds,
                'h' => Strain::Hearts,
                's' => Strain::Spades,
                'n' => Strain::NoTrump,
                _ => return None,
            };
            (strain, 2)
        };

        // Parse doubled state (x, xx, dbl, rdbl)
        let rest: String = chars[rest_start..].iter().collect();
        let doubled = if rest.is_empty() {
            Doubled::Undoubled
        } else if rest == "x" || rest == "dbl" {
            Doubled::Doubled
        } else if rest == "xx" || rest == "rdbl" {
            Doubled::Redoubled
        } else {
            return None;
        };

        Some(Contract {
            level,
            strain,
            doubled,
        })
    }
}

/// Calculate the score for a contract
///
/// # Arguments
/// * `vulnerable` - true if declarer is vulnerable
/// * `contract` - the contract being played
/// * `tricks` - tricks taken by declarer (0-13)
///
/// # Returns
/// Positive score if contract made, negative if failed
pub fn calculate_score(vulnerable: bool, contract: &Contract, tricks: u8) -> i32 {
    let tricks_needed = contract.level as i32 + 6;
    let tricks_taken = tricks as i32;
    let overtricks = tricks_taken - tricks_needed;

    if overtricks < 0 {
        // Contract failed - calculate penalty
        calculate_penalty(vulnerable, contract.doubled, -overtricks)
    } else {
        // Contract made - calculate score
        calculate_made_score(vulnerable, contract, overtricks)
    }
}

/// Calculate penalty for undertricks
fn calculate_penalty(vulnerable: bool, doubled: Doubled, undertricks: i32) -> i32 {
    match doubled {
        Doubled::Undoubled => {
            // 50 per undertrick non-vul, 100 per undertrick vul
            let per_trick = if vulnerable { 100 } else { 50 };
            -(undertricks * per_trick)
        }
        Doubled::Doubled => {
            if vulnerable {
                // First: 200, subsequent: 300 each
                let first = 200;
                let subsequent = (undertricks - 1) * 300;
                -(first + subsequent)
            } else {
                // First: 100, second: 200, third: 200, subsequent: 300 each
                let score = match undertricks {
                    1 => 100,
                    2 => 300,                 // 100 + 200
                    3 => 500,                 // 100 + 200 + 200
                    n => 500 + (n - 3) * 300, // First 3 = 500, then 300 each
                };
                -score
            }
        }
        Doubled::Redoubled => {
            // Redoubled penalties are double the doubled penalties
            let doubled_penalty = calculate_penalty(vulnerable, Doubled::Doubled, undertricks);
            doubled_penalty * 2
        }
    }
}

/// Calculate score for a made contract
fn calculate_made_score(vulnerable: bool, contract: &Contract, overtricks: i32) -> i32 {
    let mut score = 0;

    // Trick score (below the line)
    let trick_value = if contract.strain.is_minor() { 20 } else { 30 };
    let first_nt_bonus = if contract.strain == Strain::NoTrump {
        10
    } else {
        0
    };

    let trick_score = contract.level as i32 * trick_value + first_nt_bonus;

    // Apply doubling to trick score
    let trick_score = match contract.doubled {
        Doubled::Undoubled => trick_score,
        Doubled::Doubled => trick_score * 2,
        Doubled::Redoubled => trick_score * 4,
    };

    score += trick_score;

    // Game/partscore bonus
    let is_game = trick_score >= 100;
    if is_game {
        score += if vulnerable { 500 } else { 300 };
    } else {
        score += 50; // Partscore bonus
    }

    // Slam bonuses
    if contract.level == 6 {
        // Small slam
        score += if vulnerable { 750 } else { 500 };
    } else if contract.level == 7 {
        // Grand slam
        score += if vulnerable { 1500 } else { 1000 };
    }

    // Overtrick bonus
    let overtrick_value = match contract.doubled {
        Doubled::Undoubled => trick_value,
        Doubled::Doubled => {
            if vulnerable {
                200
            } else {
                100
            }
        }
        Doubled::Redoubled => {
            if vulnerable {
                400
            } else {
                200
            }
        }
    };
    score += overtricks * overtrick_value;

    // Insult bonus for making doubled/redoubled contract
    match contract.doubled {
        Doubled::Undoubled => {}
        Doubled::Doubled => score += 50,
        Doubled::Redoubled => score += 100,
    }

    score
}

/// Evaluation error type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    /// Function requires specific number of arguments
    InvalidArgumentCount {
        function: String,
        expected: usize,
        got: usize,
    },
    /// Invalid argument type or value
    InvalidArgument(String),
    /// Function not yet implemented
    NotImplemented(String),
    /// Variable not found
    UndefinedVariable(String),
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            EvalError::InvalidArgumentCount {
                function,
                expected,
                got,
            } => {
                write!(
                    f,
                    "Function {} expects {} arguments, got {}",
                    function, expected, got
                )
            }
            EvalError::InvalidArgument(msg) => write!(f, "Invalid argument: {}", msg),
            EvalError::NotImplemented(feature) => write!(f, "Not implemented: {}", feature),
            EvalError::UndefinedVariable(name) => write!(f, "Undefined variable: {}", name),
        }
    }
}

impl std::error::Error for EvalError {}

/// Evaluation context - holds the deal being evaluated and variable bindings
pub struct EvalContext<'a> {
    pub deal: &'a Deal,
    /// Variable name -> Expression tree reference mapping
    /// Variables store references to expression trees (no cloning needed)
    /// FxHashMap uses a faster (non-cryptographic) hash function
    pub variables: &'a FxHashMap<String, &'a Expr>,
    /// Cache of evaluated variable values (per-deal)
    /// Using RefCell for interior mutability since eval takes &self
    /// Keys are &str references to avoid String cloning on cache insert
    /// FxHashMap uses a faster (non-cryptographic) hash function
    cache: RefCell<FxHashMap<&'a str, i32>>,
    /// Point counts a script redefined with `pointcount` or `altcount`.
    ///
    /// `None` is the ordinary case — no script in the 1,076-script corpus uses
    /// either statement — and it means the hardcoded counts run exactly as they
    /// did before. Only a script that redefines something pays for the table.
    counts: Option<&'a PointCounts>,
}

/// Empty variables map for contexts without variables
static EMPTY_VARIABLES: std::sync::LazyLock<FxHashMap<String, &'static Expr>> =
    std::sync::LazyLock::new(FxHashMap::default);

impl<'a> EvalContext<'a> {
    /// Create a context without any variables (for simple expressions)
    pub fn new(deal: &'a Deal) -> Self {
        EvalContext {
            deal,
            variables: &EMPTY_VARIABLES,
            cache: RefCell::new(FxHashMap::default()),
            counts: None,
        }
    }

    /// Create a context with pre-defined variable references
    pub fn with_variables(deal: &'a Deal, variables: &'a FxHashMap<String, &'a Expr>) -> Self {
        EvalContext {
            deal,
            variables,
            cache: RefCell::new(FxHashMap::default()),
            counts: None,
        }
    }

    /// A context whose counts a script has redefined.
    ///
    /// Pass `None` when the script has no `pointcount` or `altcount` statement —
    /// `extract_point_counts` returns exactly that — so the ordinary path keeps
    /// running the hardcoded counts.
    pub fn with_counts(
        deal: &'a Deal,
        variables: &'a FxHashMap<String, &'a Expr>,
        counts: Option<&'a PointCounts>,
    ) -> Self {
        EvalContext {
            deal,
            variables,
            cache: RefCell::new(FxHashMap::default()),
            counts,
        }
    }
}

/// Extract variable references from a program (call once before the eval loop)
/// Returns a FxHashMap mapping variable names to references to their expression trees
pub fn extract_variables(program: &Program) -> FxHashMap<String, &Expr> {
    let mut variables = FxHashMap::default();
    for statement in &program.statements {
        if let Statement::Assignment { name, expr } = statement {
            variables.insert(name.clone(), expr);
        }
    }
    variables
}

/// Build the count table a program defines, or `None` if it defines none.
///
/// `None` is the answer for almost every script, and it is what keeps the
/// hardcoded counts on the hot path — see `counts` for why that matters.
/// Statements apply in order, so a later one wins, as in the original.
pub fn extract_point_counts(program: &Program) -> Result<Option<PointCounts>, CountError> {
    let mut counts: Option<PointCounts> = None;
    for statement in &program.statements {
        match statement {
            Statement::PointCount(values) => {
                counts
                    .get_or_insert_with(PointCounts::standard)
                    .set_row(CountRow::Hcp as usize, values)?;
            }
            Statement::AltCount { row, values } => {
                counts
                    .get_or_insert_with(PointCounts::standard)
                    .set_row(*row, values)?;
            }
            _ => {}
        }
    }
    Ok(counts)
}

/// Extract the final constraint expression from a program (call once before the eval loop)
pub fn extract_constraint(program: &Program) -> Option<&Expr> {
    let mut final_expr = None;
    for statement in &program.statements {
        match statement {
            Statement::Expression(expr) => {
                final_expr = Some(expr);
            }
            Statement::Condition(expr) => {
                final_expr = Some(expr);
            }
            _ => {}
        }
    }
    final_expr
}

/// Evaluate a constraint expression with pre-extracted variables against a deal
///
/// This is the optimized hot-path function. Call extract_variables() and
/// extract_constraint() once before the loop, then call this for each deal.
pub fn eval_with_context(
    constraint: &Expr,
    variables: &FxHashMap<String, &Expr>,
    deal: &Deal,
) -> Result<i32, EvalError> {
    eval_with_context_and_counts(constraint, variables, deal, None)
}

/// As `eval_with_context`, honouring counts the script redefined.
///
/// `counts` is `None` for any script without a `pointcount` or `altcount`
/// statement, which is the case that has to stay fast.
pub fn eval_with_context_and_counts(
    constraint: &Expr,
    variables: &FxHashMap<String, &Expr>,
    deal: &Deal,
    counts: Option<&PointCounts>,
) -> Result<i32, EvalError> {
    let ctx = EvalContext::with_counts(deal, variables, counts);
    eval(constraint, &ctx)
}

/// Evaluate a program (assignments + final expression) against a deal
///
/// NOTE: This function is convenient but not optimal for hot loops because it
/// extracts variables on every call. For performance-critical code, use
/// extract_variables(), extract_constraint(), and eval_with_context() instead.
///
/// Processes statements in order:
/// - Assignments populate the variables HashMap
/// - The last expression/condition is the constraint to evaluate
///
/// Note: Multiple bare expressions in a file - only the LAST one is used as the condition.
/// Earlier expressions are evaluated for side effects but don't affect matching.
/// This matches dealer.exe behavior where earlier bare expressions appear to be ignored.
pub fn eval_program(program: &Program, deal: &Deal) -> Result<i32, EvalError> {
    let variables = extract_variables(program);

    let constraint = extract_constraint(program).ok_or_else(|| {
        EvalError::InvalidArgument("Program must end with a constraint expression".to_string())
    })?;

    eval_with_context(constraint, &variables, deal)
}

/// Evaluate an expression against a deal
pub fn eval(expr: &Expr, ctx: &EvalContext) -> Result<i32, EvalError> {
    match expr {
        Expr::Literal(value) => Ok(*value),

        Expr::Variable(name) => {
            // Check cache first (use &str for lookup to avoid allocation)
            if let Some(&cached_value) = ctx.cache.borrow().get(name.as_str()) {
                return Ok(cached_value);
            }

            // Look up variable and evaluate its stored expression tree
            // Use get_key_value to get the key with lifetime 'a for cache insertion
            match ctx.variables.get_key_value(name) {
                Some((key, var_expr)) => {
                    let value = eval(var_expr, ctx)?;
                    // Cache the computed value using the key reference (no clone!)
                    ctx.cache.borrow_mut().insert(key.as_str(), value);
                    Ok(value)
                }
                None => Err(EvalError::UndefinedVariable(name.clone())),
            }
        }

        Expr::Position(pos) => {
            // Check if this position name is actually a variable override
            // dealer.exe allows variables named n, s, e, w to shadow position keywords
            let pos_name: &'static str = match pos {
                Position::North => "n",
                Position::South => "s",
                Position::East => "e",
                Position::West => "w",
            };

            // Check cache first for the single-letter variable
            if let Some(&cached_value) = ctx.cache.borrow().get(pos_name) {
                return Ok(cached_value);
            }

            // Check if variable is defined - if so, use it instead of position
            // Use get_key_value to get the key with proper lifetime for cache insertion
            if let Some((key, var_expr)) = ctx.variables.get_key_value(pos_name) {
                let value = eval(var_expr, ctx)?;
                ctx.cache.borrow_mut().insert(key.as_str(), value);
                return Ok(value);
            }

            // No variable override - return position index (0-3)
            Ok(*pos as i32)
        }

        Expr::BinaryOp { op, left, right } => {
            // Short-circuit evaluation for logical operators
            match op {
                BinaryOp::And => {
                    let left_val = eval(left, ctx)?;
                    if left_val == 0 {
                        return Ok(0); // Short-circuit: false && _ = false
                    }
                    let right_val = eval(right, ctx)?;
                    return Ok(if right_val != 0 { 1 } else { 0 });
                }
                BinaryOp::Or => {
                    let left_val = eval(left, ctx)?;
                    if left_val != 0 {
                        return Ok(1); // Short-circuit: true || _ = true
                    }
                    let right_val = eval(right, ctx)?;
                    return Ok(if right_val != 0 { 1 } else { 0 });
                }
                _ => {}
            }

            // Non-short-circuit operators: evaluate both sides
            let left_val = eval(left, ctx)?;
            let right_val = eval(right, ctx)?;

            match op {
                // Arithmetic
                BinaryOp::Add => Ok(left_val + right_val),
                BinaryOp::Sub => Ok(left_val - right_val),
                BinaryOp::Mul => Ok(left_val * right_val),
                BinaryOp::Div => {
                    if right_val == 0 {
                        Err(EvalError::InvalidArgument("Division by zero".to_string()))
                    } else {
                        Ok(left_val / right_val)
                    }
                }
                BinaryOp::Mod => {
                    if right_val == 0 {
                        Err(EvalError::InvalidArgument("Modulo by zero".to_string()))
                    } else {
                        Ok(left_val % right_val)
                    }
                }

                // Comparison (return 1 for true, 0 for false)
                BinaryOp::Eq => Ok(if left_val == right_val { 1 } else { 0 }),
                BinaryOp::Ne => Ok(if left_val != right_val { 1 } else { 0 }),
                BinaryOp::Lt => Ok(if left_val < right_val { 1 } else { 0 }),
                BinaryOp::Le => Ok(if left_val <= right_val { 1 } else { 0 }),
                BinaryOp::Gt => Ok(if left_val > right_val { 1 } else { 0 }),
                BinaryOp::Ge => Ok(if left_val >= right_val { 1 } else { 0 }),

                // Already handled above with short-circuit
                BinaryOp::And | BinaryOp::Or => unreachable!(),
            }
        }

        Expr::UnaryOp { op, expr } => {
            let val = eval(expr, ctx)?;
            match op {
                UnaryOp::Negate => Ok(-val),
                UnaryOp::Not => Ok(if val == 0 { 1 } else { 0 }),
            }
        }

        Expr::Ternary {
            condition,
            true_expr,
            false_expr,
        } => {
            let cond_val = eval(condition, ctx)?;
            if cond_val != 0 {
                eval(true_expr, ctx)
            } else {
                eval(false_expr, ctx)
            }
        }

        Expr::FunctionCall { func, args } => eval_function(func, args, ctx),

        Expr::ShapePattern(_pattern) => {
            // Shape patterns shouldn't be evaluated directly, they're arguments to shape()
            Err(EvalError::InvalidArgument(
                "Shape pattern can only be used as argument to shape() function".to_string(),
            ))
        }

        Expr::Card(_card) => {
            // Cards shouldn't be evaluated directly, they're arguments to hascard()
            Err(EvalError::InvalidArgument(
                "Card can only be used as argument to hascard() function".to_string(),
            ))
        }

        Expr::Suit(_suit) => {
            // Suits shouldn't be evaluated directly, they're arguments to suit-specific functions
            Err(EvalError::InvalidArgument(
                "Suit can only be used as argument to functions like losers()".to_string(),
            ))
        }
    }
}

/// One of the twelve counts, honouring a table the script redefined.
///
/// The `None` arm is the ordinary case and calls exactly the hardcoded routine
/// that ran before this existed, so a script with no `pointcount` or `altcount`
/// pays a single well-predicted branch rather than a table walk. See
/// `counts` for the measurements behind that choice.
#[inline(always)]
fn counted(
    ctx: &EvalContext,
    hand: &dealer_core::Hand,
    row: CountRow,
    suit: Option<Suit>,
    standard: impl FnOnce() -> i32,
) -> i32 {
    match ctx.counts {
        None => standard(),
        Some(counts) => match suit {
            Some(suit) => counts.count_suit(hand, row, suit),
            None => counts.count_hand(hand, row),
        },
    }
}

/// Evaluate a function call
fn eval_function(function: &Function, args: &[Expr], ctx: &EvalContext) -> Result<i32, EvalError> {
    match function {
        Function::Hcp => {
            // hcp(position) - total HCP for a hand
            // hcp(position, suit) - HCP in a specific suit
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "hcp".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }
            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            if args.len() == 2 {
                let suit = eval_suit_arg(&args[1])?;
                Ok(counted(ctx, hand, CountRow::Hcp, Some(suit), || {
                    hand.cards_in_suit(suit)
                        .iter()
                        .map(|c| c.hcp() as i32)
                        .sum()
                }))
            } else {
                Ok(counted(ctx, hand, CountRow::Hcp, None, || {
                    hand.hcp() as i32
                }))
            }
        }

        Function::Hearts => {
            if args.len() != 1 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "hearts".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }
            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);
            Ok(hand.suit_length(dealer_core::Suit::Hearts) as i32)
        }

        Function::Spades => {
            if args.len() != 1 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "spades".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }
            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);
            Ok(hand.suit_length(dealer_core::Suit::Spades) as i32)
        }

        Function::Diamonds => {
            if args.len() != 1 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "diamonds".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }
            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);
            Ok(hand.suit_length(dealer_core::Suit::Diamonds) as i32)
        }

        Function::Clubs => {
            if args.len() != 1 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "clubs".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }
            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);
            Ok(hand.suit_length(dealer_core::Suit::Clubs) as i32)
        }

        Function::Controls => {
            // controls(position) - total controls for a hand
            // controls(position, suit) - controls in a specific suit
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "controls".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }
            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            if args.len() == 2 {
                let suit = eval_suit_arg(&args[1])?;
                Ok(counted(ctx, hand, CountRow::Controls, Some(suit), || {
                    hand.cards_in_suit(suit)
                        .iter()
                        .map(|c| match c.rank {
                            dealer_core::Rank::Ace => 2,
                            dealer_core::Rank::King => 1,
                            _ => 0,
                        })
                        .sum()
                }))
            } else {
                Ok(counted(ctx, hand, CountRow::Controls, None, || {
                    hand.controls() as i32
                }))
            }
        }

        Function::Shape => {
            if args.len() != 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "shape".to_string(),
                    expected: 2,
                    got: args.len(),
                });
            }
            let position = eval_position_arg(&args[0], ctx)?;
            let pattern = match &args[1] {
                Expr::ShapePattern(p) => p,
                _ => {
                    return Err(EvalError::InvalidArgument(
                        "Second argument to shape() must be a shape pattern".to_string(),
                    ))
                }
            };

            let hand = ctx.deal.hand(position);
            let matches = eval_shape_pattern(hand, pattern)?;
            Ok(if matches { 1 } else { 0 })
        }

        Function::Losers => {
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "losers".to_string(),
                    expected: 1, // or 2 with suit
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            // Losers are derived from the controls and top3 rows, so a script
            // that redefines either moves the loser count with it — which is
            // what dealer.exe does, verified on the VM.
            if args.len() == 1 {
                Ok(match ctx.counts {
                    None => hand.losers() as i32,
                    Some(counts) => counts.losers(hand),
                })
            } else {
                let suit = eval_suit_arg(&args[1])?;
                Ok(match ctx.counts {
                    None => hand.losers_in_suit(suit) as i32,
                    Some(counts) => counts.losers_in_suit(hand, suit),
                })
            }
        }

        Function::HasCard => {
            if args.len() != 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "hascard".to_string(),
                    expected: 2,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let card = eval_card_arg(&args[1])?;
            let hand = ctx.deal.hand(position);

            Ok(if hand.has_card(card) { 1 } else { 0 })
        }

        // Alternative point counts (pt0-pt9 / readable synonyms)
        Function::Tens => {
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "tens".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            if args.len() == 1 {
                Ok(counted(ctx, hand, CountRow::Tens, None, || {
                    hand.tens() as i32
                }))
            } else {
                let suit = eval_suit_arg(&args[1])?;
                Ok(counted(ctx, hand, CountRow::Tens, Some(suit), || {
                    hand.tens_in_suit(suit) as i32
                }))
            }
        }

        Function::Jacks => {
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "jacks".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            if args.len() == 1 {
                Ok(counted(ctx, hand, CountRow::Jacks, None, || {
                    hand.jacks() as i32
                }))
            } else {
                let suit = eval_suit_arg(&args[1])?;
                Ok(counted(ctx, hand, CountRow::Jacks, Some(suit), || {
                    hand.jacks_in_suit(suit) as i32
                }))
            }
        }

        Function::Queens => {
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "queens".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            if args.len() == 1 {
                Ok(counted(ctx, hand, CountRow::Queens, None, || {
                    hand.queens() as i32
                }))
            } else {
                let suit = eval_suit_arg(&args[1])?;
                Ok(counted(ctx, hand, CountRow::Queens, Some(suit), || {
                    hand.queens_in_suit(suit) as i32
                }))
            }
        }

        Function::Kings => {
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "kings".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            if args.len() == 1 {
                Ok(counted(ctx, hand, CountRow::Kings, None, || {
                    hand.kings() as i32
                }))
            } else {
                let suit = eval_suit_arg(&args[1])?;
                Ok(counted(ctx, hand, CountRow::Kings, Some(suit), || {
                    hand.kings_in_suit(suit) as i32
                }))
            }
        }

        Function::Aces => {
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "aces".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            if args.len() == 1 {
                Ok(counted(ctx, hand, CountRow::Aces, None, || {
                    hand.aces() as i32
                }))
            } else {
                let suit = eval_suit_arg(&args[1])?;
                Ok(counted(ctx, hand, CountRow::Aces, Some(suit), || {
                    hand.aces_in_suit(suit) as i32
                }))
            }
        }

        Function::Top2 => {
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "top2".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            if args.len() == 1 {
                Ok(counted(ctx, hand, CountRow::Top2, None, || {
                    hand.top2() as i32
                }))
            } else {
                let suit = eval_suit_arg(&args[1])?;
                Ok(counted(ctx, hand, CountRow::Top2, Some(suit), || {
                    hand.top2_in_suit(suit) as i32
                }))
            }
        }

        Function::Top3 => {
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "top3".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            if args.len() == 1 {
                Ok(counted(ctx, hand, CountRow::Top3, None, || {
                    hand.top3() as i32
                }))
            } else {
                let suit = eval_suit_arg(&args[1])?;
                Ok(counted(ctx, hand, CountRow::Top3, Some(suit), || {
                    hand.top3_in_suit(suit) as i32
                }))
            }
        }

        Function::Top4 => {
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "top4".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            if args.len() == 1 {
                Ok(counted(ctx, hand, CountRow::Top4, None, || {
                    hand.top4() as i32
                }))
            } else {
                let suit = eval_suit_arg(&args[1])?;
                Ok(counted(ctx, hand, CountRow::Top4, Some(suit), || {
                    hand.top4_in_suit(suit) as i32
                }))
            }
        }

        Function::Top5 => {
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "top5".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            if args.len() == 1 {
                Ok(counted(ctx, hand, CountRow::Top5, None, || {
                    hand.top5() as i32
                }))
            } else {
                let suit = eval_suit_arg(&args[1])?;
                Ok(counted(ctx, hand, CountRow::Top5, Some(suit), || {
                    hand.top5_in_suit(suit) as i32
                }))
            }
        }

        Function::C13 => {
            if args.is_empty() || args.len() > 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "c13".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            if args.len() == 1 {
                Ok(counted(ctx, hand, CountRow::C13, None, || {
                    hand.c13() as i32
                }))
            } else {
                let suit = eval_suit_arg(&args[1])?;
                Ok(counted(ctx, hand, CountRow::C13, Some(suit), || {
                    hand.c13_in_suit(suit) as i32
                }))
            }
        }

        Function::Quality => {
            if args.len() != 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "quality".to_string(),
                    expected: 2,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let suit = eval_suit_arg(&args[1])?;
            let hand = ctx.deal.hand(position);

            Ok(hand.suit_quality(suit))
        }

        Function::Cccc => {
            if args.len() != 1 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "cccc".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;
            let hand = ctx.deal.hand(position);

            Ok(hand.cccc())
        }

        Function::Tricks => {
            // tricks(position, denomination)
            // position: north/south/east/west
            // denomination: 0=C, 1=D, 2=H, 3=S, 4=NT (or use suit keywords)
            if args.len() != 2 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "tricks".to_string(),
                    expected: 2,
                    got: args.len(),
                });
            }

            let position = eval_position_arg(&args[0], ctx)?;

            // Parse denomination - can be numeric (0-4) or suit keyword
            let denomination = match &args[1] {
                Expr::Suit(suit) => Denomination::from_suit(*suit),
                Expr::Literal(n) => match n {
                    0 => Denomination::Clubs,
                    1 => Denomination::Diamonds,
                    2 => Denomination::Hearts,
                    3 => Denomination::Spades,
                    4 => Denomination::NoTrump,
                    _ => {
                        return Err(EvalError::InvalidArgument(format!(
                            "Invalid denomination: {} (must be 0=C, 1=D, 2=H, 3=S, 4=NT)",
                            n
                        )));
                    }
                },
                _ => {
                    // Try to evaluate as an expression
                    let n = eval(&args[1], ctx)?;
                    match n {
                        0 => Denomination::Clubs,
                        1 => Denomination::Diamonds,
                        2 => Denomination::Hearts,
                        3 => Denomination::Spades,
                        4 => Denomination::NoTrump,
                        _ => {
                            return Err(EvalError::InvalidArgument(format!(
                                "Invalid denomination: {} (must be 0=C, 1=D, 2=H, 3=S, 4=NT)",
                                n
                            )));
                        }
                    }
                }
            };

            // Create solver and solve
            let solver = DoubleDummySolver::new(ctx.deal.clone());
            let tricks = solver.solve(denomination, position);

            Ok(tricks as i32)
        }

        Function::Score => {
            // score(vulnerability, contract, tricks)
            // vulnerability: 0 = non-vul, 1 = vul
            // contract: encoded as level * 10 + strain + doubled_flag * 100
            //   strain: 0=C, 1=D, 2=H, 3=S, 4=NT
            //   doubled_flag: 0=undoubled, 1=doubled, 2=redoubled
            //   Examples: 3NT = 34, 4S = 43, 3NT doubled = 134, 4Sx = 143, 4Sxx = 243
            // tricks: 0-13
            if args.len() != 3 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "score".to_string(),
                    expected: 3,
                    got: args.len(),
                });
            }

            let vul_arg = eval(&args[0], ctx)?;
            let contract_code = eval(&args[1], ctx)?;
            let tricks = eval(&args[2], ctx)?;

            // Parse vulnerability
            let vulnerable = vul_arg != 0;

            // Parse contract code
            let doubled_flag = contract_code / 100;
            let remainder = contract_code % 100;
            let level = remainder / 10;
            let strain_num = remainder % 10;

            // Validate level
            if !(1..=7).contains(&level) {
                return Err(EvalError::InvalidArgument(format!(
                    "Invalid contract level: {} (must be 1-7)",
                    level
                )));
            }

            // Parse strain
            let strain = match strain_num {
                0 => Strain::Clubs,
                1 => Strain::Diamonds,
                2 => Strain::Hearts,
                3 => Strain::Spades,
                4 => Strain::NoTrump,
                _ => {
                    return Err(EvalError::InvalidArgument(format!(
                        "Invalid strain: {} (must be 0=C, 1=D, 2=H, 3=S, 4=NT)",
                        strain_num
                    )));
                }
            };

            // Parse doubled state
            let doubled = match doubled_flag {
                0 => Doubled::Undoubled,
                1 => Doubled::Doubled,
                2 => Doubled::Redoubled,
                _ => {
                    return Err(EvalError::InvalidArgument(format!(
                        "Invalid doubled flag: {} (must be 0, 1, or 2)",
                        doubled_flag
                    )));
                }
            };

            // Validate tricks
            if !(0..=13).contains(&tricks) {
                return Err(EvalError::InvalidArgument(format!(
                    "Invalid tricks: {} (must be 0-13)",
                    tricks
                )));
            }

            let contract = Contract {
                level: level as u8,
                strain,
                doubled,
            };

            Ok(calculate_score(vulnerable, &contract, tricks as u8))
        }

        Function::Imps => {
            if args.len() != 1 {
                return Err(EvalError::InvalidArgumentCount {
                    function: "imps".to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }

            // Evaluate the score difference expression
            let score_diff = eval(&args[0], ctx)?;

            // Convert to IMPs using the standard table
            Ok(score_to_imps(score_diff))
        }
    }
}

/// Evaluate an argument that should be a position
fn eval_position_arg(arg: &Expr, _ctx: &EvalContext) -> Result<Position, EvalError> {
    match arg {
        Expr::Position(pos) => Ok(*pos),
        _ => Err(EvalError::InvalidArgument(
            "Expected position (north, south, east, west)".to_string(),
        )),
    }
}

/// Evaluate an argument that should be a suit
fn eval_suit_arg(arg: &Expr) -> Result<Suit, EvalError> {
    match arg {
        Expr::Suit(suit) => Ok(*suit),
        _ => Err(EvalError::InvalidArgument(
            "Expected suit (spades, hearts, diamonds, clubs)".to_string(),
        )),
    }
}

/// Evaluate an argument that should be a card
fn eval_card_arg(arg: &Expr) -> Result<Card, EvalError> {
    match arg {
        Expr::Card(card) => Ok(*card),
        _ => Err(EvalError::InvalidArgument(
            "Expected card (e.g., AS, KH, TC)".to_string(),
        )),
    }
}

/// Evaluate a shape pattern against a hand using precomputed bitmask.
///
/// This is O(1) - just a single bit lookup after computing the hand's shape index.
#[inline]
fn eval_shape_pattern(hand: &dealer_core::Hand, pattern: &ShapePattern) -> Result<bool, EvalError> {
    Ok(pattern.matches_index(hand.shape_index()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dealer_core::{FastDealGenerator, Suit};
    use dealer_parser::parse;

    /// The reference deal these evaluator tests are written against.
    ///
    /// Previously obtained via `DealGenerator::new(1)`, which used the ported GNU
    /// `random()`. Pinning the hand explicitly instead keeps these tests about the
    /// evaluator rather than about whichever RNG happens to be in use — they broke
    /// exactly once, when the generator changed, which is the argument for this.
    const REFERENCE_DEAL: &str =
        "n AKQT3.J6.KJ42.95 e 652.AK42.AQ87.T4 s J74.QT95.T.AK863 w 98.873.9653.QJ72";

    /// North: AKQT3.J6.KJ42.95
    fn reference_deal() -> dealer_core::Deal {
        dealer_pbn::parse_oneline(REFERENCE_DEAL).expect("reference deal must parse")
    }

    #[test]
    fn test_eval_literal() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        let expr = Expr::Literal(42);
        assert_eq!(eval(&expr, &ctx).unwrap(), 42);
    }

    #[test]
    fn test_eval_arithmetic() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // 5 + 3
        let expr = Expr::binary(BinaryOp::Add, Expr::Literal(5), Expr::Literal(3));
        assert_eq!(eval(&expr, &ctx).unwrap(), 8);

        // 10 - 4
        let expr = Expr::binary(BinaryOp::Sub, Expr::Literal(10), Expr::Literal(4));
        assert_eq!(eval(&expr, &ctx).unwrap(), 6);

        // 6 * 7
        let expr = Expr::binary(BinaryOp::Mul, Expr::Literal(6), Expr::Literal(7));
        assert_eq!(eval(&expr, &ctx).unwrap(), 42);
    }

    #[test]
    fn test_eval_comparison() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // 5 > 3 (true)
        let expr = Expr::binary(BinaryOp::Gt, Expr::Literal(5), Expr::Literal(3));
        assert_eq!(eval(&expr, &ctx).unwrap(), 1);

        // 5 < 3 (false)
        let expr = Expr::binary(BinaryOp::Lt, Expr::Literal(5), Expr::Literal(3));
        assert_eq!(eval(&expr, &ctx).unwrap(), 0);
    }

    #[test]
    fn test_eval_logical() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // 1 && 1 (true)
        let expr = Expr::binary(BinaryOp::And, Expr::Literal(1), Expr::Literal(1));
        assert_eq!(eval(&expr, &ctx).unwrap(), 1);

        // 1 && 0 (false)
        let expr = Expr::binary(BinaryOp::And, Expr::Literal(1), Expr::Literal(0));
        assert_eq!(eval(&expr, &ctx).unwrap(), 0);

        // 0 || 1 (true)
        let expr = Expr::binary(BinaryOp::Or, Expr::Literal(0), Expr::Literal(1));
        assert_eq!(eval(&expr, &ctx).unwrap(), 1);
    }

    #[test]
    fn test_eval_hcp_function() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Get north's HCP
        let north_hand = deal.hand(Position::North);
        let expected_hcp = north_hand.hcp() as i32;

        let expr = Expr::call(Function::Hcp, Expr::Position(Position::North));
        assert_eq!(eval(&expr, &ctx).unwrap(), expected_hcp);
    }

    #[test]
    fn test_eval_suit_length_functions() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        let south_hand = deal.hand(Position::South);

        // hearts(south)
        let expr = Expr::call(Function::Hearts, Expr::Position(Position::South));
        assert_eq!(
            eval(&expr, &ctx).unwrap(),
            south_hand.suit_length(Suit::Hearts) as i32
        );

        // spades(south)
        let expr = Expr::call(Function::Spades, Expr::Position(Position::South));
        assert_eq!(
            eval(&expr, &ctx).unwrap(),
            south_hand.suit_length(Suit::Spades) as i32
        );

        // diamonds(south)
        let expr = Expr::call(Function::Diamonds, Expr::Position(Position::South));
        assert_eq!(
            eval(&expr, &ctx).unwrap(),
            south_hand.suit_length(Suit::Diamonds) as i32
        );

        // clubs(south)
        let expr = Expr::call(Function::Clubs, Expr::Position(Position::South));
        assert_eq!(
            eval(&expr, &ctx).unwrap(),
            south_hand.suit_length(Suit::Clubs) as i32
        );
    }

    #[test]
    fn test_eval_parsed_constraint() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Parse and evaluate: hcp(north) >= 15
        let ast = parse("hcp(north) >= 15").unwrap();
        let result = eval(&ast, &ctx).unwrap();

        let north_hcp = deal.hand(Position::North).hcp();
        let expected = if north_hcp >= 15 { 1 } else { 0 };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_eval_complex_constraint() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Parse and evaluate: hearts(north) >= 5 && hcp(south) <= 13
        let ast = parse("hearts(north) >= 5 && hcp(south) <= 13").unwrap();
        let result = eval(&ast, &ctx).unwrap();

        let north_hearts = deal.hand(Position::North).suit_length(Suit::Hearts);
        let south_hcp = deal.hand(Position::South).hcp();
        let expected = if north_hearts >= 5 && south_hcp <= 13 {
            1
        } else {
            0
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_eval_arithmetic_combination() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Parse and evaluate: hcp(north) + hcp(south) >= 25
        let ast = parse("hcp(north) + hcp(south) >= 25").unwrap();
        let result = eval(&ast, &ctx).unwrap();

        let north_hcp = deal.hand(Position::North).hcp();
        let south_hcp = deal.hand(Position::South).hcp();
        let expected = if north_hcp + south_hcp >= 25 { 1 } else { 0 };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_shape_exact() {
        use dealer_parser::preprocess;

        // North has 5-2-4-2 shape: AKQT3.J6.KJ42.95
        let deal = reference_deal();
        let ctx = EvalContext::new(&deal);

        // North has 5 spades, 2 hearts, 4 diamonds, 2 clubs
        let input = "shape(north, 5242)";
        let ast = parse(&preprocess(input)).unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, 1);

        // Should not match different exact shape
        let input = "shape(north, 5431)";
        let ast = parse(&preprocess(input)).unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_shape_any_distribution() {
        // Find a hand with 4-3-3-3 distribution (balanced)
        let mut gen = FastDealGenerator::new(1);
        let mut found_4333 = false;

        for _ in 0..1000 {
            let deal = gen.next_deal();
            let north = deal.hand(Position::North);
            let dist = north.distribution();

            if dist == [4, 3, 3, 3] {
                let ctx = EvalContext::new(&deal);
                let ast = parse("shape(north, any 4333)").unwrap();
                let result = eval(&ast, &ctx).unwrap();
                assert_eq!(result, 1);
                found_4333 = true;
                break;
            }
        }

        assert!(
            found_4333,
            "Should find at least one 4-3-3-3 hand in 1000 deals"
        );
    }

    #[test]
    fn test_shape_wildcard() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);
        let north = deal.hand(Position::North);
        let lengths = north.suit_lengths();

        // Build pattern 54xx dynamically based on actual hand
        if lengths[0] == 5 && lengths[1] == 4 {
            let ast = parse("shape(north, 54xx)").unwrap();
            let result = eval(&ast, &ctx).unwrap();
            assert_eq!(result, 1);
        }

        // Pattern that definitely won't match
        let ast = parse("shape(north, xx00)").unwrap(); // No voids in first 2 suits
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_shape_combination() {
        let mut gen = FastDealGenerator::new(42);
        let mut found = false;

        // Find a hand that's balanced (4333 or 4432 or 5332)
        for _ in 0..1000 {
            let deal = gen.next_deal();
            let north = deal.hand(Position::North);
            let dist = north.distribution();

            if matches!(dist, [4, 3, 3, 3] | [4, 4, 3, 2] | [5, 3, 3, 2]) {
                let ctx = EvalContext::new(&deal);
                let ast = parse("shape(north, any 4333 + any 4432 + any 5332)").unwrap();
                let result = eval(&ast, &ctx).unwrap();
                assert_eq!(result, 1);
                found = true;
                break;
            }
        }

        assert!(found, "Should find balanced hand in 1000 deals");
    }

    #[test]
    fn test_shape_exclusion() {
        use dealer_parser::preprocess;

        // Test that exclusion pattern works
        let mut gen = FastDealGenerator::new(1);

        for _ in 0..100 {
            let deal = gen.next_deal();
            let ctx = EvalContext::new(&deal);
            let north = deal.hand(Position::North);
            let lengths = north.suit_lengths();

            // any 4333 - 4333 should match 4333 distributions that aren't exactly S=4,H=3,D=3,C=3
            let input = "shape(north, any 4333 - 4333)";
            let ast = parse(&preprocess(input)).unwrap();
            let result = eval(&ast, &ctx).unwrap();

            let dist = north.distribution();
            let is_any_4333 = dist == [4, 3, 3, 3];
            let is_exact_4333 = lengths == [4, 3, 3, 3];

            if is_any_4333 && !is_exact_4333 {
                assert_eq!(
                    result, 1,
                    "Should match 4333 distributions except exact 4333"
                );
            } else if is_exact_4333 {
                assert_eq!(result, 0, "Should not match exact 4333 (excluded)");
            }
        }
    }

    #[test]
    fn test_losers_total() {
        // North: AKQT3.J6.KJ42.95
        // Spades AKQ = 0, Hearts doubleton no honors = 2, Diamonds K = 2, Clubs doubleton no honors = 2
        // Total = 6 losers
        let deal = reference_deal();
        let ctx = EvalContext::new(&deal);

        let ast = parse("losers(north)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, 6);
    }

    #[test]
    fn test_losers_in_suit() {
        // Test losers in a specific suit
        let deal = reference_deal();
        let ctx = EvalContext::new(&deal);

        // North has AKQ in spades → 0 losers
        let ast = parse("losers(north, spades)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, 0);

        // North has J6 in hearts → 2 losers (doubleton without A or K)
        let ast = parse("losers(north, hearts)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, 2);
    }

    #[test]
    fn test_hascard() {
        // North: AKQT3.J6.KJ42.95
        let deal = reference_deal();
        let ctx = EvalContext::new(&deal);

        // North has AS
        let ast = parse("hascard(north, AS)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, 1);

        // North doesn't have 2S
        let ast = parse("hascard(north, 2S)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, 0);

        // North has KD
        let ast = parse("hascard(north, KD)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_losers_various_holdings() {
        use dealer_core::{Card, Hand, Rank, Suit};

        // Test specific loser counts for known holdings
        let mut hand = Hand::new();

        // AKQxx in spades = 0 losers
        hand.add_card(Card::new(Suit::Spades, Rank::Ace));
        hand.add_card(Card::new(Suit::Spades, Rank::King));
        hand.add_card(Card::new(Suit::Spades, Rank::Queen));
        hand.add_card(Card::new(Suit::Spades, Rank::Jack));
        hand.add_card(Card::new(Suit::Spades, Rank::Ten));

        assert_eq!(hand.losers_in_suit(Suit::Spades), 0);

        // Singleton Ace = 0 losers
        let mut hand2 = Hand::new();
        hand2.add_card(Card::new(Suit::Hearts, Rank::Ace));
        assert_eq!(hand2.losers_in_suit(Suit::Hearts), 0);

        // Singleton King = 1 loser
        let mut hand3 = Hand::new();
        hand3.add_card(Card::new(Suit::Hearts, Rank::King));
        assert_eq!(hand3.losers_in_suit(Suit::Hearts), 1);

        // AK doubleton = 0 losers
        let mut hand4 = Hand::new();
        hand4.add_card(Card::new(Suit::Diamonds, Rank::Ace));
        hand4.add_card(Card::new(Suit::Diamonds, Rank::King));
        assert_eq!(hand4.losers_in_suit(Suit::Diamonds), 0);

        // Qx doubleton = 2 losers
        let mut hand5 = Hand::new();
        hand5.add_card(Card::new(Suit::Clubs, Rank::Queen));
        hand5.add_card(Card::new(Suit::Clubs, Rank::Two));
        assert_eq!(hand5.losers_in_suit(Suit::Clubs), 2);
    }

    #[test]
    fn test_honor_count_functions() {
        use dealer_core::{Card, Hand, Rank, Suit};

        let mut hand = Hand::new();
        // Add AKQJT in spades
        hand.add_card(Card::new(Suit::Spades, Rank::Ace));
        hand.add_card(Card::new(Suit::Spades, Rank::King));
        hand.add_card(Card::new(Suit::Spades, Rank::Queen));
        hand.add_card(Card::new(Suit::Spades, Rank::Jack));
        hand.add_card(Card::new(Suit::Spades, Rank::Ten));

        // Add KQ in hearts
        hand.add_card(Card::new(Suit::Hearts, Rank::King));
        hand.add_card(Card::new(Suit::Hearts, Rank::Queen));

        // Add A in diamonds
        hand.add_card(Card::new(Suit::Diamonds, Rank::Ace));

        // Test individual honor counts
        assert_eq!(hand.aces(), 2);
        assert_eq!(hand.kings(), 2);
        assert_eq!(hand.queens(), 2);
        assert_eq!(hand.jacks(), 1);
        assert_eq!(hand.tens(), 1);

        // Test suit-specific counts
        assert_eq!(hand.aces_in_suit(Suit::Spades), 1);
        assert_eq!(hand.kings_in_suit(Suit::Hearts), 1);
    }

    #[test]
    fn test_top_honors_functions() {
        use dealer_core::{Card, Hand, Rank, Suit};

        let mut hand = Hand::new();
        // AKQJT in spades, KQ in hearts, A in diamonds
        hand.add_card(Card::new(Suit::Spades, Rank::Ace));
        hand.add_card(Card::new(Suit::Spades, Rank::King));
        hand.add_card(Card::new(Suit::Spades, Rank::Queen));
        hand.add_card(Card::new(Suit::Spades, Rank::Jack));
        hand.add_card(Card::new(Suit::Spades, Rank::Ten));
        hand.add_card(Card::new(Suit::Hearts, Rank::King));
        hand.add_card(Card::new(Suit::Hearts, Rank::Queen));
        hand.add_card(Card::new(Suit::Diamonds, Rank::Ace));

        // Test top2 (AK)
        assert_eq!(hand.top2(), 4); // AS, KS, KH, AD
        assert_eq!(hand.top2_in_suit(Suit::Spades), 2); // AS, KS

        // Test top3 (AKQ)
        assert_eq!(hand.top3(), 6); // AS, KS, QS, KH, QH, AD
        assert_eq!(hand.top3_in_suit(Suit::Hearts), 2); // KH, QH

        // Test top4 (AKQJ)
        assert_eq!(hand.top4(), 7); // +JS
        assert_eq!(hand.top4_in_suit(Suit::Spades), 4);

        // Test top5 (AKQJT)
        assert_eq!(hand.top5(), 8); // +TS
        assert_eq!(hand.top5_in_suit(Suit::Spades), 5);
    }

    #[test]
    fn test_c13_points() {
        use dealer_core::{Card, Hand, Rank, Suit};

        let mut hand = Hand::new();
        // A=6, K=4, Q=2, J=1
        hand.add_card(Card::new(Suit::Spades, Rank::Ace)); // 6
        hand.add_card(Card::new(Suit::Hearts, Rank::King)); // 4
        hand.add_card(Card::new(Suit::Diamonds, Rank::Queen)); // 2
        hand.add_card(Card::new(Suit::Clubs, Rank::Jack)); // 1

        assert_eq!(hand.c13(), 13);
        assert_eq!(hand.c13_in_suit(Suit::Spades), 6);
        assert_eq!(hand.c13_in_suit(Suit::Hearts), 4);
    }

    #[test]
    fn test_eval_tens_function() {
        // North: AKQT3.J6.KJ42.95
        let deal = reference_deal();
        let ctx = EvalContext::new(&deal);

        // North has T in spades
        let ast = parse("tens(north)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, 1);

        let ast = parse("tens(north, spades)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_eval_aces_kings_function() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Test aces(north)
        let ast = parse("aces(north)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        let north = deal.hand(Position::North);
        assert_eq!(result, north.aces() as i32);

        // Test kings(north)
        let ast = parse("kings(north)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, north.kings() as i32);
    }

    #[test]
    fn test_eval_top_honors() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);
        let north = deal.hand(Position::North);

        // Test top2
        let ast = parse("top2(north)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, north.top2() as i32);

        // Test top3
        let ast = parse("top3(north)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, north.top3() as i32);

        // Test top4
        let ast = parse("top4(north)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, north.top4() as i32);

        // Test top5
        let ast = parse("top5(north)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, north.top5() as i32);
    }

    #[test]
    fn test_eval_c13() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);
        let north = deal.hand(Position::North);

        let ast = parse("c13(north)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, north.c13() as i32);
    }

    #[test]
    fn test_pt_synonyms() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Test that pt0-pt9 work as synonyms
        let ast1 = parse("tens(north)").unwrap();
        let ast2 = parse("pt0(north)").unwrap();
        assert_eq!(eval(&ast1, &ctx).unwrap(), eval(&ast2, &ctx).unwrap());

        let ast1 = parse("aces(north)").unwrap();
        let ast2 = parse("pt4(north)").unwrap();
        assert_eq!(eval(&ast1, &ctx).unwrap(), eval(&ast2, &ctx).unwrap());

        let ast1 = parse("top3(north)").unwrap();
        let ast2 = parse("pt6(north)").unwrap();
        assert_eq!(eval(&ast1, &ctx).unwrap(), eval(&ast2, &ctx).unwrap());

        let ast1 = parse("c13(north)").unwrap();
        let ast2 = parse("pt9(north)").unwrap();
        assert_eq!(eval(&ast1, &ctx).unwrap(), eval(&ast2, &ctx).unwrap());
    }

    #[test]
    fn test_quality_function() {
        use dealer_core::{Card, Hand, Rank, Suit};

        // Create a hand with a strong spade suit: AKQ32
        let mut hand = Hand::new();
        hand.add_card(Card::new(Suit::Spades, Rank::Ace));
        hand.add_card(Card::new(Suit::Spades, Rank::King));
        hand.add_card(Card::new(Suit::Spades, Rank::Queen));
        hand.add_card(Card::new(Suit::Spades, Rank::Three));
        hand.add_card(Card::new(Suit::Spades, Rank::Two));

        // Add filler cards in other suits
        for _ in 0..3 {
            hand.add_card(Card::new(Suit::Hearts, Rank::Two));
        }
        for _ in 0..3 {
            hand.add_card(Card::new(Suit::Diamonds, Rank::Two));
        }
        for _ in 0..2 {
            hand.add_card(Card::new(Suit::Clubs, Rank::Two));
        }

        // Calculate expected quality for spades
        // Length = 5, SuitFactor = 50
        // A = 4*50 = 200, K = 3*50 = 150, Q = 2*50 = 100
        // Total = 450
        let quality = hand.suit_quality(Suit::Spades);
        assert_eq!(quality, 450);

        // Test with void suit
        let quality_clubs = hand.suit_quality(Suit::Clubs);
        // Clubs has 2 cards (length 2, SuitFactor = 20), but no honors
        assert_eq!(quality_clubs, 0);
    }

    #[test]
    fn test_quality_with_tens_and_nines() {
        use dealer_core::{Card, Hand, Rank, Suit};

        // Create a hand with AKT9 in hearts
        let mut hand = Hand::new();
        hand.add_card(Card::new(Suit::Hearts, Rank::Ace));
        hand.add_card(Card::new(Suit::Hearts, Rank::King));
        hand.add_card(Card::new(Suit::Hearts, Rank::Ten));
        hand.add_card(Card::new(Suit::Hearts, Rank::Nine));

        // Add filler cards
        for _ in 0..9 {
            hand.add_card(Card::new(Suit::Spades, Rank::Two));
        }

        // Length = 4, SuitFactor = 40
        // A = 4*40 = 160, K = 3*40 = 120
        // HigherHonors = 2 when we reach T
        // T with HigherHonors > 1: +40
        // 9 with HigherHonors == 2: +20 (half of 40)
        // Total = 160 + 120 + 40 + 20 = 340
        let quality = hand.suit_quality(Suit::Hearts);
        assert_eq!(quality, 340);
    }

    #[test]
    fn test_cccc_function() {
        use dealer_core::{Card, Hand, Rank, Suit};

        // Create a balanced hand with 15 HCP
        let mut north = Hand::new();
        north.add_card(Card::new(Suit::Spades, Rank::Ace));
        north.add_card(Card::new(Suit::Spades, Rank::King));
        north.add_card(Card::new(Suit::Spades, Rank::Three));
        north.add_card(Card::new(Suit::Spades, Rank::Two));

        north.add_card(Card::new(Suit::Hearts, Rank::Queen));
        north.add_card(Card::new(Suit::Hearts, Rank::Jack));
        north.add_card(Card::new(Suit::Hearts, Rank::Ten));

        north.add_card(Card::new(Suit::Diamonds, Rank::King));
        north.add_card(Card::new(Suit::Diamonds, Rank::Nine));
        north.add_card(Card::new(Suit::Diamonds, Rank::Eight));

        north.add_card(Card::new(Suit::Clubs, Rank::Ace));
        north.add_card(Card::new(Suit::Clubs, Rank::Four));
        north.add_card(Card::new(Suit::Clubs, Rank::Two));

        let cccc_value = north.cccc();

        // This should be a positive value (values are multiplied by 100)
        // The exact value depends on the algorithm, but it should be reasonable
        assert!(cccc_value > 0);

        // For this hand with 15 HCP and balanced shape, expect value around 1200-1800
        assert!(cccc_value > 1000 && cccc_value < 2000);
    }

    #[test]
    fn test_cccc_with_singleton_honors() {
        use dealer_core::{Card, Hand, Rank, Suit};

        // Create a hand with singleton King (penalized)
        let mut hand = Hand::new();
        hand.add_card(Card::new(Suit::Spades, Rank::Ace));
        hand.add_card(Card::new(Suit::Spades, Rank::Queen));
        hand.add_card(Card::new(Suit::Spades, Rank::Jack));
        hand.add_card(Card::new(Suit::Spades, Rank::Ten));
        hand.add_card(Card::new(Suit::Spades, Rank::Nine));

        hand.add_card(Card::new(Suit::Hearts, Rank::King)); // Singleton King

        hand.add_card(Card::new(Suit::Diamonds, Rank::Ace));
        hand.add_card(Card::new(Suit::Diamonds, Rank::Three));
        hand.add_card(Card::new(Suit::Diamonds, Rank::Two));

        hand.add_card(Card::new(Suit::Clubs, Rank::Queen));
        hand.add_card(Card::new(Suit::Clubs, Rank::Five));
        hand.add_card(Card::new(Suit::Clubs, Rank::Four));
        hand.add_card(Card::new(Suit::Clubs, Rank::Two));

        let cccc_value = hand.cccc();

        // The singleton King gets +200 but -150 penalty = +50 net
        // Plus shape points for singleton
        assert!(cccc_value > 0);
    }

    #[test]
    fn test_eval_quality() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Parse and evaluate quality function
        let ast = parse("quality(north, spades)").unwrap();
        let result = eval(&ast, &ctx).unwrap();

        // Result should match direct calculation
        let north = deal.hand(Position::North);
        assert_eq!(result, north.suit_quality(Suit::Spades));
    }

    #[test]
    fn test_eval_cccc() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Parse and evaluate cccc function
        let ast = parse("cccc(north)").unwrap();
        let result = eval(&ast, &ctx).unwrap();

        // Result should match direct calculation
        let north = deal.hand(Position::North);
        assert_eq!(result, north.cccc());
    }

    #[test]
    fn test_cccc_constraint() {
        let mut gen = FastDealGenerator::new(42);

        // Find a deal where north has a good CCCC evaluation
        let mut found = false;
        for _ in 0..1000 {
            let deal = gen.next_deal();
            let north = deal.hand(Position::North);

            // CCCC values around 1500+ indicate strong hands
            if north.cccc() >= 1500 {
                found = true;
                break;
            }
        }

        assert!(found, "Should find a deal with high CCCC value");
    }

    #[test]
    fn test_four_digit_number_in_shape() {
        // Test that 4-digit numbers in shape() functions work correctly
        // when preprocessed with %s marker
        use dealer_parser::preprocess;

        let deal = reference_deal();
        let ctx = EvalContext::new(&deal);

        // North has 5-2-4-2 shape with seed 1
        let input = "shape(north, 5242)";
        let preprocessed = preprocess(input);
        assert_eq!(preprocessed, "shape(north, %s5242)");

        let ast = parse(&preprocessed).unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_four_digit_number_in_comparison() {
        // Test that 4-digit numbers in regular comparisons work correctly
        use dealer_parser::preprocess;

        let mut gen = FastDealGenerator::new(42);

        // Find a deal with CCCC >= 1500
        let mut found = false;
        for _ in 0..1000 {
            let deal = gen.next_deal();
            let ctx = EvalContext::new(&deal);

            let input = "cccc(north) >= 1500";
            let preprocessed = preprocess(input);
            // Preprocessor should NOT mark this 1500 (not in shape function)
            assert_eq!(preprocessed, "cccc(north) >= 1500");

            let ast = parse(&preprocessed).unwrap();
            let result = eval(&ast, &ctx).unwrap();

            let north = deal.hand(Position::North);
            let expected = if north.cccc() >= 1500 { 1 } else { 0 };
            assert_eq!(result, expected);

            if result == 1 {
                found = true;
                break;
            }
        }

        assert!(found, "Should find a deal with CCCC >= 1500");
    }

    #[test]
    fn test_four_digit_mixed_expression() {
        // Test expression with both shape pattern AND 4-digit comparison
        use dealer_parser::preprocess;

        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // North has 5-2-4-2 shape and CCCC value with seed 1
        let input = "cccc(north) >= 1000 && shape(north, 5242)";
        let preprocessed = preprocess(input);
        // Only the shape number should be marked
        assert_eq!(preprocessed, "cccc(north) >= 1000 && shape(north, %s5242)");

        let ast = parse(&preprocessed).unwrap();
        let result = eval(&ast, &ctx).unwrap();

        let north = deal.hand(Position::North);
        let cccc_value = north.cccc();
        let cccc_ok = cccc_value >= 1000;
        // Check exact suit lengths in S-H-D-C order
        let shape_ok = north.suit_lengths() == [5, 2, 4, 2];
        let expected = if cccc_ok && shape_ok { 1 } else { 0 };

        assert_eq!(
            result,
            expected,
            "CCCC value: {}, suit_lengths: {:?}, cccc_ok: {}, shape_ok: {}",
            cccc_value,
            north.suit_lengths(),
            cccc_ok,
            shape_ok
        );
    }

    #[test]
    fn test_eval_program_simple_variable() {
        use dealer_parser::parse_program;

        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();

        // Simple variable assignment and usage
        let input = "opener = hcp(north) >= 15\nopener";
        let program = parse_program(input).unwrap();
        let result = eval_program(&program, &deal).unwrap();

        let north = deal.hand(Position::North);
        let expected = if north.hcp() >= 15 { 1 } else { 0 };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_eval_program_multiple_variables() {
        use dealer_parser::parse_program;

        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();

        // Multiple variables
        let input =
            "strong = hcp(north) >= 15\nlong_hearts = hearts(north) >= 5\nstrong && long_hearts";
        let program = parse_program(input).unwrap();
        let result = eval_program(&program, &deal).unwrap();

        let north = deal.hand(Position::North);
        let expected = if north.hcp() >= 15 && north.suit_length(Suit::Hearts) >= 5 {
            1
        } else {
            0
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_eval_program_variable_reference_in_expression() {
        use dealer_parser::parse_program;

        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();

        // Use variable in arithmetic expression
        let input = "points = hcp(north)\npoints + hcp(south) >= 25";
        let program = parse_program(input).unwrap();
        let result = eval_program(&program, &deal).unwrap();

        let north = deal.hand(Position::North);
        let south = deal.hand(Position::South);
        let expected = if north.hcp() + south.hcp() >= 25 {
            1
        } else {
            0
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_eval_program_variables_referencing_variables() {
        use dealer_parser::parse_program;

        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();

        // Variable referencing another variable
        let input = "north_hcp = hcp(north)\nopener = north_hcp >= 15\nopener";
        let program = parse_program(input).unwrap();
        let result = eval_program(&program, &deal).unwrap();

        let north = deal.hand(Position::North);
        let expected = if north.hcp() >= 15 { 1 } else { 0 };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_eval_program_undefined_variable() {
        use dealer_parser::parse_program;

        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();

        // Reference undefined variable
        let input = "undefined_var >= 15";
        let program = parse_program(input).unwrap();
        let result = eval_program(&program, &deal);

        assert!(result.is_err());
        match result {
            Err(EvalError::UndefinedVariable(name)) => {
                assert_eq!(name, "undefined_var");
            }
            _ => panic!("Expected UndefinedVariable error"),
        }
    }

    #[test]
    fn test_eval_program_complex_example() {
        use dealer_parser::parse_program;

        let mut gen = FastDealGenerator::new(1);

        // Find a deal matching complex constraints
        let mut found = false;
        for _ in 0..1000 {
            let deal = gen.next_deal();

            let input = "nt_opener = hcp(north) >= 15 && hcp(north) <= 17\n\
                         balanced = shape(north, any 4333 + any 4432 + any 5332)\n\
                         nt_opener && balanced";
            let program = parse_program(input).unwrap();
            let result = eval_program(&program, &deal).unwrap();

            if result != 0 {
                let north = deal.hand(Position::North);
                let hcp = north.hcp();
                assert!((15..=17).contains(&hcp), "HCP should be 15-17, got {}", hcp);

                let lengths = north.suit_lengths();
                let is_balanced = north.is_balanced();
                assert!(
                    is_balanced,
                    "Hand should be balanced, got suit lengths {:?}",
                    lengths
                );

                found = true;
                break;
            }
        }

        assert!(found, "Should find at least one 1NT opening hand");
    }

    #[test]
    fn test_eval_program_no_final_expression() {
        use dealer_parser::parse_program;

        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();

        // Program with only assignments, no final expression
        let input = "x = hcp(north)\ny = hcp(south)";
        let program = parse_program(input).unwrap();
        let result = eval_program(&program, &deal);

        assert!(result.is_err());
        match result {
            Err(EvalError::InvalidArgument(msg)) => {
                assert!(msg.contains("constraint expression"));
            }
            _ => panic!("Expected InvalidArgument error"),
        }
    }

    #[test]
    fn test_eval_ternary_operator() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Simple ternary: hcp(north) >= 15 ? 1 : 0
        let ast = parse("hcp(north) >= 15 ? 1 : 0").unwrap();
        let result = eval(&ast, &ctx).unwrap();

        let north_hcp = deal.hand(Position::North).hcp();
        let expected = if north_hcp >= 15 { 1 } else { 0 };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_eval_ternary_with_arithmetic() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Ternary with arithmetic: hcp(north) >= 20 ? hcp(north) + 100 : hcp(north)
        let ast = parse("hcp(north) >= 20 ? hcp(north) + 100 : hcp(north)").unwrap();
        let result = eval(&ast, &ctx).unwrap();

        let north_hcp = deal.hand(Position::North).hcp() as i32;
        let expected = if north_hcp >= 20 {
            north_hcp + 100
        } else {
            north_hcp
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_eval_nested_ternary() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Nested ternary: hcp(north) >= 15 ? (hearts(north) >= 5 ? 2 : 1) : 0
        let ast = parse("hcp(north) >= 15 ? (hearts(north) >= 5 ? 2 : 1) : 0").unwrap();
        let result = eval(&ast, &ctx).unwrap();

        let north = deal.hand(Position::North);
        let expected = if north.hcp() >= 15 {
            if north.suit_length(Suit::Hearts) >= 5 {
                2
            } else {
                1
            }
        } else {
            0
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_eval_logical_not() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Test ! operator: !(hcp(north) < 10)
        let ast = parse("!(hcp(north) < 10)").unwrap();
        let result = eval(&ast, &ctx).unwrap();

        let north_hcp = deal.hand(Position::North).hcp();
        let expected = if north_hcp < 10 { 0 } else { 1 };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_eval_not_keyword() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Test not keyword: not (hcp(north) >= 20)
        let ast = parse("not (hcp(north) >= 20)").unwrap();
        let result = eval(&ast, &ctx).unwrap();

        let north_hcp = deal.hand(Position::North).hcp();
        let expected = if north_hcp >= 20 { 0 } else { 1 };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_eval_not_in_compound() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Test NOT in compound expression: hcp(north) >= 15 && not (hearts(north) >= 5)
        let ast = parse("hcp(north) >= 15 && not (hearts(north) >= 5)").unwrap();
        let result = eval(&ast, &ctx).unwrap();

        let north = deal.hand(Position::North);
        let expected = if north.hcp() >= 15 && north.suit_length(Suit::Hearts) < 5 {
            1
        } else {
            0
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_score_to_imps() {
        // Test IMP conversion with standard table
        assert_eq!(score_to_imps(0), 0); // 0 points = 0 IMPs
        assert_eq!(score_to_imps(9), 0); // < 10 = 0 IMPs
        assert_eq!(score_to_imps(10), 1); // 10-39 = 1 IMP
        assert_eq!(score_to_imps(39), 1);
        assert_eq!(score_to_imps(40), 2); // 40-79 = 2 IMPs
        assert_eq!(score_to_imps(79), 2);
        assert_eq!(score_to_imps(80), 3); // 80-119 = 3 IMPs
        assert_eq!(score_to_imps(410), 10); // 410-489 = 10 IMPs
        assert_eq!(score_to_imps(420), 10); // 410-489 = 10 IMPs
        assert_eq!(score_to_imps(490), 11); // 490-589 = 11 IMPs
        assert_eq!(score_to_imps(1500), 17); // 1490-1739 = 17 IMPs
        assert_eq!(score_to_imps(4000), 24); // 3990+ = 24 IMPs

        // Test negative values (preserve sign)
        assert_eq!(score_to_imps(-10), -1);
        assert_eq!(score_to_imps(-420), -10);
        assert_eq!(score_to_imps(-1500), -17);
    }

    #[test]
    fn test_eval_imps() {
        use dealer_parser::parse;

        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // imps(420) should return 10 (410-489 = 10 IMPs)
        let ast = parse("imps(420)").unwrap();
        assert_eq!(eval(&ast, &ctx).unwrap(), 10);

        // imps with negative value using subtraction (parser issue with negative literals in function args)
        let ast = parse("imps(0 - 420)").unwrap();
        assert_eq!(eval(&ast, &ctx).unwrap(), -10);

        // imps with expression
        let ast = parse("imps(400 + 20)").unwrap();
        assert_eq!(eval(&ast, &ctx).unwrap(), 10);

        // imps(0)
        let ast = parse("imps(0)").unwrap();
        assert_eq!(eval(&ast, &ctx).unwrap(), 0);
    }

    #[test]
    fn test_contract_parse() {
        // Test contract parsing
        let c = Contract::parse("3n").unwrap();
        assert_eq!(c.level, 3);
        assert_eq!(c.strain, Strain::NoTrump);
        assert_eq!(c.doubled, Doubled::Undoubled);

        let c = Contract::parse("4s").unwrap();
        assert_eq!(c.level, 4);
        assert_eq!(c.strain, Strain::Spades);
        assert_eq!(c.doubled, Doubled::Undoubled);

        let c = Contract::parse("7nt").unwrap();
        assert_eq!(c.level, 7);
        assert_eq!(c.strain, Strain::NoTrump);
        assert_eq!(c.doubled, Doubled::Undoubled);

        let c = Contract::parse("3hx").unwrap();
        assert_eq!(c.level, 3);
        assert_eq!(c.strain, Strain::Hearts);
        assert_eq!(c.doubled, Doubled::Doubled);

        let c = Contract::parse("4sxx").unwrap();
        assert_eq!(c.level, 4);
        assert_eq!(c.strain, Strain::Spades);
        assert_eq!(c.doubled, Doubled::Redoubled);

        // Invalid contracts
        assert!(Contract::parse("8n").is_none()); // Level > 7
        assert!(Contract::parse("0s").is_none()); // Level < 1
        assert!(Contract::parse("3x").is_none()); // Invalid strain
    }

    #[test]
    fn test_calculate_score_made_contracts() {
        // 3NT making exactly = 400 non-vul (100 trick score + 300 game bonus)
        let contract = Contract {
            level: 3,
            strain: Strain::NoTrump,
            doubled: Doubled::Undoubled,
        };
        assert_eq!(calculate_score(false, &contract, 9), 400);

        // 3NT making exactly = 600 vul (100 trick score + 500 game bonus)
        assert_eq!(calculate_score(true, &contract, 9), 600);

        // 3NT making with 1 overtrick = 430 non-vul
        assert_eq!(calculate_score(false, &contract, 10), 430);

        // 4H making exactly = 420 non-vul (120 trick score + 300 game bonus)
        let contract = Contract {
            level: 4,
            strain: Strain::Hearts,
            doubled: Doubled::Undoubled,
        };
        assert_eq!(calculate_score(false, &contract, 10), 420);

        // 4S making exactly = 620 vul
        let contract = Contract {
            level: 4,
            strain: Strain::Spades,
            doubled: Doubled::Undoubled,
        };
        assert_eq!(calculate_score(true, &contract, 10), 620);

        // 5C making exactly = 400 non-vul (100 trick score + 300 game bonus)
        let contract = Contract {
            level: 5,
            strain: Strain::Clubs,
            doubled: Doubled::Undoubled,
        };
        assert_eq!(calculate_score(false, &contract, 11), 400);

        // 6NT making = 990 non-vul (190 trick score + 300 game + 500 small slam)
        let contract = Contract {
            level: 6,
            strain: Strain::NoTrump,
            doubled: Doubled::Undoubled,
        };
        assert_eq!(calculate_score(false, &contract, 12), 990);

        // 6NT making = 1440 vul
        assert_eq!(calculate_score(true, &contract, 12), 1440);

        // 7NT making = 1520 non-vul
        let contract = Contract {
            level: 7,
            strain: Strain::NoTrump,
            doubled: Doubled::Undoubled,
        };
        assert_eq!(calculate_score(false, &contract, 13), 1520);

        // 7NT making = 2220 vul
        assert_eq!(calculate_score(true, &contract, 13), 2220);

        // 2C partscore = 90 non-vul (40 trick score + 50 partscore)
        let contract = Contract {
            level: 2,
            strain: Strain::Clubs,
            doubled: Doubled::Undoubled,
        };
        assert_eq!(calculate_score(false, &contract, 8), 90);

        // 1NT doubled making = 180 + 50 + 300 = 530 non-vul (redoubled game)
        let contract = Contract {
            level: 1,
            strain: Strain::NoTrump,
            doubled: Doubled::Doubled,
        };
        assert_eq!(calculate_score(false, &contract, 7), 180);

        // 3NT doubled making = 750 non-vul (200 trick score + 300 game + 200 insult)
        // Wait, 3NT = 100, doubled = 200 which is game, so 200 + 300 + 50 = 550
        let contract = Contract {
            level: 3,
            strain: Strain::NoTrump,
            doubled: Doubled::Doubled,
        };
        assert_eq!(calculate_score(false, &contract, 9), 550);
    }

    #[test]
    fn test_calculate_score_failed_contracts() {
        // 3NT down 1 = -50 non-vul
        let contract = Contract {
            level: 3,
            strain: Strain::NoTrump,
            doubled: Doubled::Undoubled,
        };
        assert_eq!(calculate_score(false, &contract, 8), -50);

        // 3NT down 1 = -100 vul
        assert_eq!(calculate_score(true, &contract, 8), -100);

        // 3NT down 3 = -150 non-vul
        assert_eq!(calculate_score(false, &contract, 6), -150);

        // 4H doubled down 1 = -100 non-vul
        let contract = Contract {
            level: 4,
            strain: Strain::Hearts,
            doubled: Doubled::Doubled,
        };
        assert_eq!(calculate_score(false, &contract, 9), -100);

        // 4H doubled down 1 = -200 vul
        assert_eq!(calculate_score(true, &contract, 9), -200);

        // 4H doubled down 2 = -300 non-vul (100 + 200)
        assert_eq!(calculate_score(false, &contract, 8), -300);

        // 4H doubled down 3 = -500 non-vul (100 + 200 + 200)
        assert_eq!(calculate_score(false, &contract, 7), -500);

        // 4H doubled down 4 = -800 non-vul (100 + 200 + 200 + 300)
        assert_eq!(calculate_score(false, &contract, 6), -800);

        // 4H doubled down 2 = -500 vul (200 + 300)
        assert_eq!(calculate_score(true, &contract, 8), -500);

        // 4H redoubled down 1 = -200 non-vul
        let contract = Contract {
            level: 4,
            strain: Strain::Hearts,
            doubled: Doubled::Redoubled,
        };
        assert_eq!(calculate_score(false, &contract, 9), -200);

        // 4H redoubled down 1 = -400 vul
        assert_eq!(calculate_score(true, &contract, 9), -400);
    }

    #[test]
    fn test_eval_score() {
        use dealer_parser::parse;

        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // score(0, 34, 9) = 3NT non-vul making exactly = 400
        // Contract code: 34 = level 3, strain 4 (NT)
        let ast = parse("score(0, 34, 9)").unwrap();
        assert_eq!(eval(&ast, &ctx).unwrap(), 400);

        // score(1, 34, 9) = 3NT vul making exactly = 600
        let ast = parse("score(1, 34, 9)").unwrap();
        assert_eq!(eval(&ast, &ctx).unwrap(), 600);

        // score(0, 43, 10) = 4S non-vul making exactly = 420
        // Contract code: 43 = level 4, strain 3 (Spades)
        let ast = parse("score(0, 43, 10)").unwrap();
        assert_eq!(eval(&ast, &ctx).unwrap(), 420);

        // score(0, 34, 8) = 3NT down 1 = -50
        let ast = parse("score(0, 34, 8)").unwrap();
        assert_eq!(eval(&ast, &ctx).unwrap(), -50);

        // score(0, 143, 9) = 4S doubled down 1 = -100
        // Contract code: 143 = doubled (100) + level 4, strain 3 (Spades)
        let ast = parse("score(0, 143, 9)").unwrap();
        assert_eq!(eval(&ast, &ctx).unwrap(), -100);
    }

    #[test]
    #[ignore] // Slow: requires DDS solver (~1 sec per call)
    fn test_eval_tricks() {
        use dealer_parser::parse;

        let mut gen = FastDealGenerator::new(42);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Test tricks with numeric denomination
        // tricks(north, 4) = tricks in NT for North as declarer
        let ast = parse("tricks(north, 4)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        // Result should be 0-13
        assert!(
            (0..=13).contains(&result),
            "tricks should be 0-13, got {}",
            result
        );

        // Test tricks with suit keyword
        // tricks(south, spades) = tricks in spades for South as declarer
        let ast = parse("tricks(south, spades)").unwrap();
        let result = eval(&ast, &ctx).unwrap();
        assert!(
            (0..=13).contains(&result),
            "tricks should be 0-13, got {}",
            result
        );

        // Test that different positions can have different trick counts
        // (This is just a sanity check - the actual values depend on the deal)
        let ast_n = parse("tricks(north, 4)").unwrap();
        let ast_s = parse("tricks(south, 4)").unwrap();
        let _tricks_n = eval(&ast_n, &ctx).unwrap();
        let _tricks_s = eval(&ast_s, &ctx).unwrap();
        // Both should be valid (0-13) - we already checked above
    }

    #[test]
    #[ignore] // Slow: requires DDS solver (~1 sec per call)
    fn test_tricks_with_score() {
        use dealer_parser::parse;

        let mut gen = FastDealGenerator::new(42);
        let deal = gen.next_deal();
        let ctx = EvalContext::new(&deal);

        // Use tricks() result in score() calculation
        // score(0, 34, tricks(north, 4)) = score for 3NT non-vul based on DD tricks
        let ast = parse("score(0, 34, tricks(north, 4))").unwrap();
        let score = eval(&ast, &ctx).unwrap();

        // The score should be:
        // - Positive if making (tricks >= 9): 400+ for 3NT
        // - Negative if failing (tricks < 9): -50 per undertrick
        // We can't predict exact value, but it should be a valid bridge score
        eprintln!("3NT score with DD tricks: {}", score);
    }
}
