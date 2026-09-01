//! The language's vocabulary, in one place.
//!
//! Editors need the same word lists the parser recognises: syntax highlighting,
//! completion, and hover all depend on them. Keeping a second copy in a
//! TextMate grammar or an editor plugin means it drifts — and it did. Before
//! this module existed, `dlr.tmLanguage.json` was missing 19 functions
//! (`tens`, `jacks`, `queens`, `kings`, `aces`, `top2`..`top5`, `pt0`..`pt9`),
//! missing the `csvrpt` keyword, and listing two functions that do not exist
//! (`control`, `imp`).
//!
//! These lists are the source of truth. `tests/vocabulary_matches_grammar.rs`
//! asserts they agree with `grammar.pest`, and
//! `tests/tmlanguage_matches_vocabulary.rs` asserts the shipped TextMate grammar
//! agrees with them, so a new function cannot be added to the parser without the
//! editors noticing.

/// Functions callable in an expression, e.g. `hcp(north)`.
pub const FUNCTIONS: &[&str] = &[
    // Hand evaluation — singular and plural spellings are both accepted, as in
    // the original dealer's lexer (`controls?`, `hcps?`, `losers?`)
    "hcp", "hcps", "controls", "control", "losers", "loser", "quality", "cccc",
    // Shape and specific cards
    "shape", "hascard",
    // Suit lengths — plural and singular forms are both accepted
    "spades", "hearts", "diamonds", "clubs", "spade", "heart", "diamond", "club",
    // Named point counts, plural and singular
    "tens", "ten", "jacks", "jack", "queens", "queen", "kings", "king", "aces", "ace", "top2",
    "top3", "top4", "top5", "c13", // Indexed point counts
    "pt0", "pt1", "pt2", "pt3", "pt4", "pt5", "pt6", "pt7", "pt8", "pt9",
    // Double-dummy and scoring
    "tricks", "trick", "dds", "score", "imps", "imp", "rnd",
];

/// Statement keywords that introduce a directive.
pub const STATEMENT_KEYWORDS: &[&str] = &[
    "condition",
    "produce",
    "generate",
    "action",
    "dealer",
    "vulnerable",
    "title",
    "seed",
    "predeal",
    "csvrpt",
    "printrpt",
    "pointcount",
    "altcount",
    "average",
    "frequency",
    "printes",
    "print",
];

/// Output actions, valid inside `action` or standalone.
pub const ACTIONS: &[&str] = &[
    "printall",
    "printew",
    "printns",
    "printside",
    "printpbn",
    "printcompact",
    "printoneline",
];

/// Compass positions. Single letters are accepted but may be shadowed by a
/// variable of the same name, which the evaluator allows deliberately.
pub const POSITIONS: &[&str] = &["north", "south", "east", "west", "n", "s", "e", "w"];

/// Vulnerability settings.
pub const VULNERABILITIES: &[&str] = &["none", "ns", "ew", "all"];

/// Word-form logical operators, alternatives to `&&`, `||` and `!`.
pub const LOGICAL_WORDS: &[&str] = &["and", "or", "not"];

/// Other reserved words: `any` introduces a shape pattern, `deal` is a csvrpt
/// term, and `notrump`/`notrumps` is the fourth denomination — the same number
/// `4` that `tricks` and `score` take, spelled the way the original spells it.
pub const OTHER_KEYWORDS: &[&str] = &["any", "deal", "notrump", "notrumps"];

/// Symbolic operators, longest-first so a tokenizer matching in order does not
/// split `>=` into `>` and `=`.
pub const OPERATORS: &[&str] = &[
    "==", "!=", ">=", "<=", "&&", "||", ">", "<", "!", "?", ":", "+", "-", "*", "/", "%", "=",
];

// ---------------------------------------------------------------------------
// Documentation
//
// The lists above say which words exist; these say what they mean. They are
// here rather than in the web app so that the reference page, editor hovers and
// anything else describing the language all read from the parser's own crate,
// and so `tests/vocabulary_docs.rs` can hold them against the lists above:
// adding a function to the grammar fails the build until it is documented.
//
// Descriptions were written from the evaluator (`dealer-eval`) and checked
// against Henk Uijterwaal's input-language manual and the original C sources in
// `Dealer-cleanup` (`c4.c` for `quality`/`cccc`, `pointcount.c` for the honour
// counts, `dealer.c` for losers). Where dealer3 and the original differ, the
// entry says so in `note` rather than papering over it.
// ---------------------------------------------------------------------------

/// What one callable function does.
pub struct FunctionDoc {
    /// The name as written in a script; one of [`FUNCTIONS`].
    pub name: &'static str,
    /// Heading to file this under in a reference.
    pub group: &'static str,
    /// Call shape, with alternative forms separated by `  ·  `.
    pub signature: &'static str,
    /// One line saying what the function computes.
    pub summary: &'static str,
    /// A snippet that parses as a condition.
    pub example: &'static str,
    /// Set when this name is a second spelling of another function, which is
    /// where the real description lives.
    pub alias_of: Option<&'static str>,
    /// Anything a reader would otherwise get wrong: a scale, an encoding, or a
    /// difference from the original dealer.
    pub note: Option<&'static str>,
}

