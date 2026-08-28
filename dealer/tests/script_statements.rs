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
