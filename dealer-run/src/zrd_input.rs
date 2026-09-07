//! Reading a Pavlicek ZRD library: the deals, and their double-dummy tables.
//!
//! **Decoding only.** Nothing here solves, and nothing here knows what a table
//! is for — it reads deals, reads the tables that come with them, and hands
//! both on as `bridge_types` data. What a run makes of a table is decided where
//! deals enter the run, not here.
//!
//! That separation is the point. A ZRD record happens to carry double-dummy
//! results, but reading a file is reading a file: a reader that reached into
//! the solver's memory would tie every future format to the solver, and would
//! be untestable without it.
//!
//! An unsolved record needs no special handling either. The format writes an
//! all-zero table to mean "not solved" — an all-zero table is unreachable for a
//! real deal — and `bridge_encodings` reports that as `None`, which travels
//! onward as `None`.

use bridge_types::DdTable;
use dealer_core::Deal;

/// What reading a library found, beyond the deals themselves.
///
/// Returned rather than printed: a library does not know whether its caller has
/// a terminal, and a browser has nowhere to put a warning that a terminal would
/// write to stderr.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LibraryReport {
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

/// Read a ZRD library: every deal, and the table that came with it.
///
/// Returns the deals in file order, each with its table or `None`, and a report
/// of what else was in there. Unreadable records are skipped rather than fatal
/// — a library is millions of records and one bad one should not cost the rest
/// — but they are all named in the report so a caller can decide.
pub fn read_library(
    path: &str,
) -> Result<(Vec<(Deal, Option<DdTable>)>, LibraryReport), String> {
    use bridge_encodings::zrd::{Record, ZrdReader};

    let mut reader =
        ZrdReader::open(path).map_err(|e| format!("opening ZRD library '{}': {}", path, e))?;

    let mut deals = Vec::new();
    let mut report = LibraryReport::default();

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
                    deals.push((deal, table));
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
