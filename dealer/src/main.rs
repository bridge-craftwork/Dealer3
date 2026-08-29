mod fast_parallel;

// Documentation generated from the code rather than kept in step by hand: the
// switch comparison and the language tables in `docs/`. Test-only, since the
// binary never renders them — `cargo test -p dealer` verifies them and
// `UPDATE_DOCS=1 cargo test -p dealer` rewrites them.
#[cfg(test)]
mod generated_docs;
#[cfg(test)]
mod roadmap;
#[cfg(test)]
mod switches;

use clap::Parser;
use dealer_core::{Deal, FastDealConfig, Position, Suit, SwapMode};
use dealer_eval::{
    eval, eval_with_context_and_counts, extract_constraint, extract_point_counts,
    extract_variables, EvalContext,
};
use dealer_level::{
    check_leveling_source, hand_type_label, hand_types, insert_leveling_block, interleave,
    level_from, Leveled, Measurement, MIN_HAND_TYPE_SAMPLE,
};
use dealer_parser::{ActionType, CsvTerm, Side, Statement, VulnerabilityType};
use dealer_pbn::{
    format_hand_pbn, format_oneline, format_printall, format_printcompact, format_printew,
    format_printpbn, PbnBoard, Vulnerability,
};
use dealer_run::{MeasureStop, RunAccumulator};
use fast_parallel::{FastParallelConfig, FastSupervisor};
use std::fs::OpenOptions;
use std::io::{self, BufWriter, Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(name = "dealer")]
#[command(about = "Bridge hand generator with constraint evaluation", long_about = None)]
struct Args {
    /// Input file containing dealer script (if not provided, reads from stdin)
    #[arg(value_name = "INPUT_FILE")]
    input_file: Option<String>,

    /// Number of deals to produce (defaults to 40, or value from input file if not specified)
    /// Can be combined with --generate to limit both produced and generated counts
    #[arg(short = 'p', long = "produce")]
    produce: Option<usize>,

    /// Maximum number of hands to generate (defaults to 10000000)
    /// Can be combined with --produce to limit both generated and produced counts
    #[arg(short = 'g', long = "generate")]
    generate: Option<usize>,

    /// Random seed for generation (defaults to current time)
    #[arg(short = 's', long = "seed")]
    seed: Option<u32>,

    /// Output format (defaults to printall, or value from input file if not specified)
    #[arg(short = 'f', long = "format")]
    format: Option<OutputFormat>,

    /// Dealer position (N/E/S/W) - used with PBN format (defaults to rotating, or value from input file if not specified)
    #[arg(short = 'd', long = "dealer")]
    dealer: Option<DealerPosition>,

    /// Vulnerability (None/NS/EW/All) - used with PBN format (defaults to rotating, or value from input file if not specified)
    #[arg(long = "vulnerable")]
    vulnerability: Option<VulnerabilityArg>,

    /// Toggle verbose output - stats are hidden by default, -v shows them (matches dealer.exe -v behavior)
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Force verbose stats on (cannot be toggled off by -v or PBN output)
    #[arg(short = 'X', long = "stats-on")]
    force_verbose: bool,

    /// Print version information and exit (matches dealer.exe -V)
    #[arg(short = 'V', long = "version")]
    version: bool,

    /// Print license information and exit
    #[arg(long = "license")]
    license: bool,

    /// Print credits and exit
    #[arg(long = "credits")]
    credits: bool,

    /// Quiet mode - suppress deal output, only show statistics (matches dealer.exe -q)
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Show progress meter during generation (matches dealer.exe -m)
    #[arg(short = 'm', long = "progress")]
    progress: bool,

    /// CSV output file (append mode by default, use 'w:filename' for write mode)
    #[arg(short = 'C', long = "CSV")]
    csv_file: Option<String>,

    /// Title metadata for PBN output
    #[arg(short = 'T', long = "title")]
    title: Option<String>,

    /// Predeal cards to North (format: S8743,HA9,D642,CQT64)
    #[arg(short = 'N', long = "north")]
    north_predeal: Option<String>,

    /// Predeal cards to East (format: S8743,HA9,D642,CQT64)
    #[arg(short = 'E', long = "east")]
    east_predeal: Option<String>,

    /// Predeal cards to South (format: S8743,HA9,D642,CQT64)
    #[arg(short = 'S', long = "south")]
    south_predeal: Option<String>,

    /// Predeal cards to West (format: S8743,HA9,D642,CQT64)
    #[arg(short = 'W', long = "west")]
    west_predeal: Option<String>,

    /// Read deals from a file instead of generating random ones.
    /// Supports PBN and oneline formats (auto-detected).
    /// Use '-' to read deals from stdin (requires the script as a file argument).
    /// Lines that are not recognised as deals are ignored, so PBN metadata and
    /// stats output can be fed in directly; check the reported count to confirm
    /// every expected deal was read.
    #[arg(long = "input-deals", value_name = "SOURCE")]
    input_deals: Option<String>,

    /// Write a copy of this script with its hand types levelled
    ///
    /// Runs the script as it stands to measure how often each `HandType_*`
    /// variable comes up, works out the keep rate for each, and writes the
    /// result into the copy's `### BEGIN GENERATED LEVELING ###` block.
    /// See `docs/leveling-guide.md`.
    #[arg(long = "write-leveled", value_name = "FILE")]
    write_leveled: Option<PathBuf>,

    /// Target mix for `--write-leveled`: "even", or weights per hand type
    ///
    /// Overrides any `HandType_X_Share` the script declares. Without either,
    /// the mix is even.
    #[arg(long = "level-target", value_name = "MIX")]
    level_target: Option<String>,

    /// Most deals to produce while measuring for `--write-leveled`
    ///
    /// A ceiling, not a target: measuring stops as soon as the rarest hand type
    /// has been seen enough times to divide by. The rarest type is what sets
    /// the precision of the whole levelling, and how many deals that takes
    /// depends on how rare it is — a type at 5% of qualifying deals is pinned
    /// down by ten thousand, one at 0.2% needs a quarter of a million.
    #[arg(long = "level-measure", value_name = "N", default_value = "2000000")]
    level_measure: usize,

    /// Seconds to spend measuring for `--write-leveled` before giving up
    ///
    /// Reaching it is not an error: the levelling is written with whatever was
    /// measured, and the shortfall is reported and stamped into the file.
    #[arg(long = "level-timeout", value_name = "SECS", default_value = "60")]
    level_timeout: u64,

    /// Budget for `--write-leveled`, in deals dealt per deal kept
    #[arg(long = "level-budget", value_name = "N")]
    level_budget: Option<f64>,

    /// Order the output so each hand type appears before any repeats
    ///
    /// Needs `HandType_*` variables to classify against. Holds every produced
    /// deal until the end, since the order is not known until they all are.
    #[arg(long = "interleave")]
    interleave: bool,

    /// Fill a script parameter: `--param 1=west` puts `west` where `$1` stands
    ///
    /// DealerV2_4 sets these with `-0` to `-9`, which are dealer.exe's swapping
    /// switches here, so the spelling differs. A parameter is source rather than
    /// a value — a compass, a number, a shape, even a function name — and a `$n`
    /// with nothing behind it is an error rather than an empty space.
    #[arg(long = "param", value_name = "N=TEXT")]
    param: Vec<String>,

    /// Report the statistics as JSON instead of tables, for a tool to read
    ///
    /// Use with `-q` for a stdout that is nothing but JSON. The per-average
    /// `count` is how many deals it was measured over, which is what tells you
    /// whether a rare category was sampled enough to trust.
    #[arg(long = "stats-json")]
    stats_json: bool,

    /// Seed for `rnd()`, which draws from its own stream rather than the shuffle
    ///
    /// Long form only: the short letters are dealer.exe's and are not ours to
    /// take. Without it, `rnd()` still gives the same answers for the same
    /// deals every run — this shifts the stream when a script wants a different
    /// draw from the same deals.
    #[arg(long = "rnd-seed", value_name = "SEED", default_value = "0")]
    rnd_seed: u64,

    /// No swapping (default)
    #[arg(short = '0', overrides_with_all = ["swap_two", "swap_three"])]
    swap_off: bool,

    /// Two-way swapping: also deal each shuffle with East and West exchanged
    #[arg(short = '2', overrides_with_all = ["swap_off", "swap_three"])]
    swap_two: bool,

