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
//! ## A window, not the whole file
//!
//! A library is not read from the front, and it is not read entire. The
//! published one is ten million records, and a script asking for forty deals
//! has no use for the other 9,999,960 of them. [`Window`] says where to start
//! and how much to take; [`Start::Seed`] is what makes `-s` mean the same
//! thing for a library as for a generated run, which is to say the same seed
//! gives the same deals.
//!
//! ## Two sources, one decoder
//!
//! [`read`] takes somewhere to read from; [`read_bytes`] takes the bytes. The
//! second is the whole of the decoding, so a browser handed a file by the page
//! and a terminal handed a pipe reach the same reader rather than each growing
//! one of their own.

use bridge_encodings::zrd::{read_record, Record, ZrdReader, RECORD_LEN};
use bridge_types::DdTable;
use dealer_core::rng::Xoshiro256PlusPlus;
use dealer_core::Deal;

/// How many records the format sniff decodes before it is satisfied.
///
/// Bounded because sniffing is decoding, and the published library is ten
/// million records: checking every one of them would double the cost of
/// reading it. Sixty-four is far past the point of doubt — arbitrary bytes
/// split thirteen cards to a seat about once in four hundred, and would have to
/// do it sixty-four times running with twenty legal trick counts each time.
const SNIFF_RECORDS: usize = 64;

/// Which part of a library to read: where to start, and how much to take.
///
/// A `.zrd` is fixed-length records, so a start is arithmetic and a seek
/// rather than a scan. Nothing else here can be windowed — PBN and one-line
/// have to be read to be counted — so a start given for text declines out
/// loud rather than pretending, while a limit is honoured by taking the first
/// deals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// Which record to read first.
    pub start: Start,
    /// How many deals to take from there.
    pub take: Take,
}

impl Default for Window {
    fn default() -> Self {
        Window {
            start: Start::Record(0),
            take: Take::Pass,
        }
    }
}

impl Window {
    /// The whole library in file order: what a caller with no opinion wants.
    pub fn all() -> Self {
        Window::default()
    }
}

/// Which record a read starts at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// A record ordinal, counting from zero, and counting *records* — a
    /// separator is a record and spends one of these, which is what makes an
    /// offset arithmetic on a file position rather than a count of deals.
    ///
    /// Past the end wraps rather than failing: the library is a ring here, and
    /// there is nothing at the end of it to fall off.
    Record(u64),
    /// The run's seed, which picks the record.
    ///
    /// This is what keeps `-s` meaning one thing across both deal sources: a
    /// generated run reproduces from its seed, and so does a library run.
    Seed(u32),
}

/// How much of a library to take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Take {
    /// One pass: every deal in the library, starting at [`Start`] and coming
    /// round to it again.
    Pass,
    /// At most this many deals, and never more than the library holds. What a
    /// run's `-g` amounts to — a ceiling on what will be looked at, not a
    /// demand for that many.
    AtMost(usize),
    /// Exactly this many deals, repeating the library from the start when it
    /// runs short. That repeats *deals*, and is reported when it happens.
    Exactly(usize),
}

impl Take {
    /// The most deals this will yield, or `None` for a whole pass.
    fn limit(self) -> Option<usize> {
        match self {
            Take::Pass => None,
            Take::AtMost(n) | Take::Exactly(n) => Some(n),
        }
    }
}

impl Start {
    /// Which record to begin at, in a library of `records` records.
    ///
    /// `records` must not be zero: an empty library is turned away before this
    /// is asked, because there is no record to name.
    ///
    /// ## Why the seed is mixed rather than used as it stands
    ///
    /// DealerV2_4 does the plainer thing — its `seek_rpdd_pos()` reads the
    /// seed as a count of 1000-record blocks to skip, and refuses a seed past
    /// the end of the file. Two reasons not to follow it. A seed would mean
    /// different deals depending on how big the library is, and would be an
    /// *error* for a small one, where dealer3's `-s` is a `u32` that always
    /// stands for something. And seeds here are small integers far more often
    /// than not — `-s 1`, `-s 2`, `-s 3` across a lesson set — which under a
    /// block skip puts three runs in three adjacent blocks of a file whose
    /// records are in generated order. Mixing decorrelates them and costs
    /// nothing: one `SplitMix64` expansion and one draw.
    ///
    /// The RNG is dealer3's own, so a seed picks the same record in a browser
    /// as at a terminal.
    pub fn record(self, records: u64) -> u64 {
        match self {
            Start::Record(index) => index % records,
            Start::Seed(seed) => {
                Xoshiro256PlusPlus::seed_from_u64(seed as u64).next_u64() % records
            }
        }
    }

