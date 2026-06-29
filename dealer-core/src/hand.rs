use crate::shape::shape_to_index;
use crate::{Card, Rank, Suit};

/// Represents a single player's hand of 13 cards
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hand {
    cards: Vec<Card>,
}

impl Hand {
    /// Create a new empty hand
    pub fn new() -> Self {
        Hand { cards: Vec::new() }
    }

    /// Create a hand from a vector of cards
    pub fn from_cards(cards: Vec<Card>) -> Self {
        Hand { cards }
    }

    /// Add a card to the hand
    pub fn add_card(&mut self, card: Card) {
        self.cards.push(card);
    }

    /// Get all cards in the hand
    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    /// Get the number of cards in the hand
    pub fn len(&self) -> usize {
        self.cards.len()
    }

    /// Check if the hand is empty
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// Count cards of a specific suit
    pub fn suit_length(&self, suit: Suit) -> usize {
        self.cards.iter().filter(|c| c.suit == suit).count()
    }

    /// Get all cards of a specific suit
    pub fn cards_in_suit(&self, suit: Suit) -> Vec<Card> {
        self.cards
            .iter()
            .filter(|c| c.suit == suit)
            .copied()
            .collect()
    }

    /// Calculate total High Card Points (HCP)
    /// A=4, K=3, Q=2, J=1
    pub fn hcp(&self) -> u8 {
        self.cards.iter().map(|c| c.hcp()).sum()
    }

    /// Get the suit lengths in standard order [S, H, D, C]
    /// E.g., [5, 4, 3, 1] means 5 spades, 4 hearts, 3 diamonds, 1 club
    pub fn suit_lengths(&self) -> [usize; 4] {
        [
            self.suit_length(Suit::Spades),
            self.suit_length(Suit::Hearts),
            self.suit_length(Suit::Diamonds),
            self.suit_length(Suit::Clubs),
        ]
    }

    /// Get the distribution pattern as a sorted array [longest to shortest]
    /// E.g., [5, 4, 3, 1] for a 5-4-3-1 hand (regardless of which suits)
    pub fn distribution(&self) -> [usize; 4] {
        let mut lengths = self.suit_lengths();
        lengths.sort_by(|a, b| b.cmp(a)); // Sort descending
        lengths
    }

    /// Get the shape index (0-559) for O(1) shape mask matching.
    ///
    /// This index uniquely identifies the hand's ordered shape (S-H-D-C).
    #[inline]
    pub fn shape_index(&self) -> usize {
        let lengths = self.suit_lengths();
        shape_to_index(lengths[0], lengths[1], lengths[2], lengths[3])
    }

    /// Get the shape as a sorted string (e.g., "5-4-3-1")
    pub fn shape(&self) -> String {
        let dist = self.distribution();
        format!("{}-{}-{}-{}", dist[0], dist[1], dist[2], dist[3])
    }

    /// Check if hand is balanced (4-3-3-3, 4-4-3-2, or 5-3-3-2)
    pub fn is_balanced(&self) -> bool {
        let dist = self.distribution();
        matches!(dist, [4, 3, 3, 3] | [4, 4, 3, 2] | [5, 3, 3, 2])
    }

    /// Count controls (A=2, K=1)
    pub fn controls(&self) -> u8 {
        self.cards
            .iter()
            .map(|c| match c.rank {
                Rank::Ace => 2,
                Rank::King => 1,
                _ => 0,
            })
            .sum()
    }

    /// Count honors (A, K, Q, J, T) in a specific suit
    pub fn honors_in_suit(&self, suit: Suit) -> u8 {
        self.cards
            .iter()
            .filter(|c| c.suit == suit && c.rank >= Rank::Ten)
            .count() as u8
    }

    /// Sort the hand by suit (spades first) and rank (high to low)
    pub fn sort(&mut self) {
        self.cards.sort_by(|a, b| {
            // Sort by suit descending (Spades first)
            match b.suit.cmp(&a.suit) {
                std::cmp::Ordering::Equal => {
                    // Within same suit, sort by rank descending (Ace first)
                    b.rank.cmp(&a.rank)
                }
                other => other,
            }
        });
    }