/// Group headings, in the order a reference should present them.
pub const FUNCTION_GROUPS: &[&str] = &[
    "Hand evaluation",
    "Suit length",
    "Shape and cards",
    "Honour counts",
    "Double-dummy and scoring",
];

/// Every function in [`FUNCTIONS`], described.
pub const FUNCTION_DOCS: &[FunctionDoc] = &[
    // ---- Hand evaluation --------------------------------------------------
    FunctionDoc {
        name: "hcp",
        group: "Hand evaluation",
        signature: "hcp(compass)  ·  hcp(compass, suit)",
        summary: "High card points on the 4-3-2-1 scale: ace 4, king 3, queen 2, jack 1. \
                  With a suit, only that suit's cards are counted.",
        example: "hcp(north) >= 12 && hcp(north, spades) >= 4",
        alias_of: None,
        note: Some(
            "The 4-3-2-1 scale is the default, not a fixture: a `pointcount` statement \
             replaces it for the whole script.",
        ),
    },
    FunctionDoc {
        name: "controls",
        group: "Hand evaluation",
        signature: "controls(compass)  ·  controls(compass, suit)",
        summary: "Controls: each ace counts 2 and each king 1.",
        example: "controls(north) >= 5",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "hcps",
        group: "Hand evaluation",
        signature: "hcps(compass)  ·  hcps(compass, suit)",
        summary: "Plural spelling of `hcp`.",
        example: "hcps(north) >= 12",
        alias_of: Some("hcp"),
        note: None,
    },
    FunctionDoc {
        name: "control",
        group: "Hand evaluation",
        signature: "control(compass)  ·  control(compass, suit)",
        summary: "Singular spelling of `controls`.",
        example: "control(north) >= 5",
        alias_of: Some("controls"),
        note: None,
    },
    FunctionDoc {
        name: "losers",
        group: "Hand evaluation",
        signature: "losers(compass)  ·  losers(compass, suit)",
        summary: "Losing trick count: a void is 0; a singleton is 0 holding the ace and 1 \
                  otherwise; a doubleton is 0 holding A-K, 1 holding the ace or the king and 2 \
                  otherwise; three cards or more is 3 minus the number of A, K and Q held.",
        example: "losers(south) <= 6",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "loser",
        group: "Hand evaluation",
        signature: "loser(compass)  ·  loser(compass, suit)",
        summary: "Singular spelling of `losers`.",
        example: "loser(south) <= 6",
        alias_of: Some("losers"),
        note: None,
    },
    FunctionDoc {
        name: "quality",
        group: "Hand evaluation",
        signature: "quality(compass, suit)",
        summary: "Quality of one suit, by the algorithm published in The Bridge World, \
                  October 1982, multiplied by 100 — so 450 means 4.50.",
        example: "quality(north, spades) >= 400",
        alias_of: None,
        note: Some(
            "Each honour is worth a multiple of ten times the suit length — ace 4×, king 3×, \
             queen 2×, jack 1× — with an extra allowance for length beyond six cards, and for \
             the ten and nine when they are supported. dealer3's implementation follows the \
             original `c4.c` line for line, and was checked against dealer.exe's own output.",
        ),
    },
    FunctionDoc {
        name: "cccc",
        group: "Hand evaluation",
        signature: "cccc(compass)",
        summary: "Whole-hand evaluation by the algorithm published in The Bridge World, \
                  October 1982, multiplied by 100 — a minimum opening bid is around 1200.",
        example: "cccc(north) >= 1200",
        alias_of: None,
        note: Some(
            "Honours are valued by suit with penalties for short or unsupported ones, each \
             suit's `quality` is added, and short suits contribute shape points. dealer3's \
             implementation follows the original `c4.c` line for line, and was checked against \
             dealer.exe's own output.",
        ),
    },
    // ---- Suit length ------------------------------------------------------
    FunctionDoc {
        name: "spades",
        group: "Suit length",
        signature: "spades(compass)",
        summary: "Number of spades held.",
        example: "spades(north) + spades(south) >= 8",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "hearts",
        group: "Suit length",
        signature: "hearts(compass)",
        summary: "Number of hearts held.",
        example: "hearts(north) + hearts(south) >= 8",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "diamonds",
        group: "Suit length",
        signature: "diamonds(compass)",
        summary: "Number of diamonds held.",
        example: "diamonds(west) >= 6",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "clubs",
        group: "Suit length",
        signature: "clubs(compass)",
        summary: "Number of clubs held.",
        example: "clubs(east) <= 2",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "spade",
        group: "Suit length",
        signature: "spade(compass)",
        summary: "Singular spelling of `spades`.",
        example: "spade(north) >= 5",
        alias_of: Some("spades"),
        note: None,
    },
    FunctionDoc {
        name: "heart",
        group: "Suit length",
        signature: "heart(compass)",
        summary: "Singular spelling of `hearts`.",
        example: "heart(north) >= 5",
        alias_of: Some("hearts"),
        note: None,
    },
    FunctionDoc {
        name: "diamond",
        group: "Suit length",
        signature: "diamond(compass)",
        summary: "Singular spelling of `diamonds`.",
        example: "diamond(north) >= 5",
        alias_of: Some("diamonds"),
        note: None,
    },
    FunctionDoc {
        name: "club",
        group: "Suit length",
        signature: "club(compass)",
        summary: "Singular spelling of `clubs`.",
        example: "club(north) >= 5",
        alias_of: Some("clubs"),
        note: None,
    },
    // ---- Shape and cards --------------------------------------------------
    FunctionDoc {
        name: "shape",
        group: "Shape and cards",
        signature: "shape(compass, pattern)",
        summary: "True when the hand matches the pattern. Four digits are lengths in spades, \
                  hearts, diamonds, clubs order; `x` matches any length; `any` allows the \
                  suits in any order; `+` adds a pattern and `-` excludes one.",
        example: "shape(north, any 4333 + any 4432 + any 5332)",
        alias_of: None,
        note: Some(
            "Matching is a table lookup over all 560 shapes, so a long pattern list costs no \
             more than a short one. **Braces instead of parentheses** take François \
             Dellacherie's shape language, which says the same things far more briefly: \
             `5+` is at least five, `2-` at most two, `[3-5]` a range, `(431)` the remaining \
             suits in any order, `M` either major and `m` either minor, and `:` attaches a \
             condition on the suit lengths `s h d c`, with `,` for and. So \
             `shape{north, 4M(3+3+2+)}` is the twelve patterns above, and it is expanded \
             before the script is parsed. A `+` or `-` joining two patterns needs a space on \
             both sides, which is what keeps the one in `h+s>=10` inside its condition; and \
             only one `M` and one `m` fit in a pattern, two of a colour needing `(...)`. A \
             script parameter may stand anywhere inside one, since `--param` fills it \
             in before the shape is read.",
        ),
    },
    FunctionDoc {
        name: "hascard",
        group: "Shape and cards",
        signature: "hascard(compass, card)",
        summary: "True when the hand holds exactly that card, written rank then suit — `TC` is \
                  the ten of clubs.",
        example: "hascard(east, TC) && hascard(east, AS)",
        alias_of: None,
        note: None,
    },
    // ---- Honour counts ----------------------------------------------------
    FunctionDoc {
        name: "tens",
        group: "Honour counts",
        signature: "tens(compass)  ·  tens(compass, suit)",
        summary: "Number of tens held.",
        example: "tens(north) >= 2",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "jacks",
        group: "Honour counts",
        signature: "jacks(compass)  ·  jacks(compass, suit)",
        summary: "Number of jacks held.",
        example: "jacks(north) >= 2",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "queens",
        group: "Honour counts",
        signature: "queens(compass)  ·  queens(compass, suit)",
        summary: "Number of queens held.",
        example: "queens(north) >= 2",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "kings",
        group: "Honour counts",
        signature: "kings(compass)  ·  kings(compass, suit)",
        summary: "Number of kings held.",
        example: "kings(north) >= 2",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "aces",
        group: "Honour counts",
        signature: "aces(compass)  ·  aces(compass, suit)",
        summary: "Number of aces held.",
        example: "aces(north) >= 2",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "top2",
        group: "Honour counts",
        signature: "top2(compass)  ·  top2(compass, suit)",
        summary: "Number of the top two honours held: ace, king.",
        example: "top2(north, spades) == 2",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "top3",
        group: "Honour counts",
        signature: "top3(compass)  ·  top3(compass, suit)",
        summary: "Number of the top three honours held: ace, king, queen.",
        example: "top3(north, hearts) >= 2",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "top4",
        group: "Honour counts",
        signature: "top4(compass)  ·  top4(compass, suit)",
        summary: "Number of the top four honours held: ace, king, queen, jack.",
        example: "top4(north, hearts) >= 3",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "top5",
        group: "Honour counts",
        signature: "top5(compass)  ·  top5(compass, suit)",
        summary: "Number of the top five honours held: ace, king, queen, jack, ten.",
        example: "top5(east, spades) >= 3",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "c13",
        group: "Honour counts",
        signature: "c13(compass)  ·  c13(compass, suit)",
        summary: "C13 points: ace 6, king 4, queen 2, jack 1.",
        example: "c13(north) >= 18",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "ten",
        group: "Honour counts",
        signature: "ten(compass)  ·  ten(compass, suit)",
        summary: "Singular spelling of `tens`.",
        example: "ten(north) >= 2",
        alias_of: Some("tens"),
        note: None,
    },
    FunctionDoc {
        name: "jack",
        group: "Honour counts",
        signature: "jack(compass)  ·  jack(compass, suit)",
        summary: "Singular spelling of `jacks`.",
        example: "jack(north) >= 2",
        alias_of: Some("jacks"),
        note: None,
    },
    FunctionDoc {
        name: "queen",
        group: "Honour counts",
        signature: "queen(compass)  ·  queen(compass, suit)",
        summary: "Singular spelling of `queens`.",
        example: "queen(north) >= 2",
        alias_of: Some("queens"),
        note: None,
    },
    FunctionDoc {
        name: "king",
        group: "Honour counts",
        signature: "king(compass)  ·  king(compass, suit)",
        summary: "Singular spelling of `kings`.",
        example: "king(north) >= 2",
        alias_of: Some("kings"),
        note: None,
    },
    FunctionDoc {
        name: "ace",
        group: "Honour counts",
        signature: "ace(compass)  ·  ace(compass, suit)",
        summary: "Singular spelling of `aces`.",
        example: "ace(north) >= 2",
        alias_of: Some("aces"),
        note: None,
    },
    // The pt0..pt9 spellings are the original dealer's own, and scripts in the
    // wild use both, so each gets an entry pointing at the readable name.
    FunctionDoc {
        name: "pt0",
        group: "Honour counts",
        signature: "pt0(compass)  ·  pt0(compass, suit)",
        summary: "Another spelling of `tens`.",
        example: "pt0(north) >= 2",
        alias_of: Some("tens"),
        note: None,
    },
    FunctionDoc {
        name: "pt1",
        group: "Honour counts",
        signature: "pt1(compass)  ·  pt1(compass, suit)",
        summary: "Another spelling of `jacks`.",
        example: "pt1(north) >= 2",
        alias_of: Some("jacks"),
        note: None,
    },
    FunctionDoc {
        name: "pt2",
        group: "Honour counts",
        signature: "pt2(compass)  ·  pt2(compass, suit)",
        summary: "Another spelling of `queens`.",
        example: "pt2(north) >= 2",
        alias_of: Some("queens"),
        note: None,
    },
    FunctionDoc {
        name: "pt3",
        group: "Honour counts",
        signature: "pt3(compass)  ·  pt3(compass, suit)",
        summary: "Another spelling of `kings`.",
        example: "pt3(north) >= 2",
        alias_of: Some("kings"),
        note: None,
    },
    FunctionDoc {
        name: "pt4",
        group: "Honour counts",
        signature: "pt4(compass)  ·  pt4(compass, suit)",
        summary: "Another spelling of `aces`.",
        example: "pt4(north) >= 2",
        alias_of: Some("aces"),
        note: None,
    },
    FunctionDoc {
        name: "pt5",
        group: "Honour counts",
        signature: "pt5(compass)  ·  pt5(compass, suit)",
        summary: "Another spelling of `top2`.",
        example: "pt5(north, spades) == 2",
        alias_of: Some("top2"),
        note: None,
    },
    FunctionDoc {
        name: "pt6",
        group: "Honour counts",
        signature: "pt6(compass)  ·  pt6(compass, suit)",
        summary: "Another spelling of `top3`.",
        example: "pt6(north, spades) >= 2",
        alias_of: Some("top3"),
        note: None,
    },
    FunctionDoc {
        name: "pt7",
        group: "Honour counts",
        signature: "pt7(compass)  ·  pt7(compass, suit)",
        summary: "Another spelling of `top4`.",
        example: "pt7(north, spades) >= 3",
        alias_of: Some("top4"),
        note: None,
    },
    FunctionDoc {
        name: "pt8",
        group: "Honour counts",
        signature: "pt8(compass)  ·  pt8(compass, suit)",
        summary: "Another spelling of `top5`.",
        example: "pt8(north, spades) >= 3",
        alias_of: Some("top5"),
        note: None,
    },
    FunctionDoc {
        name: "pt9",
        group: "Honour counts",
        signature: "pt9(compass)  ·  pt9(compass, suit)",
        summary: "Another spelling of `c13`.",
        example: "pt9(north) >= 18",
        alias_of: Some("c13"),
        note: None,
    },
    // ---- Double-dummy and scoring ----------------------------------------
    FunctionDoc {
        name: "tricks",
        group: "Double-dummy and scoring",
        signature: "tricks(compass, strain)",
        summary: "Tricks that compass takes as declarer in that strain with every hand seen — \
                  the double-dummy result. Strain is a suit name, or a number: 0 clubs, \
                  1 diamonds, 2 hearts, 3 spades, 4 notrump.",
        example: "tricks(south, spades) >= 10",
        alias_of: None,
        note: Some(
            "Notrump is `notrump`, `notrumps`, or the number 4 — the original's spelling and \
             dealer3's number are the same value. Solving a deal is far slower than any other \
             function here, so a script using `tricks` wants a tight `condition` ahead of it.",
        ),
    },
    FunctionDoc {
        name: "dds",
        group: "Double-dummy and scoring",
        signature: "dds(compass, strain)",
        summary: "DealerV2_4's spelling of `tricks`.",
        example: "dds(south, notrump) >= 9",
        alias_of: Some("tricks"),
        note: Some(
            "There it reaches the DDS library where `tricks` reaches GIB's solver; dealer3 \
             solves everything through bridge-solver, so the two are one function with two \
             names. Ten of DealerV2_4's regression scripts use it.",
        ),
    },
    FunctionDoc {
        name: "trick",
        group: "Double-dummy and scoring",
        signature: "trick(compass, strain)",
        summary: "Singular spelling of `tricks`.",
        example: "trick(south, spades) >= 10",
        alias_of: Some("tricks"),
        note: None,
    },
    FunctionDoc {
        name: "imp",
        group: "Double-dummy and scoring",
        signature: "imp(scoredifference)",
        summary: "Singular spelling of `imps`.",
        example: "imp(score(nv, x4S, 10) - score(nv, x3N, 9)) >= 1",
        alias_of: Some("imps"),
        note: None,
    },
    FunctionDoc {
        name: "score",
        group: "Double-dummy and scoring",
        signature: "score(vulnerable, contract, tricks)",
        summary: "Declarer's score for a contract played at that vulnerability and making that \
                  many tricks. `vulnerable` is the word `nv` or `vul`; `contract` is a word \
                  such as `x3N`; `tricks` is 0 to 13.",
        example: "score(nv, x3N, 9) == 400",
        alias_of: None,
        note: Some(
            "A contract is one word: a lowercase `x`, the level, and the strain as an \
             uppercase letter of CDHSN. The `x` is a sigil rather than a meaning — it is \
             what lets the word be told from a number — so doubling is written as a suffix: \
             `x4Hx` is four hearts doubled and `x4Hxx` redoubled. `z` may be used in place \
             of the leading `x`, as in DealerV2_4, and means exactly the same thing. Both \
             the case and the level range are the references' own, so a word that runs here \
             runs on BBO.\n\nEither argument may also be written as the number it stands \
             for, which is what a `--param` can supply: 0 or 1 for the vulnerability, and \
             level × 5 + strain for the contract, plus 40 for each level of doubling. Strain \
             numbers match `tricks`: 0 clubs, 1 diamonds, 2 hearts, 3 spades, 4 notrump. So \
             `x3N` is 19, `x4S` is 23, `x4Sx` is 63 and `x4Sxx` is 103.",
        ),
    },
    FunctionDoc {
        name: "imps",
        group: "Double-dummy and scoring",
        signature: "imps(scoredifference)",
        summary: "Converts a difference between two scores into IMPs, by the standard table.",
        example: "imps(score(nv, x4S, 10) - score(nv, x3N, 9)) >= 1",
        alias_of: None,
        note: None,
    },
    FunctionDoc {
        name: "rnd",
        group: "Double-dummy and scoring",
        signature: "rnd(bound)",
        summary: "A random whole number from zero up to, but not including, the bound.",
        example: "rnd(10) == 3",
        alias_of: None,
        note: Some(
            "Every mention draws again, including through a variable: `r = rnd(4)` used twice \
             is two draws, as in the original. Drawn from a stream of its own, seeded from the \
             deal, so the same seed gives the same answers however many threads are running; \
             the original shares the generator it shuffles with, so calling it there changes \
             the deals. `--rnd-seed` shifts the stream. Beware locally built dealer binaries: \
             `rnd` divides by `RAND_MAX`, which describes `rand()` rather than the generator \
             it actually calls, so a build without `STD_RAND` returns values far outside the \
             bound, or negative ones. BBO's own build is correct.",
        ),
    },
];

