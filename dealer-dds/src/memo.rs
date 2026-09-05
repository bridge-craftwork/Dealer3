//! Remembering double-dummy results, so no deal is ever searched twice.
//!
//! `tricks()` is orders of magnitude more expensive than every other function
//! in the language, so what matters is not how fast one search is but how few
//! of them run. Three things would otherwise repeat work:
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
//! The first two are covered by keeping a [`DealAnalysis`] for the deal in
//! hand, on this thread. The third needs the answer to outlive both the deal
//! and the thread, so results are handed to a shared table on the way out and
//! taken back from it on the way in. Only the answers are shared — twenty
//! bytes a deal — and not the solver's caches, which run to several megabytes
//! and are worth keeping only for as long as the deal they belong to. A thread
//! that has called `tricks()` holds one deal's caches until another deal
//! arrives on it, so a worker pool retains a few megabytes a thread.

use crate::{DealAnalysis, Denomination, KnownTricks};
use dealer_core::{Deal, Position};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};

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

/// How many deals the shared table remembers.
///
/// It has to outlive one generation batch — the default is a couple of hundred
/// deals per thread — for the main thread to still find what a worker worked
/// out. This is well clear of that, and costs well under a megabyte.
const REMEMBERED_DEALS: usize = 16_384;

static REMEMBERED: LazyLock<Mutex<Remembered>> = LazyLock::new(|| {
    Mutex::new(Remembered {
        known: HashMap::new(),
        oldest_first: VecDeque::new(),
    })
});

/// Results for the deals most recently analysed on any thread.
struct Remembered {
    known: HashMap<Key, KnownTricks>,
    /// The insertion order, for evicting the oldest once the table is full.
    oldest_first: VecDeque<Key>,
}

fn recall(key: &Key) -> Option<KnownTricks> {
    // A poisoned lock would mean another thread panicked while holding it.
    // Nothing here can leave the table inconsistent, so carry on with it.
    let remembered = REMEMBERED.lock().unwrap_or_else(|e| e.into_inner());
    remembered.known.get(key).copied()
}

fn remember(key: Key, known: KnownTricks) {
    let mut remembered = REMEMBERED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = remembered.known.get_mut(&key) {
        for (existing, new) in existing.iter_mut().flatten().zip(known.iter().flatten()) {
            if existing.is_none() {
                *existing = *new;
            }
        }
        return;
    }
    if remembered.oldest_first.len() >= REMEMBERED_DEALS {
        if let Some(evicted) = remembered.oldest_first.pop_front() {
            remembered.known.remove(&evicted);
        }
    }
    remembered.known.insert(key, known);
    remembered.oldest_first.push_back(key);
}

thread_local! {
    /// The deal this thread is analysing, with the solver caches it is using.
    static CURRENT: RefCell<Option<(Key, DealAnalysis)>> = const { RefCell::new(None) };
}

/// Tricks `declarer` can take in `denomination` on `deal`.
///
/// Searched at most once per (deal, denomination, declarer), however many
/// times a script asks and from wherever it asks.
pub fn tricks(deal: &Deal, denomination: Denomination, declarer: Position) -> u8 {
    let key = key(deal);
    CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        let in_hand = matches!(&*current, Some((held, _)) if *held == key);
        if !in_hand {
            let mut analysis = DealAnalysis::new(deal);
            if let Some(known) = recall(&key) {
                analysis.preload(known);
            }
            *current = Some((key, analysis));
        }
        let analysis = match &mut *current {
            Some((_, analysis)) => analysis,
            // Unreachable: the branch above fills the slot when it is empty.
            None => unreachable!("the deal was just installed"),
        };

        let asked_before = analysis.known()[denomination as usize][declarer as usize].is_some();
        let tricks = analysis.tricks(denomination, declarer);
        if !asked_before {
            // Share it as soon as it is worked out, rather than when this deal
            // is displaced: a worker thread may only see one deal calling
            // `tricks()` in a whole batch, and the main thread needs the answer
            // whether or not another deal ever arrives to push it out.
            remember(key, analysis.known());
        }
        tricks
    })
}

/// The whole 20-entry double-dummy table for a deal.
///
/// Every cell goes through [`tricks`], so a table costs only the searches that
/// have not already been done — a script whose condition asked about one
/// denomination pays for nineteen more, not twenty — and the answers are shared
/// with every later caller the same way.
///
/// Laid out as `bridge_solver` wants it: seats N, E, S, W and strains C, D, H,
/// S, NT, which is dealer's own strain numbering too.
pub fn table(deal: &Deal) -> bridge_solver::DdTable {
    let mut table = bridge_solver::DdTable::new();
    // Denomination outermost, so the four declarers share one pair of solver
    // caches — `DealAnalysis` keeps them per denomination and throws them away
    // when the denomination changes. Seat-outermost asks for a different
    // denomination on every call and so rebuilds the caches twenty times
    // instead of five, which costs about a third of the run.
    for denomination in Denomination::ALL {
        for seat in Position::ALL {
            let tricks = self::tricks(deal, denomination, seat);
            // Both axes are converted rather than indexed. `DdTable` is keyed
            // by `Direction` and `Strain`, and this crate counts seats and
            // denominations its own way; the two happen to agree today, and
            // writing the cells by name means it does not matter if they stop.
            // Getting an axis wrong here does not fail, it negates par.
            table.set(
                bridge_solver::seat_to_direction(bridge_solver::direction_to_seat(seat)),
                bridge_solver::STRAINS[denomination as usize],
                tricks,
            );
        }
    }
    table
}

/// The par score, to North-South, at the given vulnerability.
///
/// Negative means East-West are the ones who benefit. A passed-out deal — par
/// zero — is zero, which is what the original returns too.
pub fn par_score_ns(deal: &Deal, vul_ns: bool, vul_ew: bool) -> i32 {
    bridge_solver::par(&table(deal), vul_ns, vul_ew).score_ns
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(tricks(&deal, Denomination::Spades, Position::North), 13);
        assert_eq!(tricks(&deal, Denomination::Spades, Position::North), 13);
        assert_eq!(tricks(&deal, Denomination::NoTrump, Position::North), 0);
        // Coming back to the first question must not have disturbed it.
        assert_eq!(tricks(&deal, Denomination::Spades, Position::North), 13);
    }

    #[test]
    fn a_different_deal_gets_its_own_answers() {
        let deal = spades_north();
        let other = spades_east();
        assert_eq!(tricks(&deal, Denomination::Spades, Position::North), 13);
        // East holds every spade here, so North's spade contract takes none.
        assert_eq!(tricks(&other, Denomination::Spades, Position::North), 0);
        // And going back gives the original answer again, not a stale one.
        assert_eq!(tricks(&deal, Denomination::Spades, Position::North), 13);
    }

    #[test]
    fn what_one_thread_worked_out_another_can_use() {
        // A deal no other test uses, so the shared table can only have heard
        // of it from the thread below.
        let deal = one_suit_each(Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades);
        let expected = std::thread::spawn({
            let deal = deal.clone();
            move || tricks(&deal, Denomination::Clubs, Position::North)
        })
        .join()
        .expect("the worker thread should not have panicked");

        // Displace this thread's slot, so the answer can only come from the
        // shared table.
        tricks(&spades_north(), Denomination::Spades, Position::North);
        assert_eq!(
            tricks(&deal, Denomination::Clubs, Position::North),
            expected
        );
    }
}