    /// Get a sorted copy of the hand
    pub fn sorted(&self) -> Hand {
        let mut hand = self.clone();
        hand.sort();
        hand
    }

    /// Check if hand matches an exact shape pattern (S-H-D-C order)
    /// E.g., [5, 4, 3, 1] matches only hands with exactly 5 spades, 4 hearts, 3 diamonds, 1 club
    pub fn matches_exact_shape(&self, pattern: &[u8; 4]) -> bool {
        let lengths = self.suit_lengths();
        lengths[0] == pattern[0] as usize
            && lengths[1] == pattern[1] as usize
            && lengths[2] == pattern[2] as usize
            && lengths[3] == pattern[3] as usize
    }

    /// Check if hand matches a wildcard shape pattern
    /// None means "any length" for that suit
    /// E.g., [Some(5), Some(4), None, None] matches any hand with 5 spades and 4 hearts
    pub fn matches_wildcard_shape(&self, pattern: &[Option<u8>; 4]) -> bool {
        let lengths = self.suit_lengths();
        for i in 0..4 {
            if let Some(required) = pattern[i] {
                if lengths[i] != required as usize {
                    return false;
                }
            }
        }
        true
    }

    /// Check if hand matches a distribution pattern (suit-order independent)
    /// E.g., [4, 3, 3, 3] matches any hand with one 4-card suit and three 3-card suits
    pub fn matches_distribution(&self, pattern: &[u8; 4]) -> bool {
        let mut dist = self.distribution();
        let mut pat = *pattern;
        dist.sort_unstable();
        pat.sort_unstable();

        // Convert usize to u8 for comparison
        let dist_u8: [u8; 4] = [dist[0] as u8, dist[1] as u8, dist[2] as u8, dist[3] as u8];
        dist_u8 == pat
    }

    /// Calculate losers for entire hand
    /// Uses standard loser count: A/K/Q in top 3 positions reduce losers
    pub fn losers(&self) -> u8 {
        self.losers_in_suit(Suit::Spades)
            + self.losers_in_suit(Suit::Hearts)
            + self.losers_in_suit(Suit::Diamonds)
            + self.losers_in_suit(Suit::Clubs)
    }

    /// Calculate losers in a specific suit
    /// Rules:
    /// - Void: 0 losers
    /// - Singleton: 0 if Ace, 1 otherwise
    /// - Doubleton: 0 for AK, 1 for Ax or Kx, 2 otherwise
    /// - 3+ cards: Start with 3, subtract 1 for each A/K/Q in top 3 positions
    pub fn losers_in_suit(&self, suit: Suit) -> u8 {
        let mut cards: Vec<Card> = self
            .cards
            .iter()
            .filter(|c| c.suit == suit)
            .copied()
            .collect();

        let len = cards.len();
        if len == 0 {
            return 0; // Void
        }

        // Sort by rank descending (Ace first)
        cards.sort_by_key(|b| std::cmp::Reverse(b.rank));

        if len == 1 {
            // Singleton: 0 if Ace, 1 otherwise
            if cards[0].rank == Rank::Ace {
                0
            } else {
                1
            }
        } else if len == 2 {
            // Doubleton
            let has_ace = cards.iter().any(|c| c.rank == Rank::Ace);
            let has_king = cards.iter().any(|c| c.rank == Rank::King);

            if has_ace && has_king {
                0
            } else if has_ace || has_king {
                1
            } else {
                2
            }
        } else {
            // 3+ cards: Count top 3 honors
            let mut losers = 3;
            for card in cards.iter().take(3.min(len)) {
                if matches!(card.rank, Rank::Ace | Rank::King | Rank::Queen) {
                    losers -= 1;
                }
            }
            losers
        }
    }

    /// Check if hand contains a specific card
    pub fn has_card(&self, card: Card) -> bool {
        self.cards.contains(&card)
    }

    /// Count number of tens in hand
    pub fn tens(&self) -> u8 {
        self.cards.iter().filter(|c| c.rank == Rank::Ten).count() as u8
    }