    /// Three-way swapping: deal each shuffle six ways, rotating East/South/West
    #[arg(short = '3', overrides_with_all = ["swap_off", "swap_two"])]
    swap_three: bool,

    // Deprecated switches - parse them to show helpful error messages
    /// DEPRECATED: Exhaust mode (experimental feature never completed)
    #[arg(short = 'e', hide = true)]
    exhaust: bool,

    /// DEPRECATED: Upper/lowercase toggle (cosmetic feature not implemented)
    #[arg(short = 'u', hide = true)]
    uppercase: bool,

    /// DEPRECATED: Library mode (conflicting meanings in dealer.exe vs DealerV2_4)
    #[arg(short = 'l', hide = true)]
    library: bool,

    /// Timeout in seconds (stop generation after this many seconds)
    #[arg(short = 't', long = "timeout")]
    timeout: Option<u64>,

    /// Number of worker threads for parallel generation (0 = auto-detect, 1 = single-threaded)
    /// Matches DealerV2_4's -R switch. Default is 0 (auto-detect) for maximum performance.
    #[arg(short = 'R', long = "threads", default_value = "0")]
    threads: usize,

    /// Work units per batch for parallel generation (0 = auto, typically 200 × threads)
    #[arg(long = "batch-size", default_value = "0")]
    batch_size: usize,

    /// REMOVED: legacy dealer.exe-compatible RNG mode.
    /// Still parsed so the flag reports what happened instead of an unknown-argument
    /// error. Will be dropped entirely in a future release.
    #[arg(long = "legacy", hide = true)]
    legacy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    PrintAll,
    PrintEW,
    PrintPBN,
    PrintCompact,
    PrintOneLine,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "printall" | "all" => Ok(OutputFormat::PrintAll),
            "printew" | "ew" => Ok(OutputFormat::PrintEW),
            "printpbn" | "pbn" => Ok(OutputFormat::PrintPBN),
            "printcompact" | "compact" => Ok(OutputFormat::PrintCompact),
            "printoneline" | "oneline" => Ok(OutputFormat::PrintOneLine),
            _ => Err(format!(
                "Invalid format '{}'. Valid options: printall, printew, printpbn, printcompact, printoneline",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DealerPosition {
    North,
    East,
    South,
    West,
}

impl std::str::FromStr for DealerPosition {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "N" | "NORTH" => Ok(DealerPosition::North),
            "E" | "EAST" => Ok(DealerPosition::East),
            "S" | "SOUTH" => Ok(DealerPosition::South),
            "W" | "WEST" => Ok(DealerPosition::West),
            _ => Err(format!(
                "Invalid dealer position '{}'. Valid options: N, E, S, W, North, East, South, West",
                s
            )),
        }
    }
}

impl From<DealerPosition> for Position {
    fn from(dp: DealerPosition) -> Self {
        match dp {
            DealerPosition::North => Position::North,
            DealerPosition::East => Position::East,
            DealerPosition::South => Position::South,
            DealerPosition::West => Position::West,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VulnerabilityArg {
    None,
    NS,
    EW,
    All,
}

impl std::str::FromStr for VulnerabilityArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "NONE" | "NEITHER" => Ok(VulnerabilityArg::None),
            "NS" | "N-S" | "NORTH-SOUTH" => Ok(VulnerabilityArg::NS),
            "EW" | "E-W" | "EAST-WEST" => Ok(VulnerabilityArg::EW),
            "ALL" | "BOTH" => Ok(VulnerabilityArg::All),
            _ => Err(format!(
                "Invalid vulnerability '{}'. Valid options: None, NS, EW, All",
                s
            )),
        }
    }
}

impl From<VulnerabilityArg> for Vulnerability {
    fn from(va: VulnerabilityArg) -> Self {
        match va {
            VulnerabilityArg::None => Vulnerability::None,
            VulnerabilityArg::NS => Vulnerability::NS,
            VulnerabilityArg::EW => Vulnerability::EW,
            VulnerabilityArg::All => Vulnerability::All,
        }
    }
}

/// Escape a string for JSON. Labels come from the script, so they can hold
/// quotes, backslashes and control characters.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A JSON number that round-trips, unlike the `%g` the tables print.
///
/// The tables match dealer.exe's six significant digits, which is right for
/// reading and wrong for a tool that is about to divide by the number.
fn json_number(value: f64) -> String {
    if value.is_finite() {
        format!("{}", value)
    } else {
        "null".to_string()
    }
}

/// Lay out one seat's hands the way the original's `print(...)` action does.
///
/// Four boards to a line-printer page, twenty columns each, spades down to
/// clubs, a form feed at the end. The layout is copied from `printhands` in
/// dealer.c and checked against the reference binary byte for byte — including
/// the trailing space after every card, and the `-` a void prints.
fn format_print_hands(deals: &[Deal], seat: Position) -> String {
    let name = match seat {
        Position::North => "North",
        Position::East => "East",
        Position::South => "South",
        Position::West => "West",
    };
    let mut out = format!("\n\n{} hands:\n\n\n\n", name);
    for (page, group) in deals.chunks(4).enumerate() {
        for i in 0..group.len() {
            out.push_str(&format!("{:4}.{:15}", page * 4 + i + 1, ""));
        }
        out.push('\n');
        for suit in [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs] {
            // Each column is ten card slots wide. The original pads the
            // *previous* hand out to ten before starting the next, which is
            // why the last column on a line is never padded.
            let mut cards = 10;
            for deal in group {
                for _ in cards..10 {
                    out.push_str("  ");
                }
                cards = 0;
                let mut in_suit = deal.hand(seat).cards_in_suit(suit);
                in_suit.sort_by_key(|card| std::cmp::Reverse(card.rank));
                for card in &in_suit {
                    out.push(card.rank.to_char());
                    out.push(' ');
                    cards += 1;
                }
                if cards == 0 {
                    out.push_str("- ");
                    cards = 1;
                }
            }
            out.push('\n');
        }
        out.push('\n');
    }
    out.push('\x0c');
    out
}

/// Render one board, in whichever format the run asked for.
///
/// `board_number` is the deal's place in the output rather than in the run, so
/// that `--interleave` — which does not know the order until every deal is in —
/// still numbers its boards 1, 2, 3. It also drives the dealer and
/// vulnerability rotation when the script names neither, and a practice set
/// whose rotation disagreed with its own numbering would be a puzzle.
#[allow(clippy::too_many_arguments)]
fn render_board(
    deal: &Deal,
    board_number: usize,
    hand_type: Option<&str>,
    format: OutputFormat,
    dealer: Option<Position>,
    vulnerability: Option<Vulnerability>,
    event_name: Option<&str>,
    seed: u32,
    input_file: Option<&str>,
) -> String {
    match format {
        OutputFormat::PrintAll => format_printall(deal, board_number),
        OutputFormat::PrintEW => format_printew(deal),
        OutputFormat::PrintPBN => format_printpbn(
            deal,
            &PbnBoard {
                board_number,
                dealer,
                vulnerability,
                event_name,
                seed: Some(seed),
                input_file,
                hand_type,
            },
        ),
        OutputFormat::PrintCompact => format_printcompact(deal),
        OutputFormat::PrintOneLine => format_oneline(deal),
    }
}

/// Name a set of seats for a message: "East and West", "East, South and West".
/// One `csvrpt` or `printrpt` row, without its leading space.
///
/// DealerV2_4 writes strings in single quotes, hands in its own PBN-ish
/// notation and everything else as an integer, separated by commas — and its
/// `printrpt` output is this byte for byte, which is why the two share a
/// renderer rather than a resemblance.
fn report_row(terms: &[CsvTerm], deal: &Deal, ctx: &EvalContext) -> String {
    let mut parts: Vec<String> = Vec::new();
    for term in terms {
        match term {
            CsvTerm::Expression(expr) => match eval(expr, ctx) {
                Ok(val) => parts.push(val.to_string()),
                Err(e) => {
                    eprintln!("Report evaluation error: {}", e);
                    std::process::exit(1);
                }
            },
            CsvTerm::String(s) => parts.push(format!("'{}'", s)),
            CsvTerm::Compass(pos) => parts.push(format_hand_pbn(deal.hand(*pos))),
            CsvTerm::Side(side) => {
                let (a, b) = match side {
                    Side::NS => (Position::North, Position::South),
                    Side::EW => (Position::East, Position::West),
                };
                parts.push(format!(
                    "{} {}",
                    format_hand_pbn(deal.hand(a)),
                    format_hand_pbn(deal.hand(b))
                ));
            }
            CsvTerm::Deal => parts.push(format!(
                "{} {} {} {}",
                format_hand_pbn(deal.hand(Position::North)),
                format_hand_pbn(deal.hand(Position::East)),
                format_hand_pbn(deal.hand(Position::South)),
                format_hand_pbn(deal.hand(Position::West))
            )),
        }
    }
    parts.join(",")
}

fn describe_seats(seats: &[Position]) -> String {
    let names: Vec<&str> = seats
        .iter()
        .map(|seat| match seat {
            Position::North => "North",
            Position::East => "East",
            Position::South => "South",
            Position::West => "West",
        })
        .collect();
    match names.split_last() {
        None => "no seat".to_string(),
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} and {}", rest.join(", "), last),
    }
}

