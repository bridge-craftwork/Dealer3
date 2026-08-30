//! The target mix a scenario asks for, written in the scenario itself.
//!
//! `HandType_22_24_Share = 3` rather than a switch, so a scenario carries its
//! own intended mix and both front ends read the same thing. Still only a
//! variable assignment, so a script using it parses on BBO exactly as the
//! `HandType_` convention does.

use dealer_level::{hand_type_label, hand_type_shares, hand_types, round_robin_plan};

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

// --- LevelType: levelling on a decomposition of its own -------------------
//
// `HandType_` is what deals are grouped, tagged and ordered by; `LevelType_` is
// what the keeps are computed from. Most scenarios need only the first.

use dealer_level::{group_mix, level_type_label, level_types, leveling_types};

const SPLIT: &str = "
HandType_low = hcp(south) <= 14
HandType_high = hcp(south) >= 15

LevelType_12 = hcp(south) == 12
LevelType_13 = hcp(south) == 13
LevelType_14 = hcp(south) == 14
LevelType_15 = hcp(south) >= 15

condition 1
";

#[test]
fn level_types_are_found_and_labelled() {
    let p = program(SPLIT);
    let found: Vec<&str> = level_types(&p).into_iter().map(level_type_label).collect();
    assert_eq!(found, vec!["12", "13", "14", "15"]);
}

/// The two decompositions are independent, so declaring level types must not
/// disturb the hand types the deals are still grouped by.
#[test]
fn hand_types_are_untouched_by_level_types() {
    assert_eq!(labels(SPLIT), vec!["low", "high"]);
}

/// With level types declared, they are what gets levelled — and the generated
/// block has to name them, not the hand types.
#[test]
fn the_leveling_decomposition_is_the_level_types_when_present() {
    let types = leveling_types(&program(SPLIT)).unwrap();
    assert_eq!(types.labels, vec!["12", "13", "14", "15"]);
    assert_eq!(types.prefix, "LevelType");
}

#[test]
fn the_leveling_decomposition_is_the_hand_types_otherwise() {
    let types = leveling_types(&program(THREE)).unwrap();
    assert_eq!(types.labels, vec!["a", "b", "c"]);
    assert_eq!(types.prefix, "HandType");
}

#[test]
fn shares_attach_to_the_level_types() {
    let source = format!("{SPLIT}\nLevelType_15_Share = 3\n");
    let types = leveling_types(&program(&source)).unwrap();
    assert_eq!(types.shares, vec![1.0, 1.0, 1.0, 3.0]);
}

/// Only one decomposition is levelled, so weighting both asks for two different
/// mixes and picking one silently would deliver a mix nobody asked for.
#[test]
fn shares_on_both_decompositions_are_refused() {
    let source = format!("{SPLIT}\nLevelType_12_Share = 2\nHandType_low_Share = 2\n");
    let err = leveling_types(&program(&source)).unwrap_err();
    assert!(err.contains("both"), "{err}");
}

/// Shares on the decomposition that is *not* being levelled are just as
/// ambiguous, and likelier — it is the natural mistake after adding level types.
#[test]
fn shares_on_hand_types_are_refused_when_level_types_exist() {
    let source = format!("{SPLIT}\nHandType_low_Share = 2\n");
    let err = leveling_types(&program(&source)).unwrap_err();
    assert!(err.contains("LevelType"), "{err}");
}

/// What a hand type delivers cannot be read off its own rate once the keeps are
/// applied to a different decomposition — only off how its deals crossed it.
#[test]
fn a_group_mix_follows_the_joint_distribution() {
    // Two groups over three level types. The first group holds all of type 0
    // and half of type 1; the second the rest.
    let joint = vec![vec![100, 50, 0], vec![0, 50, 100]];
    // Keep everything of the middle type, a tenth of the outer ones.
    let mix = group_mix(&joint, &[0.1, 1.0, 0.1]);
    // Group one: 100*0.1 + 50*1 = 60. Group two: 50*1 + 100*0.1 = 60. Even.
    assert!((mix[0] - 0.5).abs() < 1e-9, "{mix:?}");
    assert!((mix[1] - 0.5).abs() < 1e-9, "{mix:?}");
}

#[test]
fn a_group_mix_of_nothing_is_zero_rather_than_a_division_by_zero() {
    assert_eq!(group_mix(&[vec![0, 0]], &[1.0, 1.0]), vec![0.0]);
}

// ---------------------------------------------------------------------------
// A round robin, which reads the same declarations as counts per round.

fn plan(source: &str, produce: usize) -> dealer_level::RoundRobinPlan {
    round_robin_plan(&program(source), produce).expect("a plan")
}