    /// Count number of tens in specific suit
    pub fn tens_in_suit(&self, suit: Suit) -> u8 {
        self.cards
            .iter()
            .filter(|c| c.suit == suit && c.rank == Rank::Ten)
            .count() as u8
    }

    /// Count number of jacks in hand
    pub fn jacks(&self) -> u8 {
        self.cards.iter().filter(|c| c.rank == Rank::Jack).count() as u8
    }

    /// Count number of jacks in specific suit
    pub fn jacks_in_suit(&self, suit: Suit) -> u8 {
        self.cards
            .iter()
            .filter(|c| c.suit == suit && c.rank == Rank::Jack)
            .count() as u8
    }

    /// Count number of queens in hand
    pub fn queens(&self) -> u8 {
        self.cards.iter().filter(|c| c.rank == Rank::Queen).count() as u8
    }

    /// Count number of queens in specific suit
    pub fn queens_in_suit(&self, suit: Suit) -> u8 {
        self.cards
            .iter()
            .filter(|c| c.suit == suit && c.rank == Rank::Queen)
            .count() as u8
    }

    /// Count number of kings in hand
    pub fn kings(&self) -> u8 {
        self.cards.iter().filter(|c| c.rank == Rank::King).count() as u8
    }

    /// Count number of kings in specific suit
    pub fn kings_in_suit(&self, suit: Suit) -> u8 {
        self.cards
            .iter()
            .filter(|c| c.suit == suit && c.rank == Rank::King)
            .count() as u8
    }

    /// Count number of aces in hand
    pub fn aces(&self) -> u8 {
        self.cards.iter().filter(|c| c.rank == Rank::Ace).count() as u8
    }

    /// Count number of aces in specific suit
    pub fn aces_in_suit(&self, suit: Suit) -> u8 {
        self.cards
            .iter()
            .filter(|c| c.suit == suit && c.rank == Rank::Ace)
            .count() as u8
    }

    /// Count top 2 honors (A, K) in hand
    pub fn top2(&self) -> u8 {
        self.cards
            .iter()
            .filter(|c| matches!(c.rank, Rank::Ace | Rank::King))
            .count() as u8
    }

    /// Count top 2 honors (A, K) in specific suit
    pub fn top2_in_suit(&self, suit: Suit) -> u8 {
        self.cards
            .iter()
            .filter(|c| c.suit == suit && matches!(c.rank, Rank::Ace | Rank::King))
            .count() as u8
    }

    /// Count top 3 honors (A, K, Q) in hand
    pub fn top3(&self) -> u8 {
        self.cards
            .iter()
            .filter(|c| matches!(c.rank, Rank::Ace | Rank::King | Rank::Queen))
            .count() as u8
    }

    /// Count top 3 honors (A, K, Q) in specific suit
    pub fn top3_in_suit(&self, suit: Suit) -> u8 {
        self.cards
            .iter()
            .filter(|c| c.suit == suit && matches!(c.rank, Rank::Ace | Rank::King | Rank::Queen))
            .count() as u8
    }

    /// Count top 4 honors (A, K, Q, J) in hand
    pub fn top4(&self) -> u8 {
        self.cards
            .iter()
            .filter(|c| matches!(c.rank, Rank::Ace | Rank::King | Rank::Queen | Rank::Jack))
            .count() as u8
    }

    /// Count top 4 honors (A, K, Q, J) in specific suit
    pub fn top4_in_suit(&self, suit: Suit) -> u8 {
        self.cards
            .iter()
            .filter(|c| {
                c.suit == suit
                    && matches!(c.rank, Rank::Ace | Rank::King | Rank::Queen | Rank::Jack)
            })
            .count() as u8
    }

    /// Count top 5 honors (A, K, Q, J, T) in hand
    pub fn top5(&self) -> u8 {
        self.cards
            .iter()
            .filter(|c| {
                matches!(
                    c.rank,
                    Rank::Ace | Rank::King | Rank::Queen | Rank::Jack | Rank::Ten
                )
            })
            .count() as u8
    }

