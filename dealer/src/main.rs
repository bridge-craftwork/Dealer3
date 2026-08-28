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
use dealer_parser::{ActionType, Expr, Program, Statement, VulnerabilityType};
use dealer_pbn::{
    format_hand_pbn, format_oneline, format_printall, format_printcompact, format_printew,
    format_printpbn, PbnBoard, Vulnerability,
};
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
    /// result into the copy's `### BEGIN GENERATED LEVELING ###` block. `-p`
    /// sets how many deals to measure over. See `docs/leveling-strategy.md`.
    #[arg(long = "write-leveled", value_name = "FILE")]
    write_leveled: Option<PathBuf>,

    /// Target mix for `--write-leveled`: "even", or weights per hand type
    #[arg(long = "level-target", value_name = "MIX", default_value = "even")]
    level_target: String,

    /// Budget for `--write-leveled`, in deals dealt per deal kept
    #[arg(long = "level-budget", value_name = "N")]
    level_budget: Option<f64>,

    /// Order the output so each hand type appears before any repeats
    ///
    /// Needs `HandType_*` variables to classify against. Holds every produced
    /// deal until the end, since the order is not known until they all are.
    #[arg(long = "interleave")]
    interleave: bool,

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

/// Check a scenario can take a generated levelling block, before anything is
/// dealt. Returns the verdict variable's name and the scale of any `roll` it
/// already defines.
///
/// Up front rather than at the end: measuring is a hundred thousand deals, and
/// none of it is worth doing if the file cannot receive the answer.
fn check_leveling_source(source: &str) -> Result<(&'static str, Option<u32>), String> {
    if source.contains(LEVEL_STAMP) {
        return Err(
            "this scenario was generated by --write-leveled, not written by hand.\n       \
             Levelling it again would measure the already-levelled mix, compute keeps of \
             roughly 1,\n       and quietly write a scenario with no levelling at all. Use \
             the stock file."
                .to_string(),
        );
    }
    let (head, rest) = source.split_once(LEVEL_BEGIN).ok_or_else(|| {
        format!(
            "the scenario needs a placeholder for the generated levelling:\n    {}\n    \
             noLeveling = 1\n    levelTheDeal = noLeveling\n    {}",
            LEVEL_BEGIN, LEVEL_END
        )
    })?;
    let (_, tail) = rest
        .split_once(LEVEL_END)
        .ok_or_else(|| format!("the scenario has {} but no {}", LEVEL_BEGIN, LEVEL_END))?;

    // Whichever name the scenario already uses for the verdict.
    let verdict = ["levelTheDeal", "keepTheDeal"]
        .into_iter()
        .find(|name| tail.contains(name))
        .ok_or_else(|| {
            "nothing after the generated block uses `levelTheDeal`, so the levelling would \
             have no effect. Add it to the condition."
                .to_string()
        })?;
    let roll_scale = existing_roll(&format!("{}{}", head, tail))?;
    Ok((verdict, roll_scale))
}

