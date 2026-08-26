//! Double Dummy Solver for Bridge
//!
//! This crate provides double-dummy analysis for bridge deals, calculating
//! the number of tricks that can be made by each side in each denomination
//! when all four hands are visible.
//!
//! The search itself lives in [`bridge_solver`] (a port of
//! macroxue/bridge-solver): MTD(f) over a pattern-based transposition table
//! with hierarchical bounds, move ordering and fast trick estimation. This
//! crate is the adaptor — it converts `dealer_core` types, gets the leader and
//! the declarer-versus-NS trick conversion right, and remembers results per
//! deal, so however many times a script names `tricks()` it is searched once
//! for each (deal, denomination, declarer).

mod memo;

pub use memo::tricks;

use dealer_core::{Deal, Position, Suit};

/// The double-dummy engine, re-exported for callers that want it directly.
///
/// This used to be called `solver2`, from when there was also a solver in this
/// crate to be second to. There is not any more.
pub use bridge_solver;

use std::sync::atomic::{AtomicUsize, Ordering};

use bridge_solver::{direction_to_seat, CutoffCache, Hands, PatternCache, Solver};
use bridge_solver::{CLUB, DIAMOND, HEART, NOTRUMP, SPADE};

/// Denomination for double-dummy analysis
///
/// The discriminants are dealer's own denomination numbering — `0=C` through
/// `4=NT` — which is also `bridge_types::Strain`'s order, so `as usize`
/// indexes both tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Denomination {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
    NoTrump,
}

impl Denomination {
    /// All five denominations
    pub const ALL: [Denomination; 5] = [
        Denomination::Clubs,
        Denomination::Diamonds,
        Denomination::Hearts,
        Denomination::Spades,
        Denomination::NoTrump,
    ];

    /// Convert from Suit
    pub fn from_suit(suit: Suit) -> Self {
        match suit {
            Suit::Clubs => Denomination::Clubs,
            Suit::Diamonds => Denomination::Diamonds,
            Suit::Hearts => Denomination::Hearts,
            Suit::Spades => Denomination::Spades,
        }
    }

    /// Convert from dealer's denomination number, `0=C` through `4=NT`
    pub fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Denomination::Clubs),
            1 => Some(Denomination::Diamonds),
            2 => Some(Denomination::Hearts),
            3 => Some(Denomination::Spades),
            4 => Some(Denomination::NoTrump),
            _ => None,
        }
    }

    /// Convert to Suit (NoTrump returns None)
    pub fn to_suit(&self) -> Option<Suit> {
        match self {
            Denomination::Clubs => Some(Suit::Clubs),
            Denomination::Diamonds => Some(Suit::Diamonds),
            Denomination::Hearts => Some(Suit::Hearts),
            Denomination::Spades => Some(Suit::Spades),
            Denomination::NoTrump => None,
        }
    }

    /// Convert to character representation
    pub fn to_char(&self) -> char {
        match self {
            Denomination::Clubs => 'C',
            Denomination::Diamonds => 'D',
            Denomination::Hearts => 'H',
            Denomination::Spades => 'S',
            Denomination::NoTrump => 'N',
        }
    }

    /// The solver's trump index for this denomination
    fn trump(&self) -> usize {
        match self {
            Denomination::Clubs => CLUB,
            Denomination::Diamonds => DIAMOND,
            Denomination::Hearts => HEART,
            Denomination::Spades => SPADE,
            Denomination::NoTrump => NOTRUMP,
        }
    }
}

/// Result of double-dummy analysis for a single denomination and declarer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrickResult {
    pub denomination: Denomination,
    pub declarer: Position,
    pub tricks: u8,
}

/// Complete double-dummy analysis result for all denominations and declarers
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoubleDummyResult {
    /// Tricks by denomination and declarer
    /// Index: [denomination][declarer]
    tricks: [[u8; 4]; 5],
}

impl DoubleDummyResult {
    /// Create a new empty result
    pub fn new() -> Self {
        Self {
            tricks: [[0; 4]; 5],
        }
    }

    /// Set tricks for a specific denomination and declarer
    pub fn set_tricks(&mut self, denomination: Denomination, declarer: Position, tricks: u8) {
        let denom_idx = denomination as usize;
        let decl_idx = declarer as usize;
        self.tricks[denom_idx][decl_idx] = tricks;
    }

    /// Get tricks for a specific denomination and declarer
    pub fn get_tricks(&self, denomination: Denomination, declarer: Position) -> u8 {
        let denom_idx = denomination as usize;
        let decl_idx = declarer as usize;
        self.tricks[denom_idx][decl_idx]
    }

    /// Get all results as a vector of TrickResult
    pub fn all_results(&self) -> Vec<TrickResult> {
        let mut results = Vec::new();
        for denom in Denomination::ALL {
            for position in Position::ALL {
                results.push(TrickResult {
                    denomination: denom,
                    declarer: position,
                    tricks: self.get_tricks(denom, position),
                });
            }
        }
        results
    }
}

