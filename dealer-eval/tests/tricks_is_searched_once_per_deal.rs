//! A double-dummy result must be worked out once per deal, not once per
//! mention of it.
//!
//! Everything else in the language is cheap enough that evaluating it twice
//! costs nothing worth counting. `tricks()` is not: a search is milliseconds
//! where the rest of a deal is microseconds, so a script that names it four
//! times, or that names it in a `condition` and again in an `average`, has to
//! come out at one search per (deal, denomination, declarer) all the same.
//! Nothing about the answers reveals whether that happened, so this counts the
//! searches.
//!
//! This file holds one test on purpose: `dealer_dds::searches()` counts the
//! whole process, and cargo runs the tests within a binary concurrently.

use dealer_core::{Card, Deal, Position, Rank, Suit};
use dealer_eval::eval_program;

/// Each hand holds one whole suit, which the solver gets through in no time.
/// `first` is North's suit and the rest follow clockwise.
fn one_suit_each(first: Suit) -> Deal {
    let mut deal = Deal::new();
    let suits = [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs];
    let offset = suits.iter().position(|s| *s == first).expect("a real suit");
    for (i, position) in Position::ALL.into_iter().enumerate() {
        for rank in Rank::ALL {
            deal.hand_mut(position)
                .add_card(Card::new(suits[(offset + i) % 4], rank));
        }
    }
    deal
}

fn evaluate(script: &str, deal: &Deal) -> i32 {
    let pre = dealer_parser::preprocess(script);
    let program = dealer_parser::parse_program(&pre)
        .unwrap_or_else(|e| panic!("should have parsed {script:?}: {e}"));
    eval_program(&program, deal).unwrap_or_else(|e| panic!("should have evaluated {script:?}: {e}"))
}

/// Runs `script` and reports how many searches it took.
fn searches_for(script: &str, deal: &Deal) -> usize {
    let before = dealer_dds::searches();
    evaluate(script, deal);
    dealer_dds::searches() - before
}

#[test]
fn a_deal_is_searched_once_for_each_denomination_and_declarer() {
    let deal = one_suit_each(Suit::Spades);

    // Four mentions of two distinct questions.
    let script = "condition tricks(south, spades) + tricks(south, spades) \
                  + tricks(north, 4) + tricks(north, 4) >= 0\n";
    assert_eq!(
        searches_for(script, &deal),
        2,
        "four mentions of two questions should be two searches"
    );

    // A second evaluation builds its own context, as the statistics and the
    // csvrpt do, and must still find the answers.
    assert_eq!(
        searches_for(script, &deal),
        0,
        "the same questions about the same deal should need no further search"
    );

    // A third question about a deal already searched costs only itself.
    assert_eq!(
        searches_for("condition tricks(east, hearts) >= 0\n", &deal),
        1,
        "a new question about a known deal should be one search"
    );

    // A different deal shares nothing.
    let other = one_suit_each(Suit::Hearts);
    assert_eq!(
        searches_for("condition tricks(south, spades) >= 0\n", &other),
        1,
        "a different deal should be searched on its own account"
    );
}
