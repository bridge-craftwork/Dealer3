//! The published solved-deal library, fetched a piece at a time.
//!
//! A page that wants `tricks()`, `dds()` or `par()` to be a lookup rather than
//! a search needs deals that arrive with their double-dummy tables. Those exist
//! — 10,485,760 of them, solved by Richard Pavlicek — but the file is 241 MB,
//! which is not a download a tab can make to look at five deals.
//!
//! So the tables are published as 640 KiB pieces and the deals are not
//! published at all: they are a pure function of their index, and `rpdd-reader`
//! recreates them here, in the browser, at about 640ns each. This module is the
//! binding for that crate. It adds no arithmetic of its own — which piece holds
//! deal N, how a run crossing a boundary is stitched, how one past the end
//! wraps — all of that is the crate's, derived from a manifest.
//!
//! # Why the caller does the fetching
//!
//! `fetch` is asynchronous and this is synchronous Rust called from JavaScript,
//! so a wasm export cannot await one. The library therefore says what it needs
//! and is told:
//!
//! ```js
//! const lib = new Library(rpdd_manifest_url())
//! let need
//! while ((need = lib.needs(index, count)).length)
//!     for (const url of need) lib.supply(url, new Uint8Array(await (await fetch(url)).arrayBuffer()))
//! const out = run_json(envelope, lib.zrd(index, count))
//! ```
//!
//! The first round asks for the manifest and the second for the chunks it
//! names; the caller fetches URLs and never learns which is which.
//!
//! # Why it hands back `.zrd` bytes
//!
//! Because [`crate::run_json`] already reads them, unchanged. The
//! same bytes, the same reader and the same run as `--input-deals` at a
//! terminal — so a library run in a tab and a library run in a shell cannot
//! come to different conclusions about what they read.
//!
//! An entry point taking deals and tables directly would be a second run path.
//! The two front ends each had one of those until 2026-08-29; they drifted, and
//! nothing said so until someone compared them.

use rpdd_reader::{Library as Chunked, LibraryError};
use wasm_bindgen::prelude::*;

/// The manifest of the library published by rpdd-library.
///
/// A page may point [`Library`] at any manifest of the same shape; this is the
/// one we host, offered so a caller does not have to hold the URL itself.
#[wasm_bindgen]
pub fn rpdd_manifest_url() -> String {
    rpdd_reader::RPDD_MANIFEST.to_string()
}

/// Which record a run's seed starts at, in a library of `records` records.
///
/// This is the *whole* of what a page needs to turn its seed box into a
/// position in the library — and it is deliberately not arithmetic a page may
/// do for itself. The mapping is [`dealer_run::deal_input::Start::Seed`]'s: one
/// expansion of `SplitMix64` and one draw of xoshiro256++. A JavaScript hash
/// that looked every bit as reasonable would land somewhere else, and nothing
/// would fail — the page and the terminal would simply read different deals
/// from the same seed, and only someone comparing the two would ever find out.
///
/// So there is one implementation with two callers, as with the reader and the
/// run. `wasm/verify.mjs` checks the agreement rather than trusting it.
///
/// `records` is the size of the library being read — [`Library::total_deals`]
/// for the published one — because the seed names a position in the file it was
/// given. A partial library answers differently, and correctly so.
#[wasm_bindgen]
pub fn record_for_seed(seed: u32, records: u32) -> Result<u32, JsError> {
    if records == 0 {
        return Err(JsError::new(
            "an empty library has no record for a seed to start at",
        ));
    }
    // Fits: `records` is a u32, so the remainder of a division by it is one too.
    Ok(record_of(seed, records as u64) as u32)
}

/// The mapping itself, callable from an ordinary test.
///
/// Split out for the same reason as [`zrd_bytes`]: a `JsError` in the signature
/// is a `JsError` a host test cannot handle.
fn record_of(seed: u32, records: u64) -> u64 {
    dealer_run::deal_input::Start::Seed(seed).record(records)
}

/// A solved-deal library, described by a manifest and holding the pieces it has
/// been given.
///
/// See the [module documentation](self) for the ask/supply loop.
#[wasm_bindgen]
pub struct Library {
    inner: Chunked,
}

#[wasm_bindgen]
impl Library {
    /// A library described by the manifest at `manifest_url`.
    ///
    /// Nothing is fetched — nothing here can fetch. The first call to
    /// [`needs`](Self::needs) asks for the manifest.
    #[wasm_bindgen(constructor)]
    pub fn new(manifest_url: String) -> Library {
        Library {
            inner: Chunked::at(manifest_url),
        }
    }

    /// The manifest URL this library was pointed at.
    #[wasm_bindgen(getter)]
    pub fn manifest_url(&self) -> String {
        self.inner.manifest_url().to_string()
    }

