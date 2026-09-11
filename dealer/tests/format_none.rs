//! `-f none`: the statistics, and no deals at all.
//!
//! The point of the value is that it changes what is *written*, never what is
//! *computed*. So every test here runs the same script twice — once printing
//! deals, once not — and compares. A `none` that quietly dealt differently, or
//! stopped at a different point, would still print no boards and would look
//! exactly like a success.

use std::io::Write;
use std::process::{Command, Stdio};

/// One run, returning stdout, stderr and whether it succeeded.
fn run(script: &str, args: &[&str]) -> (String, String, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dealer"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("dealer should run");
    // A run refused at argument parsing can exit before reading its script, and
    // the write then meets a closed pipe. That is the refusal the test is after,
    // not a fault in the harness — and whether it happens is a race, so it
    // failed on one CI runner in several rather than everywhere.
    if let Err(e) = child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(script.as_bytes())
    {
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe, "write: {e}");
    }
    let out = child.wait_with_output().expect("output");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// A script asking for both kinds of statistic, so "the numbers are untouched"
/// is a claim about both.
const STATISTICS: &str = "condition hcp(north) >= 12\n\
     action printoneline,\n\
     average \"N HCP\" hcp(north),\n\
     frequency \"N HCP\" (hcp(north), 12, 20)\n";

#[test]
fn none_prints_the_statistics_and_not_one_deal() {
    let shown = run(STATISTICS, &["-f", "oneline", "-p", "40", "-s", "9", "-v"]).0;
    let (quiet, _, ok) = run(STATISTICS, &["-f", "none", "-p", "40", "-s", "9", "-v"]);
    assert!(ok, "-f none should be an ordinary run");

    // A one-line board names all four seats. None of them should appear.
    assert!(
        shown.contains(" e ") && shown.contains(" w "),
        "the control run should be printing boards:\n{shown}"
    );
    assert!(
        !quiet.contains(" e ") && !quiet.contains(" w "),
        "-f none should write no deals:\n{quiet}"
    );

    // And the statistics should survive whole — the average, the histogram, and
    // the counts underneath them. All but the clock, which is the one line that
    // is allowed to differ, and the one this change is meant to move.
    for line in shown
        .lines()
        .filter(|l| !l.contains(" e ") && !l.starts_with("Time needed"))
    {
        assert!(
            quiet.contains(line),
            "-f none dropped a line that is not a deal: {line:?}\n\
             --- with deals ---\n{shown}\n--- without ---\n{quiet}"
        );
    }
    assert!(
        quiet.contains("N HCP"),
        "the labels should be there:\n{quiet}"
    );
}

#[test]
fn none_changes_what_is_shown_and_never_what_is_computed() {
    // The claim that actually matters. `Generated`/`Produced` come from the
    // run itself, so if `none` shortened it, skipped deals or stopped early,
    // these two numbers would part company and nothing else on screen would
    // say so.
    let counts = |format: &str| {
        let out = run(STATISTICS, &["-f", format, "-p", "40", "-s", "9", "-v"]).0;
        let pick = |word: &str| {
            out.lines()
                .find(|l| l.starts_with(word))
                .map(str::to_string)
                .unwrap_or_else(|| panic!("no {word} line in:\n{out}"))
        };
        (pick("Generated"), pick("Produced"))
    };
    assert_eq!(counts("none"), counts("oneline"));
}

#[test]
fn none_leaves_the_scripts_own_output_alone() {
    // `printes` is a statement the script ran, not a rendering of a deal, and
    // `-f` has no business turning it off. Without this, a script whose whole
    // output is its own `printes` line would come back blank under `none` and
    // look like a run that matched nothing.
    let script = "condition hcp(north) >= 12\naction printes(\"N=\", hcp(north), \\n)\n";
    let shown = run(script, &["-f", "oneline", "-p", "5", "-s", "9"]).0;
    let quiet = run(script, &["-f", "none", "-p", "5", "-s", "9"]).0;

    let printed = |out: &str| {
        out.lines()
            .filter(|l| l.starts_with("N="))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(printed(&shown).len(), 5, "the control run:\n{shown}");
    assert_eq!(printed(&quiet), printed(&shown));
    assert!(!quiet.contains(" e "), "and still no boards:\n{quiet}");
}

#[test]
fn interleave_is_refused_rather_than_silently_ordering_nothing() {
    // `--interleave` holds every produced deal to order it on the way out.
    // Under `none` there is no way out, so the switch would swallow the whole
    // run's deals for a reordering nobody sees.
    let script = "HandType_Strong = hcp(north) >= 15\n\
         HandType_Weak = hcp(north) < 15\n\
         condition HandType_Strong || HandType_Weak\n";
    let (_, stderr, ok) = run(
        script,
        &["-f", "none", "--interleave", "-p", "8", "-s", "9"],
    );
    assert!(!ok, "the combination should be refused");
    assert!(
        stderr.contains("--interleave") && stderr.contains("none"),
        "the refusal should name both halves: {stderr}"
    );
}

#[test]
fn an_unknown_format_offers_none_among_the_rest() {
    let (_, stderr, ok) = run("condition 1\n", &["-f", "hands", "-p", "1"]);
    assert!(!ok, "there is no such format");
    assert!(
        stderr.contains("none"),
        "the refusal should list none: {stderr}"
    );
}
