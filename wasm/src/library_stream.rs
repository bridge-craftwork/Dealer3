//! The solved-deal library as a run reads it: fetched a slice at a time (#21).
//!
//! A run over the library used to be handed its deals up front, which meant
//! deciding in advance how many to fetch — and a cap on that, so a large `Max
//! generate` could not pull megabytes to find twenty deals. The cap then cut
//! short exactly the runs that needed more: a selective filter read one slice,
//! found a few hundred matches, and stopped with the settings asking for far
//! more.
//!
//! So the run pulls instead. It asks for a deal; when the slice in hand is
//! spent, this fetches the next one, decodes it through the reader every other
//! input goes through, and lets the fetched bytes go. A run that finds its
//! matches in the first slice fetches one slice, as before; one that needs the
//! whole library reads the whole library, holding a slice at a time.
//!
//! # Why fetching is a callback, and a synchronous one
//!
//! A run is one synchronous call, and a deal is asked for in the middle of it —
//! there is nowhere to await a `fetch`. The page runs the engine in a worker,
//! where a synchronous request is allowed, and hands one in. The pieces are
//! published `immutable`, so the browser's own cache answers a repeat.
//!
//! The fetch is generic rather than a `js_sys::Function` so a test can serve
//! the library from a fixture.

use std::collections::VecDeque;

use dealer_run::deal_input::{self, InputDeal, InputReport, LibrarySpan, Start, Window};
use dealer_run::run::SolvedDeal;
use rpdd_reader::Library as Chunked;

/// Deals read per fetch: one published piece. A slice runs to the end of the
/// piece it starts in, so after the first every slice is exactly one piece and
/// one request, and no more than one piece is ever held.
const SLICE: u64 = 65_536;

/// How many ask/supply rounds before something is wrong. The protocol takes
/// two — the manifest, then the pieces it names — and a fourth means the library
/// is asking for something it is never satisfied by.
const MAX_ROUNDS: usize = 4;

/// The published library, read a slice at a time as the run asks for deals.
pub(crate) struct LibraryDeals<F> {
    library: Chunked,
    fetch: F,
    seed: u32,
    /// Deals per slice: [`SLICE`], bar a test serving a library cut smaller.
    slice_deals: u64,
    /// Where this run starts and how many deals the library holds, once the
    /// manifest has said.
    start: Option<(u64, u64)>,
    /// Records read so far, which is how far round the library the run has got.
    read: u64,
    /// The current slice's deals not yet handed over.
    slice: VecDeque<InputDeal>,
    report: InputReport,
}

impl<F: FnMut(&str) -> Result<Vec<u8>, String>> LibraryDeals<F> {
    /// The library described by the manifest at `manifest_url`, starting where
    /// `seed` says. Nothing is fetched until the run asks for a deal.
    pub(crate) fn new(manifest_url: &str, seed: u32, fetch: F) -> Self {
        Self::sliced(manifest_url, seed, fetch, SLICE)
    }

    /// The same, reading `slice_deals` at a time: what a library published in
    /// smaller pieces lines its slices up with.
    fn sliced(manifest_url: &str, seed: u32, fetch: F, slice_deals: u64) -> Self {
        LibraryDeals {
            library: Chunked::at(manifest_url),
            fetch,
            seed,
            slice_deals,
            start: None,
            read: 0,
            slice: VecDeque::new(),
            report: InputReport {
                format: "zrd",
                ..Default::default()
            },
        }
    }

    /// Where the run starts, and how big the library is.
    ///
    /// The manifest and nothing else: the size is what the seed is reduced
    /// against, and only then is there a deal to say which piece to fetch.
    /// Asking `needs` for deal 0 would name that deal's piece as well, a 640 KiB
    /// fetch chosen by an index nobody has worked out yet.
    fn start(&mut self) -> Result<(u64, u64), String> {
        if let Some(start) = self.start {
            return Ok(start);
        }
        for _ in 0..MAX_ROUNDS {
            if self.library.total_deals().is_some() {
                break;
            }
            let needed = self.library.needs(0, 1).map_err(|e| e.to_string())?;
            let Some(url) = needed.first() else { break };
            let bytes = (self.fetch)(url)?;
            self.library.supply(url, bytes).map_err(|e| e.to_string())?;
        }
        let total = self
            .library
            .total_deals()
            .filter(|n| *n > 0)
            .ok_or_else(|| {
                format!(
                    "The library at {} did not say how many deals it holds.",
                    self.library.manifest_url()
                )
            })?;
        // The engine's mapping, and the command line's: `dealer -s N
        // --input-deals rpdd.zrd` starts at this same record.
        let first = Start::Seed(self.seed).record(total);
        self.report.library = Some(LibrarySpan {
            first_record: first,
            records: total,
            read_whole: false,
        });
        self.start = Some((first, total));
        Ok((first, total))
    }