    /// How many deals the library holds, or `undefined` until the manifest has
    /// been supplied.
    #[wasm_bindgen(getter)]
    pub fn total_deals(&self) -> Option<u32> {
        self.inner.total_deals().map(|n| n as u32)
    }

    /// The URLs still needed before [`zrd`](Self::zrd) can answer, in the order
    /// worth fetching them.
    ///
    /// An empty array means the run can be produced. Call it in a loop: the
    /// manifest comes back on the first round and the chunks it names on the
    /// second.
    ///
    /// `first_deal` past the end of the library wraps to its beginning, so any
    /// index answers and a page may derive one from a seed.
    pub fn needs(&self, first_deal: u32, count: u32) -> Result<Vec<String>, JsError> {
        self.inner
            .needs(first_deal as u64, count as u64)
            .map_err(explain)
    }

    /// Hand back the bytes fetched for a URL [`needs`](Self::needs) returned.
    ///
    /// The manifest and a chunk arrive by the same call; which one it is comes
    /// from the URL.
    pub fn supply(&mut self, url: &str, bytes: &[u8]) -> Result<(), JsError> {
        self.inner.supply(url, bytes.to_vec()).map_err(explain)
    }

    /// `count` deals from `first_deal`, each with its double-dummy table, as
    /// `.zrd` bytes to hand to [`crate::run_json`].
    ///
    /// Fails while anything is still missing; ask [`needs`](Self::needs) first.
    pub fn zrd(&self, first_deal: u32, count: u32) -> Result<Vec<u8>, JsError> {
        zrd_bytes(&self.inner, first_deal as u64, count as u64).map_err(|e| JsError::new(&e))
    }

    /// Drop every chunk held, keeping the manifest.
    ///
    /// A page walking a long way through the library would otherwise keep every
    /// piece it passed through.
    pub fn forget_chunks(&mut self) {
        self.inner.forget_chunks();
    }
}

/// The run, or why it cannot be produced yet.
///
/// Split out so it can be called from an ordinary test: `JsError::new` reaches
/// for JavaScript's `Error`, which does not exist off wasm, so a failure raised
/// through it would abort the test process rather than be something a test can
/// assert on.
fn zrd_bytes(library: &Chunked, first_deal: u64, count: u64) -> Result<Vec<u8>, String> {
    library.zrd(first_deal, count).map_err(|e| explanation(&e))
}

/// Turn a library error into a `JsError`, with `Needs` saying what to do.
fn explain(error: LibraryError) -> JsError {
    JsError::new(&explanation(&error))
}

