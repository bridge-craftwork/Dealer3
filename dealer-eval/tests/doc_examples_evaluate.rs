//! Evaluates every example in `dealer_parser::vocabulary`'s documentation.
//!
//! `dealer-parser` can only check that the examples parse. Plenty of mistakes
//! survive that: `quality(north)` parses and then fails on argument count,
//! `hascard(north, spades)` parses and then fails on argument type. Running
//! them against a real deal is what catches those, and this is the crate that
//! can, since it owns the evaluator.
//!
//! So an example in the language reference is not merely plausible — it has
//! been run.

use dealer_core::Deal;
use dealer_eval::{eval_program, EvalError};
use dealer_parser::vocabulary::*;

/// A fixed, complete deal, in the same one-line form the rest of the crate's
/// tests use. The values do not matter — only that every function gets a real
/// hand, with a spread of lengths and honours to work on.
const REFERENCE_DEAL: &str =
    "n Q9.AJ5.8762.AT72 e KT432.Q84.J3.KQ8 s J85.762.AT94.J54 w A76.KT93.KQ5.963";

fn reference_deal() -> Deal {
    dealer_pbn::parse_oneline(REFERENCE_DEAL).expect("reference deal must parse")
}

fn evaluate(script: &str, deal: &Deal) -> Result<i32, EvalError> {
    let pre = dealer_parser::preprocess(script);
    let program = dealer_parser::parse_program(&pre)
        .unwrap_or_else(|e| panic!("should have parsed: {:?}\n{}", script, e));
    eval_program(&program, deal)
}

/// `tricks` calls the double-dummy solver, and on the legacy solver a single
/// deal takes minutes rather than milliseconds — see issue #14. Its example is
/// parsed by `dealer-parser`, and its argument handling is covered by that
/// crate's own tests; running it here would make `cargo test` unusable.
fn uses_the_solver(example: &str) -> bool {
    example.contains("tricks(")
}

#[test]
fn every_function_example_evaluates() {
    let deal = reference_deal();
    for doc in FUNCTION_DOCS {
        if uses_the_solver(doc.example) {
            continue;
        }
        if let Err(e) = evaluate(&format!("condition {}\n", doc.example), &deal) {
            panic!(
                "the example for `{}` does not evaluate: {:?}\n{}",
                doc.name, doc.example, e
            );
        }
    }
}

#[test]
fn every_operator_example_evaluates() {
    let deal = reference_deal();
    for doc in OPERATOR_DOCS {
        if uses_the_solver(doc.example) {
            continue;
        }
        // Assignment is a statement, so it needs a condition after it before
        // there is anything to evaluate.
        let script = if doc.symbol == "=" {
            format!("{}\ncondition fit >= 0\n", doc.example)
        } else {
            format!("condition {}\n", doc.example)
        };
        if let Err(e) = evaluate(&script, &deal) {
            panic!(
                "the example for `{}` does not evaluate: {:?}\n{}",
                doc.symbol, doc.example, e
            );
        }
    }
}

/// The documented value of `score(0, 34, 9)` — 3NT, not vulnerable, making — is
/// stated as 400 in the reference. Check the engine agrees, so the one example
/// carrying a number cannot quietly become wrong.
#[test]
fn the_score_example_gives_the_number_it_claims() {
    let deal = reference_deal();
    assert_eq!(
        evaluate("condition score(0, 34, 9)\n", &deal).expect("score should evaluate"),
        400,
        "the `score` entry tells readers that 3NT making nine tricks scores 400"
    );
}

/// The `score` entry describes an encoding — level × 10 + strain, plus 100
/// doubled or 200 redoubled — which is the most intricate claim on the
/// reference page and the easiest to get subtly wrong. Every part of it is
/// checked here against scores that are a matter of record.
#[test]
fn the_documented_contract_encoding_is_the_real_one() {
    let deal = reference_deal();
    for (script, expected, what) in [
        (
            "condition score(0, 34, 9)\n",
            400,
            "3NT not vulnerable, making",
        ),
        ("condition score(1, 34, 9)\n", 600, "3NT vulnerable, making"),
        (
            "condition score(0, 43, 10)\n",
            420,
            "four spades not vulnerable, making",
        ),
        ("condition score(0, 134, 9)\n", 550, "3NT doubled, making"),
        (
            "condition score(0, 243, 10)\n",
            880,
            "four spades redoubled, making",
        ),
        (
            "condition score(0, 134, 7)\n",
            -300,
            "3NT doubled, two down",
        ),
    ] {
        assert_eq!(
            evaluate(script, &deal).unwrap_or_else(|e| panic!("{} should evaluate: {}", what, e)),
            expected,
            "{}",
            what
        );
    }
}

/// Likewise the claim on `quality` and `cccc` that values are multiplied by
/// 100 — here against the numbers dealer.exe itself produces.
///
/// Both were run on the Windows VM over this exact deal on 2026-08-26, with
/// North and West predealt so the two engines saw identical hands, and agreed
/// to the digit. The reference tells readers these are hundredths; this is the
/// evidence for that claim rather than an assumption about it.
#[test]
fn quality_and_cccc_match_the_original_dealer() {
    let deal = reference_deal();

    assert_eq!(
        evaluate("condition cccc(north)\n", &deal).expect("cccc should evaluate"),
        1045,
        "dealer.exe gives cccc(north) = 1045 for Q9.AJ5.8762.AT72"
    );
    assert_eq!(
        evaluate("condition quality(west, hearts)\n", &deal).expect("quality should evaluate"),
        160,
        "dealer.exe gives quality(west, hearts) = 160 for a K-T-9-3 holding"
    );

    // The same run pinned these, which the reference also describes.
    for (script, expected, what) in [
        ("condition losers(north)\n", 9, "losers"),
        ("condition c13(north)\n", 15, "c13"),
        ("condition controls(west)\n", 4, "controls"),
        ("condition top5(north)\n", 5, "top5"),
    ] {
        assert_eq!(
            evaluate(script, &deal).unwrap_or_else(|e| panic!("{} should evaluate: {}", what, e)),
            expected,
            "dealer.exe and dealer3 agreed on {} for this deal",
            what
        );
    }
}