/// One operator, and how tightly it binds.
pub struct OperatorDoc {
    /// The symbol as written; one of [`OPERATORS`].
    pub symbol: &'static str,
    /// Word form accepted in its place, where there is one.
    pub word: Option<&'static str>,
    /// Binding strength: 1 binds tightest. Operators sharing a level are
    /// applied left to right.
    pub precedence: u8,
    /// What the operator does.
    pub summary: &'static str,
    /// A snippet that parses as a condition.
    pub example: &'static str,
    pub note: Option<&'static str>,
}

/// Every operator in [`OPERATORS`], in precedence order — tightest first.
///
/// Read off `grammar.pest`: `ternary` → `logical_or` → `logical_and` →
/// `logical_not` → `comparison` → `additive` → `multiplicative` → `unary`.
pub const OPERATOR_DOCS: &[OperatorDoc] = &[
    OperatorDoc {
        symbol: "!",
        word: Some("not"),
        precedence: 1,
        summary: "True when its operand is zero, and false otherwise.",
        example: "not hcp(north) >= 12",
        note: Some(
            "`!` appears at two strengths. Written in front of a whole comparison it applies to \
             the comparison, so `not hcp(north) >= 12` means `not (hcp(north) >= 12)`. Written \
             inside an arithmetic operand it binds as tightly as a minus sign, so \
             `100 * not x` means `100 * (not x)`.",
        ),
    },
    OperatorDoc {
        symbol: "*",
        word: None,
        precedence: 2,
        summary: "Multiplication.",
        example: "hcp(north) * 2 >= 30",
        note: None,
    },
    OperatorDoc {
        symbol: "/",
        word: None,
        precedence: 2,
        summary: "Division. Whole numbers throughout, so the remainder is discarded.",
        example: "hcp(north) / 2 >= 6",
        note: None,
    },
    OperatorDoc {
        symbol: "%",
        word: None,
        precedence: 2,
        summary: "Remainder after division.",
        example: "hcp(north) % 2 == 0",
        note: None,
    },
    OperatorDoc {
        symbol: "+",
        word: None,
        precedence: 3,
        summary: "Addition.",
        example: "hcp(north) + hcp(south) >= 25",
        note: None,
    },
    OperatorDoc {
        symbol: "-",
        word: None,
        precedence: 3,
        summary: "Subtraction, and negation when written in front of a single value.",
        example: "hcp(north) - hcp(south) >= 5",
        note: Some("Negation binds as tightly as `!`, tighter than `*`."),
    },
    OperatorDoc {
        symbol: "<",
        word: None,
        precedence: 4,
        summary: "Less than.",
        example: "hcp(east) < 10",
        note: None,
    },
    OperatorDoc {
        symbol: "<=",
        word: None,
        precedence: 4,
        summary: "Less than or equal to.",
        example: "losers(north) <= 6",
        note: None,
    },
    OperatorDoc {
        symbol: ">",
        word: None,
        precedence: 4,
        summary: "Greater than.",
        example: "hcp(north) > 15",
        note: None,
    },
    OperatorDoc {
        symbol: ">=",
        word: None,
        precedence: 4,
        summary: "Greater than or equal to.",
        example: "hcp(north) >= 15",
        note: None,
    },
    OperatorDoc {
        symbol: "==",
        word: None,
        precedence: 4,
        summary: "Equal to. Note the two signs: a single `=` assigns a variable instead.",
        example: "spades(north) == 5",
        note: Some(
            "Comparisons chain: `a == b == c` is read as `a == b && b == c`, rather than \
             comparing the result of the first comparison against `c`.",
        ),
    },
    OperatorDoc {
        symbol: "!=",
        word: None,
        precedence: 4,
        summary: "Not equal to.",
        example: "spades(north) != 4",
        note: None,
    },
    OperatorDoc {
        symbol: "&&",
        word: Some("and"),
        precedence: 5,
        summary: "True when both sides are true. The right side is skipped when the left is \
                  false.",
        example: "hcp(north) >= 12 && spades(north) >= 5",
        note: None,
    },
    OperatorDoc {
        symbol: "||",
        word: Some("or"),
        precedence: 6,
        summary: "True when either side is true. The right side is skipped when the left is \
                  true.",
        example: "spades(north) >= 5 || hearts(north) >= 5",
        note: None,
    },
    OperatorDoc {
        symbol: "?",
        word: None,
        precedence: 7,
        summary: "First half of the three-way choice `test ? when_true : when_false`.",
        example: "(hcp(north) >= 12 ? spades(north) : hearts(north)) >= 5",
        note: None,
    },
    OperatorDoc {
        symbol: ":",
        word: None,
        precedence: 7,
        summary: "Second half of the three-way choice `test ? when_true : when_false`.",
        example: "(hcp(north) >= 12 ? spades(north) : hearts(north)) >= 5",
        note: None,
    },
    OperatorDoc {
        symbol: "=",
        word: None,
        precedence: 8,
        summary: "Gives a name to an expression. This is a statement, not something that can \
                  appear inside a larger expression.",
        example: "fit = spades(north) + spades(south)",
        note: Some("The name stands for the expression, which is re-evaluated for every deal."),
    },
];

