use crate::ParseError;
use dealer_core::{Card, Deal, Hand, Position, Rank, Suit};

/// Parse a deal in dealer.exe oneline format
/// Format: "n AKQT3.J6.KJ42.95 e 652.AK42.AQ87.T4 s J74.QT95.T.AK863 w 98.873.9653.QJ72"
/// Each hand is: position_char space cards_in_suit_format
pub fn parse_oneline(input: &str) -> Result<Deal, ParseError> {
    let parts: Vec<&str> = input.split_whitespace().collect();

    if parts.len() != 8 {
        return Err(ParseError {
            message: format!(
                "Expected 8 parts (4 positions + 4 hands), got {}",
                parts.len()
            ),
        });
    }

    let mut deal = Deal::new();

    // Parse each position-hand pair
    for i in 0..4 {
        let pos_str = parts[i * 2];
        let hand_str = parts[i * 2 + 1];

        let position = parse_position_char(pos_str)?;
        let hand = parse_hand(hand_str)?;

        *deal.hand_mut(position) = hand;
    }

    Ok(deal)
}

/// Format a deal in oneline format
/// Output: "n CARDS e CARDS s CARDS w CARDS\n"
pub fn format_oneline(deal: &Deal) -> String {
    let mut result = String::new();

    for &pos in &[
        Position::North,
        Position::East,
        Position::South,
        Position::West,
    ] {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push(position_char(pos));
        result.push(' ');
        result.push_str(&format_hand(deal.hand(pos)));
    }

    result.push('\n');
    result
}

/// Parse a single character position (n, e, s, w)
fn parse_position_char(s: &str) -> Result<Position, ParseError> {
    match s.to_lowercase().as_str() {
        "n" => Ok(Position::North),
        "e" => Ok(Position::East),
        "s" => Ok(Position::South),
        "w" => Ok(Position::West),
        _ => Err(ParseError {
            message: format!("Invalid position character: {}", s),
        }),
    }
}

/// Get lowercase position character
fn position_char(pos: Position) -> char {
    match pos {
        Position::North => 'n',
        Position::East => 'e',
        Position::South => 's',
        Position::West => 'w',
    }
}

/// Parse a hand in format: Spades.Hearts.Diamonds.Clubs
/// Example: "AKQT3.J6.KJ42.95" or ".QJ8.Q95432.AQ97" (void spades)
fn parse_hand(s: &str) -> Result<Hand, ParseError> {
    let suits_str: Vec<&str> = s.split('.').collect();
    if suits_str.len() != 4 {
        return Err(ParseError {
            message: format!(
                "Expected 4 suits separated by dots, got {}",
                suits_str.len()
            ),
        });
    }

    let mut hand = Hand::new();
    let suits = [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs];

    for (suit_idx, &suit_str) in suits_str.iter().enumerate() {
        let suit = suits[suit_idx];

        // Empty string means void suit
        if suit_str.is_empty() {
            continue;
        }

        // Parse each card rank in the suit
        for c in suit_str.chars() {
            let rank = parse_rank(c)?;
            hand.add_card(Card::new(suit, rank));
        }
    }

    Ok(hand)
}

/// Format a hand in Spades.Hearts.Diamonds.Clubs format
fn format_hand(hand: &Hand) -> String {
    let suits = [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs];
    let mut result = Vec::new();

    for &suit in &suits {
        let cards = hand.cards_in_suit(suit);
        if cards.is_empty() {
            result.push(String::new());
        } else {
            // Sort by rank descending (Ace first)
            let mut cards = cards;
            cards.sort_by_key(|b| std::cmp::Reverse(b.rank));

            let suit_str: String = cards.iter().map(|c| rank_char(c.rank)).collect();
            result.push(suit_str);
        }
    }

    result.join(".")
}

/// Parse a rank character
fn parse_rank(c: char) -> Result<Rank, ParseError> {
    match c.to_uppercase().next().unwrap() {
        'A' => Ok(Rank::Ace),
        'K' => Ok(Rank::King),
        'Q' => Ok(Rank::Queen),
        'J' => Ok(Rank::Jack),
        'T' => Ok(Rank::Ten),
        '9' => Ok(Rank::Nine),
        '8' => Ok(Rank::Eight),
        '7' => Ok(Rank::Seven),
        '6' => Ok(Rank::Six),
        '5' => Ok(Rank::Five),
        '4' => Ok(Rank::Four),
        '3' => Ok(Rank::Three),
        '2' => Ok(Rank::Two),
        _ => Err(ParseError {
            message: format!("Invalid rank character: {}", c),
        }),
    }
}

/// Get rank character (uppercase)
fn rank_char(rank: Rank) -> char {
    match rank {
        Rank::Ace => 'A',
        Rank::King => 'K',
        Rank::Queen => 'Q',
        Rank::Jack => 'J',
        Rank::Ten => 'T',
        Rank::Nine => '9',
        Rank::Eight => '8',
        Rank::Seven => '7',
        Rank::Six => '6',
        Rank::Five => '5',
        Rank::Four => '4',
        Rank::Three => '3',
        Rank::Two => '2',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_oneline() {
        let input = "n AKQT3.J6.KJ42.95 e 652.AK42.AQ87.T4 s J74.QT95.T.AK863 w 98.873.9653.QJ72";

        let deal = parse_oneline(input).unwrap();

        // Check north hand
        let north = deal.hand(Position::North);
        assert_eq!(north.len(), 13);
        assert_eq!(north.suit_length(Suit::Spades), 5);
        assert_eq!(north.suit_length(Suit::Hearts), 2);
        assert_eq!(north.suit_length(Suit::Diamonds), 4);
        assert_eq!(north.suit_length(Suit::Clubs), 2);
    }

    #[test]
    fn test_format_oneline() {
        let input = "n AKQT3.J6.KJ42.95 e 652.AK42.AQ87.T4 s J74.QT95.T.AK863 w 98.873.9653.QJ72";

        let deal = parse_oneline(input).unwrap();
        let output = format_oneline(&deal);

        // Parse both and compare
        let reparsed = parse_oneline(&output).unwrap();
        assert_eq!(deal, reparsed);
    }

    #[test]
    fn test_parse_void_suit() {
        // Spades void in south hand
        let input = "n AKQT3.J6.KJ42.95 e 652.AK42.AQ87.T4 s .QJ8.Q95432.AQ97 w J74.T953.T6.K863";

        let deal = parse_oneline(input).unwrap();
        let south = deal.hand(Position::South);

        assert_eq!(south.suit_length(Suit::Spades), 0);
        assert_eq!(south.len(), 13);
    }

    #[test]
    fn test_round_trip() {
        let input = "n A754.7642.KJ2.A9 e QT.AK95.87.K8652 s K93.J83.QT6543.T w J862.QT.A9.QJ743";

        let deal = parse_oneline(input).unwrap();
        let output = format_oneline(&deal);
        let reparsed = parse_oneline(&output).unwrap();

        assert_eq!(deal, reparsed);
    }
}
