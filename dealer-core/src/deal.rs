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
