//! Script-defined point counts: the `pointcount` and `altcount` statements.
//!
//! # The table
//!
//! The original dealer keeps one global table of twelve rows by thirteen ranks
//! (`pointcount.c`), fills every count for every hand in one pass, and then
//! answers `hcp`, `controls`, `tens` and the rest by looking into it.
//! `pointcount` overwrites row 0; `altcount N` overwrites row N.
//!
//! dealer3 computes each count on demand from a hardcoded match instead, which
//! is why this exists rather than the counts simply reading a table already.
//!
//! # Why the default path is untouched
//!
//! Almost no script defines a count — none of the 1,076 in the corpus does — and
//! `hcp()` sits in the innermost loop of every run. So a custom table is
//! `Option`al and the standard case still calls the same `Hand::hcp()` it always
//! did. The cost to a script that does not use these statements is one
//! perfectly-predicted branch per function call, not a table walk.
//!
//! # Compatibility notes, all measured against dealer.exe
//!
//! - Values are listed from the ace downwards; ranks not reached stay 0, so
//!   `pointcount 6 4 2 1` means A=6, K=4, Q=2, J=1 and nothing else scores.
//! - More than thirteen values is an error, as it is in the original
//!   (`too many pointcount values`).
//! - **`altcount N` is not `ptN`.** Rows 0 and 1 are `hcp` and `controls`, so
//!   `altcount 2` is what sets `pt0`. `altcount 0` overwrites `hcp`.
//! - **`losers` is derived from this table**, not computed independently:
//!   short suits read the controls row and long ones the top3 row. Zeroing
//!   either changes the loser count, which dealer.exe does and so does this.
//! - `altcount 12` and above are refused. The original accepts them and writes
//!   past the end of a twelve-row array; that is a memory bug, not a behaviour
//!   worth reproducing.
//! - A controls row scoring more than 3 in a short suit (`altcount 1 5 5`) makes
//!   the original read past the end of its loser table and return a garbage
//!   count. dealer3 clamps. See `losers_in_suit`.

use dealer_core::{Hand, Rank, Suit};

/// Rows of the count table, in the original's order (`pointcount.h`).
///
/// The order is load-bearing: `altcount N` addresses a row by this number.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CountRow {
    Hcp = 0,
    Controls = 1,
    Tens = 2,
    Jacks = 3,
    Queens = 4,
    Kings = 5,
    Aces = 6,
    Top2 = 7,
    Top3 = 8,
    Top4 = 9,
    Top5 = 10,
    C13 = 11,
}

/// How many rows the table has, and therefore the first `altcount` index the
/// original would write out of bounds.
pub const NUM_ROWS: usize = 12;

/// Ranks per row, indexed by `rank as usize - 2` so that 0 is the two and 12 the
/// ace — the same layout as the original's table.
pub const NUM_RANKS: usize = 13;

/// The standard values, straight from `pointcount.c`.
const STANDARD: [[i32; NUM_RANKS]; NUM_ROWS] = [
    //  2  3  4  5  6  7  8  9  T  J  Q  K  A
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4], // hcp
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2], // controls
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0], // tens
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0], // jacks
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0], // queens
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0], // kings
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], // aces
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1], // top2
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1], // top3
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1], // top4
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1], // top5
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 4, 6], // c13
];

/// A table of point counts a script has redefined.
///
/// Only constructed when a script actually contains `pointcount` or `altcount`;
/// see the module note on why the ordinary path must not pay for this.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PointCounts {
    rows: [[i32; NUM_RANKS]; NUM_ROWS],
}

impl Default for PointCounts {
    fn default() -> Self {
        PointCounts { rows: STANDARD }
    }
}

impl PointCounts {
    /// The standard 4-3-2-1 scale and the counts that go with it.
    pub fn standard() -> Self {
        Self::default()
    }

