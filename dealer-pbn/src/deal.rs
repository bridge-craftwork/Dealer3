use dealer_core::{Card, Deal, Hand, Position, Rank, Suit};

/// Error type for PBN parsing
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "PBN parse error: {}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// Represents a deal in PBN format
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PbnDeal {
    pub first_seat: Position,
    pub deal: Deal,
}

/// Parse a PBN [Deal "..."] tag
/// Format: [Deal "N:KQ4.QJ982..AKQ43 J653.A73.985.J97 9.K54.KQT732.652 AT872.T6.AJ64.T8"]
/// Position:Spades.Hearts.Diamonds.Clubs for each hand (clockwise from position)
pub fn parse_deal_tag(input: &str) -> Result<PbnDeal, ParseError> {
    // Remove [Deal " and trailing "]
    let trimmed = input.trim();

    if !trimmed.starts_with("[Deal \"") {
        return Err(ParseError {
            message: "Expected [Deal \"...\" format".to_string(),
        });
    }

    if !trimmed.ends_with("\"]") {
        return Err(ParseError {
            message: "Expected closing \"]".to_string(),
        });
    }

    // Extract content between quotes
    let content = &trimmed[7..trimmed.len() - 2]; // Skip [Deal " and "]

    // Split on colon to get position and hands
    let parts: Vec<&str> = content.split(':').collect();
    if parts.len() != 2 {
        return Err(ParseError {
            message: "Expected Position:Hands format".to_string(),
        });
    }

    // Parse starting position
    let first_seat = parse_position(parts[0])?;

    // Parse hands (separated by spaces)
    let hands_str: Vec<&str> = parts[1].split_whitespace().collect();
    if hands_str.len() != 4 {
        return Err(ParseError {
            message: format!("Expected 4 hands, got {}", hands_str.len()),
        });
    }

    // Parse each hand
    let mut hands = Vec::new();
    for hand_str in hands_str {
        hands.push(parse_hand(hand_str)?);
    }

    // Assign hands to positions (clockwise from first_seat)
    let mut deal = Deal::new();
    for (i, hand) in hands.into_iter().enumerate() {
        let pos = rotate_position(first_seat, i);
        *deal.hand_mut(pos) = hand;
    }

    Ok(PbnDeal { first_seat, deal })
}

/// Format a Deal as a PBN [Deal "..."] tag
pub fn format_deal_tag(deal: &Deal, first_seat: Position) -> String {
    let mut result = String::from("[Deal \"");

    // Add position
    result.push(position_char(first_seat));
    result.push(':');

    // Add hands in clockwise order from first_seat
    for i in 0..4 {
        if i > 0 {
            result.push(' ');
        }
        let pos = rotate_position(first_seat, i);
        result.push_str(&format_hand(deal.hand(pos)));
    }

    result.push_str("\"]");
    result
}

/// Parse a position character
fn parse_position(s: &str) -> Result<Position, ParseError> {
    match s.trim().to_uppercase().as_str() {
        "N" => Ok(Position::North),
        "E" => Ok(Position::East),
        "S" => Ok(Position::South),
        "W" => Ok(Position::West),
        _ => Err(ParseError {
            message: format!("Invalid position: {}", s),
        }),
    }
}

/// Get position character
fn position_char(pos: Position) -> char {
    match pos {
        Position::North => 'N',
        Position::East => 'E',
        Position::South => 'S',
        Position::West => 'W',
    }
}

/// Rotate position clockwise by n steps
fn rotate_position(start: Position, steps: usize) -> Position {
    let positions = [
        Position::North,
        Position::East,
        Position::South,
        Position::West,
    ];
    let start_idx = positions.iter().position(|&p| p == start).unwrap();
    let new_idx = (start_idx + steps) % 4;
    positions[new_idx]
}

/// Parse a hand in PBN format: Spades.Hearts.Diamonds.Clubs
/// Example: "KQ4.QJ982..AKQ43" (void diamond suit shown as empty)
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

