//! The original dealer's `-2` and `-3` swapping modes.
//!
//! One shuffle, several deals: the cards are dealt once and then the hands are
//! exchanged between seats, so a script sees the same cards arranged two or six
//! different ways before the next shuffle. `-2` exchanges East and West; `-3`
//! runs East, South and West through all six of their arrangements. North's
//! hand never moves in either.
//!
//! It exists because the same deal seen from several defensive layouts is what
//! a lead or defence simulation actually wants, and because a shuffle used to
//! be the expensive part. The deals it produces are deliberately not
//! independent samples — that is the point of it, not a flaw, but it does mean
//! a `-3` run reports six times as many deals as it has shuffles behind them.
//!
//! In the original, the swap moves whole hands without telling the shuffle,
//! which tracks predealt cards by their position in the deal array. Predealt
//! cards therefore end up in the wrong seat on the first swap and vanish
//! entirely by the second shuffle, silently. Here the swap is a pure function
//! of a correctly dealt base deal, so a predeal to a seat the swap leaves alone
//! keeps working; the caller is expected to refuse the combinations that would
//! move predealt cards, which is what [`SwapMode::moves`] is for.

use crate::{Deal, Position};

/// How many deals one shuffle produces, and how the hands are arranged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SwapMode {
    /// One deal per shuffle — the original's `-0`, and the default.
    #[default]
    None,
    /// Two deals per shuffle: as dealt, then with East and West exchanged.
    /// The original's `-2`.
    TwoWay,
    /// Six deals per shuffle: East, South and West in every arrangement.
    /// The original's `-3`.
    ThreeWay,
}

/// Where each seat's cards come from in the base deal, in `Position::ALL`
/// order, for each variant of each mode.
///
/// Read a row as "North keeps its own, East takes what *this* seat was dealt,
/// …". The original arrives at the same six by applying transpositions to a
/// running deal — swap(E,W), swap(S,W), swap(E,S), swap(E,W), swap(S,W) — which
/// is only a different way of writing the same sequence out.
const TWO_WAY: [[Position; 4]; 2] = [
    [
        Position::North,
        Position::East,
        Position::South,
        Position::West,
    ],
    [
        Position::North,
        Position::West,
        Position::South,
        Position::East,
    ],
];

const THREE_WAY: [[Position; 4]; 6] = [
    [
        Position::North,
        Position::East,
        Position::South,
        Position::West,
    ],
    [
        Position::North,
        Position::West,
        Position::South,
        Position::East,
    ],
    [
        Position::North,
        Position::West,
        Position::East,
        Position::South,
    ],
    [
        Position::North,
        Position::East,
        Position::West,
        Position::South,
    ],
    [
        Position::North,
        Position::South,
        Position::West,
        Position::East,
    ],
    [
        Position::North,
        Position::South,
        Position::East,
        Position::West,
    ],
];

impl SwapMode {
    /// How many deals this mode makes of one shuffle.
    pub fn deals_per_shuffle(self) -> usize {
        self.arrangements().len()
    }

