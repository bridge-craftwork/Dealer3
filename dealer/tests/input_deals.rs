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

/// The same for bytes, and with an extension, so a name can be made to
/// disagree with what the file holds.
fn temp_binary(tag: &str, extension: &str, contents: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dealer3-test-{}-{}-{:?}.{}",
        tag,
        std::process::id(),
        std::thread::current().id(),
        extension
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
    run_bytes(args, stdin_data.map(|data| data.as_bytes()))
}

/// The same, for input that is not text: a ZRD library is binary, and it is
/// exactly the case that used to die before the reader saw it.
fn run_bytes(args: &[&str], stdin_data: Option<&[u8]>) -> Output {
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
            stdin.write_all(data).expect("write to stdin");
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

/// A teaching-material board: the two hands the student sees, `-` for the rest.
///
/// Legal PBN and not a deal. It used to read as nothing at all — a whole file of
/// these came back as an empty file, with no warning and no deal.
const PARTIAL_BOARD: &str = "\
[Event \"Stayman\"]
[Board \"1\"]
[Deal \"W:- KT82.74.AK63.AJ7 - A4.KJ98.T872.865\"]
";

#[test]
fn a_board_with_only_two_hands_is_reported_rather_than_ignored() {
    let corpus = temp_file("partial", PARTIAL_BOARD);
    let script = temp_file("script-partial", "condition 1\n");

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
        out.stderr.contains("2 of the four hands"),
        "should say what is missing:\n{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("skipped 1"),
        "should count it:\n{}",
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

/// A board carrying a double-dummy table that is deliberately **wrong**.
///
/// The real table for this deal starts `87879...`; this says `67879...`, so
/// North's notrump result reads 6 where a solver would answer 8.
///
/// Wrong on purpose, because a correct table cannot test anything. If the tag
/// were ignored and the deal solved instead, the answer would be the same
/// either way and the test would pass while the feature was gone — which is
/// exactly what an earlier version of this test did.
const DOCTORED_BOARD: &str = "\
[Event \"doctored\"]
[Board \"1\"]
[Deal \"N:QJ3.A93.K4.KT875 A98.QJT2.97653.2 T76.K65.AQJ8.J43 K542.874.T2.AQ96\"]
[DoubleDummyTricks \"67879878793555345564\"]
";

/// A deal that arrives with a table is not solved again: the file's answer is
/// used, even when it disagrees with the solver.
///
/// These tags used to be dropped at the door. `--input-deals` read through a
/// line-oriented reader that yields deals and discards everything around them,
/// so a file annotated by our own solver lost its analysis on the way in.
///
/// Trusting the file is the decision under test, not an accident of it: a
/// library exists to save the searches, and re-solving to check would throw
/// that away. The doctored cell is what makes "trusted" observable.
#[test]
fn a_pbn_table_is_used_rather_than_solved_again() {
    let corpus = temp_file("doctored", DOCTORED_BOARD);
    let script = temp_file(
        "script-doctored",
        "condition 1\naction printrpt(\"t\", trix(deal))\n",
    );

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
        out.stderr.contains("arrived with double-dummy tables"),
        "should say the table came from the file:\n{}",
        out.stderr
    );
    // Clubs, diamonds, hearts, spades, notrump for North, then East, South,
    // West. North's notrump is the fifth number, and it is the doctored one.
    assert!(
        out.stdout
            .contains("9,7,8,7,6,3,5,5,5,3,9,7,8,7,8,4,6,5,5,4"),
        "the file's table should be used as written:\n{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("9,7,8,7,8,3,5,5,5,3"),
        "solving anyway would give 8 for North notrump, and would mean the tag was \
         ignored:\n{}",
        out.stdout
    );
}

/// Ten records of Pavlicek's solved-deal library: binary, and not valid UTF-8,
/// which is what used to stop it at the door.
const LIBRARY: &[u8] = include_bytes!("../../dealer-run/tests/fixtures/rpdd_10First.zrd");

/// A library arriving down a pipe is read as a library.
///
/// The point of the pipe is `curl -s https://…/deals.zrd | dealer script.dlr
/// --input-deals -`, which is how a library reaches this program without it
/// growing an HTTP client. It used to fail on "stream did not contain valid
/// UTF-8": stdin was read into a `String`, and the format was decided by the
/// filename, which `-` does not have.
#[test]
fn a_library_piped_in_is_read_as_one() {
    let script = temp_file("script-piped-library", "condition 1\n");

    let out = run_bytes(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            "-",
            "-f",
            "oneline",
            "-X",
        ],
        Some(LIBRARY),
    );

    assert!(out.success, "stderr:\n{}", out.stderr);
    assert!(
        out.stdout.contains("Generated 10 hands"),
        "all ten records should have been read:\n{}",
        out.stdout
    );
    assert_eq!(count_deals(&out.stdout), 10, "stdout:\n{}", out.stdout);
    // The records are solved, so their tables came through the pipe too. A
    // reader that recovered the deals and dropped the tables would produce the
    // same ten deals and say nothing here.
    assert!(
        out.stderr.contains("10 of 10 deals"),
        "the tables should have arrived with the deals:\n{}",
        out.stderr
    );
}

/// A library under a name that says otherwise is read for what it holds.
#[test]
fn a_library_named_as_pbn_is_read_and_the_mismatch_reported() {
    let corpus = temp_binary("library-as-pbn", "pbn", LIBRARY);
    let script = temp_file("script-library-as-pbn", "condition 1\n");

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

    assert!(out.success, "stderr:\n{}", out.stderr);
    assert!(
        out.stdout.contains("Generated 10 hands"),
        "stdout:\n{}",
        out.stdout
    );
    assert!(
        out.stderr.contains("whatever its name says"),
        "the name and the content disagree, and that should be said:\n{}",
        out.stderr
    );
}

/// And text under a `.zrd` name, which is the half-download case.
///
/// It used to be read as twenty-three-byte records of nothing: no deals, no
/// error, and no clue which of the two was wrong.
#[test]
fn text_named_as_a_library_is_read_and_the_mismatch_reported() {
    let corpus = temp_binary("text-as-zrd", "zrd", ONELINE_CORPUS.as_bytes());
    let script = temp_file("script-text-as-zrd", FILTER);

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

    assert!(out.success, "stderr:\n{}", out.stderr);
    assert!(
        out.stdout.contains("Generated 6 hands"),
        "the deals are there to be read:\n{}",
        out.stdout
    );
    assert_eq!(count_deals(&out.stdout), 2, "stdout:\n{}", out.stdout);
    assert!(
        out.stderr.contains("named as a Pavlicek library"),
        "the name and the content disagree, and that should be said:\n{}",
        out.stderr
    );
}

/// A library that arrived short is refused in terms of the library it is.
#[test]
fn a_library_cut_short_is_refused_with_a_reason() {
    let script = temp_file("script-truncated", "condition 1\n");

    let out = run_bytes(
        &[
            script.to_str().unwrap(),
            "--input-deals",
            "-",
            "-f",
            "oneline",
        ],
        Some(&LIBRARY[..LIBRARY.len() - 5]),
    );

    assert!(!out.success, "stdout:\n{}", out.stdout);
    assert!(
        out.stderr.contains("neither a Pavlicek library nor text"),
        "should name both formats it ruled out:\n{}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("stream did not contain valid UTF-8"),
        "that message is what this replaced:\n{}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// The window: where in a library a run starts, and how much of it it reads.
// ---------------------------------------------------------------------------

/// The deal lines of a run over the fixture library, with `args` added.
///
/// Every window test compares one reading against another, so they all go
/// through here: what is under test is which deals came back and in what
/// order, not how a deal is rendered.
fn library_run(path: &str, args: &[&str]) -> Output {
    let script = temp_file("script-window", "condition 1\n");
    let mut all = vec![
        script.to_str().expect("utf-8 path").to_string(),
        "--input-deals".to_string(),
        path.to_string(),
        "-f".to_string(),
        "oneline".to_string(),
        "-X".to_string(),
    ];
    all.extend(args.iter().map(|arg| arg.to_string()));
    let borrowed: Vec<&str> = all.iter().map(String::as_str).collect();
    let out = run(&borrowed, None);
    assert!(out.success, "stderr:\n{}", out.stderr);
    out
}

/// Just the deals, in the order they were produced.
fn deal_lines(out: &Output) -> Vec<String> {
    out.stdout
        .lines()
        .filter(|line| line.starts_with("n "))
        .map(str::to_string)
        .collect()
}

/// `--input-offset N` starts at record N, and record N is the file's own
/// numbering.
#[test]
fn an_offset_names_the_record_a_library_starts_at() {
    let library = temp_binary("window-offset", "zrd", LIBRARY);
    let path = library.to_str().expect("utf-8 path");

    let in_order = deal_lines(&library_run(path, &["--input-offset", "0"]));
    assert_eq!(in_order.len(), 10, "the fixture holds ten deals");

    let from_seven = deal_lines(&library_run(
        path,
        &["--input-offset", "7", "--input-limit", "1"],
    ));
    assert_eq!(
        from_seven,
        in_order[7..8],
        "record 7 is the eighth deal of the file"
    );

    // And the rest of the file follows it, coming round to the start.
    let whole_pass = deal_lines(&library_run(path, &["--input-offset", "7"]));
    let rotated: Vec<String> = in_order[7..]
        .iter()
        .chain(in_order[..7].iter())
        .cloned()
        .collect();
    assert_eq!(whole_pass, rotated, "a pass from an offset is still a pass");
}

/// `-s` picks the starting record, so it means for a library what it means for
/// a generated run: the same seed reads the same deals.
#[test]
fn the_seed_picks_where_a_library_starts() {
    let library = temp_binary("window-seed", "zrd", LIBRARY);
    let path = library.to_str().expect("utf-8 path");

    let once = deal_lines(&library_run(path, &["-s", "1"]));
    let again = deal_lines(&library_run(path, &["-s", "1"]));
    assert_eq!(once, again, "the same seed should read the same deals");

    let elsewhere = deal_lines(&library_run(path, &["-s", "2"]));
    assert_ne!(
        once[0], elsewhere[0],
        "a different seed should start somewhere else"
    );
    let mut sorted_once = once.clone();
    let mut sorted_elsewhere = elsewhere.clone();
    sorted_once.sort();
    sorted_elsewhere.sort();
    assert_eq!(
        sorted_once, sorted_elsewhere,
        "both are a pass over the same library, in a different order"
    );

    // The seed is doing something here, so the warning that it is ignored has
    // no business being printed.
    let out = library_run(path, &["-s", "1"]);
    assert!(
        !out.stderr.contains("--seed is ignored"),
        "the seed is not ignored for a library:\n{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("seed 1 starts this library at record"),
        "which record it picked is worth saying:\n{}",
        out.stderr
    );
}

/// Asking for more deals than the library holds wraps round, and says what
/// that does to the statistics.
#[test]
fn a_limit_past_the_end_of_the_library_repeats_deals_and_reports_it() {
    let library = temp_binary("window-wrap", "zrd", LIBRARY);
    let path = library.to_str().expect("utf-8 path");

    let out = library_run(path, &["--input-offset", "0", "--input-limit", "25"]);
    let deals = deal_lines(&out);

    assert_eq!(
        deals.len(),
        25,
        "the run asked for 25 deals:\n{}",
        out.stdout
    );
    assert_eq!(
        deals[..10],
        deals[10..20],
        "the second pass is the first one again"
    );
    assert!(
        out.stderr
            .contains("holds 10 deals and the run asked for 25"),
        "the repeat should be reported:\n{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("count the repeats"),
        "and what it does to `average` and `frequency` said:\n{}",
        out.stderr
    );
}

/// `-g` bounds what is read without asking for the library to be repeated.
#[test]
fn the_generate_ceiling_does_not_repeat_the_library() {
    let library = temp_binary("window-ceiling", "zrd", LIBRARY);
    let path = library.to_str().expect("utf-8 path");

    let out = library_run(path, &["--input-offset", "0", "-g", "25"]);

    assert_eq!(
        deal_lines(&out).len(),
        10,
        "a ceiling is not a demand:\n{}",
        out.stdout
    );
    assert!(
        !out.stderr.contains("repeat"),
        "nothing repeated, so nothing to report:\n{}",
        out.stderr
    );

    let short = library_run(path, &["--input-offset", "0", "-g", "4"]);
    assert_eq!(
        deal_lines(&short).len(),
        4,
        "and it does bound the reading:\n{}",
        short.stdout
    );
}

/// The window switches need a library to seek in.
#[test]
fn the_window_switches_need_input_deals() {
    let script = temp_file("script-window-alone", "condition 1\n");

    for switch in ["--input-offset", "--input-limit"] {
        let out = run(&[script.to_str().expect("utf-8 path"), switch, "3"], None);
        assert!(!out.success, "{} alone should be refused", switch);
        assert!(
            out.stderr.contains("needs --input-deals"),
            "and say why:\n{}",
            out.stderr
        );
    }
}

// --- Writing back what came in (#64) --------------------------------------
//
// A deal that arrived solved can leave solved. These drive the real binary
// both ways round — read a library, write PBN, read that PBN back — because
// the encodings are the point and a unit test over our own writer alone would
// prove only that it agrees with itself.

/// Run a script over the fixture library and return what it wrote.
fn library_pbn(tag: &str, extra: &[&str]) -> Output {
    let library = temp_binary(tag, "zrd", LIBRARY);
    let script = temp_file(&format!("script-{}", tag), "condition 1\n");
    let mut all = vec![
        script.to_str().expect("utf-8 path").to_string(),
        "--input-deals".to_string(),
        library.to_str().expect("utf-8 path").to_string(),
        "-f".to_string(),
        "pbn".to_string(),
        "-p".to_string(),
        "3".to_string(),
        "-s".to_string(),
        "1".to_string(),
    ];
    all.extend(extra.iter().map(|arg| arg.to_string()));
    let borrowed: Vec<&str> = all.iter().map(String::as_str).collect();
    run(&borrowed, None)
}

#[test]
fn a_pbn_export_carries_the_table_a_deal_arrived_with() {
    // The standard encoding by default: `OptimumResultTable` is PBN 2.1 §5.7,
    // where `DoubleDummyTricks` is a Bridge Composer extension.
    let out = library_pbn("dd-default", &[]);
    assert!(out.success, "{}", out.stderr);
    assert!(
        out.stdout.contains("[OptimumResultTable "),
        "the default writes the standard section:\n{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("[DoubleDummyTricks "),
        "and not the extension as well:\n{}",
        out.stdout
    );
    // Twenty rows a board, three boards.
    assert_eq!(
        out.stdout
            .lines()
            .filter(|line| line.starts_with("N NT"))
            .count(),
        3,
        "one table per board:\n{}",
        out.stdout
    );
}

#[test]
fn the_result_column_is_as_wide_as_the_table_needs() {
    // Header and rows come from one place for this reason: a header declaring
    // one width over rows padded to another is what had Bridge Composer
    // rewriting every single-digit table on open and save.
    let out = library_pbn("dd-width", &[]);
    let header = out
        .stdout
        .lines()
        .find(|line| line.starts_with("[OptimumResultTable "))
        .expect("a table header");
    let width: usize = if header.contains("Result\\2R") { 2 } else { 1 };
    let row = out
        .stdout
        .lines()
        .find(|line| line.starts_with("N NT"))
        .expect("a table row");
    let tricks = row.split_whitespace().last().expect("a trick count");
    assert_eq!(
        row.len(),
        "N NT ".len() + width,
        "the rows are padded to the width the header declares ({}): {:?}",
        header,
        row
    );
    assert!(
        tricks.parse::<u8>().is_ok(),
        "and end in a number: {:?}",
        row
    );
}

#[test]
fn dd_tags_chooses_the_encoding() {
    for (value, wants, not) in [
        ("tricks", "[DoubleDummyTricks ", "[OptimumResultTable "),
        ("optimum", "[OptimumResultTable ", "[DoubleDummyTricks "),
    ] {
        let out = library_pbn(&format!("dd-{}", value), &["--dd-tags", value]);
        assert!(out.success, "{}", out.stderr);
        assert!(
            out.stdout.contains(wants),
            "--dd-tags {} writes {}",
            value,
            wants
        );
        assert!(
            !out.stdout.contains(not),
            "--dd-tags {} writes only that:\n{}",
            value,
            out.stdout
        );
    }

    let both = library_pbn("dd-both", &["--dd-tags", "both"]);
    assert!(both.stdout.contains("[DoubleDummyTricks "));
    assert!(both.stdout.contains("[OptimumResultTable "));

    // Not free to carry: the section is twenty-two lines a board, where a whole
    // board is a few hundred bytes.
    let none = library_pbn("dd-none", &["--dd-tags", "none"]);
    assert!(
        !none.stdout.contains("DoubleDummy") && !none.stdout.contains("Optimum"),
        "--dd-tags none writes neither:\n{}",
        none.stdout
    );
}

#[test]
fn what_is_written_reads_back_as_the_same_analysis() {
    // The test that matters. Both encodings go out and come back through the
    // reader, and the answers have to survive the trip — a transposed axis
    // yields a plausible table, and for the seat axis it yields the
    // *opponents'* plausible table, so "it parsed" proves nothing on its own.
    let script = temp_file(
        "dd-roundtrip-script",
        "condition 1\naction average \"nt\" tricks(north, notrump), \
         average \"eh\" tricks(east, hearts)\n",
    );
    let script = script.to_str().expect("utf-8 path");

    let library = temp_binary("dd-roundtrip", "zrd", LIBRARY);
    let direct = run(
        &[
            script,
            "--input-deals",
            library.to_str().expect("utf-8 path"),
            "-f",
            "none",
            "-p",
            "3",
            "-s",
            "1",
        ],
        None,
    );
    assert!(direct.success, "{}", direct.stderr);
    let expected = statistics(&direct.stdout);
    assert!(
        !expected.is_empty(),
        "the run reported nothing:\n{}",
        direct.stdout
    );

    for value in ["optimum", "tricks", "both"] {
        let written = library_pbn(&format!("dd-rt-{}", value), &["--dd-tags", value]);
        let file = temp_file(&format!("dd-rt-file-{}", value), &written.stdout);
        let back = run(
            &[
                script,
                "--input-deals",
                file.to_str().expect("utf-8 path"),
                "-f",
                "none",
                "-p",
                "3",
            ],
            None,
        );
        assert!(back.success, "{}", back.stderr);
        assert!(
            back.stderr.contains("arrived with double-dummy tables"),
            "--dd-tags {} produced a file the reader sees as solved:\n{}",
            value,
            back.stderr
        );
        assert_eq!(
            statistics(&back.stdout),
            expected,
            "--dd-tags {} carried the same answers out and back",
            value
        );
    }
}

/// The `average` lines of a run, which is where the double-dummy answers show.
fn statistics(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|line| line.contains(':'))
        .map(|line| line.trim().to_string())
        .collect()
}

#[test]
fn a_generated_deal_nobody_asked_about_is_not_solved_to_fill_a_tag() {
    // The trap this switch must not become: `-f pbn` on a script that never
    // mentions double-dummy would otherwise cost twenty searches a deal, and
    // the output format would be deciding how long the run takes.
    let script = temp_file("dd-unsolved", "condition 1\n");
    let out = run(
        &[
            script.to_str().expect("utf-8 path"),
            "-f",
            "pbn",
            "-p",
            "2",
            "-s",
            "7",
        ],
        None,
    );
    assert!(out.success, "{}", out.stderr);
    assert!(
        !out.stdout.contains("OptimumResultTable") && !out.stdout.contains("DoubleDummyTricks"),
        "a deal with no table carries no tags:\n{}",
        out.stdout
    );
}
