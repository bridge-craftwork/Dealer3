use dealer_core::Position;

/// A program consists of multiple statements
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub statements: Vec<Statement>,
}

/// A statement is either an assignment, action directive, or an expression
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// Variable assignment: name = expr
    Assignment { name: String, expr: Expr },
    /// Standalone expression (the final constraint)
    Expression(Expr),
    /// Condition statement: condition expr
    Condition(Expr),
    /// Produce statement: produce N
    Produce(usize),
    /// Generate statement: generate N
    Generate(usize),
    /// Action statement: action average "label" expr, frequency "label" expr, printpbn/printall/etc
    /// Can contain multiple averages, frequencies, and optionally a format
    Action {
        averages: Vec<AverageSpec>,
        frequencies: Vec<FrequencySpec>,
        format: Option<ActionType>,
        /// `printes(...)` lists, printed per deal in the order written.
        printes: Vec<Vec<EsTerm>>,
        /// Seats named by `print(...)`, whose hands are laid out at the end.
        print_hands: Vec<Position>,
        /// `printrpt(...)` lists, one row per matching deal, in the order
        /// written. DealerV2_4's scripts reach it through `action` far more
        /// often than as a bare statement.
        print_reports: Vec<Vec<CsvTerm>>,
    },
    /// Dealer statement: dealer N/E/S/W
    Dealer(Position),
    /// Vulnerable statement: vulnerable none/NS/EW/all
    Vulnerable(VulnerabilityType),
    /// Title statement: title "text"
    ///
    /// DealerV2_4 names a run inside the script; dealer3 also has `-T`, which
    /// wins when both are given, as the command line does everywhere else.
    Title(String),
    /// Seed statement: seed N
    ///
    /// DealerV2_4's, and every one of its regression scripts opens with one.
    /// `-s` wins when both are given.
    Seed(u32),
    /// Predeal statement: predeal N/E/S/W cards
    Predeal {
        position: Position,
        cards: Vec<dealer_core::Card>,
    },
    /// CSV report statement: csvrpt(terms...)
    CsvReport(Vec<CsvTerm>),
    /// The same list, to stdout rather than a file: printrpt(terms...)
    ///
    /// DealerV2_4's screen counterpart of `csvrpt`, and byte-for-byte the same
    /// row — leading space, commas between terms, strings in single quotes. It
    /// shares `CsvTerm` because it is the same list, not a similar one.
    PrintReport(Vec<CsvTerm>),
    /// Redefine the high card point scale: pointcount 6 4 2 1
    ///
    /// Values run from the ace downwards. Ranks not reached score nothing.
    PointCount(Vec<i32>),
    /// Redefine one of the alternate counts: altcount 2 1 1 1
    ///
    /// The number is a row of the original's count table, **not** a `ptN`
    /// index — row 0 is `hcp` and row 1 is `controls`, so `altcount 2` is what
    /// sets `pt0`. Verified against dealer.exe.
    AltCount { row: usize, values: Vec<i32> },
}

/// An item in a `printes(...)` list.
///
/// The original prints expressions with `%d` and strings exactly as written,
/// with nothing in between — so the script supplies its own separators, and a
/// line ending is a `Newline` item rather than an escape inside a string.
#[derive(Debug, Clone, PartialEq)]
pub enum EsTerm {
    /// An expression, printed as a decimal integer.
    Expression(Expr),
    /// A string literal, printed exactly as written.
    String(String),
    /// A bare `\n` in the list, which the original lexes as its own token.
    Newline,
}

/// A single term in a CSV report
#[derive(Debug, Clone, PartialEq)]
pub enum CsvTerm {
    /// An expression to evaluate (e.g., hcp(north), controls(south))
    Expression(Expr),
    /// A string literal
    String(String),
    /// A single compass position (outputs hand in PBN format)
    Compass(Position),
    /// A side (NS or EW) - outputs two hands
    Side(Side),
    /// All four hands (DEAL keyword)
    Deal,
    /// `trix(compass)` or `trix(deal)`: double-dummy tricks in all five
    /// strains, one column each, for every seat listed.
    ///
    /// A term rather than a function because it is more than one number: five
    /// per seat, in the strain order the rest of the language uses (0=C to
    /// 4=NT). `trix(deal)` lists all four seats in the report's usual N, E, S,
    /// W order.
    Trix(Vec<Position>),
}

