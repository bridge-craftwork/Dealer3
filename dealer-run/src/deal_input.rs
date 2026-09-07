//! Reading deals from a file, with whatever double-dummy tables came with them.
//!
//! **Decoding only.** Nothing here solves, and nothing here knows what a table
//! is for. Reading a file is reading a file: a reader that reached into the
//! solver would tie every future format to it, and would be untestable without
//! one.
//!
//! ## A table is a property of the input, not of one format
//!
//! Three of the formats a caller can name carry double-dummy results, and this
//! module's job is to make them look the same on the way out:
//!
//! | source | where the table is |
//! |---|---|
//! | ZRD record | the last ten bytes, twenty results packed four bits each |
//! | PBN `[DoubleDummyTricks]` | the compact twenty-character encoding |
//! | PBN `[OptimumResultTable]` | the same table written as rows |
//!
//! `bridge_encodings` decodes all three; a PBN board arrives with its table
//! already in `double_dummy_tricks`. One-line and printall carry no tables and
//! yield `None`, which is not a failure — it is what an unsolved deal looks
//! like, and the run solves those on demand.
//!
//! ZRD says "not solved" with an all-zero table, which is unreachable for a
//! real deal. `bridge_encodings` reports that as `None` too, so an unsolved
//! record and a one-line deal arrive here indistinguishable, as they should.

use bridge_types::DdTable;
use dealer_core::Deal;

/// A deal from a file, and the table that came with it.
#[derive(Debug, Clone)]
pub struct InputDeal {
    pub deal: Deal,
    /// The twenty results, when the file carried them. `None` means nobody has
    /// solved this deal yet, not that it cannot be solved.
    pub table: Option<DdTable>,
}

/// What reading found, beyond the deals themselves.
///
/// Returned rather than printed: a library does not know whether its caller has
/// a terminal, and a browser has nowhere to put a warning that a terminal would
/// write to stderr.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InputReport {
    /// Which reader handled it, for a caller that wants to say so.
    pub format: &'static str,
    /// Deals whose records carried a solved table.
    pub solved: usize,
    /// Deals whose records were unsolved, and will be solved on demand.
    pub unsolved: usize,
    /// Section separators, which are not deals. Pavlicek divides a file with a
    /// record giving one seat sixteen cards; no legal deal can look like that.
    pub separators: usize,
    /// Records that could not be read or did not make a whole deal, with the
    /// reason. The deals around them are still returned.
    pub skipped: Vec<String>,
}

/// Read deals from `source`, keeping any double-dummy tables they carry.
///
/// `-` reads standard input. A `.zrd` path is a Pavlicek library; anything else
/// is text, and is tried as PBN first so that a board's tags are kept, falling
/// back to the line-oriented reader for one-line and printall.
///
/// ## Why PBN is tried first rather than detected
///
/// `DealReader` auto-detects PBN, one-line and printall, and is what
/// `--input-deals` has always used — but it yields deals with the tags dropped,
/// so a file annotated by our own solver arrives with its analysis thrown away.
/// `PbnDocument` keeps the tags but reads PBN only.
///
/// So: parse as PBN, and use it when that produced boards with cards in them.
/// A one-line file yields no boards and falls through. Nothing is given up —
/// `PbnDocument` copes with what `DealReader` was tolerating here, which was
/// measured rather than assumed: a dealer3 PBN file carrying a
/// `Generated/Produced/Time needed` trailer parses and keeps the trailer, and
/// so does one whose first line begins with the stray `<EOF>` characters
/// dealer.exe's block-comment bug emits.
pub fn read(source: &str) -> Result<(Vec<InputDeal>, InputReport), String> {
    if is_tables_only_path(source) {
        return Err(format!(
            "'{}' holds double-dummy tables with no deals in it. Read the .zrd built \
             from it instead.",
            source
        ));
    }
    if is_library_path(source) {
        return read_library(source);
    }

    let text = if source == "-" {
        use std::io::Read;
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("reading deals from standard input: {}", e))?;
        text
    } else {
        std::fs::read_to_string(source)
            .map_err(|e| format!("opening input deals file '{}': {}", source, e))?
    };

    if let Some(read) = read_pbn(&text) {
        return Ok(read);
    }
    Ok(read_lines(&text))
}