    /// Overwrite one row with values read from the ace downwards.
    ///
    /// Ranks past the end of `values` become 0, matching the original, where the
    /// row is cleared before the new values are written — so `pointcount 6 4 2 1`
    /// scores nothing below the jack.
    pub fn set_row(&mut self, row: usize, values: &[i32]) -> Result<(), CountError> {
        if row >= NUM_ROWS {
            return Err(CountError::RowOutOfRange(row));
        }
        if values.len() > NUM_RANKS {
            return Err(CountError::TooManyValues(values.len()));
        }
        self.rows[row] = [0; NUM_RANKS];
        for (i, value) in values.iter().enumerate() {
            // values[0] is the ace, which is the last column.
            self.rows[row][NUM_RANKS - 1 - i] = *value;
        }
        Ok(())
    }

    /// What one rank scores in one row.
    #[inline]
    fn value(&self, row: CountRow, rank: Rank) -> i32 {
        self.rows[row as usize][rank as usize - 2]
    }

    /// Total for a row over a whole hand.
    pub fn count_hand(&self, hand: &Hand, row: CountRow) -> i32 {
        hand.cards().iter().map(|c| self.value(row, c.rank)).sum()
    }

    /// Total for a row over one suit.
    pub fn count_suit(&self, hand: &Hand, row: CountRow, suit: Suit) -> i32 {
        hand.cards()
            .iter()
            .filter(|c| c.suit == suit)
            .map(|c| self.value(row, c.rank))
            .sum()
    }

    /// Losers in one suit, derived from the table exactly as the original does.
    ///
    /// This is the part that is easy to miss: `losers` is not independent of the
    /// counts. A void is 0; one and two card suits index a small table by their
    /// **controls** count; three or more is 3 minus the **top3** count. Redefine
    /// either row and the loser count moves with it — verified against
    /// dealer.exe, where zeroing controls took a hand from 8 losers to 10.
    ///
    /// # Where this deliberately differs from the original
    ///
    /// The original indexes those small tables with the controls count and no
    /// bounds check. With the standard row that is safe, because a doubleton can
    /// hold at most A-K and score 3. But a script may redefine the row:
    /// `altcount 1 5 5` makes an ace and a king worth five each, so a doubleton
    /// scores 10 and dealer.exe reads the eleventh element of a four-element
    /// array. Measured, it returned 12 losers for one doubleton and 20 for the
    /// hand.
    ///
    /// dealer3 clamps to the table instead, which for that case gives 8. That is
    /// a real difference in output, and it is the same call as refusing
    /// `altcount 12`: reproducing the number would mean reproducing a read past
    /// the end of an array, and the number is not meaningful in the first place.
    pub fn losers_in_suit(&self, hand: &Hand, suit: Suit) -> i32 {
        // A-K 0, ace or king 1, otherwise 2 — indexed by the controls count.
        const SINGLETON: [i32; 3] = [1, 1, 0];
        const DOUBLETON: [i32; 4] = [2, 1, 1, 0];

        let length = hand.cards().iter().filter(|c| c.suit == suit).count();
        match length {
            0 => 0,
            1 => {
                let controls = self.count_suit(hand, CountRow::Controls, suit);
                SINGLETON[controls.clamp(0, SINGLETON.len() as i32 - 1) as usize]
            }
            2 => {
                let controls = self.count_suit(hand, CountRow::Controls, suit);
                DOUBLETON[controls.clamp(0, DOUBLETON.len() as i32 - 1) as usize]
            }
            _ => 3 - self.count_suit(hand, CountRow::Top3, suit),
        }
    }

    /// Losers across the whole hand.
    pub fn losers(&self, hand: &Hand) -> i32 {
        [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs]
            .iter()
            .map(|s| self.losers_in_suit(hand, *s))
            .sum()
    }
}

/// Why a `pointcount` or `altcount` statement was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountError {
    /// More than thirteen values, as the original also rejects.
    TooManyValues(usize),
    /// `altcount` addressing a row that does not exist. The original accepts
    /// this and writes past its table; dealer3 refuses instead.
    RowOutOfRange(usize),
}

impl std::fmt::Display for CountError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            CountError::TooManyValues(n) => write!(
                f,
                "too many pointcount values: {} given, at most {} (ace down to two)",
                n, NUM_RANKS
            ),
            CountError::RowOutOfRange(n) => write!(
                f,
                "altcount {} is out of range: the counts are numbered 0 to {} \
                 (0 is hcp, 1 is controls, and 2 is pt0)",
                n,
                NUM_ROWS - 1
            ),
        }
    }
}

