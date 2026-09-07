//! What is known about a deal's double-dummy results, and how it travels.
//!
//! `tricks()` is orders of magnitude more expensive than every other function
//! in the language — a hundred milliseconds against hundreds of nanoseconds —
//! so what matters is not how fast one search is but how few of them run.
//! Three things would otherwise repeat work:
//!
//! - A script naming `tricks(south, spades)` twice — once in an `average` and
//!   again in a `frequency`, say — evaluates two separate expression nodes.
//!   The evaluator's variable cache does not help, since neither is a
//!   variable.
//! - A script asking about several denominations, or several declarers.
//! - Generation itself, which filters deals on worker threads and then works
//!   out the statistics for the matches on the main thread. A deal whose
//!   `condition` calls `tricks()` is therefore asked about twice, from two
//!   different threads, with a whole batch of other deals in between.
//!
//! The first two are covered by [`CURRENT`], the deal in hand on this thread,
//! which also holds the solver's caches — several megabytes, worth keeping
//! only for as long as the deal they belong to.
//!
//! The third used to be covered by a global table of 16,384 deals' answers,
//! keyed by their cards. That is gone. Answers now travel with their deal in a
//! [`DealTricks`], the same way a table read from a file does (#61): a worker
//! hands back what it learned along with the deal it learned it about, and the
//! main thread is holding the answers before it asks. Which is why a file's
//! table stopped being a special case — an answer from a ZRD record and an
//! answer a worker searched for arrive by the same route and are the same
//! thing.
//!
//! What a shared store did that this does not: answer about a deal nobody is
//! holding any more. Nothing wanted that. It kept tables for deals the filter
//! had rejected, it evicted from the front so a long input lost exactly the
//! answers it would need first, and a table outlived its deal by an amount
//! decided by a capacity constant.

use crate::{DealAnalysis, Denomination, KnownTricks};
use dealer_core::{Deal, Position};
use std::cell::RefCell;

/// A deal's exact identity: a bit per card, one mask per hand.
///
/// Deals are compared, never hashed down to something smaller, because two
/// different deals sharing an answer would be wrong rather than slow.
type Key = [u64; 4];

fn key(deal: &Deal) -> Key {
    let mut key = [0u64; 4];
    for (mask, position) in key.iter_mut().zip(Position::ALL) {
        for card in deal.hand(position).cards() {
            *mask |= 1 << card.to_index();
        }
    }
    key
}

/// What is known about one deal's twenty double-dummy results.
///
/// Partial by design. A deal read from a solved file knows all twenty; a deal
/// whose condition asked one question knows one; a freshly dealt deal knows
/// none. All three are this type, which is what lets the run carry answers
/// around without caring where they came from.
///
/// Cheap to copy — forty bytes — so it travels by value with its deal.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DealTricks {
    known: KnownTricks,
}

/// Nothing known, for a context built without a deal's answers to hand.
///
/// A static so that such a context can borrow one rather than own it, which is
/// what keeps the field a plain reference instead of an `Option`.
pub static NOTHING_KNOWN: DealTricks = DealTricks {
    known: [[None; 4]; 5],
};

impl DealTricks {
    /// Nothing known yet: a deal as dealt.
    pub fn nothing() -> Self {
        Self::default()
    }

    /// Everything known, from a table that arrived solved.
    ///
    /// The axis conversion happens here and in [`DealTricks::table`] and
    /// nowhere else. `DdTable` is keyed by `Direction` and `Strain`, this crate
    /// counts seats and denominations its own way, and getting either backwards
    /// returns a plausible number rather than an error — it negated a par score
    /// once and the tests still passed. One place to be wrong is better than
    /// one per caller.
    pub fn from_table(table: &bridge_solver::DdTable) -> Self {
        let mut known: KnownTricks = [[None; 4]; 5];
        for (denomination, row) in Denomination::ALL.iter().zip(known.iter_mut()) {
            for (seat, cell) in Position::ALL.iter().zip(row.iter_mut()) {
                *cell = Some(table.tricks(
                    bridge_solver::seat_to_direction(bridge_solver::direction_to_seat(*seat)),
                    bridge_solver::STRAINS[*denomination as usize],
                ));
            }
        }
        DealTricks { known }
    }

