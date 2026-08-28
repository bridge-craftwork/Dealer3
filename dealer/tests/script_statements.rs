//! `title` and `seed` as statements, and the names a script never defines.
//!
//! All three come from reading DealerV2_4's 61-script regression suite against
//! dealer3. Four of those scripts parsed. 58 open with `title "..."` and every
//! one of the 61 opens with `seed N` — and `seed` was the worse of the two,
//! because a script asking for a seed was not refused, it was silently ignored:
//! the word parsed as a variable reference and the number as an expression,
//! both discarded, and the run used the clock.

use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], script: &str) -> (String, String, i32) {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dealer3-stmt-{}-{:?}.dlr",
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

#[test]
fn a_title_statement_names_the_run() {
    let (out, _, status) = run(
        &["-p", "1", "-s", "1"],
        "title \"Weak two openings\"\ncondition hcp(north) > 10\naction printpbn\n",
    );
    assert_eq!(status, 0);
    assert!(out.contains("[Event \"Weak two openings\"]"), "got:\n{out}");
}

#[test]
fn the_switch_beats_the_statement() {
    // As `-d` beats `dealer` and `--vulnerable` beats `vulnerable`.
    let (out, _, status) = run(
        &["-p", "1", "-s", "1", "-T", "From the switch"],
        "title \"From the script\"\ncondition hcp(north) > 10\naction printpbn\n",
    );
    assert_eq!(status, 0);
    assert!(out.contains("[Event \"From the switch\"]"), "got:\n{out}");
    assert!(!out.contains("From the script"));
}

#[test]
fn a_seed_statement_is_the_same_as_the_switch() {
    let body = "condition hcp(north) > 10\naction printoneline\n";
    let (from_statement, _, status) = run(&["-p", "3"], &format!("seed 42\n{body}"));
    assert_eq!(status, 0);
    let (from_switch, _, _) = run(&["-p", "3", "-s", "42"], body);
    assert_eq!(
        from_statement, from_switch,
        "a script asking for seed 42 should deal what -s 42 deals"
    );
    assert!(!from_statement.is_empty());
}

#[test]
fn the_seed_switch_beats_the_seed_statement() {
    let body = "condition hcp(north) > 10\naction printoneline\n";
    let (overridden, _, status) = run(&["-p", "3", "-s", "7"], &format!("seed 42\n{body}"));
    assert_eq!(status, 0);
    let (plain, _, _) = run(&["-p", "3", "-s", "7"], body);
    assert_eq!(overridden, plain);
}

#[test]
fn a_misspelled_statement_keyword_is_refused() {
    // The whole reason for the check. `dealr west` is a legal program — a
    // variable reference and a compass, two bare expression statements, both
    // discarded — so nothing but a name lookup can catch it. dealer.exe says
    // `line 1: unknown variable`.
    let (out, err, status) = run(
        &["-p", "1", "-s", "1"],
        "dealr west\ncondition hcp(north) > 10\n",
    );
    assert_eq!(status, 1, "stdout was: {out}");
    assert!(err.contains("never defined"), "stderr was: {err}");
    assert!(err.contains("dealr"), "the name should be quoted: {err}");
    assert!(out.is_empty(), "no deals should have been dealt");
}

#[test]
fn a_name_used_only_in_an_action_is_checked_too() {
    // Actions are evaluated, so this one would have been caught eventually —
    // but at the first matching deal, after the run has started.
    let (_, err, status) = run(
        &["-p", "1", "-s", "1"],
        "condition hcp(north) > 10\naction average \"a\" mistyped\n",
    );
    assert_eq!(status, 1);
    assert!(err.contains("mistyped"), "stderr was: {err}");
}

#[test]
fn a_script_that_defines_what_it_uses_is_left_alone() {
    let (out, err, status) = run(
        &["-p", "2", "-s", "1"],
        "opener = hcp(north) >= 12\nbalanced = shape(north, any 4333 + any 4432)\n\
         condition opener and balanced\naction printoneline\n",
    );
    assert_eq!(status, 0, "stderr was: {err}");
    assert_eq!(out.lines().count(), 2);
}

#[test]
fn a_script_parameter_is_source_rather_than_a_value() {
    // DealerV2_4's own NTscripted.dls: one script, two notrump ranges, with
    // `$9($0)` a function name applied to a compass.
    let script = "\
NTshape = shape($0, any 4333 + any 4432 + any 5332 - 5xxx - x5xx)
condition NTshape and ($9($0) >= $1) and ($9($0) <= $2)
action printoneline
";
    let weak = [
        "-p", "40", "-g", "3000000", "-s", "1", "--param", "0=west", "--param", "1=12", "--param",
        "2=14", "--param", "9=hcp",
    ];
    let (out, err, status) = run(&weak, script);
    assert_eq!(status, 0, "stderr was: {err}");
    assert_eq!(out.lines().count(), 40);

    // The same script asking for a different band deals different hands.
    let strong: Vec<&str> = weak
        .iter()
        .map(|a| match *a {
            "1=12" => "1=15",
            "2=14" => "2=17",
            other => other,
        })
        .collect();
    let (stronger, _, status) = run(&strong, script);
    assert_eq!(status, 0);
    assert_ne!(out, stronger);
}

#[test]
fn an_unfilled_parameter_is_refused() {
    // Where DealerV2_4 scans an empty buffer and carries on.
    let (out, err, status) = run(
        &["-p", "1", "-s", "1", "--param", "0=north"],
        "condition hcp($0) >= $1\n",
    );
    assert_eq!(status, 1, "stdout was: {out}");
    assert!(err.contains("$1"), "stderr was: {err}");
    assert!(err.contains("--param 1="), "should say how: {err}");
}

#[test]
fn a_parameter_nothing_uses_is_a_warning_not_an_error() {
    let (out, err, status) = run(
        &["-p", "1", "-s", "1", "--param", "7=north"],
        "condition hcp(north) >= 10\naction printoneline\n",
    );
    assert_eq!(status, 0, "an unused parameter should not stop the run");
    assert!(err.contains("never mentions"), "stderr was: {err}");
    assert_eq!(out.lines().count(), 1);
}

#[test]
fn a_parameter_fills_a_shape_before_it_is_expanded() {
    // From DealerV2_4's FDScript_s233.dls, whose own comment says to run it
    // with `-1 north -2 '(55xx)'`. The order matters: the parameter has to be
    // in place before the shape language reads it.
    let script = "condition shape{$1, $2:d>c or h>s}\naction printoneline\n";
    let (out, err, status) = run(
        &[
            "-p", "20", "-g", "2000000", "-s", "1", "--param", "1=north", "--param", "2=(55xx)",
        ],
        script,
    );
    assert_eq!(status, 0, "stderr was: {err}");
    assert_eq!(out.lines().count(), 20);
    // Every hand is 5-5 somewhere, which is what `(55xx)` asks for.
    for line in out.lines() {
        let north = line.split_whitespace().nth(1).expect("the north holding");
        let mut lengths: Vec<usize> = north.split('.').map(str::len).collect();
        lengths.sort_unstable();
        assert_eq!(
            &lengths[2..],
            &[5, 5],
            "`{north}` is not a 5-5 hand, from: {line}"
        );
    }
}

#[test]
fn a_malformed_parameter_is_refused() {
    for spec in ["west", "x=west", "10=west"] {
        let (_, err, status) = run(&["-p", "1", "-s", "1", "--param", spec], "condition 1\n");
        assert_eq!(status, 1, "`{spec}` should be refused");
        assert!(!err.is_empty());
    }
}