    /// Fetch every piece `count` deals from `first` need.
    fn supply(&mut self, first: u64, count: u64) -> Result<(), String> {
        for _ in 0..MAX_ROUNDS {
            let needed = self
                .library
                .needs(first, count)
                .map_err(|e| e.to_string())?;
            if needed.is_empty() {
                return Ok(());
            }
            for url in needed {
                let bytes = (self.fetch)(&url)?;
                self.library
                    .supply(&url, bytes)
                    .map_err(|e| e.to_string())?;
            }
        }
        Err(
            "The solved-deal library kept asking for more pieces than it could use; nothing was \
             read. This is a bug — please report it."
                .to_string(),
        )
    }

    /// Read the next slice into hand. False once the run has been all the way
    /// round, which is where a run over the library stops: past there every
    /// deal is one it has already seen.
    fn next_slice(&mut self) -> Result<bool, String> {
        let (first, total) = self.start()?;
        if self.read >= total {
            return Ok(false);
        }
        let at = (first + self.read) % total;
        let count = (self.slice_deals - at % self.slice_deals).min(total - self.read);
        self.supply(at, count)?;
        let bytes = self.library.zrd(at, count).map_err(|e| e.to_string())?;
        // The tables are in the bytes now, so the pieces can go. This is what
        // holds a run over the whole library to a slice.
        self.library.forget_chunks();

        // The reader every other input goes through: a slice of the library is
        // a `.zrd`, and there is one answer to what is in one.
        let (deals, found) = deal_input::read_bytes(&bytes, Window::all())?;
        self.report.solved += found.solved;
        self.report.unsolved += found.unsolved;
        self.report.separators += found.separators;
        self.report.skipped.extend(found.skipped);
        self.read += count;
        if self.read >= total {
            if let Some(span) = self.report.library.as_mut() {
                span.read_whole = true;
            }
        }
        self.slice.extend(deals);
        Ok(true)
    }
}

impl<F: FnMut(&str) -> Result<Vec<u8>, String>> dealer_run::DealStream for LibraryDeals<F> {
    fn next_deal(&mut self) -> Result<Option<SolvedDeal>, String> {
        loop {
            if let Some(deal) = self.slice.pop_front() {
                return Ok(Some(dealer_run::solved(deal)));
            }
            if !self.next_slice()? {
                return Ok(None);
            }
        }
    }

    /// The published library is solved throughout, and saying so before the
    /// first slice arrives is what carries its answers from the start.
    fn carries_tables(&self) -> bool {
        true
    }

