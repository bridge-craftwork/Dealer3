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
//!
//! ## Two sources, one decoder
//!
//! [`read`] takes somewhere to read from; [`read_bytes`] takes the bytes. The
//! second is the whole of the decoding, so a browser handed a file by the page
//! and a terminal handed a pipe reach the same reader rather than each growing
//! one of their own.

use bridge_encodings::zrd::{read_record, Record, ZrdReader, RECORD_LEN};
use bridge_types::DdTable;
use dealer_core::Deal;

/// How many records the format sniff decodes before it is satisfied.
///
/// Bounded because sniffing is decoding, and the published library is ten
/// million records: checking every one of them would double the cost of
/// reading it. Sixty-four is far past the point of doubt — arbitrary bytes
/// split thirteen cards to a seat about once in four hundred, and would have to
/// do it sixty-four times running with twenty legal trick counts each time.
const SNIFF_RECORDS: usize = 64;

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
    /// What a caller may want to say about the input that was not a failure —
    /// chiefly a file whose name disagrees with what is inside it.
    pub notes: Vec<String>,
}

/// Read deals from `source`, keeping any double-dummy tables they carry.
///
/// `-` reads standard input; anything else is a path.
///
/// ## The format is what the bytes are, not what they are called
///
/// It used to be the extension, and standard input has none — so a piped
/// library was read as text and died on `stream did not contain valid UTF-8`
/// before anything else got a look at it. A pipe is the whole point here:
/// `curl -s .../deals.zrd | dealer script.dlr --input-deals -` is how a library
/// arrives without this program growing an HTTP client to fetch it.
///
/// So the content decides. A ZRD file is a whole number of 23-byte records,
/// every one of which decodes to four hands of thirteen; anything else is text.
/// The old objection to sniffing was that a record is twenty-three arbitrary
/// bytes with no header to recognise — true of the *record*, and beside the
/// point: a deal is a strong constraint, and text does not satisfy it by
/// accident.
///
/// A name that disagrees with the content is read as the content says, and
/// noted. Refusing would help nobody — a renamed or half-downloaded file still
/// holds what it holds — but a `.zrd` full of PBN used to be spent on garbage
/// records and come back as no deals at all, with nothing said.
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

    if source == "-" {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|e| format!("reading deals from standard input: {}", e))?;
        return read_named(&bytes, "standard input");
    }

    // A named library is streamed rather than read into memory to be sniffed:
    // the published one is 241 MB, and its head settles the question.
    let library = path_holds_library(source)?;
    let mut read = if library {
        read_library(source)?
    } else {
        let bytes = std::fs::read(source)
            .map_err(|e| format!("opening input deals file '{}': {}", source, e))?;
        read_named(&bytes, &format!("'{}'", source))?
    };
    if let Some(note) = name_disagrees(source, library) {
        read.1.notes.push(note);
    }
    Ok(read)
}

/// Read deals from bytes already in hand: a pipe, a download, a dropped file.
///
/// The whole of the decoding with no filesystem in it — [`read`] is this plus
/// somewhere to read from. A browser handed a file by the page reads it here,
/// so that a supplied library behaves the same in a tab as at a terminal rather
/// than through a second decoder that drifts from this one.
pub fn read_bytes(bytes: &[u8]) -> Result<(Vec<InputDeal>, InputReport), String> {
    read_named(bytes, "the supplied deals")
}

