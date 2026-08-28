//! Command-line switch compatibility, and the table in the docs.
//!
//! The three-way comparison in `docs/command_line_comparison.md` used to be
//! maintained by hand, and drifted: it listed `-R` as unimplemented after it
//! shipped, never gained `--input-deals`, `--timeout`, `--stats-on` or
//! `--batch-size`, and still said "v0.2.0, 18 switches" at 24.
//!
//! So the dealer3 column is no longer written down at all — it is derived from
//! clap, which is the thing that actually parses the command line. What *is*
//! written down is the part no program here can answer: what the original
//! dealer and DealerV2_4 do. `tests::every_switch_is_in_the_table` fails the
//! build when a switch is added to `Args` without a row, and
//! `tests::docs_are_up_to_date` fails when the generated table and the file on
//! disk disagree.
//!
//! Regenerate with:
//!
//! ```text
//! UPDATE_DOCS=1 cargo test -p dealer switches
//! ```
//!
//! # Sources
//!
//! dealer.exe's switches are its own getopt string, read from
//! `Dealer-cleanup/dealer.c`:
//!
//! ```text
//! getopt (argc, argv, "023ehuvmqXp:g:s:l:V")
//! ```
//!
//! Note `-X` is in there but missing from the program's own usage line, so the
//! usage line is not a reliable source.
//!
//! DealerV2_4's switches come from `docs/dealer_vs_dealer2_switches.md`, which
//! was compiled from its manual. They have not been re-verified against a V2_4
//! build, so treat that column as the weaker of the two.

/// Whether one of the other implementations has a switch, and what it means
/// there.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Present, and means the same thing it means in dealer3.
    Same,
    /// Present, but means something else. The text says what.
    Differs(&'static str),
    /// Not a switch in that implementation.
    Absent,
}

impl Origin {
    fn cell(self) -> String {
        match self {
            Origin::Same => "✅".to_string(),
            Origin::Differs(what) => format!("⚠️ {}", what),
            Origin::Absent => "—".to_string(),
        }
    }
}

/// One row of the comparison. dealer3's own column is deliberately absent: it
/// is derived from clap when the table is rendered.
pub struct SwitchRow {
    /// Short form as written, e.g. `-p`. Empty for long-only switches.
    pub short: &'static str,
    /// Long form as written, e.g. `--produce`. Empty for short-only switches.
    pub long: &'static str,
    /// Heading to file this under.
    pub group: &'static str,
    /// What the switch does, in one phrase.
    pub what: &'static str,
    pub dealer_exe: Origin,
    pub dealer_v2: Origin,
    pub note: Option<&'static str>,
}

/// Section order in the rendered table.
pub const GROUPS: &[&str] = &[
    "Generation",
    "Output",
    "Predeal",
    "Reporting",
    "Performance",
    "Reading deals in",
    "Recognised but not supported",
    "Not implemented",
];

