//! Integration tests for `--input-deals`.
//!
//! These drive the real `dealer` binary so that argument parsing, the reader
//! wiring and the filter path are all exercised together.
//!
//! The deal corpus below is fixed rather than generated, so the tests are
//! hermetic: they do not depend on the RNG and will keep passing when the
//! generator changes. North's HCP are noted per deal so the expected results of
//! `hcp(north) >= 13` are checkable by hand.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Six deals in oneline format. North HCP, in order: 11, 12, 12, 14, 8, 13.
/// So `hcp(north) >= 13` matches exactly two of them (deals 4 and 6).
const ONELINE_CORPUS: &str = "\
n Q9.AJ5.8762.AT72 e KT432.Q84.J3.KQ8 s J85.762.AT94.J54 w A76.KT93.KQ5.963
n QT4.QJ82.82.T763 e AJ83.A9.AK753.J5 s K962.65.Q9.AQ942 w 75.KT743.JT64.K8
n AT2.A86.J942.K52 e 94.KJ.AKQ87.JT87 s J653.Q94.653.Q63 w KQ87.T7532.T.A94
n AQ87532.T.KQ85.K e .K98752.AT74.Q85 s KJ94.A4.J3.A9642 w T6.QJ63.962.JT73
n K6.J85.QT97642.5 e 84.Q94.K3.QJ9763 s AJ753.T.AJ8.AKT4 w QT92.AK7632.5.82
n A9854.54.KT93.AQ e K73.QT872.A6.K54 s QJT62.J9.87.9832 w .AK63.QJ542.JT76
";

const FILTER: &str = "hcp(north) >= 13\n";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_dealer")
}

/// Write `contents` to a uniquely named temp file and return its path.
fn temp_file(tag: &str, contents: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dealer3-test-{}-{}-{:?}",
        tag,
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&path, contents).expect("failed to write temp file");
    path
}

struct Output {
    stdout: String,
    stderr: String,
    success: bool,
}

/// Run the binary with `args`, optionally piping `stdin_data` in.
fn run(args: &[&str], stdin_data: Option<&str>) -> Output {
    let mut child = Command::new(bin())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn dealer");

    {
        let stdin = child.stdin.as_mut().expect("stdin unavailable");
        if let Some(data) = stdin_data {
            stdin.write_all(data.as_bytes()).expect("write to stdin");
        }
    }
    // Dropping stdin closes it, so the child sees EOF.
    drop(child.stdin.take());

    let out = child.wait_with_output().expect("failed to wait for dealer");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        success: out.status.success(),
    }
}

/// Count deal lines in oneline output (they all start with "n ").
fn count_deals(stdout: &str) -> usize {
    stdout.lines().filter(|l| l.starts_with("n ")).count()
}

#[test]
fn reads_oneline_deals_from_file_and_applies_filter() {
    let corpus = temp_file("oneline", ONELINE_CORPUS);
    let script = temp_file("script-oneline", FILTER);

    let out = run(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            corpus.to_str().unwrap(),
            "-f",
            "oneline",
            "-X",
        ],
        None,
    );

    assert!(out.success, "expected success, stderr: {}", out.stderr);
    assert_eq!(
        count_deals(&out.stdout),
        2,
        "expected 2 matching deals, got:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("Generated 6 hands"),
        "expected all 6 deals to be read, got:\n{}",
        out.stdout
    );
}

#[test]
fn reads_deals_from_stdin_with_dash() {
    let script = temp_file("script-stdin", FILTER);

    let out = run(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            "-",
            "-f",
            "oneline",
            "-X",
        ],
        Some(ONELINE_CORPUS),
    );

    assert!(out.success, "expected success, stderr: {}", out.stderr);
    assert_eq!(count_deals(&out.stdout), 2, "stdout:\n{}", out.stdout);
    assert!(out.stdout.contains("Generated 6 hands"));
}

#[test]
fn dash_without_script_file_is_rejected() {
    // The script would also come from stdin, so this cannot work. It must fail
    // with a clear message rather than silently consuming the wrong input.
    let out = run(&["--input-deals", "-", "-f", "oneline"], Some(FILTER));

    assert!(
        !out.success,
        "expected failure, stdout:\n{}\nstderr:\n{}",
        out.stdout, out.stderr
    );
    assert!(
        out.stderr.contains("also being read from stdin"),
        "expected an explanatory error, stderr:\n{}",
        out.stderr
    );
}