/// Deals from PBN, with each board's table, or `None` if this is not PBN.
///
/// `None` rather than an error for anything that did not come out as boards
/// with cards: that is how a one-line file declines, and it has to fall through
/// rather than fail.
fn read_pbn(text: &str) -> Option<(Vec<InputDeal>, InputReport)> {
    let document = bridge_encodings::pbn::PbnDocument::parse(text).ok()?;
    let mut deals = Vec::new();
    let mut report = InputReport {
        format: "pbn",
        ..Default::default()
    };
    for (index, board) in document.boards().iter().enumerate() {
        match Deal::try_from(&board.deal) {
            // A board the parser accepted can still be one this program cannot
            // use. A hand of fourteen does not fit; a board a card short fits
            // and is worse, because it runs and reports statistics over a
            // twelve-card hand without a word. Both are refused here, as they
            // are on the line-oriented path.
            Ok(deal) => match deal.check_complete() {
                Ok(()) => {
                    match &board.double_dummy_tricks {
                        Some(_) => report.solved += 1,
                        None => report.unsolved += 1,
                    }
                    deals.push(InputDeal {
                        deal,
                        table: board.double_dummy_tricks,
                    });
                }
                Err(e) => report.skipped.push(format!(
                    "deal {} — it is not a whole deal: {}",
                    index + 1,
                    e
                )),
            },
            Err(e) => report.skipped.push(format!("deal {} — {}", index + 1, e)),
        }
    }
    // A document with no usable board is not PBN as far as this is concerned,
    // whatever the parser made of it.
    if deals.is_empty() {
        return None;
    }
    Some((deals, report))
}

/// Deals from the line-oriented reader: one-line, printall, and PBN it
/// recognises. No tables — none of these formats carry one.
fn read_lines(text: &str) -> (Vec<InputDeal>, InputReport) {
    use bridge_encodings::DealReader;
    let mut deals = Vec::new();
    let mut report = InputReport {
        format: "lines",
        ..Default::default()
    };
    for (index, result) in DealReader::new(std::io::Cursor::new(text)).enumerate() {
        match result {
            Ok(deal) => match Deal::try_from(deal) {
                Ok(deal) => match deal.check_complete() {
                    Ok(()) => {
                        report.unsolved += 1;
                        deals.push(InputDeal { deal, table: None });
                    }
                    Err(e) => report.skipped.push(format!(
                        "deal {} is not a whole deal: {}",
                        index + 1,
                        e
                    )),
                },
                Err(e) => report.skipped.push(format!("deal {}: {}", index + 1, e)),
            },
            Err(e) => report
                .skipped
                .push(format!("deal {} could not be read: {}", index + 1, e)),
        }
    }
    (deals, report)
}

/// Read a ZRD library: every deal, and the table that came with it.
///
/// Returns the deals in file order, each with its table or `None`, and a report
/// of what else was in there. Unreadable records are skipped rather than fatal
/// — a library is millions of records and one bad one should not cost the rest
/// — but they are all named in the report so a caller can decide.
pub fn read_library(path: &str) -> Result<(Vec<InputDeal>, InputReport), String> {
    use bridge_encodings::zrd::{Record, ZrdReader};

    let mut reader =
        ZrdReader::open(path).map_err(|e| format!("opening input deals file '{}': {}", path, e))?;

    let mut deals = Vec::new();
    let mut report = InputReport {
        format: "zrd",
        ..Default::default()
    };

    for (index, record) in reader.records().enumerate() {
        match record {
            Ok(Record::Separator) => report.separators += 1,
            Ok(Record::Deal { deal, table }) => match Deal::try_from(&deal) {
                Ok(deal) => {
                    if table.is_some() {
                        report.solved += 1;
                    } else {
                        report.unsolved += 1;
                    }
                    deals.push(InputDeal { deal, table });
                }
                Err(e) => report
                    .skipped
                    .push(format!("record {} is not a whole deal: {}", index, e)),
            },
            Err(e) => report
                .skipped
                .push(format!("record {} could not be read: {}", index, e)),
        }
    }

    Ok((deals, report))
}

/// Does this `--input-deals` source name a library rather than a text file?
///
/// By extension, because the caller names the file and the two extensions mean
/// different things. Sniffing the content would be worse than it sounds: a ZRD
/// record is twenty-three arbitrary bytes, so there is no header to recognise,
/// and a short text file can be a multiple of twenty-three by accident.
pub fn is_library_path(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.ends_with(".zrd") || lower.ends_with(".zdd")
}

/// Is this the tables-only companion format, which has no deals in it?
///
/// `.zdd` holds twenty results per record and nothing else — it is what
/// Pavlicek distributes, with the deals regenerated locally into a `.zrd`. A
/// caller asking to read deals from one has the wrong file, and saying so beats
/// reading twenty-three-byte records out of ten-byte ones.
pub fn is_tables_only_path(source: &str) -> bool {
    source.to_ascii_lowercase().ends_with(".zdd")
}