/// Format a hand in PBN format
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
    fn test_parse_deal_tag() {
        let input =
            r#"[Deal "N:KQ4.QJ982..AKQ43 J653.A73.985.J97 9.K54.KQT732.652 AT872.T6.AJ64.T8"]"#;

        let pbn_deal = parse_deal_tag(input).unwrap();

        assert_eq!(pbn_deal.first_seat, Position::North);

        // Check north hand
        let north = pbn_deal.deal.hand(Position::North);
        assert_eq!(north.len(), 13);
        assert_eq!(north.suit_length(Suit::Spades), 3);
        assert_eq!(north.suit_length(Suit::Hearts), 5);
        assert_eq!(north.suit_length(Suit::Diamonds), 0); // Void
        assert_eq!(north.suit_length(Suit::Clubs), 5);
    }

    #[test]
    fn test_format_deal_tag() {
        let input =
            r#"[Deal "N:KQ4.QJ982..AKQ43 J653.A73.985.J97 9.K54.KQT732.652 AT872.T6.AJ64.T8"]"#;

        let pbn_deal = parse_deal_tag(input).unwrap();
        let output = format_deal_tag(&pbn_deal.deal, pbn_deal.first_seat);

        // Parse both and compare deals (format might differ in card order)
        let reparsed = parse_deal_tag(&output).unwrap();
        assert_eq!(pbn_deal.deal, reparsed.deal);
        assert_eq!(pbn_deal.first_seat, reparsed.first_seat);
    }

    #[test]
    fn test_round_trip() {
        let input =
            r#"[Deal "S:AKQ.JT9.876.5432 2.AKQ.JT9.AKQ876 JT9.876.5432.JT9 87654.5432.AKQ."]"#;

        let pbn_deal = parse_deal_tag(input).unwrap();
        let output = format_deal_tag(&pbn_deal.deal, pbn_deal.first_seat);
        let reparsed = parse_deal_tag(&output).unwrap();

        assert_eq!(pbn_deal.deal, reparsed.deal);
        assert_eq!(pbn_deal.first_seat, reparsed.first_seat);
    }

    #[test]
    fn test_parse_void_suit() {
        // West has void clubs (need 13 cards total, with 0 in clubs)
        // N: 3+5+0+5=13, E: 4+3+3+3=13, S: 2+3+6+2=13, W: 5+3+5+0=13, Total: 52
        let input =
            r#"[Deal "N:KQ4.QJ982..AKQ43 J653.A74.985.J97 98.K32.KQT732.65 AT872.T65.AJ642."]"#;

        let pbn_deal = parse_deal_tag(input).unwrap();
        let west = pbn_deal.deal.hand(Position::West);

        assert_eq!(west.suit_length(Suit::Clubs), 0);
        assert_eq!(west.len(), 13);
    }

    #[test]
    fn test_parse_dealer_exe_output() {
        // These are actual outputs from dealer.exe (test1-hcp-seed1.pbn)
        let deals = [
            r#"[Deal "N:KQ4.QJ982..AKQ43 J653.A73.985.J97 9.K54.KQT732.652 AT872.T6.AJ64.T8"]"#,
            r#"[Deal "N:AQ62.942.KQ.AJ64 73.7.J8742.KQ532 KJ54.QJ3.653.T98 T98.AKT865.AT9.7"]"#,
            r#"[Deal "N:T9.KQ54.Q.AKJT72 AK64.A87.743.864 QJ3.J62.AT862.Q5 8752.T93.KJ95.93"]"#,
        ];

        for deal_str in &deals {
            let pbn_deal = parse_deal_tag(deal_str).unwrap();

            // Verify all hands have 13 cards
            for pos in [
                Position::North,
                Position::East,
                Position::South,
                Position::West,
            ] {
                assert_eq!(
                    pbn_deal.deal.hand(pos).len(),
                    13,
                    "Position {:?} should have 13 cards",
                    pos
                );
            }

            // Verify round-trip: format and re-parse should produce same deal
            let formatted = format_deal_tag(&pbn_deal.deal, pbn_deal.first_seat);
            let reparsed = parse_deal_tag(&formatted).unwrap();
            assert_eq!(pbn_deal.deal, reparsed.deal);
        }
    }
}
