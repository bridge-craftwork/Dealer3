use chrono::{Datelike, Local};
use dealer_core::{Deal, Position, Rank, Suit};

/// Print format for outputting deals
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintFormat {
    /// Print all 4 hands in newspaper-style columns (default)
    PrintAll,
    /// Print only East/West hands
    PrintEW,
    /// Print in PBN format with all metadata
    PrintPBN,
    /// Print in compact 4-line format
    PrintCompact,
    /// Print in single-line format
    PrintOneLine,
}

/// Format a deal in "printall" format (newspaper-style columns)
///
/// Example output:
/// ```text
///    1.
/// J 7 3               9 8                 A Q 5 4 2           K T 6
/// 3                   9 6 4 2             K J 8 7             A Q T 5
/// K Q J T 9 8 5       7                   3 2                 A 6 4
/// T 5                 9 8 7 4 3 2         A K                 Q J 6
/// ```
pub fn format_printall(deal: &Deal, board_number: usize) -> String {
    let mut result = String::new();

    // Print board number
    result.push_str(&format!("{:4}.\n", board_number + 1));

    // Print each suit row (spades, hearts, diamonds, clubs)
    let suits = [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs];
    let positions = [
        Position::North,
        Position::East,
        Position::South,
        Position::West,
    ];

    for &suit in &suits {
        let mut cards_count = 10;

        for &pos in &positions {
            // Add padding to align columns
            while cards_count < 10 {
                result.push_str("  ");
                cards_count += 1;
            }
            cards_count = 0;

            // Get cards in this suit for this position
            let hand = deal.hand(pos);
            let mut cards: Vec<_> = hand.cards_in_suit(suit);
            cards.sort_by_key(|b| std::cmp::Reverse(b.rank)); // High to low

            if cards.is_empty() {
                result.push_str("- ");
                cards_count = 1;
            } else {
                for card in cards {
                    result.push_str(&format!("{} ", rank_char(card.rank)));
                    cards_count += 1;
                }
            }
        }
        result.push('\n');
    }
    result.push('\n');

    result
}

/// Format a deal in "printew" format (East/West hands only)
///
/// Example output:
/// ```text
/// K T 6               9 8
/// A Q T 5             9 6 4 2
/// A 6 4               7
/// Q J 6               9 8 7 4 3 2
/// ```
pub fn format_printew(deal: &Deal) -> String {
    let mut result = String::new();

    // Print each suit row (spades, hearts, diamonds, clubs)
    let suits = [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs];
    let positions = [Position::West, Position::East];

    for &suit in &suits {
        let mut cards_count = 10;

        for &pos in &positions {
            // Add padding to align columns
            while cards_count < 10 {
                result.push_str("  ");
                cards_count += 1;
            }
            cards_count = 0;

            // Get cards in this suit for this position
            let hand = deal.hand(pos);
            let mut cards: Vec<_> = hand.cards_in_suit(suit);
            cards.sort_by_key(|b| std::cmp::Reverse(b.rank)); // High to low

            if cards.is_empty() {
                result.push_str("- ");
                cards_count = 1;
            } else {
                for card in cards {
                    result.push_str(&format!("{} ", rank_char(card.rank)));
                    cards_count += 1;
                }
            }
        }
        result.push('\n');
    }
    result.push('\n');

    result
}

/// Everything a PBN record carries besides the cards themselves.
///
/// A struct rather than seven more parameters: the list had outgrown what
/// anyone can read at a call site, and every field is optional in the sense
/// that a sensible record exists without it.
#[derive(Debug, Default, Clone)]
pub struct PbnBoard<'a> {
    /// Zero-based; the tag is written one higher.
    pub board_number: usize,
    /// Rotates with the board number when absent, as the original does.
    pub dealer: Option<Position>,
    /// Rotates with the board number when absent.
    pub vulnerability: Option<Vulnerability>,
    /// Fills the `[Event]` tag; otherwise it describes the run.
    pub event_name: Option<&'a str>,
    pub seed: Option<u32>,
    pub input_file: Option<&'a str>,
    /// The category this deal matched, from the script's `HandType_*`
    /// variables. Written as `[HandType "..."]`, which is not a standard tag —
    /// readers ignore what they do not know, so the file stays a PBN file.
    pub hand_type: Option<&'a str>,
}