    /// Count top 5 honors (A, K, Q, J, T) in specific suit
    pub fn top5_in_suit(&self, suit: Suit) -> u8 {
        self.cards
            .iter()
            .filter(|c| {
                c.suit == suit
                    && matches!(
                        c.rank,
                        Rank::Ace | Rank::King | Rank::Queen | Rank::Jack | Rank::Ten
                    )
            })
            .count() as u8
    }

    /// Calculate C13 points (A=6, K=4, Q=2, J=1)
    pub fn c13(&self) -> u8 {
        self.cards
            .iter()
            .map(|c| match c.rank {
                Rank::Ace => 6,
                Rank::King => 4,
                Rank::Queen => 2,
                Rank::Jack => 1,
                _ => 0,
            })
            .sum()
    }

    /// Calculate C13 points in specific suit
    pub fn c13_in_suit(&self, suit: Suit) -> u8 {
        self.cards
            .iter()
            .filter(|c| c.suit == suit)
            .map(|c| match c.rank {
                Rank::Ace => 6,
                Rank::King => 4,
                Rank::Queen => 2,
                Rank::Jack => 1,
                _ => 0,
            })
            .sum()
    }

    /// Calculate suit quality metric (Bridge World Oct 1982)
    /// Returns quality value multiplied by 100 to use integer math
    pub fn suit_quality(&self, suit: Suit) -> i32 {
        let mut cards: Vec<Card> = self
            .cards
            .iter()
            .filter(|c| c.suit == suit)
            .copied()
            .collect();

        let length = cards.len() as i32;
        if length == 0 {
            return 0;
        }

        // Sort by rank descending
        cards.sort_by_key(|b| std::cmp::Reverse(b.rank));

        // Detect honors
        let has_ace = cards.iter().any(|c| c.rank == Rank::Ace);
        let has_king = cards.iter().any(|c| c.rank == Rank::King);
        let has_queen = cards.iter().any(|c| c.rank == Rank::Queen);
        let has_jack = cards.iter().any(|c| c.rank == Rank::Jack);
        let has_ten = cards.iter().any(|c| c.rank == Rank::Ten);
        let has_nine = cards.iter().any(|c| c.rank == Rank::Nine);
        let has_eight = cards.iter().any(|c| c.rank == Rank::Eight);

        let mut quality = 0;
        let mut higher_honors = 0;
        let suit_factor = length * 10;

        // Basic honor values
        if has_ace {
            quality += 4 * suit_factor;
            higher_honors += 1;
        }
        if has_king {
            quality += 3 * suit_factor;
            higher_honors += 1;
        }
        if has_queen {
            quality += 2 * suit_factor;
            higher_honors += 1;
        }
        if has_jack {
            quality += suit_factor;
            higher_honors += 1;
        }

        // Long suit bonus (length > 6)
        if length > 6 {
            let mut replace_count = 3;
            if has_queen {
                replace_count -= 2;
            }
            if has_jack {
                replace_count -= 1;
            }
            if replace_count > (length - 6) {
                replace_count = length - 6;
            }
            quality += replace_count * suit_factor;
        } else {
            // Short suit (length <= 6)
            if has_ten {
                if (higher_honors > 1) || has_jack {
                    quality += suit_factor;
                } else {
                    quality += suit_factor / 2;
                }
            }
            if has_nine && ((higher_honors == 2) || has_ten || has_eight) {
                quality += suit_factor / 2;
            }
        }

        quality
    }