    fn report(&self) -> InputReport {
        self.report.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dealer_run::DealStream;
    use rpdd_reader::{RECORD_LEN, TABLE_LEN};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// The first ten records of Pavlicek's library — real deals with their real
    /// tables — served as the published library would serve them: tables only,
    /// in pieces, beside a manifest.
    const LIBRARY: &[u8] = include_bytes!("../../dealer-run/tests/fixtures/rpdd_10First.zrd");
    const DEAL_LEN: usize = RECORD_LEN - TABLE_LEN;
    const MANIFEST_URL: &str = "https://example.invalid/data/manifest.json";
    const PER_PIECE: usize = 5;

    fn piece(n: usize) -> Vec<u8> {
        LIBRARY
            .as_chunks::<RECORD_LEN>()
            .0
            .iter()
            .skip(n * PER_PIECE)
            .take(PER_PIECE)
            .flat_map(|record| record[DEAL_LEN..].iter().copied())
            .collect()
    }

    fn manifest() -> Vec<u8> {
        format!(
            r#"{{"schema":1,"record_bytes":{TABLE_LEN},"deals_per_chunk":{PER_PIECE},
                 "total_deals":10,"chunks":[
                   {{"file":"zdd/a.zdd","first_deal":0,"deals":{PER_PIECE}}},
                   {{"file":"zdd/b.zdd","first_deal":{PER_PIECE},"deals":{PER_PIECE}}}]}}"#
        )
        .into_bytes()
    }

    /// The URLs a stream fetched, in order.
    type Fetched = Rc<RefCell<Vec<String>>>;

    /// A fetch serving the fixture.
    type Fetch = Box<dyn FnMut(&str) -> Result<Vec<u8>, String>>;

    /// A stream over the fixture, and the URLs it fetched, in order.
    fn stream(seed: u32) -> (LibraryDeals<Fetch>, Fetched) {
        let fetched = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&fetched);
        let fetch: Fetch = Box::new(move |url: &str| {
            log.borrow_mut().push(url.to_string());
            Ok(match url {
                MANIFEST_URL => manifest(),
                "https://example.invalid/data/zdd/a.zdd" => piece(0),
                "https://example.invalid/data/zdd/b.zdd" => piece(1),
                other => return Err(format!("asked for an unpublished URL: {other}")),
            })
        });
        // Sliced as the fixture is published, as the real library's slices are.
        (
            LibraryDeals::sliced(MANIFEST_URL, seed, fetch, PER_PIECE as u64),
            fetched,
        )
    }

    fn drain(stream: &mut impl DealStream) -> Vec<SolvedDeal> {
        let mut deals = Vec::new();
        while let Some(deal) = stream.next_deal().expect("the fixture serves") {
            deals.push(deal);
        }
        deals
    }

    /// The whole claim: pulled a slice at a time, the deals are the library's
    /// own, in the order a seed says, once round and no further — the same
    /// deals the command line reads from the whole file with the same seed.
    #[test]
    fn a_stream_reads_the_library_once_round_from_where_the_seed_says() {
        use dealer_run::deal_input::Take;
        for seed in [0u32, 1, 7, 42] {
            let (mut library, _) = stream(seed);
            let pulled = drain(&mut library);
            let (whole, _) = dealer_run::deals_from_bytes(
                LIBRARY,
                Window {
                    start: Start::Seed(seed),
                    take: Take::Pass,
                },
            )
            .expect("the fixture is a readable library");
            assert_eq!(pulled.len(), 10, "seed {seed}: once round is ten deals");
            assert_eq!(
                pulled.iter().map(|(deal, _)| deal).collect::<Vec<_>>(),
                whole.iter().map(|(deal, _)| deal).collect::<Vec<_>>(),
                "seed {seed}: the stream's deals are not the file's, from the same seed"
            );
        }
    }

    #[test]
    fn every_deal_arrives_solved_and_the_report_says_how_far_it_got() {
        let (mut library, _) = stream(3);
        let pulled = drain(&mut library);
        assert!(
            pulled.iter().all(|(_, known)| !known.is_empty()),
            "a deal arrived without its table, so tricks() would solve it"
        );
        let report = library.report();
        assert_eq!(report.solved, 10);
        let span = report.library.expect("a library run says where it started");
        assert_eq!(span.records, 10);
        assert!(span.read_whole, "it went all the way round");
    }

    /// Nothing is fetched until a deal is wanted, and the manifest comes first
    /// on its own: a piece cannot be chosen before the seed has somewhere to
    /// start, and the size that decides that is in the manifest.
    #[test]
    fn the_manifest_is_fetched_first_and_alone() {
        let (mut library, fetched) = stream(0);
        assert!(fetched.borrow().is_empty(), "made, and already fetching");
        library.next_deal().expect("serves").expect("a deal");
        let fetched = fetched.borrow();
        assert_eq!(fetched.first().map(String::as_str), Some(MANIFEST_URL));
        assert_eq!(
            fetched.iter().filter(|url| *url == MANIFEST_URL).count(),
            1,
            "the manifest was fetched more than once: {fetched:?}"
        );
    }

    /// A run that stops early fetches only what it read.
    #[test]
    fn a_run_wanting_one_deal_fetches_one_piece() {
        let (mut library, fetched) = stream(0);
        library.next_deal().expect("serves").expect("a deal");
        assert_eq!(
            fetched.borrow().len(),
            2,
            "the manifest and one piece: {:?}",
            fetched.borrow()
        );
    }

    #[test]
    fn a_failed_fetch_ends_the_stream_with_its_reason() {
        let fetch = |_: &str| Err("Could not reach the solved-deal library".to_string());
        let mut library = LibraryDeals::new(MANIFEST_URL, 0, fetch);
        let error = library.next_deal().expect_err("nothing could be fetched");
        assert!(error.contains("Could not reach"), "{error}");
    }
}