/// The message a page should see.
///
/// `Needs` is not a failure — it is the protocol — so if it surfaces as one the
/// caller has skipped [`Library::needs`], and the message says so rather than
/// listing URLs it did not ask for.
fn explanation(error: &LibraryError) -> String {
    match error {
        LibraryError::Needs(urls) => format!(
            "{} piece(s) of the library have not been supplied yet; call needs() \
             and supply() them first",
            urls.len()
        ),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rpdd_reader::{pair, RECORD_LEN, TABLE_LEN};

    /// The first ten records of Pavlicek's library — real deals with their real
    /// tables — which is what the whole chain has to reproduce from tables
    /// alone. The same fixture `dealer-run` and the bindings' other tests read.
    const LIBRARY: &[u8] = include_bytes!("../../dealer-run/tests/fixtures/rpdd_10First.zrd");

    const DEAL_LEN: usize = RECORD_LEN - TABLE_LEN;
    const MANIFEST_URL: &str = "https://example.invalid/data/manifest.json";
    const PER_CHUNK: usize = 5;

    /// The fixture's records, as a server would hold them: tables only, in two
    /// chunks of five.
    fn chunk(n: usize) -> Vec<u8> {
        LIBRARY
            .as_chunks::<RECORD_LEN>()
            .0
            .iter()
            .skip(n * PER_CHUNK)
            .take(PER_CHUNK)
            .flat_map(|record| record[DEAL_LEN..].iter().copied())
            .collect()
    }

    fn manifest() -> Vec<u8> {
        format!(
            r#"{{"schema":1,"record_bytes":{TABLE_LEN},"deals_per_chunk":{PER_CHUNK},
                 "total_deals":10,"chunks":[
                   {{"file":"zdd/a.zdd","first_deal":0,"deals":{PER_CHUNK}}},
                   {{"file":"zdd/b.zdd","first_deal":{PER_CHUNK},"deals":{PER_CHUNK}}}]}}"#
        )
        .into_bytes()
    }

    /// Run the protocol to completion, serving from the fixture.
    fn served(first_deal: u64, count: u64) -> Vec<u8> {
        let mut library = Chunked::at(MANIFEST_URL);
        loop {
            match zrd_bytes(&library, first_deal, count) {
                Ok(bytes) => return bytes,
                Err(_) => {
                    let needed = library
                        .needs(first_deal, count)
                        .expect("the manifest describes a usable library");
                    assert!(!needed.is_empty(), "asked for nothing and still not ready");
                    for url in needed {
                        let bytes = match url.as_str() {
                            MANIFEST_URL => manifest(),
                            "https://example.invalid/data/zdd/a.zdd" => chunk(0),
                            "https://example.invalid/data/zdd/b.zdd" => chunk(1),
                            other => panic!("asked for an unpublished URL: {other}"),
                        };
                        library
                            .supply(&url, bytes)
                            .expect("supplying what was asked");
                    }
                }
            }
        }
    }

    /// The chain's whole claim: tables fetched in pieces, deals made here, and
    /// the result is the library's own records byte for byte — across a chunk
    /// boundary, which is the join a page cannot see and must not notice.
    #[test]
    fn a_run_across_two_chunks_is_the_librarys_own_records() {
        let served = served(3, 5);
        assert_eq!(
            served,
            &LIBRARY[3 * RECORD_LEN..8 * RECORD_LEN],
            "deals 3..8 served from two chunks are not the library's records"
        );
    }

    /// And a run that reaches the end continues from the beginning.
    #[test]
    fn a_run_past_the_end_wraps() {
        let served = served(9, 2);
        let expected = [
            &LIBRARY[9 * RECORD_LEN..10 * RECORD_LEN],
            &LIBRARY[..RECORD_LEN],
        ]
        .concat();
        assert_eq!(served, expected, "the wrap is not the library's records");
    }

    /// What the bindings exist for: the bytes go into the run unchanged, and
    /// the deals that come out are the library's, with their tables — so
    /// `tricks()` is a lookup and nothing is solved again.
    #[test]
    fn the_bytes_feed_the_existing_run() {
        let zrd = served(0, 10);
        let (supplied, report) =
            dealer_run::deals_from_bytes(&zrd, dealer_run::deal_input::Window::all())
                .expect("the run's own reader accepts what the library served");
        assert_eq!(supplied.len(), 10, "ten records should be ten deals");
        assert_eq!(
            report.solved, 10,
            "every deal should have arrived with its table, or tricks() is not a lookup"
        );
    }

    /// Pairing is what makes those bytes; served and paired directly must
    /// agree, or the chunk layer is slicing somewhere other than it says.
    #[test]
    fn serving_agrees_with_pairing_the_same_tables_directly() {
        let tables: Vec<u8> = [chunk(0), chunk(1)].concat();
        assert_eq!(served(0, 10), pair(&tables, 0).expect("ten records pair"));
    }

    /// The record a seed names is the record the reader starts at.
    ///
    /// The binding is three lines, so what wants testing is not the arithmetic
    /// but the tie: the deal a page derives from its seed must be the deal a
    /// run handed the same seed would begin with. Written against the reader
    /// rather than against `Start::record` — which the binding calls, and which
    /// would therefore agree with itself no matter what either of them meant.
    #[test]
    fn the_record_a_seed_names_is_where_the_reader_starts() {
        use dealer_run::deal_input::{Start, Take, Window};

        let (whole, _) = dealer_run::deals_from_bytes(LIBRARY, Window::all())
            .expect("the fixture is a readable library");
        let records = whole.len() as u64;

        for seed in [0u32, 1, 2, 42, 1_000_003, u32::MAX] {
            let record = record_of(seed, records);
            assert!(record < records, "seed {seed} names a record off the end");
            let (from_seed, _) = dealer_run::deals_from_bytes(
                LIBRARY,
                Window {
                    start: Start::Seed(seed),
                    take: Take::AtMost(1),
                },
            )
            .expect("the fixture is a readable library");
            assert_eq!(
                from_seed[0].0, whole[record as usize].0,
                "seed {seed} says record {record}, but the reader starts elsewhere"
            );
        }
    }

    /// Neighbouring seeds must not give neighbouring records. Mixing is the
    /// reason `-s 1`, `-s 2` and `-s 3` across a lesson set are not three
    /// adjacent windows of a file that is in generated order.
    #[test]
    fn adjacent_seeds_do_not_give_adjacent_records() {
        let records = 10_485_760;
        let (one, two) = (record_of(1, records), record_of(2, records));
        assert!(
            one.abs_diff(two) > 1_000,
            "seeds 1 and 2 land at {one} and {two}, which is not a mix"
        );
    }

    /// `Needs` is the protocol, not a failure, and must not reach a page as a
    /// list of URLs it never asked about.
    #[test]
    fn asking_for_a_run_before_supplying_anything_says_what_to_do() {
        let library = Chunked::at(MANIFEST_URL);
        let message = zrd_bytes(&library, 0, 1).expect_err("nothing is supplied yet");
        assert!(
            message.contains("needs()") && message.contains("supply()"),
            "unhelpful: {message}"
        );
    }
}
