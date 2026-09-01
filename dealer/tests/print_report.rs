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

/// `printns` is `printew`'s counterpart, and `printside(side)` is another
/// spelling of each. All three come from DealerV2_4, which routes them through
/// one printer.
#[test]
fn printns_and_printside_are_two_spellings_of_one_action() {
    // No `-q`: quiet mode suppresses the deals these actions print.
    let args = ["-p", "3", "-s", "1"];
    let script = "condition hcp(north) >= 15\n";
    let ns = run(&format!("{script}action printns\n"), &args);
    let side_ns = run(&format!("{script}action printside(ns)\n"), &args);
    let ew = run(&format!("{script}action printew\n"), &args);
    let side_ew = run(&format!("{script}action printside(ew)\n"), &args);

    assert_eq!(ns, side_ns, "printns and printside(ns) are one action");
    assert_eq!(ew, side_ew, "printew and printside(ew) are one action");
    assert_ne!(ns, ew, "the two partnerships are different hands");
    assert!(!ns.trim().is_empty(), "printns should print something");
}

/// The layout is `printew`'s, so the two are comparable at a glance: four suit
/// rows, two hands to a row.
#[test]
fn printns_lays_out_four_suit_rows() {
    let out = run(
        "condition hcp(north) >= 15\naction printns\n",
        &["-p", "1", "-s", "1"],
    );
    let rows: Vec<&str> = out.lines().filter(|line| !line.trim().is_empty()).collect();
    assert_eq!(rows.len(), 4, "one row a suit, got: {out:?}");
}

/// `trix(compass)` is five columns, and the numbers are the ones `tricks()`
/// gives — the same solve, reported differently.
#[test]
fn trix_reports_the_same_tricks_the_function_does() {
    let out = run(
        "condition hcp(north) >= 15\nprintrpt(trix(north), tricks(north, 0), tricks(north, 4))\n",
        &["-q", "-p", "1", "-s", "1"],
    );
    let columns: Vec<&str> = out.trim().split(',').map(|c| c.trim()).collect();
    assert_eq!(columns.len(), 7, "five strains plus two checks: {out:?}");
    // trix runs clubs, diamonds, hearts, spades, notrump.
    assert_eq!(columns[0], columns[5], "trix's first column is clubs");
    assert_eq!(columns[4], columns[6], "trix's last column is notrump");
}

#[test]
fn trix_of_the_deal_is_four_seats_of_five() {
    let out = run(
        "condition hcp(north) >= 15\nprintrpt(trix(deal))\n",
        &["-q", "-p", "1", "-s", "1"],
    );
    let columns: Vec<&str> = out.trim().split(',').collect();
    assert_eq!(columns.len(), 20, "four seats, five strains: {out:?}");
    for column in &columns {
        let tricks: u8 = column.trim().parse().expect("each column is a trick count");
        assert!(tricks <= 13, "{tricks} is not a possible trick count");
    }
}

/// `par(side)` is the deal's par score, from that side's point of view. The
/// two sides are opposites, and a compass names the side its seat is on.
#[test]
fn par_is_signed_to_the_side_that_asks() {
    let out = run(
        "condition hcp(north) >= 15\nprintrpt(par(ns), par(ew), par(north), par(south), par(east))\n",
        &["-q", "-p", "3", "-s", "1"],
    );
    for row in out.lines().filter(|line| !line.trim().is_empty()) {
        let values: Vec<i32> = row
            .trim()
            .split(',')
            .map(|v| v.trim().parse().expect("par is a score"))
            .collect();
        assert_eq!(values.len(), 5);
        assert_eq!(values[0], -values[1], "par(ew) is -par(ns)");
        assert_eq!(values[0], values[2], "north is on the NS side");
        assert_eq!(values[0], values[3], "so is south");
        assert_eq!(values[1], values[4], "east is on the EW side");
    }
}

/// Par depends on who is vulnerable, so the run's own vulnerability has to
/// reach it — that is the whole reason `EvalContext` carries one.
#[test]
fn par_follows_the_runs_vulnerability() {
    let script = "condition hcp(north) >= 15\nprintrpt(par(ns))\n";
    let none = run(
        script,
        &["-q", "-p", "5", "-s", "1", "--vulnerable", "None"],
    );
    let ns = run(script, &["-q", "-p", "5", "-s", "1", "--vulnerable", "NS"]);
    let all = run(script, &["-q", "-p", "5", "-s", "1", "--vulnerable", "All"]);
    let ew = run(script, &["-q", "-p", "5", "-s", "1", "--vulnerable", "EW"]);

    // The same deals throughout — only the scoring changes.
    assert_ne!(none, ns, "NS vulnerable has to change NS's par");
    assert_eq!(ns, all, "NS is vulnerable in both");
    assert_eq!(none, ew, "NS is not vulnerable in either");

    // A script's own `vulnerable` statement reaches it the same way.
    let by_statement = run(
        "vulnerable All\ncondition hcp(north) >= 15\nprintrpt(par(ns))\n",
        &["-q", "-p", "5", "-s", "1"],
    );
    assert_eq!(by_statement, all, "the statement and the switch agree");
}

/// A script that names no vulnerability gets none — deliberately not the
/// rotation `printpbn` applies, which would make par answer differently for
/// the same cards depending on where the board sat.
#[test]
fn par_without_a_vulnerability_assumes_neither_side() {
    let script = "condition hcp(north) >= 15\nprintrpt(par(ns))\n";
    let unset = run(script, &["-q", "-p", "5", "-s", "1"]);
    let none = run(
        script,
        &["-q", "-p", "5", "-s", "1", "--vulnerable", "None"],
    );
    assert_eq!(unset, none);
}
