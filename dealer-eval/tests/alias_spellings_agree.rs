//! Every alternative spelling must compute exactly what its counterpart does.
//!
//! The aliases are wired up in three places — `function_name` in the grammar,
//! `Function::from_name` in the AST, and `FUNCTION_DOCS` — and a mistake in any
//! one of them produces a function that parses and then quietly answers a
//! different question. Comparing them on real deals is what catches that.
//!
//! Derived from `vocabulary::FUNCTION_DOCS` rather than a hand-written list, so
//! a new alias is covered the moment it is documented.

use dealer_core::{Deal, FastDealGenerator};
use dealer_eval::eval_program;
use dealer_parser::vocabulary;

/// A spread of deals rather than one, since two spellings could agree by
/// accident on a single hand — a suit with no honours makes several of these
/// functions return 0 whatever they are.
fn deals() -> Vec<Deal> {
    let mut generator = FastDealGenerator::new(20260826);
    (0..40).map(|_| generator.next_deal()).collect()
}

/// How many deals to compare a pair of spellings on.
///
/// Forty for everything cheap. `tricks` costs a double-dummy search per deal,
/// which is milliseconds where the rest are microseconds, and a spelling either
/// reaches the same function or it does not — so a handful of deals settles it
/// just as well as forty.
fn deals_to_compare(name: &str) -> usize {
    if name.contains("trick") {
        4
    } else {
        40
    }
}

fn evaluate(script: &str, deal: &Deal) -> i32 {
    let pre = dealer_parser::preprocess(script);
    let program = dealer_parser::parse_program(&pre)
        .unwrap_or_else(|e| panic!("should have parsed {script:?}: {e}"));
    eval_program(&program, deal).unwrap_or_else(|e| panic!("should have evaluated {script:?}: {e}"))
}

#[test]
fn every_alias_computes_what_its_counterpart_computes() {
    let deals = deals();
    let mut checked = 0;

    for doc in vocabulary::FUNCTION_DOCS {
        let Some(target) = doc.alias_of else { continue };
        // The alias and its target take the same arguments, so the example
        // written for the alias works for both with only the name swapped.
        let alias_script = format!("condition {}\n", doc.example);
        let target_script = alias_script.replacen(doc.name, target, 1);
        assert_ne!(
            alias_script, target_script,
            "`{}` and `{}` produced the same script; the example does not start with the name",
            doc.name, target
        );

        for (i, deal) in deals.iter().take(deals_to_compare(doc.name)).enumerate() {
            assert_eq!(
                evaluate(&alias_script, deal),
                evaluate(&target_script, deal),
                "`{}` and `{}` disagree on deal {}",
                doc.name,
                target,
                i
            );
        }
        checked += 1;
    }

    // A guard against the loop silently doing nothing, which would leave every
    // alias unchecked while the test stayed green.
    assert!(
        checked >= 14,
        "only {} aliases were checked; FUNCTION_DOCS should carry more than that",
        checked
    );
}

/// The alias must agree with its counterpart in the two-argument form too, which
/// the one-line examples do not all exercise.
#[test]
fn aliases_agree_when_given_a_suit() {
    let deals = deals();
    let pairs = [
        ("control", "controls"),
        ("hcps", "hcp"),
        ("ten", "tens"),
        ("jack", "jacks"),
        ("queen", "queens"),
        ("king", "kings"),
        ("ace", "aces"),
    ];
    for (alias, target) in pairs {
        for suit in ["spades", "hearts", "diamonds", "clubs"] {
            for deal in &deals {
                assert_eq!(
                    evaluate(&format!("condition {}(north, {})\n", alias, suit), deal),
                    evaluate(&format!("condition {}(north, {})\n", target, suit), deal),
                    "`{}` and `{}` disagree in {}",
                    alias,
                    target,
                    suit
                );
            }
        }
    }
}

/// `notrump` is a value rather than a function alias, so it is checked through
/// `score`, which takes the same 0-4 strain numbering as `tricks` and costs
/// nothing to evaluate.
#[test]
fn notrump_is_the_strain_number_four() {
    let deal = &deals()[0];
    // 3NT making nine tricks, not vulnerable.
    let by_number = evaluate("condition score(0, 34, 9)\n", deal);
    let by_word = evaluate("condition score(0, 30 + notrump, 9)\n", deal);
    assert_eq!(by_word, by_number, "`notrump` must be the number 4");
    assert_eq!(by_number, 400);

    assert_eq!(evaluate("condition notrumps\n", deal), 4);
}

/// The parser refuses an out-of-range `altcount` using its own copy of the row
/// count, so that the error lands where the script is written. If the two ever
/// disagree, one of them is wrong about how many counts there are.
#[test]
fn the_parser_and_the_evaluator_agree_on_how_many_counts_there_are() {
    assert_eq!(
        dealer_parser::NUM_COUNT_ROWS,
        dealer_eval::counts::NUM_ROWS,
        "the parser rejects altcount values using its own row count"
    );
    assert_eq!(
        dealer_parser::MAX_COUNT_VALUES,
        dealer_eval::counts::NUM_RANKS,
        "the parser rejects overlong value lists using its own rank count"
    );
}