    /// Tricks for one (denomination, declarer), if that one has been worked out.
    pub fn get(&self, denomination: Denomination, declarer: Position) -> Option<u8> {
        self.known[denomination as usize][declarer as usize]
    }

    /// Whether nothing at all is known.
    pub fn is_empty(&self) -> bool {
        self.known.iter().flatten().all(Option::is_none)
    }

    /// The full table, if every cell is known, and `None` otherwise.
    ///
    /// Never solves. This answers "do we know this already?", which is what an
    /// exporter wants: a deal read from a solved library, or one a script has
    /// asked twenty questions about, can carry its results out again, while a
    /// deal nobody asked about is not worth twenty searches to annotate.
    pub fn table(&self) -> Option<bridge_solver::DdTable> {
        let mut table = bridge_solver::DdTable::new();
        for (denomination, row) in Denomination::ALL.iter().zip(self.known.iter()) {
            for (seat, cell) in Position::ALL.iter().zip(row.iter()) {
                table.set(
                    bridge_solver::seat_to_direction(bridge_solver::direction_to_seat(*seat)),
                    bridge_solver::STRAINS[*denomination as usize],
                    (*cell)?,
                );
            }
        }
        Some(table)
    }

    /// Take on answers worked out elsewhere for the same deal.
    ///
    /// Anything already known is kept: a double-dummy result is a property of
    /// the deal, so two answers about the same cell can only agree.
    pub fn merge(&mut self, other: &DealTricks) {
        for (mine, theirs) in self.known.iter_mut().zip(other.known.iter()) {
            for (mine, theirs) in mine.iter_mut().zip(theirs.iter()) {
                if mine.is_none() {
                    *mine = *theirs;
                }
            }
        }
    }

    fn from_known(known: KnownTricks) -> Self {
        DealTricks { known }
    }

    fn known(&self) -> KnownTricks {
        self.known
    }
}

thread_local! {
    /// The deal this thread is analysing, with the solver caches it is using.
    static CURRENT: RefCell<Option<(Key, DealAnalysis)>> = const { RefCell::new(None) };
}

/// Everything this thread has worked out about `deal`, to hand on with it.
///
/// The counterpart of the store that used to do this. A worker calls it right
/// after evaluating a deal, and what comes back travels with that deal to
/// whoever looks at it next — so an answer is searched for once even though
/// the condition and the action run on different threads.
///
/// Empty if this thread has moved on to another deal, which is correct rather
/// than unfortunate: the answers are gone either way, and saying so costs one
/// comparison instead of a lock.
pub fn learned(deal: &Deal) -> DealTricks {
    CURRENT.with(|current| {
        let current = current.borrow();
        // Checked before the key is worked out, because this is called for
        // every deal a run tests and most runs never solve anything. An empty
        // slot means this thread has never called the solver at all, and
        // answering that costs a null check rather than fifty-two cards.
        let Some((held, analysis)) = &*current else {
            return DealTricks::nothing();
        };
        if *held != key(deal) {
            return DealTricks::nothing();
        }
        DealTricks::from_known(analysis.known())
    })
}

/// Tricks `declarer` can take in `denomination` on `deal`, searching for it.
///
/// `known` is what the caller already has, which the analysis takes on before
/// searching: a script that asked about spades and now asks about hearts pays
/// for hearts only.
///
/// Searched at most once per (deal, denomination, declarer) on this thread,
/// however many times a script asks.
pub(crate) fn solve_tricks(
    known: &DealTricks,
    deal: &Deal,
    denomination: Denomination,
    declarer: Position,
) -> u8 {
    let key = key(deal);
    CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        let in_hand = matches!(&*current, Some((held, _)) if *held == key);
        if !in_hand {
            let mut analysis = DealAnalysis::new(deal);
            analysis.preload(known.known());
            *current = Some((key, analysis));
        }
        let analysis = match &mut *current {
            Some((_, analysis)) => analysis,
            // Unreachable: the branch above fills the slot when it is empty.
            None => unreachable!("the deal was just installed"),
        };
        analysis.tricks(denomination, declarer)
    })
}