#[test]
fn a_round_holds_one_of_each_by_default() {
    let p = plan(THREE, 12);
    assert_eq!(p.per_round, vec![1, 1, 1]);
    assert_eq!((p.rounds, p.remainder), (4, 0));
    assert!(p.even());
}

/// The point of the share: three of that type in every round, so a set weighted
/// 1:3:1 comes out weighted 1:3:1 exactly rather than on average.
#[test]
fn a_share_puts_that_many_of_a_type_in_every_round() {
    let source = format!("{THREE}\nHandType_b_Share = 3\n");
    let p = plan(&source, 15);
    assert_eq!(p.per_round, vec![1, 3, 1]);
    assert_eq!(p.round_size(), 5);
    assert_eq!((p.rounds, p.remainder), (3, 0));
    assert!(!p.even());
    assert_eq!((p.owed(0), p.owed(1), p.owed(2)), (3, 9, 3));
}

#[test]
fn a_remainder_is_left_for_a_partial_round() {
    // Four rounds and two over, not five-five-four: which types get the
    // leftovers is decided by whichever turn up next, so it cannot be settled
    // here.
    let p = plan(THREE, 14);
    assert_eq!((p.rounds, p.remainder), (4, 2));
}

#[test]
fn fewer_deals_than_a_round_is_all_remainder() {
    // `-p 2` over three types is two deals of two different types — a partial
    // round and nothing else, rather than an error or a third of a deal each.
    let p = plan(THREE, 2);
    assert_eq!((p.rounds, p.remainder), (0, 2));
    assert_eq!(p.owed(0), 0);
}

#[test]
fn a_scenario_naming_no_hand_types_has_no_rounds() {
    let err = round_robin_plan(&program("condition 1\n"), 12).unwrap_err();
    assert!(err.contains("HandType_"), "{err}");
}

/// A share is a count of deals per round here, so every type has to appear.
/// Levelling can express "never" as a weight of zero; a round cannot.
#[test]
fn a_share_of_zero_is_refused() {
    let source = format!("{THREE}\nHandType_b_Share = 0\n");
    let err = round_robin_plan(&program(&source), 12).unwrap_err();
    assert!(err.contains("HandType_b_Share"), "{err}");
    assert!(err.contains("1 or more"), "{err}");
}

// ---------------------------------------------------------------------------
// The prefix is a magic word, so its case is not part of it.

#[test]
fn a_prefix_is_recognised_in_any_case() {
    // What this exists to prevent: `handtype_south_15` declared no hand type
    // at all, so the scenario had none — no levelling, no PBN tags, no rounds
    // — and nothing said so, because a script with no hand types is legal.
    let source = "
handtype_a = hcp(south) <= 10
HANDTYPE_b = hcp(south) >= 11 and hcp(south) <= 20
HandType_c = hcp(south) >= 21
condition 1
";
    assert_eq!(labels(source), vec!["a", "b", "c"]);
}

#[test]
fn a_share_finds_its_type_whatever_case_either_is_written_in() {
    let source = "
handtype_a = hcp(south) <= 10
HandType_b = hcp(south) >= 11
HANDTYPE_A_share = 3
condition 1
";
    assert_eq!(
        hand_type_shares(&program(source)).unwrap(),
        vec![3.0, 1.0],
        "the share did not reach the type it names"
    );
}

/// Two variables, one category. Refused rather than resolved: everything
/// downstream identifies a category by its label — the PBN tag, the bar chart,
/// the share lookup, `--interleave` — and two rows called `a` is not a thing to
/// hand anybody.
#[test]
fn two_types_differing_only_in_case_are_refused() {
    let source = "
HandType_a = hcp(south) <= 10
handtype_A = hcp(south) >= 21
condition 1
";
    let err = leveling_types(&program(source)).unwrap_err();
    assert!(err.contains("the same category"), "{err}");
    assert!(err.contains("HandType_a"), "{err}");
    assert!(err.contains("handtype_A"), "{err}");
}

/// The variable is still case-sensitive to refer to, as it is in dealer.exe,
/// which resolves names with `strcmp`. Only the convention stopped caring.
#[test]
fn the_name_itself_is_still_case_sensitive() {
    let matching = program(
        "
handtype_a = hcp(south) <= 10
condition handtype_a
",
    );
    assert!(dealer_parser::undefined_variables(&matching).is_empty());

    let wrong = program(
        "
handtype_a = hcp(south) <= 10
condition HandType_a
",
    );
    assert_eq!(
        dealer_parser::undefined_variables(&wrong),
        vec!["HandType_a"]
    );
}