/// One statement form.
pub struct StatementDoc {
    /// The word that introduces it, or `None` for the forms that have no
    /// keyword of their own.
    pub keyword: Option<&'static str>,
    /// How it is written.
    pub form: &'static str,
    pub summary: &'static str,
    /// A snippet that parses on its own.
    pub example: &'static str,
    pub note: Option<&'static str>,
}

/// Every statement form, covering all of [`STATEMENT_KEYWORDS`].
pub const STATEMENT_DOCS: &[StatementDoc] = &[
    StatementDoc {
        keyword: Some("condition"),
        form: "condition <expression>",
        summary: "Keep a deal when the expression is anything other than zero.",
        example: "condition hcp(north) >= 15 && shape(north, any 5332)",
        note: Some(
            "One condition applies to the whole script: a second `condition` replaces the \
             first rather than adding to it. Join tests with `&&` instead.",
        ),
    },
    StatementDoc {
        keyword: Some("produce"),
        form: "produce <number>",
        summary: "Stop once this many deals have matched.",
        example: "produce 25",
        note: Some("The command line's `-p` overrides this."),
    },
    StatementDoc {
        keyword: Some("generate"),
        form: "generate <number>",
        summary: "Stop after dealing this many hands, however few matched.",
        example: "generate 100000",
        note: Some(
            "Whichever limit is reached first ends the run, so this is the guard against a \
             condition so narrow that nothing satisfies it. The command line's `-g` overrides \
             it.",
        ),
    },
    StatementDoc {
        keyword: Some("action"),
        form: "action <action>, <action>, ...",
        summary: "What to do with each matching deal: a print format, and any averages or \
                  frequencies to accumulate.",
        example: "action printoneline, average \"hcp\" hcp(north)",
        note: Some("With no `action`, the deals are printed."),
    },
    StatementDoc {
        keyword: Some("printes"),
        form: "printes(<expression> | \"string\" | \\n, ...)",
        summary: "Print a line of your own for each matching deal, from expressions and \
                  literal text.",
        example: "printes(\"N=\", hcp(north), \\n)",
        note: Some(
            "Nothing is added between terms and no line ends unless you ask for one. A line \
             ending is a bare `\\n` in the list, not an escape inside a string: the original's \
             lexer reads no escapes between quotes, so \"\\n\" there is a backslash and an `n`.",
        ),
    },
    StatementDoc {
        keyword: Some("print"),
        form: "print(<compass>, ...)",
        summary: "Lay out one seat's hands at the end of the run, four boards to a page.",
        example: "print(north)",
        note: Some(
            "A line-printer format from the original: twenty columns a board, spades down to \
             clubs, a form feed after each seat. Seats come out north, east, south, west \
             whatever order they are named in.",
        ),
    },
    StatementDoc {
        keyword: Some("average"),
        form: "average [\"label\"] <expression>",
        summary: "Report the mean of the expression over the deals that matched.",
        example: "average \"north hcp\" hcp(north)",
        note: Some("Valid on its own, or inside an `action` list."),
    },
    StatementDoc {
        keyword: Some("frequency"),
        form: "frequency [\"label\"] (<expression>, <low>, <high>)",
        summary: "Report a histogram of the expression over the deals that matched, counting \
                  from low to high inclusive.",
        example: "frequency \"north hcp\" (hcp(north), 10, 20)",
        note: Some(
            "Values outside the range are still counted, as the Low and High rows. The \
             original dealer also takes a second expression and range for a two-dimensional \
             table; dealer3 does not.",
        ),
    },
    StatementDoc {
        keyword: Some("pointcount"),
        form: "pointcount <value> <value> ...",
        summary: "Re-scale the high card points. Values run from the ace downwards, and ranks \
                  not reached score nothing.",
        example: "pointcount 6 4 2 1",
        note: Some(
            "That example is the 6-4-2-1 scale: ace 6, king 4, queen 2, jack 1, everything \
             else 0. At most thirteen values, one per rank.",
        ),
    },
    StatementDoc {
        keyword: Some("altcount"),
        form: "altcount <count> <value> <value> ...",
        summary: "Re-scale one of the other counts, the same way `pointcount` re-scales the \
                  high card points.",
        example: "altcount 2 1 1 1",
        note: Some(
            "The number is a row of the original's count table, and **it is not the `ptN` \
             number**: row 0 is `hcp`, row 1 is `controls`, and row 2 is `pt0`. So \
             `altcount 2` sets `tens`, and `altcount 0` overwrites `hcp`. Rows run 0 to 11. \
             `losers` reads the `controls` and `top3` rows, so redefining either moves the \
             loser count with it.",
        ),
    },
    StatementDoc {
        keyword: Some("dealer"),
        form: "dealer <compass>",
        summary: "Records who dealt. Affects the output only, never which deals are produced.",
        example: "dealer south",
        note: None,
    },
    StatementDoc {
        keyword: Some("vulnerable"),
        form: "vulnerable none | ns | ew | all",
        summary: "Records the vulnerability. Affects the output only, never which deals are \
                  produced.",
        example: "vulnerable ns",
        note: None,
    },
    StatementDoc {
        keyword: Some("title"),
        form: "title \"<text>\"",
        summary: "Names the run, filling the `[Event]` tag of PBN output. `-T` wins when both \
                  are given.",
        example: "title \"Weak two openings\"",
        note: Some(
            "DealerV2_4's. Nearly every script written for it opens with one, which is why \
             dealer3 accepts it as well as the switch.",
        ),
    },
    StatementDoc {
        keyword: Some("seed"),
        form: "seed <number>",
        summary: "Fixes the random seed, so the run reproduces. `-s` wins when both are given.",
        example: "seed 42",
        note: Some(
            "DealerV2_4's. Before dealer3 accepted it the word parsed as a variable and the \
             number as an expression, so the seed a script asked for was silently ignored.",
        ),
    },
    StatementDoc {
        keyword: Some("predeal"),
        form: "predeal <compass> <holding>, <holding>, ... [<compass> <holding>, ...]",
        summary: "Places cards in a hand before shuffling; the rest of the deal is dealt around \
                  them. A holding is a suit letter followed by its ranks, using T for the ten. \
                  One statement may name several seats: the holdings of a seat are separated by \
                  commas and the seats are not.",
        example: "predeal north SAKQ,HT98 south SJ32",
        note: Some(
            "The original dealer also restricts a suit's length by writing \
             `spades(north) == 5` in a `predeal`; dealer3 does not, so put that in the \
             `condition` instead.",
        ),
    },
    StatementDoc {
        keyword: Some("csvrpt"),
        form: "csvrpt(<term>, <term>, ...)",
        summary: "Writes one comma-separated row per matching deal. A term is an expression, a \
                  quoted string, a compass for that hand, `ns` or `ew` for a partnership's two \
                  hands, the word `deal` for all four, or `trix(...)` for double-dummy tricks.",
        example: "csvrpt(deal, hcp(north), \"north\")",
        note: Some(
            "Command-line only: the browser app has nowhere to write a file.\n\n`trix(compass)` \
             adds five columns — the tricks that seat takes in clubs, diamonds, hearts, spades \
             and notrump — and `trix(deal)` adds twenty, four seats in the order `deal` uses. \
             It is a term rather than a function because it is more than one number. The \
             solving is the same work `tricks()` does and is remembered per deal, so naming \
             both costs one search each, not two.",
        ),
    },
    StatementDoc {
        keyword: Some("printrpt"),
        form: "printrpt(<term>, <term>, ...)",
        summary: "Writes one comma-separated row per matching deal to the screen. The terms are \
                  `csvrpt`'s: an expression, a quoted string, a compass for that hand, `ns` or \
                  `ew` for a partnership's two hands, the word `deal` for all four, or \
                  `trix(...)`.",
        example: "printrpt(\"deal \", deal, hcp(south))",
        note: Some(
            "DealerV2_4's screen counterpart of `csvrpt`, and the same row — so the two share \
             a renderer here rather than merely resembling one another. Thirteen of its \
             regression scripts use it. Unlike `csvrpt` it works in the browser, where the \
             rows appear in the Text view with `printes` output.",
        ),
    },
    StatementDoc {
        keyword: None,
        form: "<name> = <expression>",
        summary: "Names an expression so a long condition can be written in pieces. The name \
                  stands for the expression and is worked out afresh for every deal.",
        example: "fit = spades(north) + spades(south)",
        note: None,
    },
    StatementDoc {
        keyword: None,
        form: "<expression>",
        summary: "An expression on its own is the condition, so the `condition` keyword can be \
                  left off.",
        example: "hcp(north) >= 20",
        note: Some("As with `condition`, only the last one in the script counts."),
    },
];