/// The whole 20-entry double-dummy table for a deal.
///
/// Solves it. The one entry point that is meant to: everything else takes what
/// is known and reads it. Use this to work out a table nobody has — to write
/// one into a PBN or ZRD export, say — not to answer a question about a deal
/// that already carries one.
///
/// Laid out as `bridge_solver` wants it: seats N, E, S, W and strains C, D, H,
/// S, NT, which is dealer's own strain numbering too.
pub fn solve_table(deal: &Deal) -> bridge_solver::DdTable {
    solve_tricks_table(&DealTricks::nothing(), deal)
        .table()
        .unwrap_or_else(|| {
            // Unreachable: every cell was just filled in.
            bridge_solver::DdTable::new()
        })
}

/// Everything known about `deal` after working out whatever is still missing.
fn solve_tricks_table(known: &DealTricks, deal: &Deal) -> DealTricks {
    let mut filled = *known;
    // Denomination outermost, so the four declarers share one pair of solver
    // caches — `DealAnalysis` keeps them per denomination and throws them away
    // when the denomination changes. Seat-outermost asks for a different
    // denomination on every call and so rebuilds the caches twenty times
    // instead of five, which costs about a third of the run.
    for denomination in Denomination::ALL {
        for seat in Position::ALL {
            if filled.get(denomination, seat).is_none() {
                let tricks = solve_tricks(&filled, deal, denomination, seat);
                filled.known[denomination as usize][seat as usize] = Some(tricks);
            }
        }
    }
    filled
}

/// Tricks for one (denomination, declarer), from `known` if it has them.
///
/// The variant every evaluation calls. A deal that arrived from a file already
/// solved knows the answer, and this is a lookup; a deal nobody has solved
/// knows nothing, and this searches.
pub fn tricks(
    known: &DealTricks,
    deal: &Deal,
    denomination: Denomination,
    declarer: Position,
) -> u8 {
    match known.get(denomination, declarer) {
        Some(tricks) => tricks,
        None => solve_tricks(known, deal, denomination, declarer),
    }
}