/// Side enumeration for CSV output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    NS,
    EW,
}

/// An average specification within an action statement
#[derive(Debug, Clone, PartialEq)]
pub struct AverageSpec {
    pub label: Option<String>,
    pub expr: Expr,
}

/// A frequency specification within an action statement
#[derive(Debug, Clone, PartialEq)]
pub struct FrequencySpec {
    pub label: Option<String>,
    pub expr: Expr,
    /// Optional range: (min, max) - if None, auto-detect from data
    pub range: Option<(i32, i32)>,
}

/// Vulnerability types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulnerabilityType {
    None,
    NS,
    EW,
    All,
}

impl VulnerabilityType {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "none" => Some(VulnerabilityType::None),
            "ns" => Some(VulnerabilityType::NS),
            "ew" => Some(VulnerabilityType::EW),
            "all" => Some(VulnerabilityType::All),
            _ => None,
        }
    }
}

/// Action types for output formatting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionType {
    PrintAll,
    PrintEW,
    /// North and South only, the counterpart of `PrintEW`.
    ///
    /// `printns` and `printside(ns)` are the same action. DealerV2_4 routes
    /// `printew`, `printns` and `printside(side)` through one printer; dealer3
    /// keeps two variants because nothing else needs a payload, and the two
    /// spellings of each simply resolve here.
    PrintNS,
    PrintPBN,
    PrintCompact,
    PrintOneLine,
}

impl ActionType {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "printall" => Some(ActionType::PrintAll),
            "printew" => Some(ActionType::PrintEW),
            "printns" => Some(ActionType::PrintNS),
            "printpbn" => Some(ActionType::PrintPBN),
            "printcompact" => Some(ActionType::PrintCompact),
            "printoneline" => Some(ActionType::PrintOneLine),
            _ => None,
        }
    }
}

/// Abstract Syntax Tree for dealer constraints
/// This is Clone + Send + Sync so it can be shared across threads
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Binary operation: left op right
    BinaryOp {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// Unary operation: op expr
    UnaryOp { op: UnaryOp, expr: Box<Expr> },

    /// Ternary operation: condition ? true_expr : false_expr
    Ternary {
        condition: Box<Expr>,
        true_expr: Box<Expr>,
        false_expr: Box<Expr>,
    },

    /// Function call: func(args...)
    FunctionCall { func: Function, args: Vec<Expr> },

    /// Integer literal
    Literal(i32),

    /// Position identifier (north, south, east, west)
    Position(Position),

    /// Shape pattern for matching hand distributions
    ShapePattern(ShapePattern),

    /// Card literal (e.g., AS for ace of spades, TC for ten of clubs)
    Card(dealer_core::Card),

    /// Suit literal (spades, hearts, diamonds, clubs)
    Suit(dealer_core::Suit),

    /// Variable reference (e.g., nt_opener, weak_hand)
    Variable(String),
}

/// Shape pattern for hand distribution matching
#[derive(Debug, Clone, PartialEq)]
pub struct ShapePattern {
    /// List of shape specifications combined with + and -
    pub specs: Vec<ShapeSpec>,
    /// Precomputed bitmask for O(1) shape matching (computed lazily or at parse time)
    mask: Option<dealer_core::ShapeMask>,
}

impl ShapePattern {
    /// Create a new ShapePattern from specs.
    pub fn new(specs: Vec<ShapeSpec>) -> Self {
        let mut pattern = ShapePattern { specs, mask: None };
        pattern.compute_mask();
        pattern
    }

    /// Compute and cache the shape mask.
    fn compute_mask(&mut self) {
        use dealer_core::ShapeMask;

        let mut result = ShapeMask::empty();

        for spec in &self.specs {
            let spec_mask = match &spec.shape {
                Shape::Exact(p) => ShapeMask::exact(p[0], p[1], p[2], p[3]),
                Shape::Wildcard(p) => ShapeMask::wildcard(*p),
                Shape::AnyDistribution(p) => ShapeMask::any_distribution(*p),
                Shape::AnyWildcard(p) => ShapeMask::any_wildcard(*p),
            };

            if spec.include {
                result = result.union(&spec_mask);
            } else {
                result = result.difference(&spec_mask);
            }
        }

        self.mask = Some(result);
    }

