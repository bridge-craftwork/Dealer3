//! Conversion between dealer-core types and bridge-types types.
//!
//! dealer-core defines its own Hand and Deal types with generator-specific methods,
//! while bridge-types defines Hand and Deal with evaluation/parsing methods.
//! This module provides conversions between them.

use crate::Position;

impl From<&crate::Hand> for bridge_types::Hand {
    fn from(hand: &crate::Hand) -> Self {
        bridge_types::Hand::from_cards(hand.cards().to_vec())
    }
}

impl From<crate::Hand> for bridge_types::Hand {
    fn from(hand: crate::Hand) -> Self {
        bridge_types::Hand::from_cards(hand.cards().to_vec())
    }
}

// Inbound is fallible where outbound is not, and the asymmetry is the point.
// A `crate::Hand` holds at most thirteen cards, so anything this program builds
// converts out cleanly; a `bridge_types::Hand` read from a file has whatever the
// file said. `Hand::from_cards` asserts, and an assert on untrusted input is a
// panic — a PBN board with a fourteen-card hand took the whole run down with
// `a hand cannot hold more than 13 cards` and no mention of the file it came
// from.
//
// Fewer than thirteen is *not* refused here: a partial hand is a real thing
// mid-predeal. Whether a whole deal is complete is a question about the deal,
// and [`crate::Deal::check_complete`] answers it where completeness is required.

impl TryFrom<&bridge_types::Hand> for crate::Hand {
    type Error = String;

    fn try_from(hand: &bridge_types::Hand) -> Result<Self, Self::Error> {
        let cards = hand.cards();
        if cards.len() > crate::hand::MAX_CARDS {
            return Err(format!(
                "a hand cannot hold more than {} cards, and this one has {}",
                crate::hand::MAX_CARDS,
                cards.len()
            ));
        }
        Ok(crate::Hand::from_cards(cards.to_vec()))
    }
}

impl TryFrom<bridge_types::Hand> for crate::Hand {
    type Error = String;

    fn try_from(hand: bridge_types::Hand) -> Result<Self, Self::Error> {
        (&hand).try_into()
    }
}

impl From<&crate::Deal> for bridge_types::Deal {
    fn from(deal: &crate::Deal) -> Self {
        let mut bt_deal = bridge_types::Deal::new();
        bt_deal.set_hand(
            bridge_types::Direction::North,
            deal.hand(Position::North).into(),
        );
        bt_deal.set_hand(
            bridge_types::Direction::East,
            deal.hand(Position::East).into(),
        );
        bt_deal.set_hand(
            bridge_types::Direction::South,
            deal.hand(Position::South).into(),
        );
        bt_deal.set_hand(
            bridge_types::Direction::West,
            deal.hand(Position::West).into(),
        );
        bt_deal
    }
}

impl From<crate::Deal> for bridge_types::Deal {
    fn from(deal: crate::Deal) -> Self {
        (&deal).into()
    }
}

impl TryFrom<&bridge_types::Deal> for crate::Deal {
    type Error = String;

    fn try_from(deal: &bridge_types::Deal) -> Result<Self, Self::Error> {
        let mut dc_deal = crate::Deal::new();
        for (position, direction) in [
            (Position::North, bridge_types::Direction::North),
            (Position::East, bridge_types::Direction::East),
            (Position::South, bridge_types::Direction::South),
            (Position::West, bridge_types::Direction::West),
        ] {
            *dc_deal.hand_mut(position) = deal
                .hand(direction)
                .try_into()
                // Which seat, because a file with one bad board says nothing
                // about which board or which hand without it.
                .map_err(|e| format!("{}: {}", position, e))?;
        }
        Ok(dc_deal)
    }
}

impl TryFrom<bridge_types::Deal> for crate::Deal {
    type Error = String;

    fn try_from(deal: bridge_types::Deal) -> Result<Self, Self::Error> {
        (&deal).try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Card, Rank, Suit};

    #[test]
    fn test_hand_round_trip() {
        let mut hand = crate::Hand::new();
        hand.add_card(Card::new(Suit::Spades, Rank::Ace));
        hand.add_card(Card::new(Suit::Hearts, Rank::King));

        let bt_hand: bridge_types::Hand = (&hand).into();
        let back: crate::Hand = bt_hand.try_into().expect("two cards fit in a hand");

        assert_eq!(hand.len(), back.len());
        assert_eq!(hand.hcp(), back.hcp());
    }

    #[test]
    fn test_deal_round_trip() {
        let mut deal = crate::Deal::new();
        deal.hand_mut(Position::North)
            .add_card(Card::new(Suit::Spades, Rank::Ace));
        deal.hand_mut(Position::East)
            .add_card(Card::new(Suit::Hearts, Rank::King));

        let bt_deal: bridge_types::Deal = (&deal).into();
        let back: crate::Deal = bt_deal.try_into().expect("a deal that came from one");

        assert_eq!(
            deal.hand(Position::North).len(),
            back.hand(Position::North).len()
        );
        assert_eq!(
            deal.hand(Position::East).len(),
            back.hand(Position::East).len()
        );
    }
}