/// The seats a swapping mode leaves where they were dealt.
fn unmoved_seats(swapping: SwapMode) -> Vec<Position> {
    Position::ALL
        .into_iter()
        .filter(|seat| !swapping.moves().contains(seat))
        .collect()
}

/// Parse predeal card string (format: S8743,HA9,D642,CQT64)
/// Returns a vector of cards
fn parse_predeal_cards(card_str: &str) -> Result<Vec<dealer_core::Card>, String> {
    use dealer_core::{Card, Rank, Suit};

    let mut cards = Vec::new();

    // Split by comma
    for token in card_str.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }

        // First character is suit indicator
        if token.is_empty() {
            return Err("Empty card token".to_string());
        }

        let mut chars = token.chars();
        let suit_char = chars.next().unwrap().to_uppercase().next().unwrap();

        let suit = match suit_char {
            'S' => Suit::Spades,
            'H' => Suit::Hearts,
            'D' => Suit::Diamonds,
            'C' => Suit::Clubs,
            _ => return Err(format!("Invalid suit character: {}", suit_char)),
        };

        // Remaining characters are ranks
        for rank_char in chars {
            let rank_char = rank_char.to_uppercase().next().unwrap();
            let rank = match rank_char {
                'A' => Rank::Ace,
                'K' => Rank::King,
                'Q' => Rank::Queen,
                'J' => Rank::Jack,
                'T' => Rank::Ten,
                '9' => Rank::Nine,
                '8' => Rank::Eight,
                '7' => Rank::Seven,
                '6' => Rank::Six,
                '5' => Rank::Five,
                '4' => Rank::Four,
                '3' => Rank::Three,
                '2' => Rank::Two,
                _ => return Err(format!("Invalid rank character: {}", rank_char)),
            };

            cards.push(Card::new(suit, rank));
        }
    }

    Ok(cards)
}

/// Format a float using %g-style formatting (like C's printf %g)
/// Uses 6 significant digits (not 6 decimal places) and removes trailing zeros
fn format_g(val: f64) -> String {
    // C's %g uses 6 significant digits by default, not 6 decimal places
    // It removes trailing zeros and uses %e for very large/small numbers
    if val == 0.0 {
        return "0".to_string();
    }

    // Check if it's effectively an integer
    if val == val.trunc() && val.abs() < 1e15 {
        return format!("{}", val as i64);
    }

    // Use 6 significant digits like C's %g
    // The {:.*} syntax allows runtime precision, but we need significant digits
    // Calculate how many decimal places give us 6 significant digits
    let abs_val = val.abs();
    let log10 = abs_val.log10().floor() as i32;
    let decimal_places = (5 - log10).max(0) as usize;

    if decimal_places > 0 && (-4..6).contains(&log10) {
        // Use fixed point notation
        let s = format!("{:.prec$}", val, prec = decimal_places);
        // Trim trailing zeros and decimal point
        let s = s.trim_end_matches('0').trim_end_matches('.');
        s.to_string()
    } else if (0..6).contains(&log10) {
        // Integer-like, already handled above for exact integers
        // For non-exact, format with appropriate precision
        let s = format!("{:.prec$}", val, prec = decimal_places);
        let s = s.trim_end_matches('0').trim_end_matches('.');
        s.to_string()
    } else {
        // Use scientific notation for very large/small numbers
        // C's %g uses format like 1.23457e+06, Rust's {:e} uses 1.23457e6
        let s = format!("{:.5e}", val);
        // Ensure exponent has sign and at least 2 digits like C
        if let Some(e_pos) = s.find('e') {
            let (mantissa, exp) = s.split_at(e_pos);
            let exp_num: i32 = exp[1..].parse().unwrap_or(0);
            format!("{}e{:+03}", mantissa, exp_num)
        } else {
            s
        }
    }
}