    /// Get the precomputed shape mask.
    #[inline]
    pub fn mask(&self) -> &dealer_core::ShapeMask {
        self.mask.as_ref().expect("ShapeMask not computed")
    }

    /// Check if a hand with the given shape index matches this pattern.
    #[inline]
    pub fn matches_index(&self, shape_index: usize) -> bool {
        self.mask().contains(shape_index)
    }
}

/// A single shape specification (possibly with operators)
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeSpec {
    /// Whether this is included (+) or excluded (-)
    pub include: bool,
    /// The actual shape
    pub shape: Shape,
}

/// A shape distribution pattern
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Exact shape: "5431" means exactly 5-4-3-1 in that suit order (S-H-D-C)
    Exact([u8; 4]),
    /// Wildcard shape: "54xx" means 5 spades, 4 hearts, any distribution in minors
    Wildcard([Option<u8>; 4]),
    /// Any distribution: "any 4333" means any hand with 4-3-3-3 distribution regardless of suit order
    AnyDistribution([u8; 4]),
    /// Any wildcard: "any 6xxx" means any distribution with 6 in some suit (any permutation of wildcard)
    AnyWildcard([Option<u8>; 4]),
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    // Logical
    And,
    Or,

    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Negate,
}

/// Built-in functions for hand evaluation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Function {
    /// High Card Points (A=4, K=3, Q=2, J=1)
    Hcp,

    /// Number of spades
    Spades,

    /// Number of hearts
    Hearts,

    /// Number of diamonds
    Diamonds,

    /// Number of clubs
    Clubs,

    /// Control count (A=2, K=1)
    Controls,

    /// Losers count
    Losers,

    /// Shape analysis
    Shape,

    /// Has specific card
    HasCard,

    // Alternative point counts (pt0-pt9)
    /// Number of tens
    Tens,
    /// Number of jacks
    Jacks,
    /// Number of queens
    Queens,
    /// Number of kings
    Kings,
    /// Number of aces
    Aces,
    /// Top 2 honors (AK)
    Top2,
    /// Top 3 honors (AKQ)
    Top3,
    /// Top 4 honors (AKQJ)
    Top4,
    /// Top 5 honors (AKQJT)
    Top5,
    /// C13 point count (A=6, K=4, Q=2, J=1)
    C13,

    // Hand quality functions
    /// Quality metric for a suit (Bridge World Oct 1982)
    Quality,
    /// CCCC evaluation algorithm (Bridge World Oct 1982)
    Cccc,

    // Double-dummy and scoring functions
    /// Double-dummy trick count
    Tricks,
    /// Contract score calculation
    Score,
    /// Convert score difference to IMPs
    Imps,
    /// A random number below the given bound
    ///
    /// The original draws this from the same generator it shuffles with, so
    /// calling it changes which deals come out. dealer3 keeps the two apart —
    /// see the evaluator.
    Rnd,
}