/// Every switch any of the three implementations has.
///
/// A row whose short and long forms are both unknown to clap renders as a gap
/// in the dealer3 column — that is how the "Not implemented" group works, and
/// why nothing has to be remembered when one of them is implemented.
pub const SWITCH_ROWS: &[SwitchRow] = &[
    // ---- Generation -------------------------------------------------------
    SwitchRow {
        short: "-p",
        long: "--produce",
        group: "Generation",
        what: "Stop after N deals have matched",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Same,
        note: Some("Default 40, as in the original."),
    },
    SwitchRow {
        short: "-g",
        long: "--generate",
        group: "Generation",
        what: "Stop after dealing N hands",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Same,
        note: Some("Default 10,000,000. Whichever limit is reached first ends the run."),
    },
    SwitchRow {
        short: "-s",
        long: "--seed",
        group: "Generation",
        what: "Random seed",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Same,
        note: Some(
            "Default is the clock. dealer3 has its own RNG (xoshiro256++), so a seed does not \
             reproduce dealer.exe's deals — that went with legacy mode.",
        ),
    },
    SwitchRow {
        short: "-t",
        long: "--timeout",
        group: "Generation",
        what: "Give up after N seconds",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: None,
    },
    SwitchRow {
        short: "",
        long: "--write-leveled",
        group: "Generation",
        what: "Write a copy of the script with its hand types levelled",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: Some(
            "Measures how often each `HandType_*` variable comes up, works out the keep rate \
             for each, and writes them into the copy. `-p` sets the sample. See \
             `docs/leveling-guide.md`.",
        ),
    },
    SwitchRow {
        short: "",
        long: "--level-target",
        group: "Generation",
        what: "Target mix for `--write-leveled`: even, or one weight per hand type",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: None,
    },
    SwitchRow {
        short: "",
        long: "--level-budget",
        group: "Generation",
        what: "Budget for `--write-leveled`, in deals dealt per deal kept",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: Some(
            "When a target costs more than this, exactness is relaxed rather than the rarest \
             type sacrificed: every type moves the same fraction toward its target.",
        ),
    },
    SwitchRow {
        short: "",
        long: "--param",
        group: "Generation",
        what: "Fill a script parameter: `--param 1=west` puts `west` where `$1` stands",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Differs("-0 to -9 set $0 to $9"),
        note: Some(
            "DealerV2_4's spelling collides with dealer.exe's swapping switches, which win, \
             so only the switch differs — a script written for it is unchanged. A parameter \
             is source rather than a value: a compass, a number, a shape, even a function \
             name, so `$9($0)` with `--param 9=hcp --param 0=west` is `hcp(west)`. Unlike \
             DealerV2_4, a `$n` nothing supplies is an error rather than an empty space.",
        ),
    },
    SwitchRow {
        short: "",
        long: "--interleave",
        group: "Output",
        what: "Order the output so each hand type appears before any repeats",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: Some(
            "Needs `HandType_*` variables to classify against. Rare types are spread across \
             the run rather than exhausted early. See `docs/leveling-guide.md`.",
        ),
    },
    SwitchRow {
        short: "",
        long: "--stats-json",
        group: "Reporting",
        what: "Report the statistics as JSON instead of tables",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: Some(
            "For a tool rather than a reader: full precision, and the sample size behind each \
             average. Pair with `-q` for a stdout that is nothing but JSON. See \
             `docs/leveling-guide.md`.",
        ),
    },
    SwitchRow {
        short: "",
        long: "--rnd-seed",
        group: "Generation",
        what: "Shift the stream `rnd()` draws from",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: Some(
            "`rnd()` is reproducible without it. The original draws from the generator it \
             shuffles with, so calling it there changes the deals; dealer3 keeps the two apart.",
        ),
    },
    SwitchRow {
        short: "-0",
        long: "",
        group: "Generation",
        what: "No swapping (the default)",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Differs("-x MODE"),
        note: Some("The three swapping switches override one another, so the last one wins."),
    },
    SwitchRow {
        short: "-2",
        long: "",
        group: "Generation",
        what: "Two-way swapping: deal each shuffle again with East and West exchanged",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Differs("-x MODE"),
        note: Some(
            "Refused alongside a predeal to East or West, which it would move. The original \
             allows it and loses the predealt cards without saying so.",
        ),
    },
    SwitchRow {
        short: "-3",
        long: "",
        group: "Generation",
        what: "Three-way swapping: six deals a shuffle, rotating East, South and West",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Differs("-x MODE"),
        note: Some(
            "`predeal north` still works, since North never moves — a fixed hand against six \
             defensive layouts is what the switch is for.",
        ),
    },
    // ---- Output -----------------------------------------------------------
    SwitchRow {
        short: "-f",
        long: "--format",
        group: "Output",
        what: "Output format",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: Some("The original selects a format with an `action` statement instead."),
    },
    SwitchRow {
        short: "-d",
        long: "--dealer",
        group: "Output",
        what: "Dealer position",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: Some("The original uses the `dealer` statement, which dealer3 also accepts."),
    },
    SwitchRow {
        short: "",
        long: "--vulnerable",
        group: "Output",
        what: "Vulnerability",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Differs("-P sets vulnerability for par"),
        note: Some(
            "Long form only. `-v` is verbose, as in the original — this was the 0.2.0 breaking \
             change.",
        ),
    },
    SwitchRow {
        short: "-v",
        long: "--verbose",
        group: "Output",
        what: "Toggle the closing statistics",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Same,
        note: None,
    },
    SwitchRow {
        short: "-X",
        long: "--stats-on",
        group: "Output",
        what: "Force statistics on, past any -v",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Differs("-X exports predeal holdings"),
        note: Some(
            "In dealer.exe's getopt string but not its usage line. DealerV2_4 reuses the letter \
             for something else entirely.",
        ),
    },
    SwitchRow {
        short: "-q",
        long: "--quiet",
        group: "Output",
        what: "Suppress the deals, keep the statistics",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Same,
        note: None,
    },
    SwitchRow {
        short: "-m",
        long: "--progress",
        group: "Output",
        what: "Progress meter",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Same,
        note: Some("Every 10,000 deals."),
    },
    SwitchRow {
        short: "-V",
        long: "--version",
        group: "Output",
        what: "Print the version and exit",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Same,
        note: None,
    },
    SwitchRow {
        short: "",
        long: "--license",
        group: "Output",
        what: "Print the licence and exit",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: None,
    },
    SwitchRow {
        short: "",
        long: "--credits",
        group: "Output",
        what: "Print credits and exit",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: None,
    },
    SwitchRow {
        short: "-h",
        long: "--help",
        group: "Output",
        what: "Print help and exit",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Same,
        note: None,
    },
    // ---- Predeal ----------------------------------------------------------
    SwitchRow {
        short: "-N",
        long: "--north",
        group: "Predeal",
        what: "Predeal cards to North",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: Some("Format `S8743,HA9,D642,CQT64`, as DealerV2_4 writes it."),
    },
    SwitchRow {
        short: "-E",
        long: "--east",
        group: "Predeal",
        what: "Predeal cards to East",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: None,
    },
    SwitchRow {
        short: "-S",
        long: "--south",
        group: "Predeal",
        what: "Predeal cards to South",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: None,
    },
    SwitchRow {
        short: "-W",
        long: "--west",
        group: "Predeal",
        what: "Predeal cards to West",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: None,
    },
    // ---- Reporting --------------------------------------------------------
    SwitchRow {
        short: "-C",
        long: "--CSV",
        group: "Reporting",
        what: "Write a CSV report to a file",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: Some("Appends by default; `w:filename` truncates. Driven by the `csvrpt` statement."),
    },
    SwitchRow {
        short: "-T",
        long: "--title",
        group: "Reporting",
        what: "Title for PBN output",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: None,
    },
    // ---- Performance ------------------------------------------------------
    SwitchRow {
        short: "-R",
        long: "--threads",
        group: "Performance",
        what: "Worker threads, 0 to auto-detect",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: Some(
            "DealerV2_4 uses it for its double-dummy solver; here it parallelises generation.",
        ),
    },
    SwitchRow {
        short: "",
        long: "--batch-size",
        group: "Performance",
        what: "Work units per batch when parallel",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: None,
    },
    // ---- Reading deals in -------------------------------------------------
    SwitchRow {
        short: "",
        long: "--input-deals",
        group: "Reading deals in",
        what: "Filter deals from a file instead of generating",
        dealer_exe: Origin::Differs("-l replays from a library file by index"),
        dealer_v2: Origin::Differs("-L names a library path, -l exports DL52"),
        note: Some(
            "Reads PBN or one-line, auto-detected, `-` for stdin. Unrecognised lines are \
             skipped, so check the reported count.",
        ),
    },
    // ---- Recognised but not supported -------------------------------------
    // Declared to clap as hidden arguments purely so the program can explain
    // itself. The dealer3 column derives "rejected" from that hidden flag, so
    // implementing one for real needs no bookkeeping here.
    SwitchRow {
        short: "-u",
        long: "",
        group: "Recognised but not supported",
        what: "Upper-case the honour cards in output",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Absent,
        note: Some("Cosmetic."),
    },
    SwitchRow {
        short: "-e",
        long: "",
        group: "Recognised but not supported",
        what: "Exhaust mode",
        dealer_exe: Origin::Differs("compiled out; prints \"not included\""),
        dealer_v2: Origin::Absent,
        note: Some("Never finished in the original either."),
    },
    SwitchRow {
        short: "-l",
        long: "",
        group: "Recognised but not supported",
        what: "Replay deals from a library file",
        dealer_exe: Origin::Same,
        dealer_v2: Origin::Differs("-l exports DL52"),
        note: Some("`--input-deals` covers this use case in dealer3's own way."),
    },
    SwitchRow {
        short: "",
        long: "--legacy",
        group: "Recognised but not supported",
        what: "The old single-threaded RNG mode",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Absent,
        note: Some("Removed in 0.5.0; still parsed so a script using it gets an explanation."),
    },
    // ---- Not implemented --------------------------------------------------
    // No clap argument at all, so the dealer3 column renders as a gap.
    SwitchRow {
        short: "-M",
        long: "",
        group: "Not implemented",
        what: "Double-dummy solver mode",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: Some("dealer3 has `tricks()` but no mode switch."),
    },
    SwitchRow {
        short: "-Z",
        long: "",
        group: "Not implemented",
        what: "Export in RP zrd format",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: None,
    },
    SwitchRow {
        short: "-U",
        long: "",
        group: "Not implemented",
        what: "DealerServer path",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: None,
    },
    SwitchRow {
        short: "-O",
        long: "",
        group: "Not implemented",
        what: "OPC evaluation for the opener",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: None,
    },
    SwitchRow {
        short: "-D",
        long: "",
        group: "Not implemented",
        what: "Debug verbosity 0-9",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: None,
    },
    SwitchRow {
        short: "-L",
        long: "",
        group: "Not implemented",
        what: "Path to the RP library the deals are read from",
        dealer_exe: Origin::Absent,
        dealer_v2: Origin::Same,
        note: Some(
            "The companion to DealerV2_4's `-l`; it was mentioned only in the note on \
             `--input-deals`, which left it the one V2_4 switch with no row of its own.",
        ),
    },
];

