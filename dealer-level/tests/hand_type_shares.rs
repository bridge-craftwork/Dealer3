//! The target mix a scenario asks for, written in the scenario itself.
//!
//! `HandType_22_24_Share = 3` rather than a switch, so a scenario carries its
//! own intended mix and both front ends read the same thing. Still only a
//! variable assignment, so a script using it parses on BBO exactly as the
//! `HandType_` convention does.

use dealer_level::{hand_type_label, hand_type_shares, hand_types};

fn program(source: &str) -> dealer_parser::Program {
    let pre = dealer_parser::preprocess_all(source, &Default::default()).expect("preprocesses");
    dealer_parser::parse_program(&pre).expect("parses")
}

fn labels(source: &str) -> Vec<String> {
    hand_types(&program(source))
        .into_iter()
        .map(|n| hand_type_label(n).to_string())
        .collect()
}

const THREE: &str = "
HandType_a = hcp(south) <= 10
HandType_b = hcp(south) >= 11 and hcp(south) <= 20
HandType_c = hcp(south) >= 21
condition 1
";

/// Saying nothing asks for an even mix, which is what every scenario written
/// before shares existed means.
#[test]
fn no_shares_is_an_even_mix() {
    assert_eq!(
        hand_type_shares(&program(THREE)).unwrap(),
        vec![1.0, 1.0, 1.0]
    );
}

#[test]
fn a_share_is_read_for_the_type_it_names() {
    let source = format!("{THREE}\nHandType_b_Share = 3\n");
    assert_eq!(
        hand_type_shares(&program(&source)).unwrap(),
        vec![1.0, 3.0, 1.0]
    );
}

/// The bug this ordering exists to avoid: `HandType_b_Share` starts with the
/// hand-type prefix, so without care it becomes a fourth hand type called
/// `b_Share` — overlapping `b` and breaking the partition.
#[test]
fn a_share_is_not_itself_a_hand_type() {
    let source = format!("{THREE}\nHandType_b_Share = 3\n");
    assert_eq!(labels(&source), vec!["a", "b", "c"]);
}

/// Case-insensitively, because the failure otherwise is silent: an unrecognised
/// `_share` would quietly become a hand type rather than a share.
#[test]
fn the_suffix_is_matched_whatever_its_case() {
    for spelling in ["_Share", "_share", "_SHARE"] {
        let source = format!("{THREE}\nHandType_b{spelling} = 3\n");
        assert_eq!(labels(&source), vec!["a", "b", "c"], "{spelling}");
        assert_eq!(
            hand_type_shares(&program(&source)).unwrap(),
            vec![1.0, 3.0, 1.0],
            "{spelling}"
        );
    }
}

/// A typo in the type's name would otherwise set the share of nothing at all,
/// leaving the mix quietly even.
#[test]
fn a_share_for_a_type_that_does_not_exist_is_refused() {
    let source = format!("{THREE}\nHandType_d_Share = 3\n");
    let err = hand_type_shares(&program(&source)).unwrap_err();
    assert!(err.contains("never declares"), "{err}");
    assert!(err.contains("HandType_d"), "{err}");
}

/// A share that depended on the deal would mean a different target mix on every
/// hand, which is not a thing a target can be.
#[test]
fn a_share_has_to_be_a_number() {
    let source = format!("{THREE}\nHandType_b_Share = hcp(north)\n");
    let err = hand_type_shares(&program(&source)).unwrap_err();
    assert!(err.contains("plain number"), "{err}");
}

#[test]
fn every_share_zero_is_refused() {
    let source =
        format!("{THREE}\nHandType_a_Share = 0\nHandType_b_Share = 0\nHandType_c_Share = 0\n");
    let err = hand_type_shares(&program(&source)).unwrap_err();
    assert!(err.contains("no deals at all"), "{err}");
}

/// The case shares were added for: bins that should come out equal while the
/// values inside each bin are level too. A two-wide bin's members each want
/// half again as many deals as a three-wide bin's.
#[test]
fn bins_of_different_widths_are_levelled_by_their_shares() {
    let mut source = String::new();
    for h in 12..=24 {
        source.push_str(&format!("HandType_{h} = hcp(south) == {h}\n"));
    }
    for h in [12, 13, 14, 15, 16, 17, 22, 23, 24] {
        source.push_str(&format!("HandType_{h}_Share = 2\n"));
    }
    for h in [18, 19, 20, 21] {
        source.push_str(&format!("HandType_{h}_Share = 3\n"));
    }
    source.push_str("condition 1\n");

    let shares = hand_type_shares(&program(&source)).unwrap();
    assert_eq!(shares.len(), 13);

    // Each of the five original bins asks for the same total.
    let bins: [&[usize]; 5] = [&[0, 1, 2], &[3, 4, 5], &[6, 7], &[8, 9], &[10, 11, 12]];
    let totals: Vec<f64> = bins
        .iter()
        .map(|b| b.iter().map(|i| shares[*i]).sum())
        .collect();
    assert!(
        totals.iter().all(|t| (*t - totals[0]).abs() < f64::EPSILON),
        "bins should ask for equal totals, got {totals:?}"
    );
    // And a 12 is wanted two thirds as often as an 18.
    assert!((shares[0] / shares[6] - 2.0 / 3.0).abs() < 1e-12);
}
