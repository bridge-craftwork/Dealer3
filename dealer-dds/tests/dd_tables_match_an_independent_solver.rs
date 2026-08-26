//! The double-dummy answers must match a program that shares no code with us.
//!
//! Three things in this crate can be wrong in ways that still produce
//! plausible numbers: the `dealer_core` to `bridge_types` deal conversion, the
//! opening leader (`bridge_solver::Solver` takes the leader, not the
//! declarer), and the conversion from North/South tricks to declarer tricks.
//! Each of those is silent when it is wrong. Comparing whole tables against
//! BridgeComposer's catches all three — a leader that is off by one seat, or a
//! side that is not flipped, puts every board out.
//!
//! See `fixtures/dd_tables.txt` for where the expected values come from, and
//! for what the `quick` marker on some of its lines means: an unoptimised
//! build checks those boards, an optimised one checks all of them. So
//! `cargo test --release -p dealer-dds` is the run that covers the fixture.

use dealer_core::Position;
use dealer_dds::{DealAnalysis, Denomination};

const FIXTURE: &str = include_str!("fixtures/dd_tables.txt");

/// Declarers and denominations in the order the fixture lists them.
const DECLARERS: [Position; 4] = [
    Position::North,
    Position::East,
    Position::South,
    Position::West,
];
const DENOMINATIONS: [Denomination; 5] = [
    Denomination::Clubs,
    Denomination::Diamonds,
    Denomination::Hearts,
    Denomination::Spades,
    Denomination::NoTrump,
];

struct Board {
    deal: dealer_core::Deal,
    tag: String,
    expected: [u8; 20],
}

fn boards() -> Vec<Board> {
    let checking_everything = !cfg!(debug_assertions);
    let mut boards = Vec::new();
    let mut lines = 0;
    for line in FIXTURE.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        lines += 1;
        let quick = line.starts_with("quick");
        if !quick && !checking_everything {
            continue;
        }
        let (tag, values) = line
            .split_once("\"]")
            .unwrap_or_else(|| panic!("no Deal tag on line: {line}"));
        let tag = format!("{}\"]", tag.trim_start_matches("quick").trim_start());
        let deal = dealer_pbn::parse_deal_tag(&tag)
            .unwrap_or_else(|e| panic!("{tag} should parse: {e}"))
            .deal;
        let parsed: Vec<u8> = values
            .split_whitespace()
            .map(|v| v.parse().unwrap_or_else(|_| panic!("not a count: {v}")))
            .collect();
        let expected: [u8; 20] = parsed
            .try_into()
            .unwrap_or_else(|v: Vec<u8>| panic!("expected 20 counts, got {} on {tag}", v.len()));
        boards.push(Board {
            deal,
            tag,
            expected,
        });
    }
    assert!(lines >= 30, "the fixture shrank to {lines} boards");
    assert!(!boards.is_empty(), "no board was marked `quick`");
    boards
}

#[test]
fn every_declarer_and_denomination_agrees_with_bridgecomposer() {
    for board in boards() {
        let mut analysis = DealAnalysis::new(&board.deal);
        let mut expected = board.expected.iter();
        for declarer in DECLARERS {
            for denomination in DENOMINATIONS {
                assert_eq!(
                    analysis.tricks(denomination, declarer),
                    *expected.next().expect("twenty results per board"),
                    "{:?} by {:?} on {}",
                    denomination,
                    declarer,
                    board.tag
                );
            }
        }
    }
}