/// One output action.
pub struct ActionDoc {
    /// One of [`ACTIONS`].
    pub name: &'static str,
    pub summary: &'static str,
    pub note: Option<&'static str>,
    /// How the action is written, when the name alone is not a whole action.
    ///
    /// `None` means the name is the form, which is true of all but
    /// `printside`, whose side has to be given.
    pub form: Option<&'static str>,
}

/// Every action in [`ACTIONS`], described.
pub const ACTION_DOCS: &[ActionDoc] = &[
    ActionDoc {
        name: "printall",
        summary: "All four hands, laid out around the compass. This is what happens with no \
                  action given.",
        note: None,
        form: None,
    },
    ActionDoc {
        name: "printew",
        summary: "East and West only, West on the left.",
        note: Some("The same action as `printside(ew)`."),
        form: None,
    },
    ActionDoc {
        name: "printns",
        summary: "North and South only, South on the left.",
        note: Some(
            "The counterpart of `printew`, and the same action as `printside(ns)`. South \
             sits left for the reason West does in `printew`: the pair reads as one auction, \
             and the hand that speaks first goes on the left. From DealerV2_4.",
        ),
        form: None,
    },
    ActionDoc {
        name: "printside",
        summary: "One partnership's two hands: `printside(ns)` or `printside(ew)`.",
        note: Some(
            "DealerV2_4's one action for both partnerships. `printside(ns)` and `printns` \
             are the same thing, as are `printside(ew)` and `printew`.",
        ),
        form: Some("printside(ns)"),
    },
    ActionDoc {
        name: "printpbn",
        summary: "PBN, the record format other bridge programs read.",
        note: None,
        form: None,
    },
    ActionDoc {
        name: "printcompact",
        summary: "Four lines per deal.",
        note: None,
        form: None,
    },
    ActionDoc {
        name: "printoneline",
        summary: "One line per deal.",
        note: None,
        form: None,
    },
];