/// The par score to North-South, from `known` if it has every cell.
///
/// Par is derived from all twenty results, so a deal that arrived solved needs
/// no search at all — which is the difference between a hundred milliseconds
/// and none. A deal that knows some of them pays only for the rest.
///
/// Negative means East-West are the ones who benefit. A passed-out deal — par
/// zero — is zero, which is what the original returns too.
pub fn par_score_ns(known: &DealTricks, deal: &Deal, vul_ns: bool, vul_ew: bool) -> i32 {
    let table = match known.table() {
        Some(table) => table,
        None => match solve_tricks_table(known, deal).table() {
            Some(table) => table,
            // Unreachable: solving fills every cell.
            None => return 0,
        },
    };
    bridge_solver::par(&table, vul_ns, vul_ew).score_ns
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nothing() -> DealTricks {
        DealTricks::nothing()
    }
    use dealer_core::{Card, Rank, Suit};

    /// Each hand holds one whole suit — a deal the solver gets through quickly.
    fn one_suit_each(north: Suit, east: Suit, south: Suit, west: Suit) -> Deal {
        let mut deal = Deal::new();
        for (position, suit) in [
            (Position::North, north),
            (Position::East, east),
            (Position::South, south),
            (Position::West, west),
        ] {
            for rank in Rank::ALL {
                deal.hand_mut(position).add_card(Card::new(suit, rank));
            }
        }
        deal
    }

    fn spades_north() -> Deal {
        one_suit_each(Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs)
    }

    fn spades_east() -> Deal {
        one_suit_each(Suit::Hearts, Suit::Spades, Suit::Clubs, Suit::Diamonds)
    }

    #[test]
    fn a_key_tells_deals_apart_and_is_blind_to_card_order() {
        assert_ne!(key(&spades_north()), key(&spades_east()));

        let mut same_cards_added_backwards = Deal::new();
        for (position, suit) in [
            (Position::North, Suit::Spades),
            (Position::East, Suit::Hearts),
            (Position::South, Suit::Diamonds),
            (Position::West, Suit::Clubs),
        ] {
            for rank in Rank::ALL.iter().rev() {
                same_cards_added_backwards
                    .hand_mut(position)
                    .add_card(Card::new(suit, *rank));
            }
        }
        assert_eq!(key(&spades_north()), key(&same_cards_added_backwards));
    }

    #[test]
    fn repeated_questions_agree() {
        let deal = spades_north();
        assert_eq!(
            solve_tricks(&nothing(), &deal, Denomination::Spades, Position::North),
            13
        );
        assert_eq!(
            solve_tricks(&nothing(), &deal, Denomination::Spades, Position::North),
            13
        );
        assert_eq!(
            solve_tricks(&nothing(), &deal, Denomination::NoTrump, Position::North),
            0
        );
        // Coming back to the first question must not have disturbed it.
        assert_eq!(
            solve_tricks(&nothing(), &deal, Denomination::Spades, Position::North),
            13
        );
    }

    #[test]
    fn a_different_deal_gets_its_own_answers() {
        let deal = spades_north();
        let other = spades_east();
        assert_eq!(
            solve_tricks(&nothing(), &deal, Denomination::Spades, Position::North),
            13
        );
        // East holds every spade here, so North's spade contract takes none.
        assert_eq!(
            solve_tricks(&nothing(), &other, Denomination::Spades, Position::North),
            0
        );
        // And going back gives the original answer again, not a stale one.
        assert_eq!(
            solve_tricks(&nothing(), &deal, Denomination::Spades, Position::North),
            13
        );
    }

    /// What a worker learns comes back with the deal, which is how the main
    /// thread avoids searching for it again.
    ///
    /// This is the case a global store used to cover. The difference is that
    /// the answers now have to be carried: a thread that has moved on knows
    /// nothing, and it is the caller holding `DealTricks` that remembers.
    #[test]
    fn what_one_thread_worked_out_travels_back_with_the_deal() {
        let deal = one_suit_each(Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades);
        let (expected, carried) = std::thread::spawn({
            let deal = deal.clone();
            move || {
                let tricks = solve_tricks(&nothing(), &deal, Denomination::Clubs, Position::North);
                (tricks, learned(&deal))
            }
        })
        .join()
        .expect("the worker thread should not have panicked");

        assert_eq!(
            carried.get(Denomination::Clubs, Position::North),
            Some(expected),
            "the worker should hand back what it worked out"
        );

        // Displace this thread's slot, so nothing here knows the deal.
        solve_tricks(
            &nothing(),
            &spades_north(),
            Denomination::Spades,
            Position::North,
        );
        assert!(
            learned(&deal).is_empty(),
            "this thread has moved on and should admit to knowing nothing"
        );

        // The carried answers are enough: no search, and the same number.
        let before = crate::searches();
        assert_eq!(
            tricks(&carried, &deal, Denomination::Clubs, Position::North),
            expected
        );
        assert_eq!(crate::searches(), before, "that should not have searched");
    }

    #[test]
    fn a_table_survives_the_round_trip_through_deal_tricks() {
        let deal = spades_north();
        let table = solve_table(&deal);
        let known = DealTricks::from_table(&table);
        assert_eq!(
            known.table().expect("every cell is known"),
            table,
            "converting a table in and back out must not move anything"
        );
        assert_eq!(
            known.get(Denomination::Spades, Position::North),
            Some(13),
            "North holds every spade"
        );
    }

    #[test]
    fn partial_knowledge_is_kept_and_completed() {
        let deal = spades_north();
        let mut known = nothing();
        known.merge(&DealTricks::from_table(&solve_table(&deal)));
        assert!(known.table().is_some(), "merging a full table completes it");

        let mut one_cell = nothing();
        one_cell.known[Denomination::Spades as usize][Position::North as usize] = Some(13);
        assert!(!one_cell.is_empty());
        assert!(
            one_cell.table().is_none(),
            "one cell is not a table, and must not pretend to be"
        );
    }
}