impl std::error::Error for CountError {}

#[cfg(test)]
mod tests {
    use super::*;
    use dealer_core::Deal;

    fn north() -> Hand {
        // N SAK HQJT98 D543 C942 — the hand the dealer.exe comparison used.
        let deal: Deal = dealer_pbn::parse_oneline(
            "n AK.QJT98.543.942 e QJT98.AK.AKQ.AKQ s 7654.7654.JT9.J8 w 32.32.8762.T7653",
        )
        .expect("reference deal");
        for (label, hand) in [
            ("north", &deal.north),
            ("east", &deal.east),
            ("south", &deal.south),
            ("west", &deal.west),
        ] {
            assert_eq!(hand.len(), 13, "{} is not a full hand", label);
        }
        deal.north
    }

    #[test]
    fn the_default_table_reproduces_the_standard_counts() {
        let hand = north();
        let counts = PointCounts::standard();
        assert_eq!(counts.count_hand(&hand, CountRow::Hcp), hand.hcp() as i32);
        assert_eq!(
            counts.count_hand(&hand, CountRow::Controls),
            hand.controls() as i32
        );
        assert_eq!(counts.count_hand(&hand, CountRow::Tens), hand.tens() as i32);
        assert_eq!(counts.count_hand(&hand, CountRow::Top3), hand.top3() as i32);
        assert_eq!(counts.count_hand(&hand, CountRow::C13), hand.c13() as i32);
        assert_eq!(counts.losers(&hand), hand.losers() as i32);
    }

    #[test]
    fn values_are_read_from_the_ace_downwards_and_the_rest_are_zero() {
        let hand = north();
        let mut counts = PointCounts::standard();
        counts
            .set_row(CountRow::Hcp as usize, &[6, 4, 2, 1])
            .unwrap();
        // A K Q J T in this hand, so 6 + 4 + 2 + 1 and the ten scores nothing.
        assert_eq!(counts.count_hand(&hand, CountRow::Hcp), 13);
    }

    #[test]
    fn a_full_row_of_thirteen_is_accepted_and_fourteen_is_not() {
        let mut counts = PointCounts::standard();
        assert!(counts.set_row(0, &[1; 13]).is_ok());
        assert_eq!(
            counts.set_row(0, &[1; 14]),
            Err(CountError::TooManyValues(14))
        );
    }

    /// The one measured case where dealer3 and dealer.exe disagree, pinned so
    /// the divergence is deliberate rather than drift. dealer.exe answers 20
    /// here, by reading past the end of its loser table.
    #[test]
    fn an_oversized_controls_row_clamps_instead_of_reading_past_the_table() {
        let hand = north();
        let mut counts = PointCounts::standard();
        counts
            .set_row(CountRow::Controls as usize, &[5, 5])
            .unwrap();

        assert_eq!(counts.count_hand(&hand, CountRow::Controls), 10);
        assert_eq!(
            counts.losers(&hand),
            8,
            "the A-K doubleton should clamp to the best entry, not index past it"
        );
    }

    #[test]
    fn altcount_rows_beyond_the_table_are_refused() {
        let mut counts = PointCounts::standard();
        assert!(counts.set_row(11, &[1]).is_ok());
        assert_eq!(counts.set_row(12, &[1]), Err(CountError::RowOutOfRange(12)));
    }

    /// The behaviour verified against dealer.exe: losers moves when the controls
    /// or top3 rows are redefined.
    #[test]
    fn losers_follows_the_controls_and_top3_rows() {
        let hand = north();
        assert_eq!(PointCounts::standard().losers(&hand), 8);

        let mut zero_controls = PointCounts::standard();
        zero_controls
            .set_row(CountRow::Controls as usize, &[])
            .unwrap();
        assert_eq!(
            zero_controls.losers(&hand),
            10,
            "the A-K doubleton should go from 0 losers to 2 with controls zeroed"
        );

        let mut zero_top3 = PointCounts::standard();
        zero_top3.set_row(CountRow::Top3 as usize, &[]).unwrap();
        assert!(
            zero_top3.losers(&hand) > 8,
            "zeroing top3 must cost the long suits their winners"
        );
    }
}