/// Something the original dealer accepts that dealer3 does not.
pub struct NotSupported {
    /// The word as the original spells it.
    pub name: &'static str,
    /// What to write instead, or why it is absent.
    pub instead: &'static str,
}

/// Words from the original dealer's input language that dealer3 does not accept.
///
/// Each is **reserved in the grammar** so that using one is a syntax error. That
/// is the point of the list: before it, an unrecognised word was read as a
/// variable, a variable is an ordinary expression, and a statement therefore
/// turned quietly into a different statement — `tricks(north, notrumps)` asked
/// about clubs, and `pointcount 6 4 2 1` was thrown away while the script ran on
/// the scale it was trying to replace. No error, no output, exit 0.
///
/// Two tests hold this list in place. `tests/vocabulary_docs.rs` fails if one of
/// these is implemented but still listed here, and
/// `tests/vocabulary_matches_grammar.rs` fails if the list and the grammar's
/// `reserved_unsupported` rule disagree — so implementing one means taking it
/// out of both, and the language reference stops mentioning it automatically.
pub const NOT_SUPPORTED: &[NotSupported] = &[NotSupported {
    name: "evalcontract",
    instead: "The original parses it and then aborts on an assertion, so there is \
                  nothing to be compatible with. Use `score` and `tricks`.",
}];