/// What dealer3 does with a switch, derived from clap rather than recorded.
///
/// The three states are distinguishable because the deprecated switches are all
/// declared `hide = true`: they are parsed only so the program can explain
/// itself rather than fail with "unexpected argument". So a hidden argument is
/// one dealer3 recognises and refuses, and a visible one is implemented.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Support {
    /// Implemented.
    Yes,
    /// Parsed, then rejected with an explanation.
    Rejected,
    /// Not accepted at all.
    No,
}

impl Support {
    fn cell(self) -> &'static str {
        match self {
            Support::Yes => "✅",
            Support::Rejected => "⚠️ rejected with a message",
            Support::No => "❌",
        }
    }
}

/// Look a switch up in the parser that actually parses the command line.
pub fn support(short: &str, long: &str) -> Support {
    use clap::CommandFactory;
    let command = crate::Args::command();
    let found = command.get_arguments().find(|arg| {
        let short_matches =
            !short.is_empty() && arg.get_short().is_some_and(|c| format!("-{}", c) == short);
        let long_matches =
            !long.is_empty() && arg.get_long().is_some_and(|l| format!("--{}", l) == long);
        short_matches || long_matches
    });
    match found {
        None => Support::No,
        Some(arg) if arg.is_hide_set() => Support::Rejected,
        Some(_) => Support::Yes,
    }
}

