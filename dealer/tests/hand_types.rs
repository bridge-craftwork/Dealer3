//! `HandType_*` variables, and the `[HandType "..."]` tag they produce.
//!
//! The prefix is a convention rather than syntax, which is the point: a script
//! using it still parses on the original dealer, and these scenarios run on
//! BBO. The cost is that the parser cannot check it — a misspelled name is
//! silently not a category — so the names found are always reported, and these
//! tests hold that reporting to what the script actually declares.

use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], script: &str) -> (String, String, i32) {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dealer3-handtype-{}-{:?}.dlr",
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

const BANDS: &str = "\
HandType_12_14 = hcp(south) >= 12 and hcp(south) <= 14
HandType_15_17 = hcp(south) >= 15 and hcp(south) <= 17
HandType_18_up = hcp(south) >= 18
condition shape(south, any 4333 + any 4432 + any 5332) and hcp(south) >= 12
action printpbn
";

#[test]
fn every_board_carries_the_type_it_matched() {
    let (out, _, status) = run(&["-p", "40", "-s", "1"], BANDS);
    assert_eq!(status, 0);
    let boards = out.matches("[Board \"").count();
    let tags = out.matches("[HandType \"").count();
    assert_eq!(boards, 40);
    assert_eq!(tags, 40, "every board should be classified");
    for label in ["12_14", "15_17", "18_up"] {
        assert!(
            out.contains(&format!("[HandType \"{label}\"]")),
            "no board tagged {label}"
        );
    }
    // The prefix is stripped; leaving it in would read `HandType_12_14`.
    assert!(!out.contains("[HandType \"HandType"));
}

/// The tag is not standard PBN, so a script naming no types must not emit it —
/// a file that gains a tag nobody asked for is a file that surprises a reader.
#[test]
fn a_script_without_hand_types_gets_no_tag() {
    let (out, _, status) = run(
        &["-p", "5", "-s", "1"],
        "condition hcp(north) >= 10\naction printpbn\n",
    );
    assert_eq!(status, 0);
    assert!(out.contains("[Board \""));
    assert!(!out.contains("HandType"));
}

/// The reporting that makes a misspelled prefix visible. Without it, a script
/// meaning to declare five categories and declaring four would produce a
/// practice set quietly missing one.
#[test]
fn the_types_found_are_reported_by_name() {
    let (out, _, status) = run(&["-q", "-v", "-p", "5", "-s", "1"], BANDS);
    assert_eq!(status, 0);
    assert!(
        out.contains("Hand types 3: 12_14, 15_17, 18_up"),
        "expected the names in the stats, got:\n{out}"
    );

    let typo = BANDS.replace("HandType_18_up", "HandTyp_18_up");
    let (out, _, _) = run(&["-q", "-v", "-p", "5", "-s", "1"], &typo);
    assert!(
        out.contains("Hand types 2: 12_14, 15_17"),
        "a misspelled name should show as a shorter list, got:\n{out}"
    );
}

#[test]
fn the_types_are_in_the_json_too() {
    // With their shares: checking a levelled scenario delivered its mix is why
    // a build step reads this at all, and the run has already counted them.
    let (out, _, status) = run(&["-q", "--stats-json", "-p", "200", "-s", "1"], BANDS);
    assert_eq!(status, 0);
    for label in ["12_14", "15_17", "18_up"] {
        assert!(
            out.contains(&format!(r#"{{ "name": "{label}", "produced": "#)),
            "got:\n{out}"
        );
    }
    // Declaration order, and the shares add to one over the produced deals.
    let at = |label: &str| out.find(&format!(r#""name": "{label}""#)).expect(label);
    assert!(at("12_14") < at("15_17") && at("15_17") < at("18_up"));
    let shares: f64 = out
        .split(r#""share": "#)
        .skip(1)
        .map(|piece| {
            piece
                .split(&[',', ' ', '}'][..])
                .next()
                .expect("a share")
                .parse::<f64>()
                .expect("a number")
        })
        .sum();
    assert!(
        (shares - 1.0).abs() < 1e-9,
        "the types partition these deals, so the shares should sum to 1, got {shares}"
    );
}

/// Declaration order, not alphabetical: it is the order the author thought in,
/// and it is the order an interleaved set walks through.
#[test]
fn the_order_is_the_order_they_are_declared() {
    let script = "\
HandType_zzz = hcp(south) >= 18
HandType_aaa = hcp(south) <= 17
condition 1
";
    let (out, _, _) = run(&["-q", "-v", "-p", "2", "-s", "1"], script);
    assert!(out.contains("Hand types 2: zzz, aaa"), "got:\n{out}");
}

/// Categories have to partition the deals. Picking the first match silently
/// would make a set wrong about what it contains, so it is refused.
#[test]
fn overlapping_types_are_refused() {
    let script = "\
HandType_strong = hcp(south) >= 15
HandType_balanced = shape(south, any 4333 + any 4432 + any 5332)
condition 1
action printpbn
";
    let (_, err, status) = run(&["-p", "20", "-s", "1"], script);
    assert_eq!(status, 1);
    assert!(err.contains("have to partition"), "got: {err}");
}

/// A deal matching none of them is not an error — a script may classify only
/// part of what it produces — but it carries no tag.
#[test]
fn a_deal_matching_nothing_is_left_untagged() {
    let script = "\
HandType_huge = hcp(south) >= 30
condition 1
action printpbn
";
    let (out, _, status) = run(&["-p", "3", "-s", "1"], script);
    assert_eq!(status, 0);
    assert_eq!(out.matches("[Board \"").count(), 3);
    assert_eq!(out.matches("[HandType").count(), 0);
}