/// Every reserved word, for "is this an identifier or a keyword" checks.
pub fn all_reserved() -> Vec<&'static str> {
    let mut v = Vec::new();
    for list in [
        FUNCTIONS,
        STATEMENT_KEYWORDS,
        ACTIONS,
        POSITIONS,
        VULNERABILITIES,
        LOGICAL_WORDS,
        OTHER_KEYWORDS,
    ] {
        v.extend_from_slice(list);
    }
    v.sort_unstable();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_duplicates_within_a_list() {
        for (name, list) in [
            ("FUNCTIONS", FUNCTIONS),
            ("STATEMENT_KEYWORDS", STATEMENT_KEYWORDS),
            ("ACTIONS", ACTIONS),
            ("POSITIONS", POSITIONS),
            ("VULNERABILITIES", VULNERABILITIES),
            ("LOGICAL_WORDS", LOGICAL_WORDS),
            ("OTHER_KEYWORDS", OTHER_KEYWORDS),
            ("OPERATORS", OPERATORS),
        ] {
            let mut seen = list.to_vec();
            seen.sort_unstable();
            let before = seen.len();
            seen.dedup();
            assert_eq!(before, seen.len(), "{} contains duplicates", name);
        }
    }

    #[test]
    fn operators_are_longest_first() {
        // A tokenizer trying these in order must not match `>` before `>=`.
        for (i, op) in OPERATORS.iter().enumerate() {
            for longer in &OPERATORS[i + 1..] {
                assert!(
                    !longer.starts_with(op),
                    "`{}` precedes `{}`, which it is a prefix of",
                    op,
                    longer
                );
            }
        }
    }

    #[test]
    fn all_reserved_covers_every_list() {
        let all = all_reserved();
        for f in FUNCTIONS {
            assert!(all.contains(f), "{} missing from all_reserved()", f);
        }
        for k in STATEMENT_KEYWORDS {
            assert!(all.contains(k), "{} missing from all_reserved()", k);
        }
    }
}