impl Default for DoubleDummyResult {
    fn default() -> Self {
        Self::new()
    }
}

/// How many searches have run, for tests that care that a result was
/// remembered rather than worked out again.
static SEARCHES: AtomicUsize = AtomicUsize::new(0);

/// How many double-dummy searches this process has run.
///
/// Every answer either comes from a search or from something already
/// remembered, so a test can bracket a piece of work with this and see how
/// much of it the memo absorbed. Counts every thread.
pub fn searches() -> usize {
    SEARCHES.load(Ordering::Relaxed)
}

/// Whatever is known about one deal so far, indexed `[denomination][declarer]`
/// in the same order [`Denomination::ALL`] and [`Position::ALL`] give.
pub type KnownTricks = [[Option<u8>; 4]; 5];

/// How wide the solver's cutoff and pattern caches are, in bits.
///
/// The same size `bridge_solver::solve_dd_table` uses. Narrowing them is not
/// worth the memory it saves: measured over 200 deals, 14 bits costs about 8%
/// and 12 bits about 27%.
const CACHE_BITS: usize = 16;

/// Double-dummy analysis of one deal, solved on demand and remembered.
///
/// Build one per deal and ask it for as many (denomination, declarer) pairs as
/// the script wants: each pair is searched once, and consecutive questions
/// about the same denomination share the solver's cutoff and pattern caches,
/// which is where most of the saving is. A script asking about a single
/// denomination never pays for the other four.
pub struct DealAnalysis {
    hands: Hands,
    /// Tricks available in total — 13 for a complete deal.
    total: u8,
    /// Answers already known, indexed `[denomination][declarer]`.
    known: KnownTricks,
    /// The caches, and the denomination they belong to. They are only valid
    /// within one denomination, so a change of denomination replaces them.
    caches: Option<(Denomination, Box<Caches>)>,
}

/// The solver's two per-denomination caches, boxed together because they run
/// to several megabytes between them.
struct Caches {
    cutoff: CutoffCache,
    pattern: PatternCache,
}

impl DealAnalysis {
    /// Begin analysing a deal. No search runs until a result is asked for.
    pub fn new(deal: &Deal) -> Self {
        let bt_deal: bridge_types::Deal = deal.into();
        let hands = Hands::from_deal(&bt_deal);
        let total = hands.num_tricks() as u8;
        Self {
            hands,
            total,
            known: [[None; 4]; 5],
            caches: None,
        }
    }

    /// Everything searched, or handed to [`preload`], so far.
    ///
    /// [`preload`]: DealAnalysis::preload
    pub fn known(&self) -> KnownTricks {
        self.known
    }

    /// Take on answers worked out elsewhere for the same deal.
    ///
    /// Anything already known here is kept; a double-dummy result is a
    /// property of the deal, so the two can only agree.
    pub fn preload(&mut self, known: KnownTricks) {
        for (mine, theirs) in self.known.iter_mut().zip(known) {
            for (mine, theirs) in mine.iter_mut().zip(theirs) {
                if mine.is_none() {
                    *mine = theirs;
                }
            }
        }
    }

    /// Tricks `declarer` can take in `denomination`, searching only if this
    /// pair has not been asked for before.
    pub fn tricks(&mut self, denomination: Denomination, declarer: Position) -> u8 {
        let denom_idx = denomination as usize;
        let decl_idx = declarer as usize;
        if let Some(tricks) = self.known[denom_idx][decl_idx] {
            return tricks;
        }
        let tricks = self.search(denomination, declarer);
        self.known[denom_idx][decl_idx] = Some(tricks);
        tricks
    }

    /// Every denomination and declarer, filling in whatever is still unknown.
    pub fn table(&mut self) -> DoubleDummyResult {
        let mut result = DoubleDummyResult::new();
        // Denomination outermost, so all four declarers share one pair of caches.
        for denomination in Denomination::ALL {
            for declarer in Position::ALL {
                result.set_tricks(denomination, declarer, self.tricks(denomination, declarer));
            }
        }
        result
    }

    /// Run the search for one (denomination, declarer) pair.
    ///
    /// `Solver::new` takes the *leader*, not the declarer, and returns tricks
    /// for *North/South*, not for the declarer. Both conversions happen here;
    /// getting either wrong yields plausible but wrong numbers.
    fn search(&mut self, denomination: Denomination, declarer: Position) -> u8 {
        SEARCHES.fetch_add(1, Ordering::Relaxed);
        let seat = direction_to_seat(declarer);
        let leader = (seat + 1) % 4;
        let solver = Solver::new(self.hands, denomination.trump(), leader);
        let caches = self.caches_for(denomination);
        let ns = solver.solve_with_caches(&mut caches.cutoff, &mut caches.pattern);
        match declarer {
            Position::North | Position::South => ns,
            Position::East | Position::West => self.total - ns,
        }
    }