    /// Calculate CCCC hand evaluation (Bridge World Oct 1982)
    /// Returns evaluation multiplied by 100 to use integer math
    pub fn cccc(&self) -> i32 {
        let mut eval = 0;
        let mut shape_points = 0;

        // Evaluate each suit
        for suit in [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs] {
            let mut cards: Vec<Card> = self
                .cards
                .iter()
                .filter(|c| c.suit == suit)
                .copied()
                .collect();

            let length = cards.len();

            // Shape points for short suits
            if length < 3 {
                shape_points += (3 - length) as i32 * 100;
            }

            if length == 0 {
                continue; // Void suit
            }

            // Sort by rank descending
            cards.sort_by_key(|b| std::cmp::Reverse(b.rank));

            // Detect honors
            let has_ace = cards.iter().any(|c| c.rank == Rank::Ace);
            let has_king = cards.iter().any(|c| c.rank == Rank::King);
            let has_queen = cards.iter().any(|c| c.rank == Rank::Queen);
            let has_jack = cards.iter().any(|c| c.rank == Rank::Jack);
            let has_ten = cards.iter().any(|c| c.rank == Rank::Ten);
            let has_nine = cards.iter().any(|c| c.rank == Rank::Nine);

            let mut higher_honors = 0;

            // Ace: +300
            if has_ace {
                eval += 300;
                higher_honors += 1;
            }

            // King: +200, -150 if singleton
            if has_king {
                eval += 200;
                if length == 1 {
                    eval -= 150;
                }
                higher_honors += 1;
            }

            // Queen: +100, penalties for shortage/isolation
            if has_queen {
                eval += 100;
                if length == 1 {
                    eval -= 75;
                }
                if length == 2 {
                    eval -= 25;
                }
                if higher_honors == 0 {
                    eval -= 25;
                }
                higher_honors += 1;
            }

            // Jack: bonus based on higher honors
            if has_jack {
                if higher_honors == 2 {
                    eval += 50;
                }
                if higher_honors == 1 {
                    eval += 25;
                }
                higher_honors += 1;
            }

            // Ten: bonus based on support
            if has_ten {
                if higher_honors == 2 {
                    eval += 25;
                }
                if (higher_honors == 1) && has_nine {
                    eval += 25;
                }
            }

            // Add suit quality
            eval += self.suit_quality(suit);
        }

        // Final shape adjustment
        if shape_points == 0 {
            eval -= 50;
        } else {
            eval += shape_points - 100;
        }

        eval
    }
}

impl Default for Hand {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hcp_calculation() {
        let mut hand = Hand::new();
        hand.add_card(Card::new(Suit::Spades, Rank::Ace)); // 4
        hand.add_card(Card::new(Suit::Hearts, Rank::King)); // 3
        hand.add_card(Card::new(Suit::Diamonds, Rank::Queen)); // 2
        hand.add_card(Card::new(Suit::Clubs, Rank::Jack)); // 1
        hand.add_card(Card::new(Suit::Spades, Rank::Seven)); // 0

        assert_eq!(hand.hcp(), 10);
    }

    #[test]
    fn test_suit_length() {
        let mut hand = Hand::new();
        hand.add_card(Card::new(Suit::Spades, Rank::Ace));
        hand.add_card(Card::new(Suit::Spades, Rank::King));
        hand.add_card(Card::new(Suit::Spades, Rank::Queen));
        hand.add_card(Card::new(Suit::Hearts, Rank::Ace));
        hand.add_card(Card::new(Suit::Hearts, Rank::King));

        assert_eq!(hand.suit_length(Suit::Spades), 3);
        assert_eq!(hand.suit_length(Suit::Hearts), 2);
        assert_eq!(hand.suit_length(Suit::Diamonds), 0);
        assert_eq!(hand.suit_length(Suit::Clubs), 0);
    }

    #[test]
    fn test_balanced_hand() {
        let mut hand = Hand::new();
        // Create a 4-3-3-3 hand
        for _ in 0..4 {
            hand.add_card(Card::new(Suit::Spades, Rank::Two));
        }
        for _ in 0..3 {
            hand.add_card(Card::new(Suit::Hearts, Rank::Two));
        }
        for _ in 0..3 {
            hand.add_card(Card::new(Suit::Diamonds, Rank::Two));
        }
        for _ in 0..3 {
            hand.add_card(Card::new(Suit::Clubs, Rank::Two));
        }

        assert!(hand.is_balanced());
        assert_eq!(hand.distribution(), [4, 3, 3, 3]);
    }

    #[test]
    fn test_controls() {
        let mut hand = Hand::new();
        hand.add_card(Card::new(Suit::Spades, Rank::Ace)); // 2
        hand.add_card(Card::new(Suit::Hearts, Rank::King)); // 1
        hand.add_card(Card::new(Suit::Diamonds, Rank::Ace)); // 2

        assert_eq!(hand.controls(), 5);
    }
}
