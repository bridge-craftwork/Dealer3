//! `printrpt`: DealerV2_4's `csvrpt` to the screen.
//!
//! The two statements share a renderer in the CLI because they are the same
//! row — DealerV2_4's own reference output differs only in where it goes. The
//! browser has a second copy of that renderer, since the wasm cannot call into
//! the binary, so the shape is pinned here.

use std::io::Write;
use std::process::{Command, Stdio};

fn run(script: &str, args: &[&str]) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dealer"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("dealer should run");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(script.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("output");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The row's shape: a leading space, commas between terms, strings in single
/// quotes. Anything else and a DealerV2_4 script's output stops being readable
/// by whatever consumed it there.
#[test]
fn a_row_is_quoted_and_comma_separated() {
    let out = run(
        "produce 1\ncondition 1\nprintrpt(\"label\", hcp(north), north)\n",
        &["-q", "-s", "7"],
    );
    let line = out.lines().next().expect("a row");
    assert!(
        line.starts_with(' '),
        "row should lead with a space: {line:?}"
    );
    let parts: Vec<&str> = line.trim_start().split(',').collect();
    assert_eq!(parts.len(), 3, "{line:?}");
    assert_eq!(parts[0], "'label'");
    assert!(
        parts[1].parse::<i32>().is_ok(),
        "an expression is an integer: {:?}",
        parts[1]
    );
    // A hand: four suits separated by dots.
    assert_eq!(parts[2].matches('.').count(), 3, "{:?}", parts[2]);
}

/// One row per matching deal, per statement, in the order written.
#[test]
fn each_statement_writes_a_row_for_every_deal() {
    let out = run(
        "produce 3\ncondition 1\nprintrpt(\"a\")\nprintrpt(\"b\")\n",
        &["-q", "-s", "7"],
    );
    let rows: Vec<&str> = out.lines().filter(|l| l.contains('\'')).collect();
    assert_eq!(rows.len(), 6, "three deals x two statements: {out:?}");
    assert_eq!(rows[0].trim(), "'a'");
    assert_eq!(rows[1].trim(), "'b'");
}

/// DealerV2_4's scripts reach it through `action` far more often than as a bare
/// statement, and the suite has several with more than one in a list.
#[test]
fn it_works_inside_an_action_list() {
    let out = run(
        "produce 1\ncondition 1\naction printrpt(\"x\", deal),\n       printrpt(\"y\", ns)\n",
        &["-q", "-s", "7"],
    );
    let rows: Vec<&str> = out.lines().filter(|l| l.contains('\'')).collect();
    assert_eq!(rows.len(), 2, "{out:?}");
    // `deal` is four hands, `ns` is two.
    assert_eq!(rows[0].matches('.').count(), 12, "{:?}", rows[0]);
    assert_eq!(rows[1].matches('.').count(), 6, "{:?}", rows[1]);
}

/// `printrpt` and `csvrpt` are one row written to two places, so the same terms
/// have to render identically. They share a function; this is what says so.
#[test]
fn it_renders_what_csvrpt_renders() {
    let terms = "\"s\", hcp(north), north, ns, deal";
    let printed = run(
        &format!("produce 2\ncondition 1\nprintrpt({terms})\n"),
        &["-q", "-s", "11"],
    );

    let dir = std::env::temp_dir();
    let csv = dir.join(format!("dealer3-printrpt-{}.csv", std::process::id()));
    run(
        &format!("produce 2\ncondition 1\ncsvrpt({terms})\n"),
        &["-q", "-s", "11", "-C", &csv.display().to_string()],
    );
    let written = std::fs::read_to_string(&csv).expect("csv written");
    let _ = std::fs::remove_file(&csv);

    let rows = |s: &str| -> Vec<String> {
        s.lines()
            .filter(|l| l.contains('\''))
            .map(|l| l.to_string())
            .collect()
    };
    assert_eq!(rows(&printed), rows(&written), "the two must agree");
    assert_eq!(rows(&printed).len(), 2);
}