/// The levelling, laid out for a terminal.
///
/// Presentation rather than arithmetic, so it lives with the front end that has
/// a terminal to lay it out on. The engine returns the numbers; the browser
/// draws bars from the same ones.
fn leveling_summary(leveled: &Leveled, measured: &Measurement) -> String {
    let width = leveled
        .plans
        .iter()
        .map(|p| p.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let mut out = format!(
        "measured over {} deals\n  {:<width$}  {:>9} {:>9} {:>9} {:>8} {:>9}\n",
        measured.produced,
        "type",
        "natural",
        "target",
        "mix",
        "keep",
        "seen",
        width = width
    );
    for plan in &leveled.plans {
        out.push_str(&format!(
            "  {:<width$}  {:>9.5} {:>9.5} {:>9.5} {:>8.4} {:>9}\n",
            plan.name,
            plan.natural,
            plan.target,
            plan.mix,
            plan.keep,
            plan.seen,
            width = width
        ));
    }
    let rarest = leveled
        .plans
        .iter()
        .min_by(|a, b| a.natural.total_cmp(&b.natural))
        .expect("at least one hand type");
    let relative = if rarest.seen > 0 {
        (rarest.natural * (1.0 - rarest.natural) / measured.produced as f64).sqrt() / rarest.natural
    } else {
        f64::INFINITY
    };
    out.push_str(&format!(
        "\n  exactness {:.3}{}\n  acceptance {:.4} of qualifying deals\n  about {:.0} deals \
         dealt per deal kept\n  keeps pinned down by `{}`, the rarest, seen {} times: +-{:.1}%",
        leveled.lambda,
        if leveled.lambda >= 0.999 {
            ""
        } else {
            "  (relaxed to fit the budget)"
        },
        leveled.acceptance,
        1.0 / (leveled.base_rate * leveled.acceptance),
        rarest.name,
        rarest.seen,
        100.0 * relative,
    ));
    out
}

fn main() {
    let args = Args::parse();

    // Handle version flag (matches dealer.exe -V behavior)
    if args.version {
        println!("dealer3 version {}", env!("CARGO_PKG_VERSION"));
        println!("Rust implementation of dealer.exe");
        println!("Compatible with dealer.exe and DealerV2_4");
        std::process::exit(0);
    }

    // Handle license flag
    if args.license {
        println!("License");
        println!("-------");
        println!();
        println!("This software is released into the public domain under The Unlicense.");
        println!();
        println!("You are free to use, modify, distribute, and incorporate this software");
        println!("for any purpose, with or without modification.");
        println!();
        println!("The original dealer program was also released into the public domain.");
        println!("Other independent implementations may be licensed differently.");
        println!();
        println!("See the LICENSE file in the source repository for full details.");
        std::process::exit(0);
    }

    // Handle credits flag
    if args.credits {
        println!("Credits");
        println!("-------");
        println!();
        println!("Original dealer");
        println!("  Hans van Staveren (public domain)");
        println!();
        println!("Key contributors");
        println!("  Henk Uijterwaal");
        println!("  Bruce Moore");
        println!("  Francois Dellacherie");
        println!("  Robin Barker");
        println!("  Danil Suits");
        println!("  Alex Martelli");
        println!("  Paul Hankin");
        println!("  Micke Hovmoller");
        println!("  Paul Baxter");
        println!();
        println!("dealer2");
        println!("  Greg Morse (GPLv3, independent)");
        println!();
        println!("dealer3 (Rust edition)");
        println!("  Rick Wilson");
        println!();
        // `tricks()` is the one part of dealer3 whose work is done by someone
        // else's algorithm, and bridge-solver ships in every build including
        // the browser one, so it belongs here rather than only in a manifest.
        println!("Double-dummy solver behind tricks()");
        println!("  bridge-solver (MIT OR Apache-2.0), a Rust port by Rick Wilson of");
        println!("  macroxue/bridge-solver by Hanhong Xue");
        println!();
        println!("See documentation for full contribution details.");
        std::process::exit(0);
    }

    // --legacy was removed in 0.5.0. It is still parsed so that scripts using it
    // get an explanation rather than clap's "unexpected argument" error. Drop the
    // flag entirely once the deprecation window has passed (target: 2027).
    if args.legacy {
        eprintln!("Error: legacy mode has been removed.");
        eprintln!();
        eprintln!("'--legacy' selected a single-threaded mode using a port of the GNU");
        eprintln!("random() from the original C dealer, so that '-s' reproduced");
        eprintln!("dealer.exe's exact deal sequence. That RNG has been removed.");
        eprintln!();
        eprintln!("dealer3 still accepts the same scripts and produces the same kinds of");
        eprintln!("deals; only the specific sequence for a given seed has changed.");
        eprintln!();
        eprintln!("If you need dealer.exe's exact sequence, use dealer.exe, or see the");
        eprintln!("dealer-legacy-shuffle repository for the extracted implementation.");
        eprintln!();
        eprintln!("Suggestion: remove '--legacy' from your command.");
        std::process::exit(1);
    }

    // Check for deprecated switches and provide helpful error messages
    if args.exhaust {
        eprintln!("Error: Switch '-e' (exhaust mode) is not supported in dealer3.");
        eprintln!();
        eprintln!("Reason: Exhaust mode was an experimental alpha feature in dealer.exe");
        eprintln!("        that was never completed or documented.");
        eprintln!();
        eprintln!("Suggestion: Remove the '-e' switch from your command.");
        std::process::exit(1);
    }

    if args.uppercase {
        eprintln!("Error: Switch '-u' (upper/lowercase toggle) is not supported in dealer3.");
        eprintln!();
        eprintln!("Reason: This is a cosmetic feature with low priority.");
        eprintln!();
        eprintln!("Suggestion: Remove the '-u' switch from your command.");
        eprintln!("            dealer3 uses standard uppercase card symbols (AKQJT).");
        std::process::exit(1);
    }

    if args.library {
        eprintln!("Error: Switch '-l' (library mode) is not supported in dealer3.");
        eprintln!();
        eprintln!("Reason: The '-l' switch has conflicting meanings:");
        eprintln!("        - In dealer.exe: Read deals from library.dat");
        eprintln!("        - In DealerV2_4: Export to DL52 format");
        eprintln!();
        eprintln!("Suggestion: Remove the '-l' switch from your command.");
        eprintln!("            Future versions may add library support with a different switch.");
        std::process::exit(1);
    }

    // Open CSV file if requested
    let mut csv_writer: Option<BufWriter<std::fs::File>> = None;
    if let Some(csv_arg) = &args.csv_file {
        let (filename, write_mode) = if let Some(stripped) = csv_arg.strip_prefix("w:") {
            (stripped, true)
        } else {
            (csv_arg.as_str(), false)
        };

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(!write_mode)
            .truncate(write_mode)
            .open(filename)
            .unwrap_or_else(|e| {
                eprintln!("ERROR!! Open CSV Report file FAILED");
                eprintln!(
                    "ERROR!! Can't open [{}] for {}",
                    filename,
                    if write_mode { "write" } else { "append" }
                );
                eprintln!("{}", e);
                std::process::exit(1);
            });

        csv_writer = Some(BufWriter::new(file));
    }

    // Read constraint from input file or stdin
    let mut constraint_str = String::new();
    if let Some(ref input_file) = args.input_file {
        std::fs::File::open(input_file)
            .and_then(|mut f| f.read_to_string(&mut constraint_str))
            .unwrap_or_else(|e| {
                eprintln!("Error reading input file '{}': {}", input_file, e);
                std::process::exit(1);
            });
    } else {
        io::stdin()
            .read_to_string(&mut constraint_str)
            .expect("Failed to read constraint from stdin");
    }

    // Kept as read for `--write-leveled`, which writes a copy: trimming would
    // silently drop the file's last newline from the generated one.
    let untrimmed = constraint_str.clone();
    let constraint_str = constraint_str.trim();

    // Refuse a scenario that cannot receive a levelling block before dealing
    // anything: measuring is a hundred thousand deals, and none of it is worth
    // doing if the answer has nowhere to go.
    let mut leveling_source: Option<String> = None;
    if args.write_leveled.is_some() {
        if args.input_file.is_none() {
            eprintln!(
                "Error: --write-leveled needs the scenario as a file argument, since it \
                 writes a copy of it."
            );
            std::process::exit(1);
        }
        if args.interleave {
            // The measuring run deals the natural mix, so there is nothing
            // worth walking through, and the deals would be held for an
            // ordering that never happens — silently swallowed. Level first,
            // then interleave the generated scenario.
            eprintln!(
                "Error: --interleave has nothing to order during --write-leveled: that run \
                 measures the\n       scenario as it stands. Level it first, then run the \
                 generated file with --interleave."
            );
            std::process::exit(1);
        }
        match insert_leveling_block(&untrimmed) {
            Ok(prepared) => leveling_source = Some(prepared),
            Err(message) => {
                eprintln!("Error: {}", message);
                std::process::exit(1);
            }
        }
        if let Err(message) =
            check_leveling_source(leveling_source.as_deref().unwrap_or(constraint_str))
        {
            eprintln!("Error: {}", message);
            std::process::exit(1);
        }
    }
    // Measuring runs the prepared scenario, so a placeholder written in above is
    // the one the keeps are computed against.
    let constraint_str: &str = leveling_source.as_deref().unwrap_or(constraint_str);

    // Fill the script parameters, expand the `shape{...}` shapes, then mark
    // four-digit shape literals.
    let mut params = dealer_parser::ScriptParams::default();
    for spec in &args.param {
        if let Err(message) = params.set(spec) {
            eprintln!("Error: {}", message);
            std::process::exit(1);
        }
    }
    for index in params.unused(constraint_str) {
        eprintln!(
            "Warning: --param {} was given and the script never mentions `${}`",
            index, index
        );
    }
    let preprocessed = match dealer_parser::preprocess_all(constraint_str, &params) {
        Ok(text) => text,
        Err(message) => {
            eprintln!("Error: {}", message);
            std::process::exit(1);
        }
    };

    // Parse the program (may include variable assignments and action blocks)
    let program = match dealer_parser::parse_program(&preprocessed) {
        Ok(program) => program,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            std::process::exit(1);
        }
    };

    // Names with nothing behind them. A bare expression is a legal statement, so
    // a misspelled keyword parses rather than failing — `dealr west` is a
    // variable reference and a compass, both discarded. dealer.exe answers
    // `line 1: unknown variable`; without this, dealer3 dealt on quietly.
    let unknown = dealer_parser::undefined_variables(&program);
    if !unknown.is_empty() {
        eprintln!(
            "Error: {} used but never defined: {}.\n       A misspelled statement keyword \
             looks like this — `dealr west` is a name and a compass, not a `dealer` \
             statement.",
            if unknown.len() == 1 {
                "a name is"
            } else {
                "names are"
            },
            unknown.join(", ")
        );
        std::process::exit(1);
    }

    // Reported after the undefined names, not before: `dealr west` is a bare
    // seat too, and there the misspelled keyword is the thing worth saying.
    let dangling = dealer_parser::dangling_seats(&program);
    if let Some(seat) = dangling.first() {
        eprintln!(
            "Error: `{seat}` is on its own, which does nothing.\n       \
             A seat in a `predeal` needs its holdings — `predeal north SAKQ {seat} SJ32`, \
             not `predeal north SAKQ {seat}`."
        );
        std::process::exit(1);
    }

    // Extract action block directives from the program
    let mut produce_count_from_input: Option<usize> = None;
    let mut generate_count_from_input: Option<usize> = None;
    let mut format_from_input: Option<OutputFormat> = None;
    let mut dealer_from_input: Option<DealerPosition> = None;
    let mut vuln_from_input: Option<VulnerabilityArg> = None;
    let mut title_from_input: Option<String> = None;
    let mut seed_from_input: Option<u32> = None;

    // `average` and `frequency` are accumulated by `dealer-run`, which reads
    // them off the program itself — see `RunAccumulator`.

    // Track CSV report statements
    use dealer_parser::EsTerm;
    let mut csv_reports: Vec<Vec<CsvTerm>> = Vec::new();
    // The same lists, to stdout rather than a file.
    let mut print_reports: Vec<Vec<CsvTerm>> = Vec::new();
    // `printes(...)` lists, printed per matching deal in the order written.
    let mut printes_reports: Vec<Vec<EsTerm>> = Vec::new();
    // Seats named by `print(...)`, whose hands are laid out once at the end.
    let mut print_hand_seats: Vec<Position> = Vec::new();

    for statement in &program.statements {
        match statement {
            Statement::Produce(n) => produce_count_from_input = Some(*n),
            Statement::Generate(n) => generate_count_from_input = Some(*n),
            Statement::Action {
                format: action_format,
                printes: printes_specs,
                print_hands,
                print_reports: report_specs,
                ..
            } => {
                print_reports.extend(report_specs.iter().cloned());
                // Extract format if present
                if let Some(action_type) = action_format {
                    format_from_input = Some(match action_type {
                        ActionType::PrintAll => OutputFormat::PrintAll,
                        ActionType::PrintEW => OutputFormat::PrintEW,
                        ActionType::PrintPBN => OutputFormat::PrintPBN,
                        ActionType::PrintCompact => OutputFormat::PrintCompact,
                        ActionType::PrintOneLine => OutputFormat::PrintOneLine,
                    });
                }
                printes_reports.extend(printes_specs.iter().cloned());
                for seat in print_hands {
                    if !print_hand_seats.contains(seat) {
                        print_hand_seats.push(*seat);
                    }
                }
            }
            Statement::Dealer(pos) => {
                dealer_from_input = Some(match pos {
                    Position::North => DealerPosition::North,
                    Position::East => DealerPosition::East,
                    Position::South => DealerPosition::South,
                    Position::West => DealerPosition::West,
                });
            }
            Statement::Vulnerable(vuln) => {
                vuln_from_input = Some(match *vuln {
                    VulnerabilityType::None => VulnerabilityArg::None,
                    VulnerabilityType::NS => VulnerabilityArg::NS,
                    VulnerabilityType::EW => VulnerabilityArg::EW,
                    VulnerabilityType::All => VulnerabilityArg::All,
                });
            }
            Statement::Title(text) => {
                title_from_input = Some(text.clone());
            }
            Statement::Seed(value) => {
                seed_from_input = Some(*value);
            }
            Statement::CsvReport(terms) => {
                csv_reports.push(terms.clone());
            }
            Statement::PrintReport(terms) => {
                print_reports.push(terms.clone());
            }
            _ => {}
        }
    }

    // Extract variables and constraint from program (do this once before the loop)
    // This avoids cloning expression trees on every iteration
    let program_variables = extract_variables(&program);
    let constraint = extract_constraint(&program);

    // `None` unless the script redefines a count, which keeps the hardcoded
    // counts on the hot path for every script that does not.
    let point_counts = match extract_point_counts(&program) {
        Ok(counts) => counts,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    let point_counts = point_counts.as_ref();

    // Determine limits for generation
    // -g limits total hands generated, -p limits matching hands produced
    // When both are specified, stop when either limit is reached
    // dealer.exe defaults: -g 10000000 (10M), -p 40
    // IMPORTANT: We must respect the generate limit to match dealer.exe behavior.
    // Without this, dealer3 could run forever trying to produce rare hands.
    let max_generate = args
        .generate
        .or(generate_count_from_input)
        .unwrap_or(10_000_000);
    let produce_count = args
        .produce
        .or(produce_count_from_input)
        .unwrap_or_else(|| {
            if args.generate.is_some() {
                usize::MAX // No produce limit when only -g is specified
            } else {
                40 // dealer.exe default for -p
            }
        });

    // A measuring run is sized by what it is measuring, not by `-p`, and `-p`
    // means something else here anyway — a scenario's own `produce 5000` is
    // about the practice set, not about how well its rates are known. So
    // `--level-measure` is the ceiling and the rarest type decides when to stop
    // below it. Without this the measurement was whatever `-p` happened to say,
    // which for a type at 0.2% of qualifying deals was nowhere near enough.
    let produce_count = if args.write_leveled.is_some() {
        args.level_measure
    } else {
        produce_count
    };

    let output_format = args
        .format
        .or(format_from_input)
        .unwrap_or(OutputFormat::PrintAll); // Default format (matches dealer.exe)

    let dealer_position = args.dealer.or(dealer_from_input);

    // `-s` beats a `seed` statement, and the clock is the last resort. Resolved
    // here rather than before parsing, because the script may name it.
    let seed = args.seed.or(seed_from_input).unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_micros() as u32
    });

    let vulnerability = args.vulnerability.or(vuln_from_input);

    // `-T` beats a `title` statement, the way `-d` beats `dealer` and
    // `--vulnerable` beats `vulnerable`.
    let title = args.title.clone().or(title_from_input);

    // Start timing
    let start_time = SystemTime::now();

    dealer_eval::rnd::set_seed(args.rnd_seed);

    // Categories of hand the script names, for the PBN tag and for ordering a
    // practice set. Empty for almost every script, and then nothing changes.
    let hand_type_names = hand_types(&program);

    // What the keeps are computed from, which is not always what the deals are
    // grouped by: a scenario may level on `LevelType_` while still being tagged,
    // ordered and reported by `HandType_`. Resolved here so a mixed-up set of
    // shares is refused before anything is dealt.
    let leveling = match dealer_level::leveling_types(&program) {
        Ok(types) => types,
        Err(message) => {
            eprintln!("Error: {}", message);
            std::process::exit(1);
        }
    };
    // The loop's body, shared with the browser. It owns the classification
    // against both decompositions, the `average`s and `frequency`s, and the
    // rule for when a measuring run has learnt enough.
    let mut accumulator = match RunAccumulator::new(&program, MeasureStop::standard()) {
        Ok(accumulator) => accumulator,
        Err(message) => {
            eprintln!("Error: {}", message);
            std::process::exit(1);
        }
    };

    // How many deals each shuffle turns into. The three switches override one
    // another, so the last one written wins, as it does under getopt.
    let swapping = if args.swap_three {
        SwapMode::ThreeWay
    } else if args.swap_two {
        SwapMode::TwoWay
    } else {
        SwapMode::None
    };

    // Collect predeal configuration (shared between legacy and fast modes)
    let mut fast_predeal_config = FastDealConfig::new();

    // Apply command-line predeal switches
    if let Some(ref cards_str) = args.north_predeal {
        match parse_predeal_cards(cards_str) {
            Ok(cards) => {
                if let Err(e) = fast_predeal_config.predeal(Position::North, &cards) {
                    eprintln!("Error predealing to North: {}", e);
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Error parsing North predeal cards '{}': {}", cards_str, e);
                std::process::exit(1);
            }
        }
    }

    if let Some(ref cards_str) = args.east_predeal {
        match parse_predeal_cards(cards_str) {
            Ok(cards) => {
                if let Err(e) = fast_predeal_config.predeal(Position::East, &cards) {
                    eprintln!("Error predealing to East: {}", e);
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Error parsing East predeal cards '{}': {}", cards_str, e);
                std::process::exit(1);
            }
        }
    }

    if let Some(ref cards_str) = args.south_predeal {
        match parse_predeal_cards(cards_str) {
            Ok(cards) => {
                if let Err(e) = fast_predeal_config.predeal(Position::South, &cards) {
                    eprintln!("Error predealing to South: {}", e);
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Error parsing South predeal cards '{}': {}", cards_str, e);
                std::process::exit(1);
            }
        }
    }

    if let Some(ref cards_str) = args.west_predeal {
        match parse_predeal_cards(cards_str) {
            Ok(cards) => {
                if let Err(e) = fast_predeal_config.predeal(Position::West, &cards) {
                    eprintln!("Error predealing to West: {}", e);
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Error parsing West predeal cards '{}': {}", cards_str, e);
                std::process::exit(1);
            }
        }
    }

    // Apply predeal statements from input file
    for statement in &program.statements {
        if let Statement::Predeal { position, cards } = statement {
            if let Err(e) = fast_predeal_config.predeal(*position, cards) {
                eprintln!("Predeal error: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Check if we have any predeal
    let has_predeal = fast_predeal_config.predeal_count(Position::North) > 0
        || fast_predeal_config.predeal_count(Position::East) > 0
        || fast_predeal_config.predeal_count(Position::South) > 0
        || fast_predeal_config.predeal_count(Position::West) > 0;

    // Swapping rearranges whole hands after the deal, so a predeal to a seat it
    // moves would be honoured on the first deal of each shuffle and quietly
    // broken on the rest. The original does exactly that and says nothing; here
    // it is refused, and only for the seats actually at risk — `predeal north`
    // with `-3`, a fixed declarer against six defensive layouts, is the whole
    // point of the switch and keeps working.
    let clashing_predeals: Vec<Position> = swapping
        .moves()
        .iter()
        .copied()
        .filter(|seat| fast_predeal_config.predeal_count(*seat) > 0)
        .collect();
    if !clashing_predeals.is_empty() {
        eprintln!(
            "Error: '{}' swapping moves the cards of {}, so it cannot be combined with a \
             predeal to {}.",
            swapping.switch(),
            describe_seats(swapping.moves()),
            describe_seats(&clashing_predeals)
        );
        eprintln!();
        eprintln!(
            "Reason: swapping exchanges whole hands between seats after the deal, which \
             would move predealt cards to a seat the script did not ask for."
        );
        eprintln!();
        eprintln!(
            "Suggestion: predeal only to {}, which '{}' leaves in place, or drop the switch.",
            describe_seats(&unmoved_seats(swapping)),
            swapping.switch()
        );
        std::process::exit(1);
    }

    // Validate --input-deals conflicts
    if let Some(ref source) = args.input_deals {
        if swapping != SwapMode::None {
            eprintln!(
                "Error: '{}' swapping rearranges deals this program shuffled, so it has \
                 nothing to do with deals read by --input-deals.",
                swapping.switch()
            );
            std::process::exit(1);
        }
        if has_predeal {
            eprintln!(
                "Error: --input-deals cannot be combined with predeal (command-line or script)"
            );
            std::process::exit(1);
        }
        // `--input-deals -` reads deals from stdin, but stdin is already consumed by the
        // script when no script file argument is given. Require an explicit script file.
        if source == "-" && args.input_file.is_none() {
            eprintln!(
                "Error: --input-deals - reads deals from stdin, but the script is also being \
                 read from stdin.\n       Pass the script as a file argument instead, e.g.: \
                 dealer script.dlr --input-deals -"
            );
            std::process::exit(1);
        }
        if args.seed.is_some() {
            eprintln!("Warning: --seed is ignored when using --input-deals");
        }
    }

    let mut produced = 0;
    let mut generated: usize = 0;

    // `print(...)` lays its hands out at the end, four boards to a page, so it
    // is the one action that needs every produced deal kept. Nothing is kept
    // unless a script asks for it.
    let mut printed_deals: Vec<Deal> = Vec::new();

    // `--interleave` cannot print as it goes: the order is not known until
    // every deal is in. Each entry is one deal and the type it matched, held
    // unrendered because the board number depends on where it lands.
    let mut held: Vec<(Option<String>, Deal)> = Vec::new();

    // Verbose flag for stats output (matches dealer.exe behavior)
    // Default is true (stats shown), -v toggles it off
    // -X forces stats on (cannot be toggled off)
    // Note: We intentionally don't replicate dealer.exe's PBN verbose toggle bug
    // dealer.exe behavior: stats hidden by default, -v shows them
    let verbose_stats = args.force_verbose || args.verbose;

    // Progress meter variables (matches dealer.exe behavior)
    let progress_interval = 10000; // Show progress every 10,000 deals
    let mut last_progress_report = 0;

    // Track if we timed out
    let mut timed_out = false;

    // Only a measuring run stops early, and only it needs to ask.
    let measuring = args.write_leveled.is_some();

    // The body of the loop, for the deals the condition accepted. Classifying
    // and counting is `dealer-run`'s; what is left here is what a terminal does
    // with a deal and a browser does not.
    //
    // The accumulator is a parameter rather than captured so the loop outside
    // can ask it whether a measuring run has finished — which is what the
    // `Cell` this replaces was working around.
    let mut process_matching_deal =
        |deal: &Deal,
         produced: usize,
         accumulator: &mut RunAccumulator,
         csv_writer: &mut Option<BufWriter<std::fs::File>>,
         printed_deals: &mut Vec<Deal>| {
            if !print_hand_seats.is_empty() {
                printed_deals.push(deal.clone());
            }

            let matched = accumulator
                .observe(deal, &program_variables, point_counts)
                .unwrap_or_else(|e| {
                    // With the deal, when there is one to show: two definitions
                    // written pages apart overlap on a corner neither author
                    // had in mind, and the corner is what has to be looked at.
                    match e.deal() {
                        Some(deal) => eprintln!(
                            "Error: {}\n       The deal:\n       {}",
                            e,
                            format_oneline(deal).trim_end()
                        ),
                        None => eprintln!("Error: {}", e),
                    }
                    std::process::exit(1);
                });
            let hand_type = matched
                .hand_type
                .map(|i| accumulator.hand_type_labels()[i].as_str());

            // printes: the script's own formatted output, with nothing added
            // between terms and no line ending unless the script asked for one.
            if !printes_reports.is_empty() {
                let ctx = EvalContext::with_counts(deal, &program_variables, point_counts);
                for terms in &printes_reports {
                    for term in terms {
                        match term {
                            EsTerm::String(text) => print!("{}", text),
                            EsTerm::Newline => println!(),
                            EsTerm::Expression(expr) => match eval(expr, &ctx) {
                                Ok(value) => print!("{}", value),
                                Err(e) => {
                                    eprintln!("printes evaluation error: {}", e);
                                    std::process::exit(1);
                                }
                            },
                        }
                    }
                }
            }

            // In quiet mode, don't print deals (only statistics)
            if !args.quiet {
                // `--interleave` holds the deal rather than the rendered
                // board: the board number belongs to the position a deal ends
                // up in, which is not known until every deal is in.
                if args.interleave {
                    held.push((hand_type.map(str::to_string), deal.clone()));
                } else {
                    print!(
                        "{}",
                        render_board(
                            deal,
                            produced,
                            hand_type,
                            output_format,
                            dealer_position.map(|d| d.into()),
                            vulnerability.map(|v| v.into()),
                            title.as_deref(),
                            seed,
                            args.input_file.as_deref(),
                        )
                    );
                }
            }

            // Report rows: `csvrpt` to the file, `printrpt` to stdout. One
            // renderer, because DealerV2_4's two statements differ only in
            // where the row goes — same terms, same quoting, same commas.
            if (!csv_reports.is_empty() && csv_writer.is_some()) || !print_reports.is_empty() {
                let ctx = EvalContext::with_counts(deal, &program_variables, point_counts);

                for csv_terms in &csv_reports {
                    let row = report_row(csv_terms, deal, &ctx);
                    if let Some(writer) = csv_writer.as_mut() {
                        writeln!(writer, " {}", row).unwrap_or_else(|e| {
                            eprintln!("Error writing CSV: {}", e);
                            std::process::exit(1);
                        });
                    }
                }
                for terms in &print_reports {
                    println!(" {}", report_row(terms, deal, &ctx));
                }
            }
        };

    // Choose execution mode: input-deals, legacy, or fast (parallel)
    if let Some(ref input_deals_source) = args.input_deals {
        // Input-deals mode: read deals from a file or stdin, apply filter
        use bridge_encodings::DealReader;
        use std::io::{BufRead, BufReader};

        // `-` means stdin; the guard above guarantees the script came from a file.
        let source: Box<dyn BufRead> = if input_deals_source == "-" {
            Box::new(BufReader::new(io::stdin()))
        } else {
            let file = std::fs::File::open(input_deals_source).unwrap_or_else(|e| {
                eprintln!(
                    "Error opening input deals file '{}': {}",
                    input_deals_source, e
                );
                std::process::exit(1);
            });
            Box::new(BufReader::new(file))
        };
        let deal_reader = DealReader::new(source);

        // DealReader silently ignores lines it does not recognise as deals, which is
        // what lets PBN metadata and stats output be fed in directly. It only yields
        // Err for I/O failures (e.g. invalid UTF-8 mid-stream). Those are recoverable,
        // so skip rather than abort, and report the total at exit. Because unreadable
        // *content* is indistinguishable from metadata, callers that need to know every
        // deal arrived should compare the reported count against an expected total.
        let mut skipped: usize = 0;
        const MAX_SKIP_WARNINGS: usize = 10;

        for deal_result in deal_reader {
            // Check timeout every 1000 deals
            if let Some(timeout_secs) = args.timeout {
                if generated.is_multiple_of(1000) {
                    let elapsed = start_time.elapsed().unwrap().as_secs();
                    if elapsed >= timeout_secs {
                        timed_out = true;
                        eprintln!(
                            "Timeout after {} seconds ({} generated, {} produced)",
                            elapsed, generated, produced
                        );
                        break;
                    }
                }
            }

            let bt_deal = match deal_result {
                Ok(d) => d,
                Err(e) => {
                    skipped += 1;
                    if skipped <= MAX_SKIP_WARNINGS {
                        eprintln!("Warning: skipping unreadable deal: {}", e);
                        if skipped == MAX_SKIP_WARNINGS {
                            eprintln!(
                                "Warning: further malformed-deal warnings suppressed; \
                                 total will be reported at exit"
                            );
                        }
                    }
                    continue;
                }
            };

            // Convert bridge_types::Deal → dealer_core::Deal
            let deal: Deal = bt_deal.into();
            generated += 1;

            // Show progress meter if enabled
            if args.progress && generated - last_progress_report >= progress_interval {
                let elapsed = start_time.elapsed().unwrap().as_secs_f64();
                eprintln!(
                    "Generated: {} hands, Produced: {} hands, Time: {:.1}s",
                    generated, produced, elapsed
                );
                last_progress_report = generated;
            }

            // Evaluate constraint
            let eval_result = match constraint {
                Some(expr) => {
                    eval_with_context_and_counts(expr, &program_variables, &deal, point_counts)
                }
                None => Ok(1),
            };

            match eval_result {
                Ok(result) if result != 0 => {
                    process_matching_deal(
                        &deal,
                        produced,
                        &mut accumulator,
                        &mut csv_writer,
                        &mut printed_deals,
                    );
                    produced += 1;
                    if produced >= produce_count {
                        break;
                    }
                }
                Ok(_) => continue,
                Err(e) => {
                    eprintln!("Evaluation error: {}", e);
                    std::process::exit(1);
                }
            }

            if generated >= max_generate {
                break;
            }
        }

        if skipped > 0 {
            eprintln!("Warning: skipped {} unreadable deal(s) from input", skipped);
        }
    } else {
        // Fast mode: parallel execution with xoshiro256++ RNG
        // Deals are independent - same seed produces same sequence
        let config = FastParallelConfig {
            num_threads: args.threads,
        };

        let mut supervisor = if has_predeal {
            FastSupervisor::with_predeal(seed as u64, fast_predeal_config, config)
        } else {
            FastSupervisor::new(seed as u64, config)
        }
        .with_swapping(swapping);

        let actual_batch_size = if args.batch_size == 0 {
            200 * if args.threads == 0 {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1)
            } else {
                args.threads
            }
        } else {
            args.batch_size
        };

        while produced < produce_count && generated < max_generate {
            // Enough of every hand type to divide by, which is the only thing a
            // measuring run is for.
            // `measuring &&` because the accumulator answers for any script
            // whose categories are all well sampled; only a measuring run has
            // reason to stop there. The flag this replaced was set nowhere else,
            // so the guard was implicit.
            if measuring && accumulator.measure_satisfied() {
                break;
            }
            // Check timeout before each batch
            if let Some(timeout_secs) = args.timeout {
                let elapsed = start_time.elapsed().unwrap().as_secs();
                if elapsed >= timeout_secs {
                    timed_out = true;
                    eprintln!(
                        "Timeout after {} seconds ({} generated, {} produced)",
                        elapsed, generated, produced
                    );
                    break;
                }
            }

            // The measuring run's own clock, so a scenario whose rarest type is
            // desperately rare stops on a deadline instead of grinding to
            // `--level-measure`. Not an error: the levelling is written with
            // what was measured and the shortfall reported.
            if args.write_leveled.is_some() {
                let elapsed = start_time.elapsed().unwrap().as_secs();
                if elapsed >= args.level_timeout {
                    eprintln!(
                        "Note: stopped measuring after {}s ({} produced). \
                         Raise --level-timeout to measure for longer.",
                        elapsed, produced
                    );
                    break;
                }
            }

            // Calculate batch size for this iteration
            let remaining_to_generate = max_generate - generated;
            let batch_size = actual_batch_size.min(remaining_to_generate);

            if batch_size == 0 {
                break;
            }

            // Process batch in parallel
            // The filter closure evaluates the constraint for each deal
            let results = supervisor.process_batch(batch_size, |deal| {
                match constraint {
                    Some(expr) => {
                        // Note: This creates a new EvalContext for each deal in parallel
                        // The program_variables are shared (read-only)
                        match eval_with_context_and_counts(
                            expr,
                            &program_variables,
                            deal,
                            point_counts,
                        ) {
                            Ok(result) => result != 0,
                            Err(_) => false, // Treat errors as non-matching
                        }
                    }
                    None => true, // No constraint = always match
                }
            });

            // Process results in order, stopping when we have enough
            for result in results {
                generated += 1;

                // Show progress meter if enabled
                if args.progress && generated - last_progress_report >= progress_interval {
                    let elapsed = start_time.elapsed().unwrap().as_secs_f64();
                    eprintln!(
                        "Generated: {} hands, Produced: {} hands, Time: {:.1}s",
                        generated, produced, elapsed
                    );
                    last_progress_report = generated;
                }

                if result.passed && produced < produce_count {
                    process_matching_deal(
                        &result.deal,
                        produced,
                        &mut accumulator,
                        &mut csv_writer,
                        &mut printed_deals,
                    );
                    produced += 1;

                    // Stop counting generated deals once we've produced enough
                    // This matches dealer.exe behavior where it stops at the deal that
                    // satisfied the produce count
                    if produced >= produce_count {
                        break;
                    }

                    // A measuring run stops on the very deal that finished the
                    // job, not at the end of the batch it fell in. Batches are
                    // 200 deals per thread, so stopping at a batch boundary
                    // would make the measurement — and the generated file that
                    // comes out of it — depend on how many cores the machine
                    // has. The example pair in `examples/` is regenerated and
                    // diffed by CI, which would fail on any machine but the one
                    // it was written on.
                    if measuring && accumulator.measure_satisfied() {
                        break;
                    }
                }
            }
        }
    }

    // Calculate elapsed time
    let elapsed = start_time.elapsed().unwrap();
    let elapsed_secs = elapsed.as_secs_f64();

    // Taken before `finish` consumes the accumulator, which is what bins the
    // frequencies: the shares are wanted further down than the statistics are.
    let hand_type_counts = accumulator.hand_type_counts().to_vec();

    // `--level-plan`: report what would level the hand types, and stop. The
    // run above did the measuring — every produced deal was classified — so
    // this is arithmetic on counts already gathered.
    if let Some(ref leveled_path) = args.write_leveled {
        // The levelling decomposition, which is the hand types unless the
        // scenario declared `LevelType_` variables of its own.
        let labels = leveling.labels.clone();
        let measured = accumulator.measurement(generated);

        // The switch wins over the script, as `-s` does over `seed`. Without
        // either, the script's own `_Share` declarations answer — and those
        // default to 1 each, which is an even mix.
        let weights = match &args.level_target {
            Some(spec) => match dealer_level::parse_level_target(spec, &labels) {
                Ok(weights) => weights,
                Err(message) => {
                    eprintln!("Error: {}", message);
                    std::process::exit(1);
                }
            },
            None => leveling.shares.clone(),
        };

        // The prepared scenario, not the file: a placeholder written in above
        // belongs in the copy as well, or the two would disagree.
        let source = leveling_source.clone().unwrap_or_default();
        let leveled = match level_from(
            &source,
            &measured,
            &weights,
            args.level_budget,
            seed,
            MIN_HAND_TYPE_SAMPLE,
        ) {
            Ok(leveled) => leveled,
            Err(message) => {
                eprintln!("Error: {}", message);
                std::process::exit(1);
            }
        };
        for warning in &leveled.warnings {
            eprintln!("Warning: {}", warning);
        }
        if let Err(e) = std::fs::write(leveled_path, &leveled.script) {
            eprintln!("Error: could not write {}: {}", leveled_path.display(), e);
            std::process::exit(1);
        }
        eprintln!(
            "{}\n\nwrote {}",
            leveling_summary(&leveled, &measured),
            leveled_path.display()
        );
        return;
    }

    // Held output, reordered so a practice set walks through the categories
    // rather than meeting them as they happen to fall.
    if args.interleave && !held.is_empty() {
        let mut buckets: Vec<(Option<String>, Vec<usize>)> = Vec::new();
        for (index, (hand_type, _)) in held.iter().enumerate() {
            match buckets.iter_mut().find(|(name, _)| name == hand_type) {
                Some((_, deals)) => deals.push(index),
                None => buckets.push((hand_type.clone(), vec![index])),
            }
        }
        if verbose_stats {
            let mut sizes: Vec<String> = buckets
                .iter()
                .map(|(name, deals)| {
                    format!("{} {}", name.as_deref().unwrap_or("(untyped)"), deals.len())
                })
                .collect();
            sizes.sort();
            eprintln!(
                "Interleaved {} rounds: {}",
                buckets.iter().map(|(_, d)| d.len()).max().unwrap_or(0),
                sizes.join(", ")
            );
        }
        let labels: Vec<&str> = hand_type_names.iter().map(|n| hand_type_label(n)).collect();
        // Numbered by where a board lands, not by when it was produced. The
        // order is the whole point of the switch, and a reader that sorts or
        // renumbers by `[Board]` would otherwise undo it without saying so.
        for (position, index) in interleave(&labels, buckets, seed as u64)
            .into_iter()
            .enumerate()
        {
            let (hand_type, deal) = &held[index];
            print!(
                "{}",
                render_board(
                    deal,
                    position,
                    hand_type.as_deref(),
                    output_format,
                    dealer_position.map(|d| d.into()),
                    vulnerability.map(|v| v.into()),
                    title.as_deref(),
                    seed,
                    args.input_file.as_deref(),
                )
            );
        }
    }

    // `print(...)` output, before the statistics as in the original. Seats come
    // out north, east, south, west whatever order the script named them, since
    // the original collects them into a bitmask.
    if !print_hand_seats.is_empty() && !printed_deals.is_empty() {
        for seat in Position::ALL {
            if print_hand_seats.contains(&seat) {
                print!("{}", format_print_hands(&printed_deals, seat));
            }
        }
    }

    // One JSON object instead of the tables, for a tool rather than a reader.
    // Everything a caller needs to compute levelling keeps is here: each
    // average's value and the count it was measured over, and the frequency
    // bins with what fell outside a declared range.
    let dealer_run::Stats {
        averages,
        frequencies,
    } = accumulator.finish();

    if args.stats_json {
        let mut out = String::from("{\n");
        out.push_str(&format!("  \"generated\": {},\n", generated));
        out.push_str(&format!("  \"produced\": {},\n", produced));
        match args.input_deals {
            Some(ref source) => {
                out.push_str(&format!("  \"input_deals\": {},\n", json_string(source)))
            }
            None => out.push_str(&format!("  \"seed\": {},\n", seed)),
        }
        out.push_str(&format!("  \"seconds\": {},\n", json_number(elapsed_secs)));
        out.push_str(&format!("  \"timed_out\": {},\n", timed_out));
        // Names and shares together: checking a levelled scenario delivered
        // its mix is the point of running one, and reading it off here saves
        // every scenario carrying an `average` statement per type to say what
        // the run already counted.
        out.push_str("  \"hand_types\": [");
        for (i, name) in hand_type_names.iter().enumerate() {
            let label = hand_type_label(name);
            let count = hand_type_counts.get(i).copied().unwrap_or(0);
            let share = if produced > 0 {
                count as f64 / produced as f64
            } else {
                0.0
            };
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            out.push_str(&format!(
                "    {{ \"name\": {}, \"produced\": {}, \"share\": {} }}",
                json_string(label),
                count,
                json_number(share)
            ));
        }
        if !hand_type_names.is_empty() {
            out.push('\n');
            out.push_str("  ");
        }
        out.push_str("],\n");

        out.push_str("  \"averages\": [");
        for (i, average) in averages.iter().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            out.push_str(&format!(
                "    {{ \"label\": {}, \"value\": {}, \"count\": {} }}",
                match &average.label {
                    Some(text) => json_string(text),
                    None => "null".to_string(),
                },
                json_number(average.value),
                average.count
            ));
        }
        out.push_str(if averages.is_empty() {
            "],\n"
        } else {
            "\n  ],\n"
        });

        out.push_str("  \"frequencies\": [");
        for (i, frequency) in frequencies.iter().enumerate() {
            let bins: Vec<String> = frequency
                .bins
                .iter()
                .map(|(value, count)| format!("{{ \"value\": {}, \"count\": {} }}", value, count))
                .collect();
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            out.push_str(&format!(
                "    {{ \"label\": {}, \"min\": {}, \"max\": {}, \"below\": {}, \"above\": {}, \"total\": {}, \"bins\": [{}] }}",
                match &frequency.label {
                    Some(text) => json_string(text),
                    None => "null".to_string(),
                },
                match frequency.min {
                    Some(min) => min.to_string(),
                    None => "null".to_string(),
                },
                match frequency.max {
                    Some(max) => max.to_string(),
                    None => "null".to_string(),
                },
                frequency.below,
                frequency.above,
                frequency.total,
                bins.join(", ")
            ));
        }
        out.push_str(if frequencies.is_empty() {
            "]\n"
        } else {
            "\n  ]\n"
        });

        out.push('}');
        println!("{}", out);

        if timed_out {
            std::process::exit(2);
        }
        return;
    }

    // Print averages if any were requested (format matches dealer.exe %g format)
    // dealer.exe outputs averages to stdout without any prefix
    if !averages.is_empty() {
        for average in &averages {
            // Output using %g-style formatting to match dealer.exe
            // %g removes trailing zeros and uses shortest representation
            match &average.label {
                Some(label) => println!("{}: {}", label, format_g(average.value)),
                None => println!("Average: {}", format_g(average.value)),
            }
        }
    }

    // Print frequency tables if any were requested (format matches dealer.exe)
    if !frequencies.is_empty() {
        for frequency in &frequencies {
            match &frequency.label {
                // dealer.exe format: "Frequency <label>:" - preserve label exactly
                Some(label) => println!("Frequency {}:", label),
                None => println!("Frequency :"),
            }

            // Print frequency table (format matches dealer.exe: "%5d\t%8ld")
            // dealer.exe prints "Low" and "High" rows for out-of-range values
            // when a range is specified.
            if frequency.min.is_some() && frequency.below > 0 {
                println!("Low\t{:8}", frequency.below);
            }
            for (value, count) in &frequency.bins {
                println!("{:5}\t{:8}", value, count);
            }
            if frequency.max.is_some() && frequency.above > 0 {
                println!("High\t{:8}", frequency.above);
            }
        }
    }

    // Print stats if verbose_stats is true (matches dealer.exe behavior)
    // verbose_stats starts true and is toggled by PBN output
    // So: PBN with odd count = no stats, PBN with even count = stats, other formats = always stats
    if verbose_stats {
        // Named rather than merely counted: the prefix is a convention the
        // parser cannot check, so a misspelling shows up as a missing name
        // instead of a set that is quietly short of a category.
        if !hand_type_names.is_empty() {
            println!(
                "Hand types {}: {}",
                hand_type_names.len(),
                hand_type_names
                    .iter()
                    .map(|n| hand_type_label(n))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        println!("Generated {} hands", generated);
        println!("Produced {} hands", produced);
        if let Some(ref source) = args.input_deals {
            println!("Input deals from {}", source);
        } else {
            println!("Initial random seed {}", seed);
        }
        println!("Time needed  {:7.3} sec", elapsed_secs);
    }

    // Exit with error code if timed out
    if timed_out {
        std::process::exit(2);
    }
}
