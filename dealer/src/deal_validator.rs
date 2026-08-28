//! deal-validator: Validate bridge deals against filter conditions
//!
//! This tool reads deals from stdin (in oneline format) and validates them
//! against a filter file. It's used to verify that the fast parallel dealer
//! produces deals that satisfy the given constraints.
//!
//! # Usage
//!
//! ```bash
//! # Validate deals from dealer3 against a filter
//! echo "hcp(north) >= 20" | dealer -p 100 -s 42 -f oneline | deal-validator filter.dlr
//!
//! # Or with stdin for filter
//! dealer -p 100 -s 42 -f oneline < test.dlr | deal-validator filter.dlr
//! ```
//!
//! # Exit Codes
//!
//! - 0: All deals pass the filter
//! - 1: One or more deals failed the filter
//! - 2: Error (parse error, file not found, etc.)

use clap::Parser;
use dealer_core::{Deal, Position};
use dealer_eval::{eval_with_context, extract_constraint, extract_variables};
use dealer_pbn::{parse_deal_tag, parse_oneline};
use std::io::{self, BufRead, Write};
use std::time::Instant;

#[derive(Parser)]
#[command(name = "deal-validator")]
#[command(about = "Validate bridge deals against filter conditions")]
#[command(
    long_about = "Reads deals from stdin (oneline format) and validates against a filter file.\n\n\
    Exit codes:\n  \
    0 = All deals pass\n  \
    1 = One or more deals failed\n  \
    2 = Error"
)]
struct Args {
    /// Filter file containing the constraint to validate against
    #[arg(value_name = "FILTER_FILE")]
    filter_file: String,

    /// Show each deal's pass/fail status (verbose output)
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Show deals that fail the filter
    #[arg(short = 'f', long = "show-failures")]
    show_failures: bool,

    /// Continue processing even after failures (default: stop on first failure)
    #[arg(short = 'c', long = "continue")]
    continue_on_failure: bool,

    /// Quiet mode - only output final summary
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

/// Try to parse a line as a deal in any supported format
fn try_parse_deal(line: &str) -> Option<Deal> {
    // Try oneline format first (most common for piped output)
    // Format: "n CARDS e CARDS s CARDS w CARDS"
    if let Ok(deal) = parse_oneline(line) {
        return Some(deal);
    }

    // Try PBN [Deal "..."] format
    // Format: [Deal "N:Spades.Hearts.Diamonds.Clubs ...]"]
    if line.starts_with("[Deal ") {
        if let Ok(pbn_deal) = parse_deal_tag(line) {
            return Some(pbn_deal.deal);
        }
    }

    None
}

fn main() {
    let args = Args::parse();

    // Read and parse filter file
    let filter_content = match std::fs::read_to_string(&args.filter_file) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading filter file '{}': {}", args.filter_file, e);
            std::process::exit(2);
        }
    };

    // Preprocess and parse the filter
    let preprocessed = match dealer_parser::preprocess_all(&filter_content) {
        Ok(text) => text,
        Err(message) => {
            eprintln!("Error in filter file: {}", message);
            std::process::exit(2);
        }
    };
    let program = match dealer_parser::parse_program(&preprocessed) {
        Ok(program) => program,
        Err(e) => {
            eprintln!("Parse error in filter file: {}", e);
            std::process::exit(2);
        }
    };

    // Extract variables and constraint
    let variables = extract_variables(&program);
    let constraint = extract_constraint(&program);

    if constraint.is_none() && !args.quiet {
        eprintln!("Warning: No condition found in filter file - all deals will pass");
    }

    let start_time = Instant::now();
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();

    let mut total_deals = 0usize;
    let mut passed_deals = 0usize;
    let mut failed_deals = 0usize;
    let mut parse_errors = 0usize;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Error reading stdin: {}", e);
                std::process::exit(2);
            }
        };

        let line = line.trim();

        // Skip empty lines and lines that don't look like deals
        if line.is_empty() {
            continue;
        }

        // Skip lines that are statistics output (Generated, Produced, etc.)
        if line.starts_with("Generated")
            || line.starts_with("Produced")
            || line.starts_with("Initial")
            || line.starts_with("Time")
        {
            continue;
        }

        // Try to parse as a deal (supports multiple formats)
        let deal = match try_parse_deal(line) {
            Some(d) => d,
            None => {
                // Not a valid deal line - skip silently
                // This handles any other non-deal output
                parse_errors += 1;
                continue;
            }
        };

        total_deals += 1;

        // Evaluate constraint
        let passes = match constraint {
            Some(expr) => match eval_with_context(expr, &variables, &deal) {
                Ok(result) => result != 0,
                Err(e) => {
                    eprintln!("Evaluation error on deal {}: {}", total_deals, e);
                    std::process::exit(2);
                }
            },
            None => true, // No constraint = always pass
        };

        if passes {
            passed_deals += 1;
            if args.verbose {
                writeln!(stdout, "PASS: {}", line).unwrap();
            }
        } else {
            failed_deals += 1;
            if args.show_failures || args.verbose {
                writeln!(stdout, "FAIL: {}", line).unwrap();
            }
            if !args.continue_on_failure {
                if !args.quiet {
                    eprintln!();
                    eprintln!("First failing deal (#{}):", total_deals);
                    eprintln!("  {}", line);
                    print_deal_details(&deal);
                }
                std::process::exit(1);
            }
        }
    }

    let elapsed = start_time.elapsed();

    // Print summary
    if !args.quiet {
        eprintln!();
        eprintln!("=== Validation Summary ===");
        eprintln!("Filter file: {}", args.filter_file);
        eprintln!("Total deals: {}", total_deals);
        eprintln!(
            "Passed:      {} ({:.1}%)",
            passed_deals,
            if total_deals > 0 {
                100.0 * passed_deals as f64 / total_deals as f64
            } else {
                0.0
            }
        );
        eprintln!(
            "Failed:      {} ({:.1}%)",
            failed_deals,
            if total_deals > 0 {
                100.0 * failed_deals as f64 / total_deals as f64
            } else {
                0.0
            }
        );
        if parse_errors > 0 {
            eprintln!("Skipped:     {} (non-deal lines)", parse_errors);
        }
        eprintln!("Time:        {:.3}s", elapsed.as_secs_f64());
    }

    if failed_deals > 0 {
        if !args.quiet {
            eprintln!();
            eprintln!(
                "❌ VALIDATION FAILED: {} deals did not match filter",
                failed_deals
            );
        }
        std::process::exit(1);
    } else if total_deals > 0 {
        if !args.quiet {
            eprintln!();
            eprintln!(
                "✅ VALIDATION PASSED: All {} deals match filter",
                total_deals
            );
        }
        std::process::exit(0);
    } else {
        if !args.quiet {
            eprintln!();
            eprintln!("⚠️  No deals to validate");
        }
        std::process::exit(0);
    }
}

/// Print details about a deal for debugging
fn print_deal_details(deal: &Deal) {
    eprintln!();
    eprintln!(
        "  North: {} cards, {} HCP",
        deal.hand(Position::North).len(),
        deal.hand(Position::North).hcp()
    );
    eprintln!(
        "  East:  {} cards, {} HCP",
        deal.hand(Position::East).len(),
        deal.hand(Position::East).hcp()
    );
    eprintln!(
        "  South: {} cards, {} HCP",
        deal.hand(Position::South).len(),
        deal.hand(Position::South).hcp()
    );
    eprintln!(
        "  West:  {} cards, {} HCP",
        deal.hand(Position::West).len(),
        deal.hand(Position::West).hcp()
    );
}
