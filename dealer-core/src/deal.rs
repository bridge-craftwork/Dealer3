use crate::{Hand, Position};

/// Represents a complete bridge deal (4 hands of 13 cards each)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deal {
    pub north: Hand,
    pub east: Hand,
    pub south: Hand,
    pub west: Hand,
}

impl Deal {
    /// Create a new empty deal
    pub fn new() -> Self {
        Deal {
            north: Hand::new(),
            east: Hand::new(),
            south: Hand::new(),
            west: Hand::new(),
        }
    }

    /// Get a reference to a hand by position
    pub fn hand(&self, position: Position) -> &Hand {
        match position {
            Position::North => &self.north,
            Position::East => &self.east,
            Position::South => &self.south,
            Position::West => &self.west,
        }
    }

    /// Whether this really is a deal: four hands of thirteen, no card twice.
    ///
    /// `Deal::new()` is empty and hands fill up one card at a time, so the type
    /// cannot enforce this — a partial deal is a real thing mid-predeal. Where
    /// completeness *is* required, it has to be asked for, and reading deals
    /// from a file is the case that matters: everything downstream assumes a
    /// full deck. A board a card short otherwise ran and reported statistics
    /// over a twelve-card hand with nothing to say it had.
    pub fn check_complete(&self) -> Result<(), String> {
        let mut seen = [false; 52];
        for position in Position::ALL {
            let hand = self.hand(position);
            if hand.len() != 13 {
                return Err(format!(
                    "{} has {} cards, and a hand has 13",
                    position,
                    hand.len()
                ));
            }
            for card in hand.cards() {
                let index = card.to_index() as usize;
                if std::mem::replace(&mut seen[index], true) {
                    return Err(format!("{} appears more than once", card));
                }
            }
        }
        Ok(())
    }

    /// Get a mutable reference to a hand by position
    pub fn hand_mut(&mut self, position: Position) -> &mut Hand {
        match position {
            Position::North => &mut self.north,
            Position::East => &mut self.east,
            Position::South => &mut self.south,
            Position::West => &mut self.west,
        }
    }

    /// Sort all hands in the deal
    pub fn sort_all_hands(&mut self) {
        self.north.sort();
        self.east.sort();
        self.south.sort();
        self.west.sort();
    }
}

impl Default for Deal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_whole_deal_passes_the_completeness_check() {
        let mut deal = Deal::new();
        for (i, position) in Position::ALL.iter().enumerate() {
            for rank in 0..13 {
                deal.hand_mut(*position).add_card(
                    crate::Card::from_index((i * 13 + rank) as u8).expect("a card index"),
                );
            }
        }
        assert_eq!(deal.check_complete(), Ok(()));
    }

    #[test]
    fn a_hand_short_of_a_card_is_not_a_whole_deal() {
        // The case that used to run: a board a card short reported statistics
        // over a twelve-card hand without a word.
        let mut deal = Deal::new();
        for (i, position) in Position::ALL.iter().enumerate() {
            let cards = if i == 3 { 12 } else { 13 };
            for rank in 0..cards {
                deal.hand_mut(*position).add_card(
                    crate::Card::from_index((i * 13 + rank) as u8).expect("a card index"),
                );
            }
        }
        let err = deal.check_complete().expect_err("should refuse");
        assert!(err.contains("West"), "should name the seat: {err}");
        assert!(err.contains("12"), "should say how many: {err}");
    }

    #[test]
    fn a_card_dealt_twice_is_not_a_whole_deal() {
        let mut deal = Deal::new();
        for (i, position) in Position::ALL.iter().enumerate() {
            for rank in 0..13 {
                // West gets North's first card instead of his own last.
                let index = if i == 3 && rank == 12 {
                    0
                } else {
                    i * 13 + rank
                };
                deal.hand_mut(*position)
                    .add_card(crate::Card::from_index(index as u8).expect("a card index"));
            }
        }
        let err = deal.check_complete().expect_err("should refuse");
        assert!(err.contains("more than once"), "got: {err}");
    }
    use super::*;

    // Deal generation is covered in `fast_deal.rs`, which owns the generator.
    // What remains here tests `Deal` itself.

    #[test]
    fn test_partner_positions() {
        assert_eq!(Position::North.partner(), Position::South);
        assert_eq!(Position::South.partner(), Position::North);
        assert_eq!(Position::East.partner(), Position::West);
        assert_eq!(Position::West.partner(), Position::East);
    }

    #[test]
    fn test_empty_deal_has_no_cards() {
        let deal = Deal::new();
        for pos in Position::ALL {
            assert_eq!(deal.hand(pos).len(), 0);
        }
    }
}