/// Format a deal in PBN (Portable Bridge Notation) format.
///
/// Writes the standard tags — Event, Site, Date, Board, the four player
/// placeholders, Dealer, Vulnerable, Deal and the contract placeholders — plus
/// `[HandType]` when [`PbnBoard::hand_type`] is set.
pub fn format_printpbn(deal: &Deal, board: &PbnBoard) -> String {
    let PbnBoard {
        board_number,
        dealer,
        vulnerability,
        event_name,
        seed,
        input_file,
        hand_type,
    } = *board;
    let mut result = String::new();

    // Event tag - title takes precedence over seed/file
    // Format matches dealer.exe: "Hand simulated by dealer with file <path>, seed <n>"
    if let Some(title) = event_name {
        result.push_str(&format!("[Event \"{}\"]\n", title));
    } else {
        let mut event = String::from("Hand simulated by dealer");
        if let Some(file) = input_file {
            event.push_str(&format!(" with file {}", file));
        }
        if let Some(seed_val) = seed {
            event.push_str(&format!(", seed {}", seed_val));
        }
        result.push_str(&format!("[Event \"{}\"]\n", event));
    }

    // Site and Date
    result.push_str("[Site \"-\"]\n");

    // Current date in PBN format (YYYY.MM.DD)
    let now = Local::now();
    result.push_str(&format!(
        "[Date \"{:04}.{:02}.{:02}\"]\n",
        now.year(),
        now.month(),
        now.day()
    ));

    result.push_str(&format!("[Board \"{}\"]\n", board_number + 1));

    // Which category of hand this is, when the script named its types. Not a
    // standard PBN tag, but readers ignore tags they do not know, so a file
    // carrying it stays a PBN file everywhere else.
    if let Some(name) = hand_type {
        result.push_str(&format!("[HandType \"{}\"]\n", name));
    }

    // Player names (placeholders)
    result.push_str("[West \"-\"]\n");
    result.push_str("[North \"-\"]\n");
    result.push_str("[East \"-\"]\n");
    result.push_str("[South \"-\"]\n");

    // Dealer - rotates by board number if not specified
    let dealer_pos = dealer.unwrap_or(match board_number % 4 {
        0 => Position::North,
        1 => Position::East,
        2 => Position::South,
        _ => Position::West,
    });
    result.push_str(&format!(
        "[Dealer \"{}\"]\n",
        position_char_upper(dealer_pos)
    ));

    // Vulnerability - rotates by board number if not specified
    let vuln = vulnerability.unwrap_or_else(|| {
        // Standard rotation: None, NS, EW, All, NS, EW, All, None, EW, All, None, NS, All, None, NS, EW
        let board_vul = [0, 1, 2, 3, 1, 2, 3, 0, 2, 3, 0, 1, 3, 0, 1, 2];
        match board_vul[board_number % 16] {
            0 => Vulnerability::None,
            1 => Vulnerability::NS,
            2 => Vulnerability::EW,
            _ => Vulnerability::All,
        }
    });
    result.push_str(&format!(
        "[Vulnerable \"{}\"]\n",
        vulnerability_string(vuln)
    ));

    // Deal tag
    result.push_str("[Deal \"N:");
    for pos in [
        Position::North,
        Position::East,
        Position::South,
        Position::West,
    ] {
        let hand = deal.hand(pos);
        for suit in [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs] {
            let mut cards: Vec<_> = hand.cards_in_suit(suit);
            cards.sort_by_key(|b| std::cmp::Reverse(b.rank)); // High to low

            for card in cards {
                result.push(rank_char(card.rank));
            }

            if suit != Suit::Clubs {
                result.push('.');
            }
        }
        if pos != Position::West {
            result.push(' ');
        }
    }
    result.push_str("\"]\n");

    // Placeholder tags for game info
    result.push_str("[Declarer \"?\"]\n");
    result.push_str("[Contract \"?\"]\n");
    result.push_str("[Result \"?\"]\n");
    result.push('\n');

    result
}

