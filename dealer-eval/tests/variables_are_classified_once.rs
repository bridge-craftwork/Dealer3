//! Whether a variable can reach `rnd()` depends on the script, not the deal.
//!
//! It used to be worked out inside the per-deal context, so every deal walked
//! the whole definition graph again — once per variable. On a scenario whose
//! definitions build on one another that is quadratic, and it dominated
//! everything else: 200,000 deals of a real scenario took 9.2s against the
//! original dealer's 0.46s, and the walk was almost all of it. Now 0.35s.
//!
//! A timing test would be flaky, so what is pinned here is the shape: the cost
//! of classifying must not grow with how deeply the definitions nest.

use std::time::Instant;

use dealer_eval::extract_variables;
use dealer_parser::parse_program;

/// A chain of `n` variables, each built on the one before.
fn chain(n: usize) -> String {
    let mut s = String::from("v0 = hcp(south) > 5\n");
    for i in 1..=n {
        s.push_str(&format!("v{i} = v{} or hcp(north) > {i}\n", i - 1));
    }
    s.push_str(&format!("condition v{n}\n"));
    s
}

fn classify(n: usize) -> std::time::Duration {
    let program = parse_program(&chain(n)).expect("parses");
    let started = Instant::now();
    // Building the set is the whole classification.
    for _ in 0..200 {
        let vars = extract_variables(&program);
        std::hint::black_box(vars.len());
    }
    started.elapsed()
}

/// Doubling the depth must not square the cost.
///
/// Generous bounds — this is guarding an order of growth, not a constant, and
/// CI machines are noisy. Quadratic would show as roughly 4x per doubling; the
/// bar here is 3x, which linear-ish work clears easily and quadratic does not.
#[test]
fn classifying_does_not_grow_quadratically_with_depth() {
    // Warm up, so the first measurement is not paying for page faults.
    let _ = classify(40);

    let small = classify(40);
    let large = classify(80);

    assert!(
        large < small * 3,
        "doubling the chain took {large:?} against {small:?} — that is the \
         quadratic classification coming back"
    );
}

/// And the answers themselves are unchanged: a variable reaching `rnd()`
/// through any number of others is still volatile, and one that cannot is not.
#[test]
fn volatility_still_follows_the_definitions() {
    let program = parse_program(
        "plain = hcp(north)\n\
         roll = rnd(100)\n\
         once = roll + 1\n\
         twice = once + plain\n\
         condition twice > 0\n",
    )
    .expect("parses");
    let vars = extract_variables(&program);

    assert!(!vars.is_volatile("plain"), "no rnd anywhere beneath it");
    assert!(vars.is_volatile("roll"), "calls rnd itself");
    assert!(vars.is_volatile("once"), "reaches rnd through one hop");
    assert!(vars.is_volatile("twice"), "and through two");
    assert!(!vars.is_volatile("absent"), "a name that is not a variable");
}

/// The walk that classifies has to terminate on a script whose variables refer
/// to each other, which is unevaluatable but must not hang the classifier.
#[test]
fn a_cycle_does_not_hang_the_classifier() {
    for script in ["a = a\n", "a = b\nb = a\n", "a = b + 1\nb = a + rnd(4)\n"] {
        let program = parse_program(script).expect("parses");
        let vars = extract_variables(&program);
        // The assertion is that this returned at all.
        assert!(!vars.is_empty(), "{script:?}");
    }
}
