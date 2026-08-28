//! `--interleave`, driven end to end.
//!
//! The ordering function has its own unit tests in `main.rs`. What those cannot
//! see is the part that made the switch wrong in the first place: the run holds
//! every produced deal, and a board's number belongs to where it lands rather
//! than to when it was dealt. A file numbered in production order looks sorted
//! to any reader that trusts `[Board]`, and sorting it would undo the ordering
//! without an error.

use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], script: &str) -> (String, String, i32) {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dealer3-interleave-{}-{:?}.dlr",
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

/// Three bands of very different rarity, so a naive order would exhaust the
/// rare one early and the ordering has something to do.
const BANDS: &str = "\
HandType_weak = hcp(south) <= 9
HandType_mid = hcp(south) >= 10 and hcp(south) <= 14
HandType_strong = hcp(south) >= 15
condition 1
action printpbn
";

fn tags(out: &str, tag: &str) -> Vec<String> {
    out.lines()
        .filter_map(|line| line.strip_prefix(&format!("[{tag} \"")))
        .filter_map(|rest| rest.strip_suffix("\"]"))
        .map(str::to_string)
        .collect()
}

#[test]
fn the_boards_are_numbered_in_the_order_they_come_out() {
    let (out, _, status) = run(&["-p", "24", "-s", "3", "--interleave"], BANDS);
    assert_eq!(status, 0);
    let numbers: Vec<usize> = tags(&out, "Board")
        .iter()
        .map(|n| n.parse().expect("board number"))
        .collect();
    assert_eq!(
        numbers,
        (1..=24).collect::<Vec<_>>(),
        "interleaved boards must be numbered 1..n in the order they are written, \
         or a reader that sorts by [Board] undoes the ordering"
    );
}

#[test]
fn the_ordering_walks_through_the_types() {
    let (plain, _, _) = run(&["-p", "24", "-s", "3"], BANDS);
    let (out, _, status) = run(&["-p", "24", "-s", "3", "--interleave"], BANDS);
    assert_eq!(status, 0);

    let ordered = tags(&out, "HandType");
    assert_eq!(ordered.len(), 24);

    // The rarest type is spread across the run rather than gathered at the
    // front or exhausted early: with the deals in production order the last
    // third of the set can hold none at all.
    let rarest = {
        let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for label in &ordered {
            *counts.entry(label.as_str()).or_default() += 1;
        }
        let (label, _) = counts
            .into_iter()
            .min_by_key(|(label, n)| (*n, *label))
            .expect("at least one type");
        label.to_string()
    };
    let where_rare: Vec<usize> = ordered
        .iter()
        .enumerate()
        .filter(|(_, label)| **label == rarest)
        .map(|(i, _)| i)
        .collect();
    assert!(
        where_rare.len() >= 2,
        "the fixture needs at least two of the rare type to say anything about spread"
    );
    let last = *where_rare.last().unwrap();
    assert!(
        last >= 16,
        "the last `{rarest}` lands at {last} of 24; the ordering is meant to reach \
         the end of the run, not run out early"
    );

    // Same deals, reordered — nothing gained, nothing dropped.
    let mut before = tags(&plain, "Deal");
    let mut after = tags(&out, "Deal");
    before.sort();
    after.sort();
    assert_eq!(before, after);
}

#[test]
fn the_rotation_follows_the_numbering() {
    // Dealer and vulnerability rotate with the board number when the script
    // names neither. Rotating on the production number would leave a practice
    // set whose board 1 is vulnerable and whose board 2 is not.
    let (out, _, status) = run(&["-p", "8", "-s", "3", "--interleave"], BANDS);
    assert_eq!(status, 0);
    assert_eq!(
        tags(&out, "Vulnerable"),
        ["None", "NS", "EW", "All", "NS", "EW", "All", "None"],
        "vulnerability should rotate with the board number as written"
    );
}

#[test]
fn deals_matching_no_type_still_come_out() {
    // The types need not cover everything here — only levelling requires that.
    // A deal belonging to no round belongs at the end, not in the bin.
    let script = "\
HandType_strong = hcp(south) >= 15
condition 1
action printpbn
";
    let (out, _, status) = run(&["-p", "20", "-s", "5", "--interleave"], script);
    assert_eq!(status, 0);
    assert_eq!(tags(&out, "Board").len(), 20, "no deal may be dropped");
    let labels = tags(&out, "HandType");
    assert!(!labels.is_empty() && labels.len() < 20);
}

#[test]
fn a_script_naming_no_types_is_left_alone() {
    let script = "condition hcp(south) >= 10\naction printpbn\n";
    let (out, _, status) = run(&["-p", "10", "-s", "1", "--interleave"], script);
    assert_eq!(status, 0);
    let numbers: Vec<usize> = tags(&out, "Board")
        .iter()
        .map(|n| n.parse().expect("board number"))
        .collect();
    assert_eq!(numbers, (1..=10).collect::<Vec<_>>());
}

#[test]
fn it_orders_the_other_formats_too() {
    // printall carries a number of its own; the rest carry none, but all of
    // them have to come out in the interleaved order rather than as dealt.
    let (out, _, status) = run(&["-p", "12", "-s", "3", "--interleave", "-f", "all"], BANDS);
    assert_eq!(status, 0);
    let numbers: Vec<usize> = out
        .lines()
        .filter_map(|line| line.trim().strip_suffix('.'))
        .filter_map(|n| n.parse().ok())
        .collect();
    assert_eq!(numbers, (1..=12).collect::<Vec<_>>());

    let (oneline, _, status) = run(
        &["-p", "12", "-s", "3", "--interleave", "-f", "oneline"],
        BANDS,
    );
    assert_eq!(status, 0);
    assert_eq!(oneline.lines().count(), 12);
}

#[test]
fn it_is_refused_while_writing_a_levelled_scenario() {
    // That run measures the scenario as it stands, so there is no practice set
    // to order — and the held deals would be swallowed rather than printed.
    let script = "\
HandType_weak = hcp(south) <= 9
HandType_rest = hcp(south) >= 10
### BEGIN GENERATED LEVELING ###
noLeveling = 1
levelTheDeal = noLeveling
### END GENERATED LEVELING ###
condition levelTheDeal
action printoneline
";
    let mut out_path = std::env::temp_dir();
    out_path.push(format!(
        "dealer3-interleave-level-{}.dlr",
        std::process::id()
    ));
    let (out, err, status) = run(
        &[
            "-p",
            "2000",
            "-s",
            "1",
            "--write-leveled",
            out_path.to_str().expect("path"),
            "--interleave",
        ],
        script,
    );
    assert_eq!(status, 1, "the combination should be refused, not ignored");
    assert!(err.contains("--interleave"), "stderr was: {err}");
    assert!(out.is_empty());
    assert!(!out_path.exists(), "nothing should have been written");
    let _ = std::fs::remove_file(&out_path);
}