/// [`read_bytes`], with a name for the bytes so an error can say where they came
/// from. Nothing else differs between the two, and nothing else may.
fn read_named(bytes: &[u8], what: &str) -> Result<(Vec<InputDeal>, InputReport), String> {
    if looks_like_library(bytes) {
        // `Cursor` is `Read + Seek`, which is all `ZrdReader` ever wanted, so a
        // library that came down a pipe needs no decoding of its own.
        let reader = ZrdReader::new(std::io::Cursor::new(bytes))
            .map_err(|e| format!("reading {} as a library: {}", what, e))?;
        return Ok(read_records(reader));
    }

    let text = std::str::from_utf8(bytes).map_err(|e| {
        // Neither format: worth saying which two were ruled out, because the
        // likely cause is a library that arrived short and a caller staring at
        // a UTF-8 complaint has no way to guess that.
        let leftover = bytes.len() % RECORD_LEN;
        let hint = if leftover == 0 {
            String::new()
        } else {
            format!(
                " A library is a whole number of {}-byte records, and this is {} bytes with \
                 {} left over — which is what a download cut short looks like.",
                RECORD_LEN,
                bytes.len(),
                leftover
            )
        };
        format!(
            "{} is neither a Pavlicek library nor text: {}.{}",
            what, e, hint
        )
    })?;

    if let Some(read) = read_pbn(text) {
        return Ok(read);
    }
    Ok(read_lines(text))
}

/// Do these bytes hold a Pavlicek library?
///
/// A whole number of 23-byte records, of which the first [`SNIFF_RECORDS`] all
/// decode. Empty is not a library: it is nothing, and answering yes would send
/// an empty file to the reader least able to say what was wrong with it.
pub fn looks_like_library(bytes: &[u8]) -> bool {
    if bytes.is_empty() || !bytes.len().is_multiple_of(RECORD_LEN) {
        return false;
    }
    records_decode(&bytes[..bytes.len().min(RECORD_LEN * SNIFF_RECORDS)])
}

/// Does every whole record in `head` decode? The one place that test lives, so
/// a file on disk and a pipe cannot come to different conclusions about the
/// same bytes.
fn records_decode(head: &[u8]) -> bool {
    let (records, _) = head.as_chunks::<RECORD_LEN>();
    records.iter().all(|record| read_record(record).is_ok())
}

/// Does the file at `path` hold a library? Reads its head, not the whole file.
fn path_holds_library(path: &str) -> Result<bool, String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("opening input deals file '{}': {}", path, e))?;
    let len = file
        .metadata()
        .map_err(|e| format!("opening input deals file '{}': {}", path, e))?
        .len();
    if len == 0 || !len.is_multiple_of(RECORD_LEN as u64) {
        return Ok(false);
    }
    let head_len = len.min((RECORD_LEN * SNIFF_RECORDS) as u64) as usize;
    let mut head = vec![0u8; head_len];
    file.read_exact(&mut head)
        .map_err(|e| format!("reading input deals file '{}': {}", path, e))?;
    Ok(records_decode(&head))
}

