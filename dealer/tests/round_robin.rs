//! `--round-robin`, driven end to end.
//!
//! The arithmetic has unit tests in `dealer-level` and the filling has them in
//! `dealer-run`. What only the whole binary can show is the thing the switch
//! exists for: that the PBN which lands on disk holds exactly the counts asked
//! for, on any seed; that a set which could not be filled says which types came
//! up short rather than quietly delivering fewer; and that it composes with
//! `--interleave` — fill the rounds, then order them.

use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], script: &str) -> (String, String, i32) {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dealer3-round-robin-{}-{:?}.dlr",
        std::process::id(),
        std::thread::current().id()
    ));
    let mut file = std::fs::File::create(&path).expect("temp script");
    file.write_all(script.as_bytes()).expect("temp script");
    let out = Command::new(env!("CARGO_BIN_EXE_dealer"))
        .args(args)
        .arg(&path)
        .stdin(Stdio::null())
        .output()
        .expect("dealer should run");
    let _ = std::fs::remove_file(&path);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Bands of very different rarity, which is the case a round robin is for: `strong`
/// is about one deal in three hundred and the other two arrive constantly.
const BANDS: &str = "\
HandType_weak = hcp(south) <= 10
HandType_mid = hcp(south) >= 11 and hcp(south) <= 21
HandType_strong = hcp(south) >= 22
condition 1
";

/// `[HandType]` tags in the order they appear, since a PBN is what this makes.
fn tags(pbn: &str) -> Vec<String> {
    pbn.lines()
        .filter_map(|line| line.strip_prefix("[HandType \""))
        .filter_map(|rest| rest.split('"').next())
        .map(|s| s.to_string())
        .collect()
}

fn counts(pbn: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for tag in tags(pbn) {
        *counts.entry(tag).or_insert(0) += 1;
    }
    counts
}

#[test]
fn produce_divides_evenly_among_the_types() {
    let (out, _, code) = run(
        &["--round-robin", "-p", "15", "-s", "1", "-f", "printpbn"],
        BANDS,
    );
    assert_eq!(code, 0);
    let counts = counts(&out);
    assert_eq!(counts.len(), 3, "{counts:?}");
    assert!(
        counts.values().all(|n| *n == 5),
        "a round that is not even is not a round: {counts:?}"
    );
}

/// The whole reason for the switch. A levelled twelve-board set is four of each
/// on average and 6/1/5 without anything having gone wrong; this is four.
#[test]
fn every_seed_gives_the_same_counts() {
    for seed in ["1", "2", "99"] {
        let (out, _, code) = run(
            &["--round-robin", "-p", "12", "-s", seed, "-f", "printpbn"],
            BANDS,
        );
        assert_eq!(code, 0);
        for (band, count) in counts(&out) {
            assert_eq!(count, 4, "seed {seed} gave {count} of {band}");
        }
    }
}

/// A remainder is a partial round, not a top-up of whichever type is handiest.
#[test]
fn a_remainder_makes_a_partial_round_without_repeating_a_type() {
    let (out, _, code) = run(
        &["--round-robin", "-p", "14", "-s", "1", "-f", "printpbn"],
        BANDS,
    );
    assert_eq!(code, 0);
    let counts = counts(&out);
    assert_eq!(counts.values().sum::<usize>(), 14);
    for (band, count) in &counts {
        assert!(
            (4..=5).contains(count),
            "{band} came out at {count}: {counts:?}"
        );
    }
    assert_eq!(
        counts.values().filter(|n| **n == 5).count(),
        2,
        "{counts:?}"
    );
}

/// Fewer deals than types: every deal is a different type, none dealt twice.
#[test]
fn fewer_deals_than_types_gives_one_each_of_some() {
    let (out, _, code) = run(
        &["--round-robin", "-p", "2", "-s", "1", "-f", "printpbn"],
        BANDS,
    );
    assert_eq!(code, 0);
    let counts = counts(&out);
    assert_eq!(counts.len(), 2, "a short round repeated a type: {counts:?}");
    assert!(counts.values().all(|n| *n == 1), "{counts:?}");
}

