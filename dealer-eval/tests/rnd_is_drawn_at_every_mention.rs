//! A variable holding `rnd()` must not freeze its value for the deal.
//!
//! Caching a variable's value was invisible for as long as every expression in
//! the language was a pure function of the deal — which it was, until `rnd()`
//! arrived. The original stores expression trees and walks them afresh at every
//! mention, so `r == r` is false there about as often as two draws differ. It
//! has to be false here too, or a script's frequencies change between engines
//! with nothing to show for it.
//!
//! Verified against BBO's own script tester, which is the engine these scripts
//! actually run on: `r = rnd(1000000)` with `average r == r` reports 0 there
//! and used to report 1 here.

use dealer_core::{Deal, FastDealGenerator};
use dealer_eval::{eval_with_context, extract_constraint, extract_variables};

fn deals(count: usize) -> Vec<Deal> {
    let mut generator = FastDealGenerator::new(20260827);
    (0..count).map(|_| generator.next_deal()).collect()
}

/// Evaluate `script`'s final expression over `count` deals and report how often
/// it was true.
fn hit_rate(script: &str, count: usize) -> f64 {
    let pre = dealer_parser::preprocess(script);
    let program = dealer_parser::parse_program(&pre)
        .unwrap_or_else(|e| panic!("should have parsed {script:?}: {e}"));
    let variables = extract_variables(&program);
    let constraint = extract_constraint(&program).expect("script should end in an expression");
    let hits = deals(count)
        .iter()
        .filter(|deal| {
            eval_with_context(constraint, &variables, deal).expect("should have evaluated") != 0
        })
        .count();
    hits as f64 / count as f64
}

#[test]
fn a_variable_holding_rnd_is_drawn_afresh_at_every_mention() {
    // Two independent draws from a million agree essentially never.
    let rate = hit_rate("r = rnd(1000000)\nr == r\n", 2000);
    assert!(
        rate < 0.01,
        "`r == r` was true {rate} of the time, so the value is being cached"
    );
}

/// The property is transitive: a variable that merely *refers* to one holding
/// `rnd()` is just as volatile.
#[test]
fn the_taint_follows_variable_references() {
    for script in [
        "r = rnd(1000000)\nb = r\nb == b\n",
        "r = rnd(1000000)\nb = r + 0\nb == b\n",
        "r = rnd(1000000)\nb = r > 500000 ? 1 : 0\nc = b + r\nc == c\n",
        "r = rnd(1000000)\nb = hcp(north) + r\nb == b\n",
    ] {
        let rate = hit_rate(script, 2000);
        assert!(rate < 0.05, "{script:?} was stable {rate} of the time");
    }
}

/// And it stops there. Everything else in the language is a pure function of
/// the deal, so caching those is still invisible — and still worth 9.6x.
#[test]
fn a_variable_without_rnd_is_still_stable() {
    for script in [
        "v = hcp(north)\nv == v\n",
        "v = shape(north, any 4333)\nw = v\nw == w\n",
        "a = hcp(north)\nb = a + controls(south)\nb == b\n",
    ] {
        assert_eq!(hit_rate(script, 500), 1.0, "{script:?}");
    }
}

/// The frequencies a levelling script depends on. Each mention is its own coin
/// flip, so two mentions of a one-in-four test fire together one time in
/// sixteen — which is what BBO reports for the same script.
#[test]
fn each_mention_is_an_independent_draw() {
    let quarter = "keep = (rnd(1000) % 4 + 4) % 4 == 0\n";
    let once = hit_rate(&format!("{quarter}keep\n"), 20000);
    assert!(
        (0.22..0.28).contains(&once),
        "one mention fired {once} of the time, wanted about 0.25"
    );
    let twice = hit_rate(&format!("{quarter}keep && keep\n"), 20000);
    assert!(
        (0.045..0.08).contains(&twice),
        "two mentions fired {twice} of the time, wanted about 0.0625"
    );
}

/// Written inline rather than through a variable, which always drew afresh.
#[test]
fn inline_calls_are_unaffected() {
    let rate = hit_rate("rnd(1000000) == rnd(1000000)\n", 2000);
    assert!(rate < 0.01, "inline calls agreed {rate} of the time");
}