/// Vulnerability enum for PBN format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vulnerability {
    None,
    NS,
    EW,
    All,
}

fn vulnerability_string(vuln: Vulnerability) -> &'static str {
    match vuln {
        Vulnerability::None => "None",
        Vulnerability::NS => "NS",
        Vulnerability::EW => "EW",
        Vulnerability::All => "All",
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

/// Get position character (uppercase)
fn position_char_upper(pos: Position) -> char {
    match pos {
        Position::North => 'N',
        Position::East => 'E',
        Position::South => 'S',
        Position::West => 'W',
    }
}

/// Get position character (lowercase)
fn position_char_lower(pos: Position) -> char {
    match pos {
        Position::North => 'n',
        Position::East => 'e',
        Position::South => 's',
        Position::West => 'w',
    }
}

/// Format a deal in "printcompact" format (4 lines, one per position)
///
/// Example output:
/// ```text
/// n KQ4.QJ982..AKQ43
/// e J653.A73.985.J97
/// s 9.K54.KQT732.652
/// w AT872.T6.AJ64.T8
/// ```
pub fn format_printcompact(deal: &Deal) -> String {
    let mut result = String::new();

    for pos in [
        Position::North,
        Position::East,
        Position::South,
        Position::West,
    ] {
        result.push(position_char_lower(pos));
        result.push(' ');

        let hand = deal.hand(pos);
        for suit in [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs] {
            let mut cards: Vec<_> = hand.cards_in_suit(suit);
            cards.sort_by_key(|b| std::cmp::Reverse(b.rank)); // High to low

            for card in cards {
                result.push(rank_char(card.rank));
            }

            if suit != Suit::Clubs {
                result.push('.');
            }
        }
        result.push('\n');
    }

    result
}

/// Format a single hand in PBN format (without position prefix)
/// Returns a string like "AKQ.JT9.876.5432"
pub fn format_hand_pbn(hand: &dealer_core::Hand) -> String {
    let mut result = String::new();

    for suit in [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs] {
        let mut cards: Vec<_> = hand.cards_in_suit(suit);
        cards.sort_by_key(|b| std::cmp::Reverse(b.rank)); // High to low

        for card in cards {
            result.push(rank_char(card.rank));
        }

        if suit != Suit::Clubs {
            result.push('.');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use dealer_core::FastDealGenerator;

    #[test]
    fn test_format_printall() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let output = format_printall(&deal, 0);

        // Should contain board number
        assert!(output.contains("   1."));
        // Should have 5 lines (board number + 4 suits + blank)
        assert_eq!(output.lines().count(), 6);
    }

    #[test]
    fn test_format_printew() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let output = format_printew(&deal);

        // Should have 5 lines (4 suits + blank)
        assert_eq!(output.lines().count(), 5);
        // Should not contain North or South hands
        // (checked by ensuring output is shorter than printall)
        let printall_output = format_printall(&deal, 0);
        assert!(output.len() < printall_output.len());
    }

    #[test]
    fn test_format_printpbn() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let output = format_printpbn(
            &deal,
            &PbnBoard {
                board_number: 0,
                seed: Some(1),
                ..Default::default()
            },
        );

        // Should contain standard PBN tags
        assert!(output.contains("[Event "));
        assert!(output.contains("[Board \"1\"]"));
        assert!(output.contains("[Dealer "));
        assert!(output.contains("[Vulnerable "));
        assert!(output.contains("[Deal \"N:"));
    }

    #[test]
    fn test_printpbn_dealer_rotation() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();

        // Board 0 -> North dealer
        let output0 = format_printpbn(
            &deal,
            &PbnBoard {
                board_number: 0,
                ..Default::default()
            },
        );
        assert!(output0.contains("[Dealer \"N\"]"));

        // Board 1 -> East dealer
        let output1 = format_printpbn(
            &deal,
            &PbnBoard {
                board_number: 1,
                ..Default::default()
            },
        );
        assert!(output1.contains("[Dealer \"E\"]"));

        // Board 2 -> South dealer
        let output2 = format_printpbn(
            &deal,
            &PbnBoard {
                board_number: 2,
                ..Default::default()
            },
        );
        assert!(output2.contains("[Dealer \"S\"]"));

        // Board 3 -> West dealer
        let output3 = format_printpbn(
            &deal,
            &PbnBoard {
                board_number: 3,
                ..Default::default()
            },
        );
        assert!(output3.contains("[Dealer \"W\"]"));
    }

    #[test]
    fn test_printpbn_vulnerability_rotation() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();

        // Board 0 -> None
        let output0 = format_printpbn(
            &deal,
            &PbnBoard {
                board_number: 0,
                ..Default::default()
            },
        );
        assert!(output0.contains("[Vulnerable \"None\"]"));

        // Board 1 -> NS
        let output1 = format_printpbn(
            &deal,
            &PbnBoard {
                board_number: 1,
                ..Default::default()
            },
        );
        assert!(output1.contains("[Vulnerable \"NS\"]"));

        // Board 2 -> EW
        let output2 = format_printpbn(
            &deal,
            &PbnBoard {
                board_number: 2,
                ..Default::default()
            },
        );
        assert!(output2.contains("[Vulnerable \"EW\"]"));

        // Board 3 -> All
        let output3 = format_printpbn(
            &deal,
            &PbnBoard {
                board_number: 3,
                ..Default::default()
            },
        );
        assert!(output3.contains("[Vulnerable \"All\"]"));
    }

    /// The tag a script's `HandType_*` variables produce. Not standard PBN, so
    /// it has to sit where a reader that does not know it will skip it, and be
    /// absent entirely when the script names no types.
    #[test]
    fn test_printpbn_hand_type_tag() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();

        let tagged = format_printpbn(
            &deal,
            &PbnBoard {
                hand_type: Some("22_24"),
                ..Default::default()
            },
        );
        assert!(tagged.contains("[HandType \"22_24\"]"), "{}", tagged);
        // Right after the board number, where a reader scanning headers finds it.
        let board = tagged.find("[Board ").expect("a board tag");
        let hand_type = tagged.find("[HandType ").expect("a hand type tag");
        assert!(hand_type > board);

        let untagged = format_printpbn(
            &deal,
            &PbnBoard {
                board_number: 0,
                ..Default::default()
            },
        );
        assert!(!untagged.contains("HandType"));
    }

    #[test]
    fn test_printpbn_explicit_dealer_and_vuln() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();

        let output = format_printpbn(
            &deal,
            &PbnBoard {
                dealer: Some(Position::South),
                vulnerability: Some(Vulnerability::All),
                event_name: Some("Test Event"),
                ..Default::default()
            },
        );

        assert!(output.contains("[Dealer \"S\"]"));
        assert!(output.contains("[Vulnerable \"All\"]"));
    }

    #[test]
    fn test_format_printcompact() {
        let mut gen = FastDealGenerator::new(1);
        let deal = gen.next_deal();
        let output = format_printcompact(&deal);

        // Should have 4 lines (one per position)
        assert_eq!(output.lines().count(), 4);
        // Each line should start with position char
        assert!(output.lines().next().unwrap().starts_with('n'));
        assert!(output.lines().nth(1).unwrap().starts_with('e'));
        assert!(output.lines().nth(2).unwrap().starts_with('s'));
        assert!(output.lines().nth(3).unwrap().starts_with('w'));
        // Each line should contain dots separating suits
        for line in output.lines() {
            assert_eq!(line.matches('.').count(), 3);
        }
    }
}