    /// The caches for `denomination`, allocating them if the last search was
    /// in a different denomination. Cache entries are keyed by position alone,
    /// so they must not be carried across a change of trump suit.
    fn caches_for(&mut self, denomination: Denomination) -> &mut Caches {
        let reusable = matches!(&self.caches, Some((denom, _)) if *denom == denomination);
        if !reusable {
            self.caches = Some((
                denomination,
                Box::new(Caches {
                    cutoff: CutoffCache::new(CACHE_BITS),
                    pattern: PatternCache::new(CACHE_BITS),
                }),
            ));
        }
        match &mut self.caches {
            Some((_, caches)) => caches,
            // Unreachable: the branch above assigns `Some` whenever it is `None`.
            None => unreachable!("caches were just assigned"),
        }
    }
}

/// Tricks `declarer` can take in `denomination` on `deal`, as a one-shot.
///
/// Use [`DealAnalysis`] instead when more than one question will be asked
/// about the same deal.
pub fn solve(deal: &Deal, denomination: Denomination, declarer: Position) -> u8 {
    DealAnalysis::new(deal).tricks(denomination, declarer)
}

/// The full 5x4 table for a deal.
pub fn solve_all(deal: &Deal) -> DoubleDummyResult {
    DealAnalysis::new(deal).table()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dealer_core::{Card, Rank};

    #[test]
    fn test_denomination_conversion() {
        assert_eq!(Denomination::from_suit(Suit::Spades), Denomination::Spades);
        assert_eq!(Denomination::from_suit(Suit::Hearts), Denomination::Hearts);
        assert_eq!(
            Denomination::from_suit(Suit::Diamonds),
            Denomination::Diamonds
        );
        assert_eq!(Denomination::from_suit(Suit::Clubs), Denomination::Clubs);
    }

    #[test]
    fn test_denomination_to_char() {
        assert_eq!(Denomination::Spades.to_char(), 'S');
        assert_eq!(Denomination::Hearts.to_char(), 'H');
        assert_eq!(Denomination::Diamonds.to_char(), 'D');
        assert_eq!(Denomination::Clubs.to_char(), 'C');
        assert_eq!(Denomination::NoTrump.to_char(), 'N');
    }

    #[test]
    fn denomination_index_matches_dealers_numbering() {
        for (index, denomination) in Denomination::ALL.iter().enumerate() {
            assert_eq!(Denomination::from_index(index as i32), Some(*denomination));
            assert_eq!(*denomination as usize, index);
        }
        assert_eq!(Denomination::from_index(5), None);
        assert_eq!(Denomination::from_index(-1), None);
    }

    #[test]
    fn test_double_dummy_result() {
        let mut result = DoubleDummyResult::new();
        result.set_tricks(Denomination::Spades, Position::North, 10);
        assert_eq!(result.get_tricks(Denomination::Spades, Position::North), 10);
    }

    /// Create a simple deal where each hand has one suit (fast to solve)
    fn create_simple_deal() -> Deal {
        let ranks = [
            Rank::Ace,
            Rank::King,
            Rank::Queen,
            Rank::Jack,
            Rank::Ten,
            Rank::Nine,
            Rank::Eight,
            Rank::Seven,
            Rank::Six,
            Rank::Five,
            Rank::Four,
            Rank::Three,
            Rank::Two,
        ];
        let mut deal = Deal::new();
        for &rank in &ranks {
            deal.hand_mut(Position::North)
                .add_card(Card::new(Suit::Spades, rank));
        }
        for &rank in &ranks {
            deal.hand_mut(Position::East)
                .add_card(Card::new(Suit::Hearts, rank));
        }
        for &rank in &ranks {
            deal.hand_mut(Position::South)
                .add_card(Card::new(Suit::Diamonds, rank));
        }
        for &rank in &ranks {
            deal.hand_mut(Position::West)
                .add_card(Card::new(Suit::Clubs, rank));
        }
        deal
    }

    #[test]
    fn test_solver_creation() {
        let result = solve_all(&create_simple_deal());
        assert_eq!(result.all_results().len(), 20); // 5 denominations × 4 positions
    }

    #[test]
    fn test_solver_basic() {
        // Test with a simple deal (one suit per hand)
        let deal = create_simple_deal();

        // In NT with N declarer, E leads hearts, E/W win all tricks
        assert_eq!(solve(&deal, Denomination::NoTrump, Position::North), 0);

        // With Spades trump, N/S win all tricks (N has all spades)
        assert_eq!(solve(&deal, Denomination::Spades, Position::North), 13);
    }

    #[test]
    fn memo_answers_match_a_fresh_solve() {
        let deal = create_simple_deal();
        let table = solve_all(&deal);
        let mut analysis = DealAnalysis::new(&deal);
        for denomination in Denomination::ALL {
            for declarer in Position::ALL {
                assert_eq!(
                    analysis.tricks(denomination, declarer),
                    table.get_tricks(denomination, declarer),
                    "{:?} by {:?}",
                    denomination,
                    declarer
                );
                // Asking twice must give the same answer from the memo.
                assert_eq!(
                    analysis.tricks(denomination, declarer),
                    table.get_tricks(denomination, declarer)
                );
            }
        }
    }
}