impl Function {
    /// Parse function name from string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hcp" | "hcps" => Some(Function::Hcp),
            "spades" | "spade" => Some(Function::Spades),
            "hearts" | "heart" => Some(Function::Hearts),
            "diamonds" | "diamond" => Some(Function::Diamonds),
            "clubs" | "club" => Some(Function::Clubs),
            "controls" | "control" => Some(Function::Controls),
            "losers" | "loser" => Some(Function::Losers),
            "shape" => Some(Function::Shape),
            "hascard" => Some(Function::HasCard),
            "tens" | "ten" | "pt0" => Some(Function::Tens),
            "jacks" | "jack" | "pt1" => Some(Function::Jacks),
            "queens" | "queen" | "pt2" => Some(Function::Queens),
            "kings" | "king" | "pt3" => Some(Function::Kings),
            "aces" | "ace" | "pt4" => Some(Function::Aces),
            "top2" | "pt5" => Some(Function::Top2),
            "top3" | "pt6" => Some(Function::Top3),
            "top4" | "pt7" => Some(Function::Top4),
            "top5" | "pt8" => Some(Function::Top5),
            "c13" | "pt9" => Some(Function::C13),
            "quality" => Some(Function::Quality),
            "cccc" => Some(Function::Cccc),
            "tricks" | "trick" | "dds" => Some(Function::Tricks),
            "score" => Some(Function::Score),
            "imps" | "imp" => Some(Function::Imps),
            "rnd" => Some(Function::Rnd),
            _ => None,
        }
    }

    /// The canonical spelling, for error messages.
    ///
    /// One of the names `parse` accepts, so a message never names a function
    /// the script could not have written.
    pub fn name(&self) -> &'static str {
        match self {
            Function::Hcp => "hcp",
            Function::Spades => "spades",
            Function::Hearts => "hearts",
            Function::Diamonds => "diamonds",
            Function::Clubs => "clubs",
            Function::Controls => "controls",
            Function::Losers => "losers",
            Function::Shape => "shape",
            Function::HasCard => "hascard",
            Function::Tens => "tens",
            Function::Jacks => "jacks",
            Function::Queens => "queens",
            Function::Kings => "kings",
            Function::Aces => "aces",
            Function::Top2 => "top2",
            Function::Top3 => "top3",
            Function::Top4 => "top4",
            Function::Top5 => "top5",
            Function::C13 => "c13",
            Function::Quality => "quality",
            Function::Cccc => "cccc",
            Function::Tricks => "tricks",
            Function::Score => "score",
            Function::Imps => "imps",
            Function::Rnd => "rnd",
        }
    }

    /// How many arguments the function takes, as an inclusive range.
    ///
    /// The single source of truth: the evaluator checks against this rather
    /// than carrying a count of its own per function, and the check that used
    /// to live in each arm ran only once a deal had been dealt. See #36 — an
    /// argument count is a property of the script, not of a deal, so it is
    /// knowable before the first card comes out.
    pub fn arity(&self) -> (usize, usize) {
        match self {
            // A hand, or a hand and one of its suits.
            Function::Hcp
            | Function::Controls
            | Function::Losers
            | Function::Tens
            | Function::Jacks
            | Function::Queens
            | Function::Kings
            | Function::Aces
            | Function::Top2
            | Function::Top3
            | Function::Top4
            | Function::Top5
            | Function::C13 => (1, 2),

            // A hand, or a number.
            Function::Spades
            | Function::Hearts
            | Function::Diamonds
            | Function::Clubs
            | Function::Cccc
            | Function::Imps
            | Function::Rnd => (1, 1),

            // A hand and something about it.
            Function::Shape | Function::HasCard | Function::Quality | Function::Tricks => (2, 2),

            Function::Score => (3, 3),
        }
    }
}

impl Expr {
    /// Helper to create a binary operation
    pub fn binary(op: BinaryOp, left: Expr, right: Expr) -> Self {
        Expr::BinaryOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Helper to create a unary operation
    pub fn unary(op: UnaryOp, expr: Expr) -> Self {
        Expr::UnaryOp {
            op,
            expr: Box::new(expr),
        }
    }

    /// Helper to create a function call with a single argument
    pub fn call(func: Function, arg: Expr) -> Self {
        Expr::FunctionCall {
            func,
            args: vec![arg],
        }
    }

    /// Helper to create a function call with multiple arguments
    pub fn call_multi(func: Function, args: Vec<Expr>) -> Self {
        Expr::FunctionCall { func, args }
    }

    /// Helper to create a ternary operation
    pub fn ternary(condition: Expr, true_expr: Expr, false_expr: Expr) -> Self {
        Expr::Ternary {
            condition: Box::new(condition),
            true_expr: Box::new(true_expr),
            false_expr: Box::new(false_expr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_construction() {
        // Build AST for: hcp(north) >= 15
        let ast = Expr::binary(
            BinaryOp::Ge,
            Expr::call(Function::Hcp, Expr::Position(Position::North)),
            Expr::Literal(15),
        );

        match ast {
            Expr::BinaryOp { op, .. } => assert_eq!(op, BinaryOp::Ge),
            _ => panic!("Expected BinaryOp"),
        }
    }

    #[test]
    fn test_function_parse() {
        assert_eq!(Function::parse("hcp"), Some(Function::Hcp));
        assert_eq!(Function::parse("hearts"), Some(Function::Hearts));
        assert_eq!(Function::parse("HCP"), Some(Function::Hcp));
        assert_eq!(Function::parse("invalid"), None);
    }
}