/// How a switch is written, for the first column.
fn spelling(row: &SwitchRow) -> String {
    match (row.short.is_empty(), row.long.is_empty()) {
        (false, false) => format!("`{}`, `{}`", row.short, row.long),
        (false, true) => format!("`{}`", row.short),
        (true, false) => format!("`{}`", row.long),
        (true, true) => "—".to_string(),
    }
}

/// Render the comparison table as markdown.
pub fn render_table() -> String {
    let mut out = String::new();
    let supported = SWITCH_ROWS
        .iter()
        .filter(|r| support(r.short, r.long) == Support::Yes)
        .count();

    out.push_str(&format!(
        "dealer3 implements **{} of the {} switches** listed here. The dealer3 column is read \
         from the argument parser itself, so it cannot drift; the other two columns are \
         reference data (see `dealer/src/switches.rs` for their provenance).\n\n\
         In the dealer3 column ✅ is implemented and ⚠️ means the switch is parsed and then \
         refused with an explanation, so a script using it gets told rather than ignored. In \
         the other two columns ✅ means the same meaning, ⚠️ a different one, and — not \
         present at all.\n",
        supported,
        SWITCH_ROWS.len()
    ));

    for group in GROUPS {
        let rows: Vec<&SwitchRow> = SWITCH_ROWS.iter().filter(|r| r.group == *group).collect();
        if rows.is_empty() {
            continue;
        }
        out.push_str(&format!("\n### {}\n\n", group));
        out.push_str("| Switch | What it does | dealer3 | dealer.exe | DealerV2_4 | Notes |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for row in rows {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                spelling(row),
                row.what,
                support(row.short, row.long).cell(),
                row.dealer_exe.cell(),
                row.dealer_v2.cell(),
                row.note.unwrap_or("")
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::collections::BTreeSet;

    /// Arguments clap defines, by whichever spelling the table would use.
    fn clap_switches() -> BTreeSet<String> {
        crate::Args::command()
            .get_arguments()
            .map(|arg| match (arg.get_short(), arg.get_long()) {
                (_, Some(long)) => format!("--{}", long),
                (Some(short), None) => format!("-{}", short),
                (None, None) => String::new(),
            })
            .filter(|s| !s.is_empty())
            .collect()
    }

    #[test]
    fn every_switch_is_in_the_table() {
        let documented: BTreeSet<String> = SWITCH_ROWS
            .iter()
            .flat_map(|r| [r.short.to_string(), r.long.to_string()])
            .filter(|s| !s.is_empty())
            .collect();

        let missing: Vec<_> = clap_switches()
            .into_iter()
            .filter(|s| !documented.contains(s))
            .collect();

        assert!(
            missing.is_empty(),
            "these switches exist but are not in SWITCH_ROWS: {:?}\n\n\
             The comparison table in docs/command_line_comparison.md is generated from that \
             list, so a switch missing from it is a switch nobody can look up.",
            missing
        );
    }

    #[test]
    fn no_switch_is_listed_twice() {
        let mut seen = Vec::new();
        for row in SWITCH_ROWS {
            for spelling in [row.short, row.long] {
                if !spelling.is_empty() {
                    seen.push(spelling);
                }
            }
        }
        let mut unique = seen.clone();
        unique.sort_unstable();
        let before = unique.len();
        unique.dedup();
        assert_eq!(
            before,
            unique.len(),
            "a switch appears in two rows: {:?}",
            seen
        );
    }

    #[test]
    fn rows_are_filled_in() {
        for row in SWITCH_ROWS {
            assert!(
                !(row.short.is_empty() && row.long.is_empty()),
                "a row has neither a short nor a long form: {}",
                row.what
            );
            assert!(!row.what.trim().is_empty(), "a row has no description");
            assert!(
                GROUPS.contains(&row.group),
                "{} is in group {:?}, which is not in GROUPS",
                row.what,
                row.group
            );
        }
    }

    /// A row filed under one of the unsupported groups that turns out to be
    /// implemented is a row somebody forgot to move.
    #[test]
    fn the_unsupported_groups_really_are_unsupported() {
        for row in SWITCH_ROWS
            .iter()
            .filter(|r| r.group == "Not implemented" || r.group == "Recognised but not supported")
        {
            assert_ne!(
                support(row.short, row.long),
                Support::Yes,
                "`{}{}` is filed under {:?} but is implemented — move it to a real group",
                row.short,
                row.long,
                row.group
            );
        }
    }

    /// And the reverse: a switch that merely prints an explanation must not sit
    /// in a group that claims dealer3 supports it.
    #[test]
    fn rejected_switches_are_not_filed_as_working() {
        for row in SWITCH_ROWS {
            if support(row.short, row.long) == Support::Rejected {
                assert_eq!(
                    row.group, "Recognised but not supported",
                    "`{}{}` is parsed only to be refused, so it belongs under \"Recognised but \
                     not supported\", not {:?}",
                    row.short, row.long, row.group
                );
            }
        }
    }

    #[test]
    fn docs_are_up_to_date() {
        crate::generated_docs::check_or_update(
            "command_line_comparison.md",
            "switches",
            &render_table(),
        );
    }
}