#[test]
fn reads_pbn_deal_tags() {
    // Two PBN deals surrounded by metadata, which must be ignored.
    let pbn = "\
[Event \"Test\"]
[Site \"Nowhere\"]
[Board \"1\"]
[Deal \"N:AQ87532.T.KQ85.K .K98752.AT74.Q85 KJ94.A4.J3.A9642 T6.QJ63.962.JT73\"]

[Event \"Test\"]
[Board \"2\"]
[Deal \"N:Q9.AJ5.8762.AT72 KT432.Q84.J3.KQ8 J85.762.AT94.J54 A76.KT93.KQ5.963\"]
";
    let corpus = temp_file("pbn", pbn);
    let script = temp_file("script-pbn", FILTER);

    let out = run(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            corpus.to_str().unwrap(),
            "-f",
            "oneline",
            "-X",
        ],
        None,
    );

    assert!(out.success, "expected success, stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("Generated 2 hands"),
        "expected 2 deals read past the metadata, got:\n{}",
        out.stdout
    );
    // Only the 14-HCP deal clears the filter.
    assert_eq!(count_deals(&out.stdout), 1, "stdout:\n{}", out.stdout);
}

/// A PBN board whose West holds fourteen cards, the rest of it well formed.
///
/// The reader accepts it — a hand of any length parses — and it is this program
/// that cannot use it, which is why the check has to be here rather than there.
const FOURTEEN_CARD_BOARD: &str = "\
[Board \"1\"]
[Deal \"N:AKQJ.AKQ.AKQ.AKQ 432.432.432.5432 T98.T98.T98.T987 7655.J765.J765.J6\"]
";

/// The same board a card short instead: West with twelve.
const TWELVE_CARD_BOARD: &str = "\
[Board \"1\"]
[Deal \"N:AKQJ.AKQ.AKQ.AKQ 432.432.432.5432 T98.T98.T98.T987 765.J765.J765.J\"]
";

#[test]
fn a_hand_of_fourteen_is_skipped_rather_than_taking_the_run_down() {
    // It used to panic: `a hand cannot hold more than 13 cards`, with no
    // mention of the file it came from, let alone which board or which seat.
    let corpus = temp_file("fourteen", FOURTEEN_CARD_BOARD);
    let script = temp_file("script-fourteen", "condition 1\n");

    let out = run(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            corpus.to_str().unwrap(),
            "-p",
            "1",
        ],
        None,
    );

    assert!(out.success, "should not panic; stderr:\n{}", out.stderr);
    assert!(!out.stderr.contains("panicked"), "stderr:\n{}", out.stderr);
    assert!(
        out.stderr.contains("West"),
        "should name the seat:\n{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("14"),
        "should say how many:\n{}",
        out.stderr
    );
    assert_eq!(count_deals(&out.stdout), 0, "stdout:\n{}", out.stdout);
}

#[test]
fn a_board_a_card_short_is_skipped_rather_than_run() {
    // Worse than the panic while it lasted: this one fitted, so it ran and
    // reported statistics over a twelve-card hand with nothing to say it had.
    let corpus = temp_file("twelve", TWELVE_CARD_BOARD);
    let script = temp_file("script-twelve", "condition 1\n");

    let out = run(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            corpus.to_str().unwrap(),
            "-p",
            "1",
        ],
        None,
    );

    assert!(out.success, "stderr:\n{}", out.stderr);
    assert!(
        out.stderr.contains("not a whole deal"),
        "stderr:\n{}",
        out.stderr
    );
    assert_eq!(count_deals(&out.stdout), 0, "stdout:\n{}", out.stdout);
}

#[test]
fn unrecognised_lines_are_ignored() {
    // DealReader skips anything it cannot parse as a deal, which is what allows
    // PBN metadata and stats output to be piped in. Interleave junk with deals
    // and confirm only the real deals are counted.
    let mut mixed = String::new();
    for (i, line) in ONELINE_CORPUS.lines().enumerate() {
        mixed.push_str(line);
        mixed.push('\n');
        if i == 1 {
            mixed.push_str("this is not a deal at all\n");
            mixed.push_str("Generated 999 hands\n");
        }
    }
    let corpus = temp_file("mixed", &mixed);
    let script = temp_file("script-mixed", FILTER);

    let out = run(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            corpus.to_str().unwrap(),
            "-f",
            "oneline",
            "-X",
        ],
        None,
    );

    assert!(out.success, "expected success, stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("Generated 6 hands"),
        "junk lines should not affect the deal count, got:\n{}",
        out.stdout
    );
    assert_eq!(count_deals(&out.stdout), 2, "stdout:\n{}", out.stdout);
}

#[test]
fn empty_input_produces_nothing() {
    let corpus = temp_file("empty", "");
    let script = temp_file("script-empty", FILTER);

    let out = run(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            corpus.to_str().unwrap(),
            "-f",
            "oneline",
            "-X",
        ],
        None,
    );

    assert!(out.success, "expected success, stderr: {}", out.stderr);
    assert_eq!(count_deals(&out.stdout), 0);
    assert!(
        out.stdout.contains("Generated 0 hands"),
        "stdout:\n{}",
        out.stdout
    );
}

#[test]
fn produce_limit_stops_early() {
    // Filter matches everything; -p 3 must stop after 3 deals.
    let corpus = temp_file("produce", ONELINE_CORPUS);
    let script = temp_file("script-produce", "hcp(north) >= 0\n");

    let out = run(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            corpus.to_str().unwrap(),
            "-p",
            "3",
            "-f",
            "oneline",
            "-X",
        ],
        None,
    );

    assert!(out.success, "expected success, stderr: {}", out.stderr);
    assert_eq!(count_deals(&out.stdout), 3, "stdout:\n{}", out.stdout);
    assert!(
        out.stdout.contains("Produced 3 hands"),
        "stdout:\n{}",
        out.stdout
    );
}

#[test]
fn generate_limit_caps_deals_read() {
    // -g 2 must stop reading after 2 deals even though 6 are available.
    let corpus = temp_file("generate", ONELINE_CORPUS);
    let script = temp_file("script-generate", "hcp(north) >= 0\n");

    let out = run(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            corpus.to_str().unwrap(),
            "-g",
            "2",
            "-f",
            "oneline",
            "-X",
        ],
        None,
    );

    assert!(out.success, "expected success, stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("Generated 2 hands"),
        "stdout:\n{}",
        out.stdout
    );
}

#[test]
fn input_exhausted_before_produce_target_is_not_an_error() {
    // Only 2 deals match, but -p 40 is requested. Running out of input should
    // exit cleanly with what was found, not hang or fail.
    let corpus = temp_file("exhausted", ONELINE_CORPUS);
    let script = temp_file("script-exhausted", FILTER);

    let out = run(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            corpus.to_str().unwrap(),
            "-p",
            "40",
            "-f",
            "oneline",
            "-X",
        ],
        None,
    );

    assert!(out.success, "expected success, stderr: {}", out.stderr);
    assert_eq!(count_deals(&out.stdout), 2, "stdout:\n{}", out.stdout);
    assert!(
        out.stdout.contains("Produced 2 hands"),
        "stdout:\n{}",
        out.stdout
    );
}

#[test]
fn predeal_conflict_is_rejected() {
    let corpus = temp_file("predeal", ONELINE_CORPUS);
    let script = temp_file("script-predeal", FILTER);

    let out = run(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            corpus.to_str().unwrap(),
            "-N",
            "SA,HK",
        ],
        None,
    );

    assert!(!out.success, "expected failure, stdout:\n{}", out.stdout);
    assert!(
        out.stderr.contains("cannot be combined with predeal"),
        "stderr:\n{}",
        out.stderr
    );
}

#[test]
fn missing_input_file_is_reported() {
    let script = temp_file("script-missing", FILTER);
    let out = run(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            "/nonexistent/path/to/deals.pbn",
        ],
        None,
    );

    assert!(!out.success, "expected failure, stdout:\n{}", out.stdout);
    assert!(
        out.stderr.contains("Error opening input deals file"),
        "stderr:\n{}",
        out.stderr
    );
}