    /// What to say about starting there, if anything.
    ///
    /// Said for a seed always, because a run nobody can reproduce is worth a
    /// line, and for an ordinal only when it moved — record 0 is where a file
    /// starts anyway.
    fn note(self, start: u64, records: u64) -> Option<String> {
        match self {
            Start::Seed(seed) => Some(format!(
                "seed {} starts this library at record {} of {}; the same seed reads the same deals.",
                seed, start, records
            )),
            Start::Record(index) if index >= records => Some(format!(
                "record {} is past the end of a {}-record library; starting at record {}.",
                index, records, start
            )),
            Start::Record(0) => None,
            Start::Record(_) => Some(format!(
                "starting this library at record {} of {}.",
                start, records
            )),
        }
    }
}

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
///
/// `window` says which part of a library to read; it is [`Window::all`] for a
/// caller that wants the file as it stands. Text is read entire whatever the
/// window says, bar its limit — see [`Window`].
pub fn read(source: &str, window: Window) -> Result<(Vec<InputDeal>, InputReport), String> {
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
        return read_named(&bytes, "standard input", window);
    }

    // A named library is streamed rather than read into memory to be sniffed:
    // the published one is 241 MB, and its head settles the question.
    let library = path_holds_library(source)?;
    let mut read = if library {
        read_library(source, window)?
    } else {
        let bytes = std::fs::read(source)
            .map_err(|e| format!("opening input deals file '{}': {}", source, e))?;
        read_named(&bytes, &format!("'{}'", source), window)?
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
pub fn read_bytes(bytes: &[u8], window: Window) -> Result<(Vec<InputDeal>, InputReport), String> {
    read_named(bytes, "the supplied data", window)
}

/// [`read_bytes`], with a name for the bytes so an error can say where they came
/// from. Nothing else differs between the two, and nothing else may.
fn read_named(
    bytes: &[u8],
    what: &str,
    window: Window,
) -> Result<(Vec<InputDeal>, InputReport), String> {
    if looks_like_library(bytes) {
        // `Cursor` is `Read + Seek`, which is all `ZrdReader` ever wanted, so a
        // library that came down a pipe needs no decoding of its own.
        let reader = ZrdReader::new(std::io::Cursor::new(bytes))
            .map_err(|e| format!("reading {} as a library: {}", what, e))?;
        return Ok(read_records(reader, window));
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

    let mut read = match read_pbn(text) {
        Some(read) => read,
        None => read_lines(text),
    };
    narrow_text(&mut read.0, &mut read.1, what, window);
    Ok(read)
}

/// Apply what of a window a text file can honour, and say so about the rest.
///
/// A limit is honoured: the deals are in hand, and keeping only the first `n`
/// is what "read only what the run will use" comes to for a format that has to
/// be read to be counted. A start is not: there is no record to seek to, and a
/// seed picking a starting board in a forty-board PBN file would rotate a file
/// somebody assembled in the order they meant.
///
/// So a seed goes quietly — it is the run's seed, not something the caller
/// asked of this file — and a named record says out loud that it did nothing.
fn narrow_text(deals: &mut Vec<InputDeal>, report: &mut InputReport, what: &str, window: Window) {
    if let Start::Record(index) = window.start {
        if index > 0 {
            report.notes.push(format!(
                "{} is not a Pavlicek library, so there is no record {} to start at; \
                 it was read from the beginning.",
                what, index
            ));
        }
    }
    if let Some(limit) = window.take.limit() {
        if deals.len() > limit {
            deals.truncate(limit);
            recount(deals, report);
        }
    }
}

/// Put `solved` and `unsolved` back in step with the deals that survived.
///
/// The two counts describe what the caller is being handed, not what the file
/// held, so anything that adds or removes deals owes them a recount.
fn recount(deals: &[InputDeal], report: &mut InputReport) {
    report.solved = deals.iter().filter(|read| read.table.is_some()).count();
    report.unsolved = deals.len() - report.solved;
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

/// Read `window` of a ZRD library at a path, with the tables that came along.
///
/// Streamed, because the published library is 241 MB and a path is the one
/// source that need not be held in memory to be read. The window is why that
/// matters: a run wanting forty deals out of ten million records now reads
/// forty-odd records, not ten million.
pub fn read_library(path: &str, window: Window) -> Result<(Vec<InputDeal>, InputReport), String> {
    let reader =
        ZrdReader::open(path).map_err(|e| format!("opening input deals file '{}': {}", path, e))?;
    Ok(read_records(reader, window))
}

/// The window's records of an open library, in file order from its start.
///
/// Returns the deals with their tables or `None`, and a report of what else was
/// in there. Unreadable records are skipped rather than fatal — a library is
/// millions of records and one bad one should not cost the rest — but they are
/// all named in the report so a caller can decide.
///
/// ## The file is a ring
///
/// Reading runs from the window's first record to the end and then round to it
/// again, so a start near the end of the file still yields a whole library
/// rather than a tail. One pass at most: coming back to where it began is
/// where a pass stops, whatever the limit says, because everything past that
/// point is a deal this run has already seen.
///
/// [`Take::Exactly`] is the one caller that wants more than a pass, and it
/// repeats what the pass found rather than reading the file again — the same
/// records would come back, at the price of reading them twice.
///
/// Generic over the source because that is the only difference between a
/// library on disk and one that came down a pipe: the pipe's bytes arrive in a
/// `Cursor`, and everything after that is the same decoding. Two copies of this
/// loop would be two answers to what a separator is.
fn read_records<R: std::io::Read + std::io::Seek>(
    mut reader: ZrdReader<R>,
    window: Window,
) -> (Vec<InputDeal>, InputReport) {
    let mut deals = Vec::new();
    let mut report = InputReport {
        format: "zrd",
        ..Default::default()
    };

    let records = reader.len();
    if records == 0 {
        return (deals, report);
    }

    let start = window.start.record(records);
    if let Some(note) = window.start.note(start, records) {
        report.notes.push(note);
    }
    let limit = window.take.limit();

    for step in 0..records {
        if limit.is_some_and(|want| deals.len() >= want) {
            break;
        }
        // The offset counts records rather than deals, which is what makes it
        // a seek: `index` is the ordinal the file itself numbers by, and a
        // separator spends one of them.
        let index = (start + step) % records;
        match reader.record(index) {
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

    if let Take::Exactly(want) = window.take {
        repeat_to(&mut deals, &mut report, want);
    }

    (deals, report)
}

/// Go round the library again until `want` deals are in hand, and say so.
///
/// The repeats are the deals already read, in the order they were read: going
/// back to the file would return the same records at the price of reading them
/// twice.
///
/// ## Why this is worth a note
///
/// A run that wraps sees some deals more than once, and `average` and
/// `frequency` count each sighting. That is a sample with replacement drawn in
/// a correlated order — fine for "deal me hands to look at", misleading for
/// "what fraction of deals are like this". Neither this module nor the run can
/// tell which of the two the caller wanted, so it says what happened and lets
/// them judge.
///
/// A library with no deals in it is left alone: repeating nothing yields
/// nothing, and looping to find that out would never end.
fn repeat_to(deals: &mut Vec<InputDeal>, report: &mut InputReport, want: usize) {
    let pass = deals.len();
    if pass == 0 || pass >= want {
        return;
    }

    report.notes.push(format!(
        "the library holds {} deals and the run asked for {}; deals repeat from the start \
         after the first pass, so `average` and `frequency` count the repeats.",
        pass, want
    ));

    while deals.len() < want {
        // From the front each time, which is the same thing as carrying on
        // round the ring: every copy but the last is a whole pass.
        let more = (want - deals.len()).min(pass);
        deals.extend_from_within(..more);
    }
    recount(deals, report);
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

        let (deals, report) =
            read_bytes(LIBRARY, Window::all()).expect("a library should read from bytes");

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

        let (deals, report) =
            read_bytes(text.as_bytes(), Window::all()).expect("text should read as text");
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

        let (deals, report) =
            read_bytes(pbn.as_bytes(), Window::all()).expect("PBN should read from bytes");
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

        let error =
            read_bytes(truncated, Window::all()).expect_err("a part-record library cannot be read");
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

        let (deals, report) =
            read(source, Window::all()).expect("the content is a library whatever the name");
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

        let (deals, report) =
            read(source, Window::all()).expect("the content is text whatever the name");
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

        let (from_path, path_report) = read(source, Window::all()).expect("read from a path");
        let (from_bytes, bytes_report) =
            read_bytes(LIBRARY, Window::all()).expect("read from bytes");

        assert_eq!(path_report, bytes_report);
        assert_eq!(as_pbn(&from_path), as_pbn(&from_bytes));

        let _ = std::fs::remove_file(&path);
    }

    /// The whole fixture in file order, as PBN: what a window is measured
    /// against. Read through the same door as everything else, so a change to
    /// the decoding moves the expectation with it rather than breaking it.
    fn library_in_order() -> Vec<String> {
        let (deals, _) = read_bytes(LIBRARY, Window::all()).expect("the fixture reads");
        as_pbn(&deals)
    }

    /// One record with four zero bytes where the cards go: a separator.
    ///
    /// Synthesised rather than found, because Pavlicek's first ten records have
    /// none in them, and what needs testing is a record that is not a deal
    /// sitting between records that are.
    const SEPARATOR: [u8; RECORD_LEN] = [0u8; RECORD_LEN];

    #[test]
    fn an_offset_starts_at_that_record_and_comes_round() {
        let ordered = library_in_order();

        let (deals, report) = read_bytes(
            LIBRARY,
            Window {
                start: Start::Record(7),
                take: Take::Pass,
            },
        )
        .expect("a library reads from an offset");

        assert_eq!(deals.len(), 10, "a pass is still the whole library");
        let rotated: Vec<String> = ordered[7..]
            .iter()
            .chain(ordered[..7].iter())
            .cloned()
            .collect();
        assert_eq!(
            as_pbn(&deals),
            rotated,
            "reading should run from record 7 to the end and round to the start"
        );
        assert_eq!(report.solved, 10);
        assert!(
            report.notes.iter().any(|n| n.contains("record 7 of 10")),
            "where it started is worth saying: {:?}",
            report.notes
        );
    }

    #[test]
    fn a_limit_reads_only_the_deals_it_asks_for() {
        let ordered = library_in_order();

        let (deals, report) = read_bytes(
            LIBRARY,
            Window {
                start: Start::Record(4),
                take: Take::AtMost(3),
            },
        )
        .expect("a library reads a window");

        assert_eq!(as_pbn(&deals), ordered[4..7], "three deals from record 4");
        assert_eq!(
            report.solved, 3,
            "the counts describe what was handed over, not what the file holds"
        );
        assert_eq!(report.unsolved, 0);
    }

    #[test]
    fn a_ceiling_larger_than_the_library_stops_after_one_pass() {
        // `-g` is a ceiling on what a run will look at, not a demand for that
        // many deals — and it is ten million by default, which is no reason to
        // repeat a ten-record library a million times.
        let (deals, report) = read_bytes(
            LIBRARY,
            Window {
                start: Start::Record(0),
                take: Take::AtMost(25),
            },
        )
        .expect("a library reads");

        assert_eq!(deals.len(), 10, "one pass, and no deal read twice");
        assert!(
            !report.notes.iter().any(|note| note.contains("repeat")),
            "nothing repeated, so nothing to report: {:?}",
            report.notes
        );
    }

    #[test]
    fn asking_for_more_deals_than_the_library_holds_repeats_them_and_says_so() {
        let ordered = library_in_order();

        let (deals, report) = read_bytes(
            LIBRARY,
            Window {
                start: Start::Record(0),
                take: Take::Exactly(25),
            },
        )
        .expect("a library reads");

        assert_eq!(deals.len(), 25, "the run asked for 25 and gets 25");
        let pbn = as_pbn(&deals);
        assert_eq!(pbn[..10], ordered[..], "the first pass is the library");
        assert_eq!(pbn[10..20], ordered[..], "and the second is the same again");
        assert_eq!(pbn[20..], ordered[..5], "the last pass is a part of one");
        assert_eq!(
            report.solved, 25,
            "the counts describe the deals handed over, repeats and all"
        );
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.contains("holds 10 deals and the run asked for 25")),
            "a run whose statistics count deals twice should say so: {:?}",
            report.notes
        );
    }

    #[test]
    fn the_seed_picks_the_start_and_picks_the_same_one_every_time() {
        let by_seed = |seed: u32| {
            let (deals, _) = read_bytes(
                LIBRARY,
                Window {
                    start: Start::Seed(seed),
                    take: Take::Pass,
                },
            )
            .expect("a library reads from a seed");
            as_pbn(&deals)
        };

        assert_eq!(by_seed(1), by_seed(1), "the same seed reads the same deals");

        // And the seed has to reach the offset at all: eight seeds that all
        // started at the same record would satisfy the line above while
        // ignoring the seed completely.
        let starts: std::collections::BTreeSet<String> =
            (1u32..=8).map(|seed| by_seed(seed)[0].clone()).collect();
        assert!(
            starts.len() > 1,
            "different seeds should start in different places: {:?}",
            starts
        );
    }

    #[test]
    fn an_offset_counts_records_and_a_limit_counts_deals() {
        // The two count different things, which is the whole of the separator
        // question: a separator is a record, so it spends an offset, and it is
        // not a deal, so it does not spend a limit.
        let ordered = library_in_order();
        let mut spliced = SEPARATOR.to_vec();
        spliced.extend_from_slice(LIBRARY);

        let (deals, report) = read_bytes(
            &spliced,
            Window {
                start: Start::Record(3),
                take: Take::AtMost(2),
            },
        )
        .expect("a library with a separator in it reads");
        assert_eq!(
            as_pbn(&deals),
            ordered[2..4],
            "record 3 is the third deal, because record 0 is the separator"
        );
        assert_eq!(report.separators, 0, "the separator is behind the start");

        let (deals, report) = read_bytes(
            &spliced,
            Window {
                start: Start::Record(0),
                take: Take::AtMost(2),
            },
        )
        .expect("a library with a separator in it reads");
        assert_eq!(
            as_pbn(&deals),
            ordered[..2],
            "a separator does not spend the limit"
        );
        assert_eq!(report.separators, 1, "but it is still counted");
    }

    #[test]
    fn a_library_with_no_deals_in_it_is_not_repeated_for_ever() {
        // Repeating nothing yields nothing, and a loop looking for the deals it
        // was asked for would never come back.
        let separators = SEPARATOR.repeat(3);

        let (deals, report) = read_bytes(
            &separators,
            Window {
                start: Start::Record(0),
                take: Take::Exactly(100),
            },
        )
        .expect("a library of separators is still a library");

        assert!(deals.is_empty(), "there were no deals to read");
        assert_eq!(report.separators, 3);
    }

    #[test]
    fn text_honours_a_limit_and_says_a_start_did_nothing() {
        let text = ONELINE.repeat(4);

        let (deals, report) = read_bytes(
            text.as_bytes(),
            Window {
                start: Start::Record(2),
                take: Take::AtMost(3),
            },
        )
        .expect("text reads");

        assert_eq!(report.format, "lines");
        assert_eq!(deals.len(), 3, "the limit is what a run will spend");
        assert_eq!(report.unsolved, 3, "and the counts follow it");
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.contains("no record 2 to start at")),
            "a start there is nothing to seek in should say so: {:?}",
            report.notes
        );
    }

    #[test]
    fn a_tables_only_file_is_still_refused_by_name() {
        let error = read("deals.zdd", Window::all()).expect_err(".zdd holds no deals");
        assert!(error.contains("no deals in it"), "{}", error);
    }
}