/// Write `source` with its levelling block filled in.
///
/// Returns the human-readable summary. Everything it refuses is a way the
/// method goes quietly wrong: a generated file fed back in would measure the
/// already-levelled mix and compute keeps of roughly 1, writing a scenario with
/// no levelling at all and a stamp that agrees with itself.
#[allow(clippy::too_many_arguments)]
fn write_leveled(
    source: &str,
    path: &std::path::Path,
    plans: &[LevelPlan],
    lambda: f64,
    acceptance: f64,
    base_rate: f64,
    measured: usize,
    seed: u32,
) -> Result<String, String> {
    if source.contains(LEVEL_STAMP) {
        return Err(
            "this scenario was generated by --write-leveled, not written by hand.\n       \
             Levelling it again would measure the already-levelled mix, compute keeps of \
             roughly 1,\n       and quietly write a scenario with no levelling at all. Use \
             the stock file."
                .to_string(),
        );
    }
    let (head, rest) = source.split_once(LEVEL_BEGIN).ok_or_else(|| {
        format!(
            "the scenario needs a placeholder for the generated levelling:\n    {}\n    \
             noLeveling = 1\n    levelTheDeal = noLeveling\n    {}",
            LEVEL_BEGIN, LEVEL_END
        )
    })?;
    let (_, tail) = rest
        .split_once(LEVEL_END)
        .ok_or_else(|| format!("the scenario has {} but no {}", LEVEL_BEGIN, LEVEL_END))?;

    // Whichever name the scenario already uses for the verdict.
    let verdict = ["levelTheDeal", "keepTheDeal"]
        .into_iter()
        .find(|name| tail.contains(name))
        .ok_or_else(|| {
            "nothing after the generated block uses `levelTheDeal`, so the levelling would \
             have no effect. Add it to the condition."
                .to_string()
        })?;

    let outside = format!("{}{}", head, tail);
    let roll_scale = existing_roll(&outside)?;
    let scale = roll_scale.unwrap_or(1000);

    let width = plans.iter().map(|p| p.name.len()).max().unwrap_or(0).max(4);
    let mut block = vec![
        LEVEL_BEGIN.to_string(),
        format!("{}. Do not edit; edit the stock", LEVEL_STAMP),
        "# scenario and regenerate, or the two will disagree without saying so.".to_string(),
        "#".to_string(),
        format!("# measured over  {} deals, seed {}", measured, seed),
        "#".to_string(),
        format!(
            "# {:<width$}  {:>9} {:>9} {:>8} {:>9}",
            "type",
            "natural",
            "target",
            "keep",
            "seen",
            width = width
        ),
    ];
    for plan in plans {
        block.push(format!(
            "# {:<width$}  {:>9.5} {:>9.5} {:>8.4} {:>9}",
            plan.name,
            plan.natural,
            plan.target,
            plan.keep,
            plan.seen,
            width = width
        ));
    }
    let rarest = plans
        .iter()
        .min_by(|a, b| a.natural.total_cmp(&b.natural))
        .expect("at least one hand type");
    let relative = if rarest.seen > 0 {
        (rarest.natural * (1.0 - rarest.natural) / measured as f64).sqrt() / rarest.natural
    } else {
        f64::INFINITY
    };
    block.extend([
        "#".to_string(),
        format!(
            "# exactness      {:.3}{}",
            lambda,
            if lambda >= 0.999 {
                "  (full)"
            } else {
                "  (relaxed to fit the budget)"
            }
        ),
        format!("# acceptance     {:.4} of qualifying deals", acceptance),
        format!(
            "# cost           about {:.0} deals dealt per deal kept",
            1.0 / (base_rate * acceptance)
        ),
        format!(
            "# precision      keeps set by {}, the rarest, seen {} times (+-{:.1}%); expect \
             the mix within +-{:.2} points",
            rarest.name,
            rarest.seen,
            100.0 * relative,
            100.0 * rarest.mix * relative
        ),
        String::new(),
    ]);
    if roll_scale.is_some() {
        block.push(format!(
            "# `roll` comes from the scenario, drawing over 0..{}.",
            scale - 1
        ));
    } else {
        block.push(canonical_roll(scale));
    }
    block.push(String::new());
    for plan in plans {
        let keep = ((plan.keep * scale as f64).round() as u32).clamp(1, scale);
        if keep >= scale {
            block.push(format!("level_{0} = {1}_{0}", plan.name, HAND_TYPE_PREFIX));
        } else {
            block.push(format!(
                "level_{0} = {1}_{0} and roll < {2}",
                plan.name, HAND_TYPE_PREFIX, keep
            ));
        }
    }
    block.push(format!(
        "{} = {}",
        verdict,
        plans
            .iter()
            .map(|p| format!("level_{}", p.name))
            .collect::<Vec<_>>()
            .join(" or ")
    ));
    block.push(LEVEL_END.to_string());

    let generated = format!("{}{}{}", head, block.join("\n"), tail);
    let generated = fill_mix_markers(&generated, plans)?;
    std::fs::write(path, &generated)
        .map_err(|e| format!("could not write {}: {}", path.display(), e))?;

    let mut summary = format!(
        "measured over {} deals\n  {:<width$}  {:>9} {:>9} {:>9} {:>8} {:>9}\n",
        measured,
        "type",
        "natural",
        "target",
        "mix",
        "keep",
        "seen",
        width = width
    );
    for plan in plans {
        summary.push_str(&format!(
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
    summary.push_str(&format!(
        "\n  exactness {:.3}{}\n  acceptance {:.4} of qualifying deals\n  about {:.0} deals \
         dealt per deal kept\n  keeps pinned down by `{}`, the rarest, seen {} times: \
         +-{:.1}%\n\nwrote {}",
        lambda,
        if lambda >= 0.999 {
            ""
        } else {
            "  (relaxed to fit the budget)"
        },
        acceptance,
        1.0 / (base_rate * acceptance),
        rarest.name,
        rarest.seen,
        100.0 * relative,
        path.display()
    ));
    Ok(summary)
}

/// The markers a stock scenario leaves for the generated levelling.
const LEVEL_BEGIN: &str = "### BEGIN GENERATED LEVELING ###";
const LEVEL_END: &str = "### END GENERATED LEVELING ###";
/// Written into every generated file, so one fed back in is recognised.
const LEVEL_STAMP: &str = "# Generated by dealer --write-leveled";

/// Whether a scenario already defines `roll`, and over what range.
///
/// A scenario may get `roll` from an include, and writing a second definition
/// would be noise at best. So an existing one is used as it stands — but only
/// in the shape the keeps assume, a uniform draw over `0..scale-1` on every
/// build. `rnd(N)` on its own is not that: a locally built dealer returns
/// values outside the bound, or negative ones.
fn existing_roll(text: &str) -> Result<Option<u32>, String> {
    let assignments: Vec<&str> = text
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("roll") && trimmed[4..].trim_start().starts_with('=')
        })
        .collect();
    match assignments.len() {
        0 => return Ok(None),
        1 => {}
        n => {
            return Err(format!(
                "`roll` is assigned {} times outside the generated block. Leave exactly one.",
                n
            ))
        }
    }
    let line = assignments[0].trim();
    let numbers: Vec<u32> = line
        .split(|c: char| !c.is_ascii_digit())
        .filter(|piece| !piece.is_empty())
        .filter_map(|piece| piece.parse().ok())
        .collect();
    let canonical = numbers
        .first()
        .map(|n| canonical_roll(*n))
        .unwrap_or_default();
    if numbers.len() == 4 && numbers.iter().all(|n| n == &numbers[0]) && line == canonical {
        Ok(Some(numbers[0]))
    } else {
        Err(format!(
            "`roll` is already defined, but not in the form the keeps assume:\n    \
             found     {}\n    expected  roll = (rnd(N) % N + N) % N\n       The double \
             modulo is what makes the draw uniform on every build; without it a locally \
             built dealer returns values outside the bound, or negative ones.",
            line
        ))
    }
}

fn canonical_roll(scale: u32) -> String {
    format!("roll = (rnd({0}) % {0} + {0}) % {0}", scale)
}

/// Fill `{{level-mix:type}}` and `{{level-mix}}` from the mix that was chosen.
///
/// The player-facing text is written by hand and drifts: one scenario in the
/// corpus advertised 23% for a band delivering 19.3%. Filling it from the same
/// numbers as the keeps is the only way the two cannot disagree.
fn fill_mix_markers(text: &str, plans: &[LevelPlan]) -> Result<String, String> {
    let share = |plan: &LevelPlan| {
        let value = 100.0 * plan.mix;
        if (value - value.round()).abs() < 0.05 {
            format!("{:.0}%", value)
        } else {
            format!("{:.1}%", value)
        }
    };
    let width = plans.iter().map(|p| p.name.len()).max().unwrap_or(0);
    let all: String = plans
        .iter()
        .map(|p| format!("{:<width$}  {}", p.name, share(p), width = width))
        .collect::<Vec<_>>()
        .join("\n");

    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{level-mix") {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let end = tail
            .find("}}")
            .ok_or_else(|| "a `{{level-mix` marker is never closed".to_string())?;
        let inside = &tail[2..end];
        let named = inside.strip_prefix("level-mix:").map(str::trim);
        match named {
            None => out.push_str(&all),
            Some(name) => match plans.iter().find(|p| p.name == name) {
                Some(plan) => out.push_str(&share(plan)),
                None => {
                    return Err(format!(
                        "`{{{{level-mix:{}}}}}` names a hand type the scenario does not \
                         declare. Known: {}",
                        name,
                        plans
                            .iter()
                            .map(|p| p.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                }
            },
        }
        rest = &tail[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Fewest sightings of a type worth dividing by.
///
/// A keep is `mix / natural`, so a relative error in a measured rate passes
/// straight into the delivered mix. At 500 sightings that error is about 4.5%,
/// which is a point on a 20% target — a reasonable place for a hard stop.
const MIN_HAND_TYPE_SAMPLE: usize = 500;

/// Read `--level-target`: "even", or one weight per hand type.
fn parse_level_target(spec: &str, labels: &[String]) -> Result<Vec<f64>, String> {
    let weights: Vec<f64> = if spec.eq_ignore_ascii_case("even") {
        vec![1.0; labels.len()]
    } else {
        let parsed: Result<Vec<f64>, _> =
            spec.split(',').map(|w| w.trim().parse::<f64>()).collect();
        let parsed = parsed.map_err(|_| {
            format!(
                "--level-target `{}` is not `even` or a list of numbers",
                spec
            )
        })?;
        if parsed.len() != labels.len() {
            return Err(format!(
                "--level-target has {} weights for {} hand types",
                parsed.len(),
                labels.len()
            ));
        }
        if parsed.iter().any(|w| *w < 0.0) {
            return Err("--level-target weights cannot be negative".to_string());
        }
        parsed
    };
    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return Err("--level-target weights sum to zero".to_string());
    }
    Ok(weights.into_iter().map(|w| w / total).collect())
}

/// One hand type's share of the run, and what to do about it.
struct LevelPlan {
    name: String,
    natural: f64,
    target: f64,
    mix: f64,
    keep: f64,
    seen: usize,
}

/// Work out the keeps that would bring `natural` to `target`.
///
/// `keep = mix / natural`, scaled so the largest is 1 — keep the type that is
/// furthest from its target always, and every other in proportion. The cost
/// follows from the single worst ratio rather than from a sum:
///
///     acceptance = 1 / max(target_j / natural_j)
///
/// When that is dearer than the budget allows, exactness is relaxed rather than
/// the rarest type sacrificed: every type moves the same fraction `lambda` of
/// the way from nature toward its target. The cost is affine in `lambda`, so
/// the affordable fraction is closed-form and there is nothing to search for.
fn level_plan(
    names: &[String],
    natural: &[f64],
    target: &[f64],
    seen: &[usize],
    budget_acceptance: Option<f64>,
) -> (Vec<LevelPlan>, f64, f64) {
    let ratio_max = target
        .iter()
        .zip(natural)
        .map(|(t, n)| if *n > 0.0 { t / n } else { 0.0 })
        .fold(0.0f64, f64::max);

    let lambda = match budget_acceptance {
        Some(budget) if ratio_max > 1.0 => {
            (((1.0 / budget) - 1.0) / (ratio_max - 1.0)).clamp(0.0, 1.0)
        }
        _ => 1.0,
    };
    let acceptance = if ratio_max > 1.0 {
        1.0 / (1.0 + lambda * (ratio_max - 1.0))
    } else {
        1.0
    };

    let mix: Vec<f64> = natural
        .iter()
        .zip(target)
        .map(|(n, t)| (1.0 - lambda) * n + lambda * t)
        .collect();
    let scale = mix
        .iter()
        .zip(natural)
        .map(|(m, n)| if *n > 0.0 { m / n } else { 0.0 })
        .fold(0.0f64, f64::max);

    let plans = (0..names.len())
        .map(|i| LevelPlan {
            name: names[i].clone(),
            natural: natural[i],
            target: target[i],
            mix: mix[i],
            keep: if natural[i] > 0.0 && scale > 0.0 {
                (mix[i] / natural[i]) / scale
            } else {
                0.0
            },
            seen: seen[i],
        })
        .collect();
    (plans, lambda, acceptance)
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

/// Reorder produced deals so a practice set walks through the categories.
///
/// Naive round-robin empties the small buckets first, so the last rounds lose
/// types — exactly the lumpiness the ordering is meant to remove. Instead each
/// bucket's deals are spread evenly across the whole run: with shares of
/// 20/20/20/20/10/10, the four common types appear in every round and the two
/// rare ones in alternate rounds, so every round holds five deals rather than
/// six and then four.
///
/// The spreading is Bresenham's, one accumulator per bucket. Buckets of equal
/// size are given staggered starting credit, or two half-density types would
/// land in the same rounds and leave the others empty.
///
/// Deals carrying no type are dealt out last, in the order they were produced:
/// they belong to no round, and dropping them would lose deals the script
/// asked for.
fn interleave(order: &[&str], mut buckets: Vec<(Option<String>, Vec<usize>)>) -> Vec<usize> {
    // Declaration order, so the rounds read the way the script is written.
    buckets.sort_by_key(|(name, _)| match name {
        Some(n) => order.iter().position(|o| o == n).unwrap_or(usize::MAX),
        None => usize::MAX,
    });
    let untyped = match buckets.last() {
        Some((None, _)) => buckets.pop().map(|(_, deals)| deals).unwrap_or_default(),
        _ => Vec::new(),
    };
    if buckets.is_empty() {
        return untyped;
    }

    let rounds = buckets.iter().map(|(_, d)| d.len()).max().unwrap_or(0);
    // Stagger within groups of equal size: buckets of the same density
    // otherwise share a phase and pile into the same rounds.
    let mut credit = vec![0f64; buckets.len()];
    let mut seen: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let sizes: Vec<usize> = buckets.iter().map(|(_, d)| d.len()).collect();
    for (i, size) in sizes.iter().enumerate() {
        let group = sizes.iter().filter(|s| *s == size).count();
        let position = seen.entry(*size).or_insert(0);
        // Half a step of credit to start with, so a bucket holding one deal
        // lands in the middle of the run rather than at the very end.
        credit[i] = (*position as f64 + 0.5) / group as f64;
        *position += 1;
    }

    let mut taken = vec![0usize; buckets.len()];
    let mut out = Vec::new();
    for _ in 0..rounds {
        for (i, (_, deals)) in buckets.iter().enumerate() {
            if deals.is_empty() {
                continue;
            }
            credit[i] += deals.len() as f64 / rounds as f64;
            if credit[i] >= 1.0 - 1e-9 && taken[i] < deals.len() {
                credit[i] -= 1.0;
                out.push(deals[taken[i]]);
                taken[i] += 1;
            }
        }
    }
    // Rounding can leave a deal or two behind; they follow in order.
    for (i, (_, deals)) in buckets.iter().enumerate() {
        out.extend(deals[taken[i]..].iter().copied());
    }
    out.extend(untyped);
    out
}

/// The prefix that marks a variable as naming a category of hand.
///
/// A convention rather than syntax, so a script using it still parses
/// everywhere the original does — which matters, because these scenarios run on
/// BBO. The cost is that a misspelled name is silently not a category, so the
/// count found is always reported.
const HAND_TYPE_PREFIX: &str = "HandType";

/// The script's hand-type variables, in the order it declares them.
///
/// Declaration order, not alphabetical: it is the order the author thought in,
/// and it is what an interleaved practice set walks through.
fn hand_types(program: &Program) -> Vec<&str> {
    let mut found = Vec::new();
    for statement in &program.statements {
        if let Statement::Assignment { name, .. } = statement {
            if name.starts_with(HAND_TYPE_PREFIX) && !found.contains(&name.as_str()) {
                found.push(name.as_str());
            }
        }
    }
    found
}

/// What goes in the `[HandType "..."]` tag: the name without its prefix, and
/// without the separator someone will have written after it.
fn hand_type_label(name: &str) -> &str {
    name.trim_start_matches(HAND_TYPE_PREFIX)
        .trim_start_matches(['_', '-'])
}

/// Name a set of seats for a message: "East and West", "East, South and West".
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

    // Refuse a scenario that cannot receive a levelling block before dealing
    // anything: measuring is a hundred thousand deals, and none of it is worth
    // doing if the answer has nowhere to go.
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
        if let Err(message) = check_leveling_source(constraint_str) {
            eprintln!("Error: {}", message);
            std::process::exit(1);
        }
    }

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
    use dealer_parser::{CsvTerm, EsTerm, Side};
    let mut csv_reports: Vec<Vec<CsvTerm>> = Vec::new();
    // `printes(...)` lists, printed per matching deal in the order written.
    let mut printes_reports: Vec<Vec<EsTerm>> = Vec::new();
    // Seats named by `print(...)`, whose hands are laid out once at the end.
    let mut print_hand_seats: Vec<Position> = Vec::new();

    for statement in &program.statements {
        match statement {
            Statement::Produce(n) => produce_count_from_input = Some(*n),
            Statement::Generate(n) => generate_count_from_input = Some(*n),
            Statement::Action {
                averages: avg_specs,
                frequencies: freq_specs,
                format: action_format,
                printes: printes_specs,
                print_hands,
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

    let output_format = args
        .format
        .or(format_from_input)
        .unwrap_or(OutputFormat::PrintAll); // Default format (matches dealer.exe)

    let dealer_position = args.dealer.or(dealer_from_input);

    let vulnerability = args.vulnerability.or(vuln_from_input);

    // Start timing
    let start_time = SystemTime::now();

    dealer_eval::rnd::set_seed(args.rnd_seed);

    // Categories of hand the script names, for the PBN tag and for ordering a
    // practice set. Empty for almost every script, and then nothing changes.
    let hand_type_names = hand_types(&program);

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

    // How often each hand type came up, for `--level-plan`.
    let mut hand_type_counts: HashMap<String, usize> = hand_type_names
        .iter()
        .map(|n| (hand_type_label(n).to_string(), 0usize))
        .collect();

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
    // `held` and `hand_type_counts` are captured rather than passed: the
    // parameter list was already at the edge of readable, and unlike the others
    // nothing touches these two until the run is over.
    let mut process_matching_deal =
        |deal: &Deal,
         produced: usize,
         averages: &mut Vec<(Option<String>, Expr, f64, usize)>,
         frequencies: &mut Vec<(
            Option<String>,
            Expr,
            HashMap<i32, usize>,
            Option<(i32, i32)>,
        )>,
         csv_writer: &mut Option<BufWriter<std::fs::File>>,
         printed_deals: &mut Vec<Deal>| {
            if !print_hand_seats.is_empty() {
                printed_deals.push(deal.clone());
            }
            // Calculate averages for this matching deal
            if !averages.is_empty() || !frequencies.is_empty() {
                let ctx = EvalContext::with_counts(deal, &program_variables, point_counts);

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

            // Which category of hand this is, when the script names any. Two
            // matching is refused: the categories are meant to partition the deals,
            // and a tag that silently picked the first would leave a practice set
            // quietly wrong about what it contains.
            let hand_type = if hand_type_names.is_empty() {
                None
            } else {
                let ctx = EvalContext::with_counts(deal, &program_variables, point_counts);
                let mut matched: Option<&str> = None;
                for name in &hand_type_names {
                    match eval(&Expr::Variable(name.to_string()), &ctx) {
                        Ok(0) => {}
                        Ok(_) => match matched {
                            None => matched = Some(name),
                            Some(first) => {
                                eprintln!(
                                    "Error: a deal is both `{}` and `{}`. Hand types have to \
                                 partition the deals, so at most one may match.",
                                    first, name
                                );
                                std::process::exit(1);
                            }
                        },
                        Err(e) => {
                            eprintln!("Hand type `{}` could not be evaluated: {}", name, e);
                            std::process::exit(1);
                        }
                    }
                }
                matched.map(hand_type_label)
            };
            if let Some(label) = hand_type {
                *hand_type_counts.entry(label.to_string()).or_insert(0) += 1;
            }

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
                            args.title.as_deref(),
                            seed,
                            args.input_file.as_deref(),
                        )
                    );
                }
            }

            // Write CSV reports if any
            if !csv_reports.is_empty() && csv_writer.is_some() {
                let ctx = EvalContext::with_counts(deal, &program_variables, point_counts);

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
                        &mut averages,
                        &mut frequencies,
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
                        &mut averages,
                        &mut frequencies,
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
                }
            }
        }
    }

    // Calculate elapsed time
    let elapsed = start_time.elapsed().unwrap();
    let elapsed_secs = elapsed.as_secs_f64();

    // `--level-plan`: report what would level the hand types, and stop. The
    // run above did the measuring — every produced deal was classified — so
    // this is arithmetic on counts already gathered.
    if let Some(ref leveled_path) = args.write_leveled {
        if hand_type_names.is_empty() {
            eprintln!(
                "Error: --level-plan needs hand types to level. Name them with variables \
                 beginning `{}`.",
                HAND_TYPE_PREFIX
            );
            std::process::exit(1);
        }
        if produced == 0 {
            eprintln!("Error: nothing was produced, so there is nothing to measure.");
            std::process::exit(1);
        }

        let labels: Vec<String> = hand_type_names
            .iter()
            .map(|n| hand_type_label(n).to_string())
            .collect();
        let seen: Vec<usize> = labels.iter().map(|l| hand_type_counts[l]).collect();
        let classified: usize = seen.iter().sum();
        if classified != produced {
            eprintln!(
                "Error: {} of {} deals matched no hand type. They have to partition what \
                 the scenario produces, or the keeps will not add up.",
                produced - classified,
                produced
            );
            std::process::exit(1);
        }
        let thin: Vec<String> = labels
            .iter()
            .zip(&seen)
            .filter(|(_, n)| **n < MIN_HAND_TYPE_SAMPLE)
            .map(|(l, n)| format!("{} seen {} times", l, n))
            .collect();
        if !thin.is_empty() {
            eprintln!(
                "Error: measured on too few deals to divide by: {}.\n       Raise -p above \
                 {}, or widen the rare types.",
                thin.join("; "),
                produced
            );
            std::process::exit(1);
        }

        let natural: Vec<f64> = seen.iter().map(|n| *n as f64 / produced as f64).collect();
        let target = match parse_level_target(&args.level_target, &labels) {
            Ok(t) => t,
            Err(message) => {
                eprintln!("Error: {}", message);
                std::process::exit(1);
            }
        };
        let base_rate = produced as f64 / generated as f64;
        let budget_acceptance = args
            .level_budget
            .map(|budget| ((1.0 / budget) / base_rate).min(1.0));
        let (plans, lambda, acceptance) =
            level_plan(&labels, &natural, &target, &seen, budget_acceptance);

        let source = match args.input_file.as_ref().map(std::fs::read_to_string) {
            Some(Ok(text)) => text,
            Some(Err(e)) => {
                eprintln!("Error: could not re-read the scenario: {}", e);
                std::process::exit(1);
            }
            None => {
                eprintln!(
                    "Error: --write-leveled needs the scenario as a file argument, since it \
                     writes a copy of it."
                );
                std::process::exit(1);
            }
        };
        match write_leveled(
            &source,
            leveled_path,
            &plans,
            lambda,
            acceptance,
            base_rate,
            produced,
            seed,
        ) {
            Ok(summary) => eprintln!("{}", summary),
            Err(message) => {
                eprintln!("Error: {}", message);
                std::process::exit(1);
            }
        }
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
        for (position, index) in interleave(&labels, buckets).into_iter().enumerate() {
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
                    args.title.as_deref(),
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
        out.push_str("  \"hand_types\": [");
        out.push_str(
            &hand_type_names
                .iter()
                .map(|n| json_string(hand_type_label(n)))
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push_str("],\n");

        out.push_str("  \"averages\": [");
        for (i, (label, _, sum, count)) in averages.iter().enumerate() {
            let value = if *count > 0 {
                sum / (*count as f64)
            } else {
                0.0
            };
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            out.push_str(&format!(
                "    {{ \"label\": {}, \"value\": {}, \"count\": {} }}",
                match label {
                    Some(text) => json_string(text),
                    None => "null".to_string(),
                },
                json_number(value),
                count
            ));
        }
        out.push_str(if averages.is_empty() {
            "],\n"
        } else {
            "\n  ],\n"
        });

        out.push_str("  \"frequencies\": [");
        for (i, (label, _, histogram, range)) in frequencies.iter().enumerate() {
            let (min_val, max_val) = match range {
                Some((min, max)) => (*min, *max),
                None if !histogram.is_empty() => (
                    *histogram.keys().min().unwrap_or(&0),
                    *histogram.keys().max().unwrap_or(&0),
                ),
                None => (0, 0),
            };
            let below: usize = histogram
                .iter()
                .filter(|(&k, _)| k < min_val)
                .map(|(_, &v)| v)
                .sum();
            let above: usize = histogram
                .iter()
                .filter(|(&k, _)| k > max_val)
                .map(|(_, &v)| v)
                .sum();
            let bins: Vec<String> = (min_val..=max_val)
                .map(|v| {
                    format!(
                        "{{ \"value\": {}, \"count\": {} }}",
                        v,
                        histogram.get(&v).unwrap_or(&0)
                    )
                })
                .collect();
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            out.push_str(&format!(
                "    {{ \"label\": {}, \"min\": {}, \"max\": {}, \"below\": {}, \"above\": {}, \"total\": {}, \"bins\": [{}] }}",
                match label {
                    Some(text) => json_string(text),
                    None => "null".to_string(),
                },
                match range {
                    Some((min, _)) => min.to_string(),
                    None => "null".to_string(),
                },
                match range {
                    Some((_, max)) => max.to_string(),
                    None => "null".to_string(),
                },
                below,
                above,
                histogram.values().sum::<usize>(),
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

#[cfg(test)]
mod interleave_tests {
    use super::interleave;

    fn buckets(sizes: &[(&str, usize)]) -> Vec<(Option<String>, Vec<usize>)> {
        let mut next = 0;
        let mut out = Vec::new();
        for (name, n) in sizes {
            out.push((Some((*name).to_string()), (next..next + n).collect()));
            next += n;
        }
        out
    }

    /// Which bucket each position in the result came from.
    fn shape(order: &[&str], sizes: &[(&str, usize)]) -> Vec<usize> {
        let b = buckets(sizes);
        let total: usize = sizes.iter().map(|(_, n)| n).sum();
        let mut owner = vec![0usize; total];
        for (i, (_, deals)) in b.iter().enumerate() {
            for d in deals {
                owner[*d] = i;
            }
        }
        interleave(order, b).into_iter().map(|d| owner[d]).collect()
    }

    #[test]
    fn equal_buckets_come_out_one_of_each_per_round() {
        let got = shape(&["a", "b", "c"], &[("a", 3), ("b", 3), ("c", 3)]);
        assert_eq!(got.len(), 9);
        for round in got.chunks(3) {
            let mut seen = round.to_vec();
            seen.sort_unstable();
            assert_eq!(seen, vec![0, 1, 2], "every round should hold one of each");
        }
    }

    /// The case naive round-robin gets wrong: two half-size buckets have to
    /// land in alternate rounds, not both in the first half, or the last
    /// rounds lose types — the very lumpiness the ordering removes.
    #[test]
    fn half_size_buckets_alternate_instead_of_running_out() {
        let got = shape(
            &["a", "b", "c", "d"],
            &[("a", 4), ("b", 4), ("c", 2), ("d", 2)],
        );
        assert_eq!(got.len(), 12);
        for (i, round) in got.chunks(3).enumerate() {
            assert!(round.contains(&0), "round {i} lost `a`: {round:?}");
            assert!(round.contains(&1), "round {i} lost `b`: {round:?}");
            assert!(
                round.contains(&2) ^ round.contains(&3),
                "round {i} should hold exactly one rare type: {round:?}"
            );
        }
    }

    #[test]
    fn every_deal_comes_out_exactly_once() {
        for sizes in [
            vec![("a", 7), ("b", 3), ("c", 1)],
            vec![("a", 1), ("b", 1)],
            vec![("a", 10)],
            vec![("a", 5), ("b", 5), ("c", 5), ("d", 2), ("e", 1)],
        ] {
            let order: Vec<&str> = sizes.iter().map(|(n, _)| *n).collect();
            let total: usize = sizes.iter().map(|(_, n)| n).sum();
            let mut got = interleave(&order, buckets(&sizes));
            assert_eq!(got.len(), total, "for {sizes:?}");
            got.sort_unstable();
            assert_eq!(got, (0..total).collect::<Vec<_>>(), "for {sizes:?}");
        }
    }

    /// Deals the script did not classify still have to appear, or a run would
    /// silently produce fewer boards than it reports.
    #[test]
    fn untyped_deals_follow_at_the_end() {
        let b = vec![
            (Some("a".to_string()), vec![0, 1]),
            (None, vec![2, 3]),
            (Some("b".to_string()), vec![4, 5]),
        ];
        let got = interleave(&["a", "b"], b);
        assert_eq!(got.len(), 6);
        assert_eq!(&got[4..], &[2, 3], "untyped deals come last, in order");
    }

    #[test]
    fn the_rounds_follow_declaration_order() {
        let b = vec![
            (Some("second".to_string()), vec![10]),
            (Some("first".to_string()), vec![20]),
        ];
        assert_eq!(interleave(&["first", "second"], b), vec![20, 10]);
    }
}