#[test]
fn a_round_that_runs_out_of_deals_names_the_types_that_came_up_short() {
    let (out, err, code) = run(
        &[
            "--round-robin",
            "-p",
            "15",
            "-g",
            "300",
            "-s",
            "1",
            "-f",
            "printpbn",
        ],
        BANDS,
    );
    // Not a failure: a short set is still a set.
    assert_eq!(code, 0);
    assert!(err.contains("short of the round"), "{err}");
    assert!(
        err.contains("strong"),
        "the rare band is the one that ran out: {err}"
    );
    // And what it did deliver is still exact for the types that filled.
    let counts = counts(&out);
    assert_eq!(counts["weak"], 5);
    assert_eq!(counts["mid"], 5);
    assert!(counts.get("strong").copied().unwrap_or(0) < 5, "{counts:?}");
}

/// Fill the rounds, then order them: a set both exactly even and walking
/// through the types rather than meeting them as they happen to fall.
#[test]
fn a_round_robin_composes_with_interleave() {
    let (out, _, code) = run(
        &[
            "--round-robin",
            "-p",
            "12",
            "--interleave",
            "-s",
            "1",
            "-f",
            "printpbn",
        ],
        BANDS,
    );
    assert_eq!(code, 0);
    let tags = tags(&out);
    assert_eq!(tags.len(), 12);
    for (round, chunk) in tags.chunks(3).enumerate() {
        let mut seen: Vec<&String> = chunk.iter().collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 3, "round {round} repeats a type: {chunk:?}");
    }
}

#[test]
fn a_scenario_with_no_hand_types_is_told_so() {
    let (_, err, code) = run(
        &["--round-robin", "-p", "12", "-s", "1"],
        "condition hcp(north) >= 15\n",
    );
    assert_ne!(code, 0);
    assert!(err.contains("HandType_"), "{err}");
}

/// A share is how many of that type appear in every round. The point of it: a
/// set weighted 1:3:1 comes out weighted 1:3:1 exactly, not on average.
#[test]
fn a_share_puts_that_many_of_a_type_in_every_round() {
    let script = format!("{BANDS}HandType_mid_Share = 3\n");
    // A round is 1 + 3 + 1, so 15 deals is three complete rounds.
    let (out, _, code) = run(
        &["--round-robin", "-p", "15", "-s", "1", "-f", "printpbn"],
        &script,
    );
    assert_eq!(code, 0);
    let counts = counts(&out);
    assert_eq!(counts["weak"], 3);
    assert_eq!(counts["mid"], 9);
    assert_eq!(counts["strong"], 3);
}

/// A round deals every type at least once, so a share of zero has no meaning
/// here — levelling is where a weight can say "never".
#[test]
fn a_share_of_zero_is_refused() {
    let script = format!("{BANDS}HandType_mid_Share = 0\n");
    let (_, err, code) = run(&["--round-robin", "-p", "12", "-s", "1"], &script);
    assert_ne!(code, 0);
    assert!(err.contains("HandType_mid_Share"), "{err}");
    assert!(err.contains("1 or more"), "{err}");
}

/// Two capabilities on the same declarations, not two answers to one question.
/// `--level` measures the scenario and writes the copy; `--round-robin` decides
/// which of its deals reach the file. Together: a script to publish and an exact
/// set to hand out, from one run.
#[test]
fn a_levelling_and_a_round_robin_do_different_jobs() {
    let (out, err, code) = run(
        &[
            "--round-robin",
            "--level",
            "-p",
            "12",
            "-s",
            "1",
            "-f",
            "printpbn",
        ],
        BANDS,
    );
    assert_eq!(code, 0, "{err}");
    for (band, count) in counts(&out) {
        assert_eq!(count, 4, "{band} came out at {count}");
    }
    // The levelling ran too, and reported what it measured.
    assert!(err.contains("natural"), "no levelling summary: {err}");
}
