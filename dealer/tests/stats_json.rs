//! `--stats-json` has to be machine-readable, which the tables are not.
//!
//! The tables print averages through `%g`, six significant digits, which is
//! right for a person and wrong for a tool that is about to divide by the
//! number to work out a levelling keep. And a label containing a colon or a
//! quote makes the text output ambiguous. So this asserts the output really
//! parses, really round-trips the values, and survives labels chosen to break
//! a hand-rolled writer.

use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], script: &str) -> String {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dealer3-json-{}-{:?}.dlr",
        std::process::id(),
        std::thread::current().id()
    ));
    let mut file = std::fs::File::create(&path).expect("temp script");
    file.write_all(script.as_bytes()).expect("temp script");
    let output = Command::new(env!("CARGO_BIN_EXE_dealer"))
        .args(args)
        .arg(&path)
        .stdin(Stdio::null())
        .output()
        .expect("dealer should run");
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "dealer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn json(args: &[&str], script: &str) -> serde_json::Value {
    let out = run(args, script);
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("not JSON: {e}\n{out}"))
}

#[test]
fn with_quiet_the_whole_of_stdout_is_json() {
    let v = json(
        &["-q", "--stats-json", "-p", "50", "-s", "1"],
        "condition hcp(north) >= 10\naction average \"n\" hcp(north)\n",
    );
    assert_eq!(v["produced"], 50);
    assert_eq!(v["seed"], 1);
    assert_eq!(v["timed_out"], false);
    assert!(v["generated"].as_u64().expect("generated") >= 50);
    assert!(v["seconds"].as_f64().expect("seconds") >= 0.0);
}

/// The count is what tells a caller whether a rare category was sampled enough
/// to divide by, which is the whole reason for reporting it.
#[test]
fn every_average_carries_its_value_and_its_sample_size() {
    let v = json(
        &["-q", "--stats-json", "-p", "400", "-s", "7"],
        "condition 1\naction average \"aces\" aces(north), average \"hcp\" hcp(north)\n",
    );
    let averages = v["averages"].as_array().expect("averages");
    assert_eq!(averages.len(), 2);
    assert_eq!(averages[0]["label"], "aces");
    assert_eq!(averages[0]["count"], 400);
    assert_eq!(averages[1]["label"], "hcp");
    // A whole deck holds 4 aces and 40 points, so one hand averages 1 and 10.
    let aces = averages[0]["value"].as_f64().expect("a number");
    let hcp = averages[1]["value"].as_f64().expect("a number");
    assert!((0.7..1.3).contains(&aces), "aces averaged {aces}");
    assert!((9.0..11.0).contains(&hcp), "hcp averaged {hcp}");
}

/// Full precision, not the tables' six significant digits — a keep computed
/// from a rounded rate is a rounded keep.
#[test]
fn values_are_not_rounded_the_way_the_tables_round_them() {
    let script = "condition 1\naction average \"rate\" hcp(north) >= 13\n";
    let v = json(&["-q", "--stats-json", "-p", "30000", "-s", "3"], script);
    let value = v["averages"][0]["value"].as_f64().expect("a number");
    let text = run(&["-q", "-p", "30000", "-s", "3"], script);
    let printed: f64 = text
        .split(": ")
        .nth(1)
        .expect("a printed average")
        .trim()
        .parse()
        .expect("a number");
    assert!((value - printed).abs() < 1e-4, "{value} vs {printed}");
    assert!(
        value.to_string().len() > printed.to_string().len(),
        "JSON gave {value}, no more precise than the table's {printed}"
    );
}

#[test]
fn frequencies_come_with_their_bins_and_what_fell_outside() {
    let v = json(
        &["-q", "--stats-json", "-p", "2000", "-s", "5"],
        "condition 1\naction frequency \"nhcp\" (hcp(north), 8, 12)\n",
    );
    let f = &v["frequencies"][0];
    assert_eq!(f["label"], "nhcp");
    assert_eq!(f["min"], 8);
    assert_eq!(f["max"], 12);
    let bins = f["bins"].as_array().expect("bins");
    assert_eq!(bins.len(), 5, "one bin per value from 8 to 12");
    assert_eq!(bins[0]["value"], 8);
    let inside: u64 = bins
        .iter()
        .map(|b| b["count"].as_u64().expect("count"))
        .sum();
    let below = f["below"].as_u64().expect("below");
    let above = f["above"].as_u64().expect("above");
    assert_eq!(
        inside + below + above,
        f["total"].as_u64().expect("total"),
        "the bins and the tails should account for every deal"
    );
    assert_eq!(f["total"], 2000);
    assert!(
        below > 0 && above > 0,
        "8..12 should have tails either side"
    );
}

/// Labels come from the script, so they can hold characters that break a
/// hand-rolled JSON writer. Not a double quote — the language has no string
/// escapes, so a label simply ends at the next one — but a backslash will go
/// straight through, and a colon or comma is what makes the *text* output
/// ambiguous in the first place.
#[test]
fn awkward_labels_survive() {
    let label = r#"a \ backslash: colon, comma, é"#;
    let v = json(
        &["-q", "--stats-json", "-p", "10", "-s", "1"],
        &format!("condition 1\naction average \"{label}\" hcp(north)\n"),
    );
    assert_eq!(v["averages"][0]["label"], label);
}

#[test]
fn a_script_with_no_statistics_still_gives_an_object() {
    let v = json(
        &["-q", "--stats-json", "-p", "5", "-s", "1"],
        "condition 1\n",
    );
    assert_eq!(v["averages"].as_array().expect("averages").len(), 0);
    assert_eq!(v["frequencies"].as_array().expect("frequencies").len(), 0);
    assert_eq!(v["produced"], 5);
}
