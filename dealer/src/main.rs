mod fast_parallel;
mod parallel;

use clap::Parser;
use dealer_core::{Deal, DealGenerator, FastDealConfig, Position};
use dealer_eval::{eval, eval_with_context, extract_constraint, extract_variables, EvalContext};
use dealer_parser::{ActionType, Expr, Statement, VulnerabilityType};
use dealer_pbn::{
    format_hand_pbn, format_oneline, format_printall, format_printcompact, format_printew,
    format_printpbn, Vulnerability,
};
use fast_parallel::{FastParallelConfig, FastSupervisor};
use std::fs::OpenOptions;
use std::io::{self, BufWriter, Read, Write};
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

    // Deprecated switches - parse them to show helpful error messages
    /// DEPRECATED: 2-way swapping mode (not supported - incompatible with predeal)
    #[arg(short = '2', hide = true)]
    swap_2: bool,

    /// DEPRECATED: 3-way swapping mode (not supported - incompatible with predeal)
    #[arg(short = '3', hide = true)]
    swap_3: bool,

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

    /// Use legacy mode: single-threaded with dealer.exe-compatible RNG.
    /// Required for bit-for-bit output comparison with dealer.exe.
    /// Without this flag, dealer3 uses a faster parallel algorithm that produces
    /// statistically equivalent but different random deals.
    #[arg(long = "legacy")]
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
        println!("See documentation for full contribution details.");
        std::process::exit(0);
    }

    // Check for deprecated switches and provide helpful error messages
    if args.swap_2 {
        eprintln!("Error: Switch '-2' (2-way swapping) is not supported in dealer3.");
        eprintln!();
        eprintln!("Reason: Swapping modes are incompatible with predeal functionality,");
        eprintln!("        which is a core feature of dealer3.");
        eprintln!();
        eprintln!("Suggestion: Remove the '-2' switch from your command.");
        eprintln!("            If you need swapping, use the original dealer.exe.");
        std::process::exit(1);
    }

    if args.swap_3 {
        eprintln!("Error: Switch '-3' (3-way swapping) is not supported in dealer3.");
        eprintln!();
        eprintln!("Reason: Swapping modes are incompatible with predeal functionality,");
        eprintln!("        which is a core feature of dealer3.");
        eprintln!();
        eprintln!("Suggestion: Remove the '-3' switch from your command.");
        eprintln!("            If you need swapping, use the original dealer.exe.");
        std::process::exit(1);
    }

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

    // Use provided seed or default to current time (microsecond resolution)
    let seed = args.seed.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_micros() as u32
    });

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

    let constraint_str = constraint_str.trim();

    // Preprocess to mark 4-digit numbers in shape() functions
    let preprocessed = dealer_parser::preprocess(constraint_str);

    // Parse the program (may include variable assignments and action blocks)
    let program = match dealer_parser::parse_program(&preprocessed) {
        Ok(program) => program,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            std::process::exit(1);
        }
    };

    // Extract action block directives from the program
    let mut produce_count_from_input: Option<usize> = None;
    let mut generate_count_from_input: Option<usize> = None;
    let mut format_from_input: Option<OutputFormat> = None;
    let mut dealer_from_input: Option<DealerPosition> = None;
    let mut vuln_from_input: Option<VulnerabilityArg> = None;

    // Track average statements: (label, expression, sum, count)
    let mut averages: Vec<(Option<String>, Expr, f64, usize)> = Vec::new();

    // Track frequency statements: (label, expression, histogram, range)
    use std::collections::HashMap;
    #[allow(clippy::type_complexity)]
    let mut frequencies: Vec<(
        Option<String>,
        Expr,
        HashMap<i32, usize>,
        Option<(i32, i32)>,
    )> = Vec::new();

    // Track CSV report statements
    use dealer_parser::{CsvTerm, Side};
    let mut csv_reports: Vec<Vec<CsvTerm>> = Vec::new();

    for statement in &program.statements {
        match statement {
            Statement::Produce(n) => produce_count_from_input = Some(*n),
            Statement::Generate(n) => generate_count_from_input = Some(*n),
            Statement::Action {
                averages: avg_specs,
                frequencies: freq_specs,
                format: action_format,
            } => {
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
                // Extract averages if present
                for avg_spec in avg_specs {
                    averages.push((avg_spec.label.clone(), avg_spec.expr.clone(), 0.0, 0));
                }
                // Extract frequencies if present
                for freq_spec in freq_specs {
                    frequencies.push((
                        freq_spec.label.clone(),
                        freq_spec.expr.clone(),
                        HashMap::new(),
                        freq_spec.range,
                    ));
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
            Statement::CsvReport(terms) => {
                csv_reports.push(terms.clone());
            }
            _ => {}
        }
    }

    // Extract variables and constraint from program (do this once before the loop)
    // This avoids cloning expression trees on every iteration
    let program_variables = extract_variables(&program);
    let constraint = extract_constraint(&program);

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

    let output_format = args
        .format
        .or(format_from_input)
        .unwrap_or(OutputFormat::PrintAll); // Default format (matches dealer.exe)

    let dealer_position = args.dealer.or(dealer_from_input);

    let vulnerability = args.vulnerability.or(vuln_from_input);

    // Start timing
    let start_time = SystemTime::now();

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

    // Validate --input-deals conflicts
    if let Some(ref source) = args.input_deals {
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
        if args.legacy {
            eprintln!("Warning: --legacy is ignored when using --input-deals");
        }
    }

    let mut produced = 0;
    let mut generated: usize = 0;

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

    // Helper closure to process a matching deal (averages, frequencies, output, CSV)
    #[allow(clippy::type_complexity)]
    let process_matching_deal =
        |deal: &Deal,
         produced: usize,
         averages: &mut Vec<(Option<String>, Expr, f64, usize)>,
         frequencies: &mut Vec<(
            Option<String>,
            Expr,
            HashMap<i32, usize>,
            Option<(i32, i32)>,
        )>,
         csv_writer: &mut Option<BufWriter<std::fs::File>>| {
            // Calculate averages for this matching deal
            if !averages.is_empty() || !frequencies.is_empty() {
                let ctx = EvalContext::with_variables(deal, &program_variables);

                for (_, expr, sum, count) in averages.iter_mut() {
                    match eval(expr, &ctx) {
                        Ok(val) => {
                            *sum += val as f64;
                            *count += 1;
                        }
                        Err(e) => {
                            eprintln!("Average evaluation error: {}", e);
                            std::process::exit(1);
                        }
                    }
                }

                // Calculate frequencies for this matching deal
                for (_, expr, histogram, _) in frequencies.iter_mut() {
                    match eval(expr, &ctx) {
                        Ok(val) => {
                            *histogram.entry(val).or_insert(0) += 1;
                        }
                        Err(e) => {
                            eprintln!("Frequency evaluation error: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }

            // In quiet mode, don't print deals (only statistics)
            if !args.quiet {
                let output = match output_format {
                    OutputFormat::PrintAll => format_printall(deal, produced),
                    OutputFormat::PrintEW => format_printew(deal),
                    OutputFormat::PrintPBN => {
                        let dealer_pos = dealer_position.map(|d| d.into());
                        let vuln = vulnerability.map(|v| v.into());
                        let event_name = args.title.as_deref();
                        let input_file = args.input_file.as_deref();
                        format_printpbn(
                            deal,
                            produced,
                            dealer_pos,
                            vuln,
                            event_name,
                            Some(seed),
                            input_file,
                        )
                    }
                    OutputFormat::PrintCompact => format_printcompact(deal),
                    OutputFormat::PrintOneLine => format_oneline(deal),
                };
                print!("{}", output);
            }

            // Write CSV reports if any
            if !csv_reports.is_empty() && csv_writer.is_some() {
                let ctx = EvalContext::with_variables(deal, &program_variables);

                for csv_terms in &csv_reports {
                    let mut line_parts: Vec<String> = Vec::new();

                    for term in csv_terms {
                        match term {
                            CsvTerm::Expression(expr) => match eval(expr, &ctx) {
                                Ok(val) => line_parts.push(val.to_string()),
                                Err(e) => {
                                    eprintln!("CSV evaluation error: {}", e);
                                    std::process::exit(1);
                                }
                            },
                            CsvTerm::String(s) => {
                                line_parts.push(format!("'{}'", s));
                            }
                            CsvTerm::Compass(pos) => {
                                let hand = deal.hand(*pos);
                                line_parts.push(format_hand_pbn(hand));
                            }
                            CsvTerm::Side(side) => {
                                let (pos1, pos2) = match side {
                                    Side::NS => (Position::North, Position::South),
                                    Side::EW => (Position::East, Position::West),
                                };
                                let hand1 = deal.hand(pos1);
                                let hand2 = deal.hand(pos2);
                                line_parts.push(format!(
                                    "{} {}",
                                    format_hand_pbn(hand1),
                                    format_hand_pbn(hand2)
                                ));
                            }
                            CsvTerm::Deal => {
                                let n = deal.hand(Position::North);
                                let e = deal.hand(Position::East);
                                let s = deal.hand(Position::South);
                                let w = deal.hand(Position::West);
                                line_parts.push(format!(
                                    "{} {} {} {}",
                                    format_hand_pbn(n),
                                    format_hand_pbn(e),
                                    format_hand_pbn(s),
                                    format_hand_pbn(w)
                                ));
                            }
                        }
                    }

                    // Write line with space before first item, commas between items
                    if let Some(writer) = csv_writer.as_mut() {
                        writeln!(writer, " {}", line_parts.join(",")).unwrap_or_else(|e| {
                            eprintln!("CSV write error: {}", e);
                            std::process::exit(1);
                        });
                    }
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
                Some(expr) => eval_with_context(expr, &program_variables, &deal),
                None => Ok(1),
            };

            match eval_result {
                Ok(result) if result != 0 => {
                    process_matching_deal(
                        &deal,
                        produced,
                        &mut averages,
                        &mut frequencies,
                        &mut csv_writer,
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
    } else if args.legacy {
        // Legacy mode: single-threaded with gnurandom for exact dealer.exe compatibility
        // Initialize the legacy DealGenerator and apply predeal
        let mut generator = DealGenerator::new(seed);

        // Apply predeal to legacy generator
        for statement in &program.statements {
            if let Statement::Predeal { position, cards } = statement {
                if let Err(e) = generator.predeal(*position, cards) {
                    eprintln!("Predeal error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        // Command-line predeals were already validated with fast_predeal_config,
        // now apply them to the legacy generator
        if let Some(ref cards_str) = args.north_predeal {
            if let Ok(cards) = parse_predeal_cards(cards_str) {
                let _ = generator.predeal(Position::North, &cards);
            }
        }
        if let Some(ref cards_str) = args.east_predeal {
            if let Ok(cards) = parse_predeal_cards(cards_str) {
                let _ = generator.predeal(Position::East, &cards);
            }
        }
        if let Some(ref cards_str) = args.south_predeal {
            if let Ok(cards) = parse_predeal_cards(cards_str) {
                let _ = generator.predeal(Position::South, &cards);
            }
        }
        if let Some(ref cards_str) = args.west_predeal {
            if let Ok(cards) = parse_predeal_cards(cards_str) {
                let _ = generator.predeal(Position::West, &cards);
            }
        }

        while produced < produce_count && generated < max_generate {
            // Check timeout (check every 1000 deals to avoid excessive time calls)
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

            let deal = generator.generate();
            generated += 1;

            // Show progress meter if enabled (matches dealer.exe -m)
            if args.progress && generated - last_progress_report >= progress_interval {
                let elapsed = start_time.elapsed().unwrap().as_secs_f64();
                eprintln!(
                    "Generated: {} hands, Produced: {} hands, Time: {:.1}s",
                    generated, produced, elapsed
                );
                last_progress_report = generated;
            }

            // Evaluate constraint with pre-extracted variables (optimized hot path)
            let eval_result = match constraint {
                Some(expr) => eval_with_context(expr, &program_variables, &deal),
                None => Ok(1), // No constraint = always match
            };

            match eval_result {
                Ok(result) if result != 0 => {
                    // Constraint satisfied (non-zero = true)
                    process_matching_deal(
                        &deal,
                        produced,
                        &mut averages,
                        &mut frequencies,
                        &mut csv_writer,
                    );
                    produced += 1;
                }
                Ok(_) => {
                    // Constraint not satisfied (zero = false)
                    continue;
                }
                Err(e) => {
                    eprintln!("Evaluation error: {}", e);
                    std::process::exit(1);
                }
            }
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
        };

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
                        match eval_with_context(expr, &program_variables, deal) {
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
                        &mut averages,
                        &mut frequencies,
                        &mut csv_writer,
                    );
                    produced += 1;

                    // Stop counting generated deals once we've produced enough
                    // This matches dealer.exe behavior where it stops at the deal that
                    // satisfied the produce count
                    if produced >= produce_count {
                        break;
                    }
                }
            }
        }
    }

    // Calculate elapsed time
    let elapsed = start_time.elapsed().unwrap();
    let elapsed_secs = elapsed.as_secs_f64();

    // Print averages if any were requested (format matches dealer.exe %g format)
    // dealer.exe outputs averages to stdout without any prefix
    if !averages.is_empty() {
        for (label, _, sum, count) in &averages {
            let avg = if *count > 0 {
                sum / (*count as f64)
            } else {
                0.0
            };
            // Output using %g-style formatting to match dealer.exe
            // %g removes trailing zeros and uses shortest representation
            if let Some(label_text) = label {
                println!("{}: {}", label_text, format_g(avg));
            } else {
                println!("Average: {}", format_g(avg));
            }
        }
    }

    // Print frequency tables if any were requested (format matches dealer.exe)
    if !frequencies.is_empty() {
        for (label, _, histogram, range) in &frequencies {
            if let Some(label_text) = label {
                // dealer.exe format: "Frequency <label>:" - preserve label exactly as defined
                println!("Frequency {}:", label_text);
            } else {
                println!("Frequency :");
            }

            // Determine range to display
            let (min_val, max_val) = if let Some((min, max)) = range {
                (*min, *max)
            } else if !histogram.is_empty() {
                let min = *histogram.keys().min().unwrap();
                let max = *histogram.keys().max().unwrap();
                (min, max)
            } else {
                (0, 0)
            };

            // Print frequency table (format matches dealer.exe: "%5d\t%8ld")
            // dealer.exe prints "Low" and "High" rows for out-of-range values when a range is specified
            if range.is_some() {
                // Count values below the range
                let low_count: usize = histogram
                    .iter()
                    .filter(|(&k, _)| k < min_val)
                    .map(|(_, &v)| v)
                    .sum();
                if low_count > 0 {
                    println!("Low\t{:8}", low_count);
                }
            }

            for val in min_val..=max_val {
                let count = histogram.get(&val).unwrap_or(&0);
                println!("{:5}\t{:8}", val, count);
            }

            if range.is_some() {
                // Count values above the range
                let high_count: usize = histogram
                    .iter()
                    .filter(|(&k, _)| k > max_val)
                    .map(|(_, &v)| v)
                    .sum();
                if high_count > 0 {
                    println!("High\t{:8}", high_count);
                }
            }
        }
    }

    // Print stats if verbose_stats is true (matches dealer.exe behavior)
    // verbose_stats starts true and is toggled by PBN output
    // So: PBN with odd count = no stats, PBN with even count = stats, other formats = always stats
    if verbose_stats {
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