    /// The seats whose cards a swap moves.
    ///
    /// A predeal to any other seat survives the swap untouched, so this is the
    /// list to check a predeal against rather than refusing the two outright.
    pub fn moves(self) -> &'static [Position] {
        match self {
            SwapMode::None => &[],
            SwapMode::TwoWay => &[Position::East, Position::West],
            SwapMode::ThreeWay => &[Position::East, Position::South, Position::West],
        }
    }

    /// The switch that asks for this mode, for error messages.
    pub fn switch(self) -> &'static str {
        match self {
            SwapMode::None => "-0",
            SwapMode::TwoWay => "-2",
            SwapMode::ThreeWay => "-3",
        }
    }

    fn arrangements(self) -> &'static [[Position; 4]] {
        match self {
            SwapMode::None => &TWO_WAY[..1],
            SwapMode::TwoWay => &TWO_WAY,
            SwapMode::ThreeWay => &THREE_WAY,
        }
    }

    /// Variant `variant` of `deal`, where variant 0 is the deal as dealt.
    ///
    /// # Panics
    ///
    /// If `variant` is not below [`deals_per_shuffle`].
    ///
    /// [`deals_per_shuffle`]: SwapMode::deals_per_shuffle
    pub fn apply(self, deal: &Deal, variant: usize) -> Deal {
        let sources = &self.arrangements()[variant];
        let mut swapped = Deal::new();
        for (seat, source) in Position::ALL.into_iter().zip(sources) {
            *swapped.hand_mut(seat) = *deal.hand(*source);
        }
        swapped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Card, Rank, Suit};

    /// A deal whose hands are trivially distinguishable: each seat holds one
    /// suit, so a hand can be named by the suit in it.
    fn one_suit_each() -> Deal {
        let mut deal = Deal::new();
        for (position, suit) in [
            (Position::North, Suit::Spades),
            (Position::East, Suit::Hearts),
            (Position::South, Suit::Diamonds),
            (Position::West, Suit::Clubs),
        ] {
            for rank in Rank::ALL {
                deal.hand_mut(position).add_card(Card::new(suit, rank));
            }
        }
        deal
    }

    fn seats(deal: &Deal) -> [Suit; 4] {
        Position::ALL.map(|p| deal.hand(p).cards()[0].suit)
    }

    #[test]
    fn variant_zero_is_the_deal_as_dealt() {
        let deal = one_suit_each();
        for mode in [SwapMode::None, SwapMode::TwoWay, SwapMode::ThreeWay] {
            assert_eq!(mode.apply(&deal, 0), deal, "{:?}", mode);
        }
    }

    #[test]
    fn two_way_exchanges_east_and_west_only() {
        let deal = one_suit_each();
        assert_eq!(SwapMode::TwoWay.deals_per_shuffle(), 2);
        assert_eq!(
            seats(&SwapMode::TwoWay.apply(&deal, 1)),
            [Suit::Spades, Suit::Clubs, Suit::Diamonds, Suit::Hearts]
        );
    }

    /// The six arrangements the original prints, in its order. Taken from a run
    /// of the reference binary at seed 1, not from reading its source.
    #[test]
    fn three_way_matches_the_originals_order() {
        let deal = one_suit_each();
        let (n, e, s, w) = (Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs);
        let expected = [
            [n, e, s, w],
            [n, w, s, e],
            [n, w, e, s],
            [n, e, w, s],
            [n, s, w, e],
            [n, s, e, w],
        ];
        assert_eq!(SwapMode::ThreeWay.deals_per_shuffle(), 6);
        for (variant, expected) in expected.into_iter().enumerate() {
            assert_eq!(
                seats(&SwapMode::ThreeWay.apply(&deal, variant)),
                expected,
                "variant {}",
                variant
            );
        }
    }

    #[test]
    fn every_arrangement_is_a_distinct_permutation() {
        let deal = one_suit_each();
        for mode in [SwapMode::TwoWay, SwapMode::ThreeWay] {
            let mut seen = Vec::new();
            for variant in 0..mode.deals_per_shuffle() {
                let arrangement = seats(&mode.apply(&deal, variant));
                assert!(
                    !seen.contains(&arrangement),
                    "{:?} repeats an arrangement at variant {}",
                    mode,
                    variant
                );
                seen.push(arrangement);
            }
        }
    }

    #[test]
    fn north_never_moves_and_the_cards_are_conserved() {
        let deal = one_suit_each();
        for mode in [SwapMode::None, SwapMode::TwoWay, SwapMode::ThreeWay] {
            for variant in 0..mode.deals_per_shuffle() {
                let swapped = mode.apply(&deal, variant);
                assert_eq!(swapped.hand(Position::North), deal.hand(Position::North));
                let mut cards: Vec<u8> = Position::ALL
                    .iter()
                    .flat_map(|p| swapped.hand(*p).cards().iter().map(|c| c.to_index()))
                    .collect();
                cards.sort_unstable();
                cards.dedup();
                assert_eq!(cards.len(), 52, "{:?} variant {}", mode, variant);
            }
        }
    }

    /// Only the seats `moves` names may differ from the base deal — that is
    /// what lets a predeal to any other seat stand.
    #[test]
    fn only_the_seats_it_names_are_moved() {
        let deal = one_suit_each();
        for mode in [SwapMode::None, SwapMode::TwoWay, SwapMode::ThreeWay] {
            for variant in 0..mode.deals_per_shuffle() {
                let swapped = mode.apply(&deal, variant);
                for seat in Position::ALL {
                    if !mode.moves().contains(&seat) {
                        assert_eq!(
                            swapped.hand(seat),
                            deal.hand(seat),
                            "{:?} moved {:?} at variant {}",
                            mode,
                            seat,
                            variant
                        );
                    }
                }
            }
        }
    }
}