/// A note for a file whose name says one format and whose content says another.
///
/// Said rather than refused: the content is what gets read either way, and a
/// file renamed on its way through a browser or a shared drive still holds what
/// it holds. But a `.zrd` that is really PBN used to be spent on garbage records
/// and come back as no deals with no complaint, and a `.pbn` that is really a
/// library used to fail on `stream did not contain valid UTF-8`, which named
/// neither the file nor the reason.
fn name_disagrees(path: &str, holds_library: bool) -> Option<String> {
    let named_library = path.to_ascii_lowercase().ends_with(".zrd");
    match (named_library, holds_library) {
        (true, false) => Some(format!(
            "'{}' is named as a Pavlicek library but does not hold one; reading it as text.",
            path
        )),
        (false, true) => Some(format!(
            "'{}' holds a Pavlicek library whatever its name says; reading it as one.",
            path
        )),
        _ => None,
    }
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

/// Read a ZRD library from a path: every deal, and the table that came with it.
///
/// Streamed, because the published library is 241 MB and a path is the one
/// source that need not be held in memory to be read.
pub fn read_library(path: &str) -> Result<(Vec<InputDeal>, InputReport), String> {
    let reader =
        ZrdReader::open(path).map_err(|e| format!("opening input deals file '{}': {}", path, e))?;
    Ok(read_records(reader))
}

/// Every record of an open library, in file order.
///
/// Returns the deals with their tables or `None`, and a report of what else was
/// in there. Unreadable records are skipped rather than fatal — a library is
/// millions of records and one bad one should not cost the rest — but they are
/// all named in the report so a caller can decide.
///
/// Generic over the source because that is the only difference between a
/// library on disk and one that came down a pipe: the pipe's bytes arrive in a
/// `Cursor`, and everything after that is the same decoding. Two copies of this
/// loop would be two answers to what a separator is.
fn read_records<R: std::io::Read + std::io::Seek>(
    mut reader: ZrdReader<R>,
) -> (Vec<InputDeal>, InputReport) {
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

    (deals, report)
}

/// Is this the tables-only companion format, which has no deals in it?
///
/// `.zdd` holds twenty results per record and nothing else — it is what
/// Pavlicek distributes, with the deals regenerated locally into a `.zrd`. A
/// caller asking to read deals from one has the wrong file, and saying so beats
/// reading twenty-three-byte records out of ten-byte ones.
///
/// By name, unlike the library check, because there is nothing to sniff: a
/// `.zdd` record is ten bytes of trick counts, which is not a constraint any
/// text would struggle to meet.
pub fn is_tables_only_path(source: &str) -> bool {
    source.to_ascii_lowercase().ends_with(".zdd")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ten records of Pavlicek's library, all solved, no separators.
    ///
    /// A real file rather than records written here: what is under test is
    /// reading something this code did not write, and a round trip cannot tell
    /// that apart from agreeing with itself.
    const LIBRARY: &[u8] = include_bytes!("../tests/fixtures/rpdd_10First.zrd");

    /// The first record of the fixture, as `bridge_encodings`' own tests have
    /// it. Pins the order as well as the count: ten deals in the wrong order,
    /// or ten copies of one, would satisfy a count.
    const FIRST_DEAL: &str =
        "N:J873.J42.Q65.KT2 AT652.A976.AJ82. Q4.85.KT9.A87643 K9.KQT3.743.QJ95";

    const ONELINE: &str =
        "n J873.J42.Q65.KT2 e AT652.A976.AJ82. s Q4.85.KT9.A87643 w K9.KQT3.743.QJ95\n";

    /// A path in the temporary directory, unique to this process and thread.
    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "dealer3-deal-input-{}-{:?}-{}",
            std::process::id(),
            std::thread::current().id(),
            name
        ));
        path
    }

    /// The deals as PBN, for comparing two readings of the same bytes.
    fn as_pbn(deals: &[InputDeal]) -> Vec<String> {
        deals
            .iter()
            .map(|read| bridge_types::Deal::from(&read.deal).to_pbn(bridge_types::Direction::North))
            .collect()
    }

    #[test]
    fn a_library_in_memory_is_recognised_and_read() {
        assert!(looks_like_library(LIBRARY));

        let (deals, report) = read_bytes(LIBRARY).expect("a library should read from bytes");

        assert_eq!(report.format, "zrd");
        assert_eq!(deals.len(), 10, "the fixture holds ten records");
        assert_eq!(report.solved, 10, "every record of it is solved");
        assert_eq!(report.unsolved, 0);
        assert_eq!(report.separators, 0);
        assert!(report.skipped.is_empty(), "skipped: {:?}", report.skipped);
        assert!(
            report.notes.is_empty(),
            "bytes have no name to disagree with: {:?}",
            report.notes
        );
        assert_eq!(as_pbn(&deals)[0], FIRST_DEAL);
        assert!(
            deals.iter().all(|read| read.table.is_some()),
            "every deal should have brought its table through"
        );
    }

    #[test]
    fn text_whose_length_is_a_multiple_of_a_record_is_still_text() {
        // The length test alone would let this through, so it is the record
        // decoding that has to refuse it — which is the whole claim the sniff
        // makes.
        let mut text = ONELINE.to_string();
        while !text.len().is_multiple_of(RECORD_LEN) {
            text.push('\n');
        }
        assert!(
            text.len().is_multiple_of(RECORD_LEN),
            "this test is pointless unless the length is a whole number of records"
        );

        assert!(!looks_like_library(text.as_bytes()));

        let (deals, report) = read_bytes(text.as_bytes()).expect("text should read as text");
        assert_eq!(report.format, "lines");
        assert_eq!(deals.len(), 1);
        assert_eq!(report.unsolved, 1);
        assert!(report.skipped.is_empty(), "skipped: {:?}", report.skipped);
    }

    #[test]
    fn a_pbn_board_read_from_bytes_still_arrives_with_its_table() {
        let pbn = "[Board \"1\"]\n\
                   [Deal \"N:QJ3.A93.K4.KT875 A98.QJT2.97653.2 T76.K65.AQJ8.J43 K542.874.T2.AQ96\"]\n\
                   [DoubleDummyTricks \"87879878793555345564\"]\n";

        let (deals, report) = read_bytes(pbn.as_bytes()).expect("PBN should read from bytes");
        assert_eq!(report.format, "pbn");
        assert_eq!(deals.len(), 1);
        assert_eq!(report.solved, 1);
        assert!(deals[0].table.is_some());
    }

    #[test]
    fn a_library_cut_short_says_so_rather_than_complaining_about_utf8() {
        // What a `curl` that lost its connection leaves behind. It used to
        // arrive as "stream did not contain valid UTF-8", which names neither
        // the format that was expected nor the reason it was not found.
        let truncated = &LIBRARY[..LIBRARY.len() - 5];
        assert!(!looks_like_library(truncated));

        let error = read_bytes(truncated).expect_err("a part-record library cannot be read");
        assert!(
            error.contains("neither a Pavlicek library nor text"),
            "should rule out both formats: {}",
            error
        );
        assert!(
            error.contains("left over"),
            "should say the records do not come out whole: {}",
            error
        );
    }

    #[test]
    fn a_library_under_a_text_name_is_read_as_a_library_and_noted() {
        let path = temp_path("named.pbn");
        std::fs::write(&path, LIBRARY).expect("write fixture");
        let source = path.to_str().expect("utf-8 path");

        let (deals, report) = read(source).expect("the content is a library whatever the name");
        assert_eq!(report.format, "zrd");
        assert_eq!(deals.len(), 10);
        assert_eq!(report.solved, 10);
        assert!(
            report.notes.iter().any(|n| n.contains("whatever its name")),
            "the disagreement should be said out loud: {:?}",
            report.notes
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn text_under_a_library_name_is_read_as_text_and_noted() {
        let path = temp_path("named.zrd");
        std::fs::write(&path, ONELINE).expect("write fixture");
        let source = path.to_str().expect("utf-8 path");

        let (deals, report) = read(source).expect("the content is text whatever the name");
        assert_eq!(report.format, "lines");
        assert_eq!(deals.len(), 1, "it used to come back as no deals at all");
        assert!(
            report
                .notes
                .iter()
                .any(|n| n.contains("named as a Pavlicek library")),
            "the disagreement should be said out loud: {:?}",
            report.notes
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_library_reads_the_same_from_a_path_as_from_a_pipe() {
        // The two sources may differ in where the bytes come from and nowhere
        // else; a streaming reader and a `Cursor` that disagreed about a record
        // would be invisible from either side alone.
        let path = temp_path("same.zrd");
        std::fs::write(&path, LIBRARY).expect("write fixture");
        let source = path.to_str().expect("utf-8 path");

        let (from_path, path_report) = read(source).expect("read from a path");
        let (from_bytes, bytes_report) = read_bytes(LIBRARY).expect("read from bytes");

        assert_eq!(path_report, bytes_report);
        assert_eq!(as_pbn(&from_path), as_pbn(&from_bytes));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_tables_only_file_is_still_refused_by_name() {
        let error = read("deals.zdd").expect_err(".zdd holds no deals");
        assert!(error.contains("no deals in it"), "{}", error);
    }
}
