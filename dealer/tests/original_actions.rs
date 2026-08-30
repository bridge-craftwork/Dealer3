//! `print(...)`, `printes(...)` and `rnd()` — the last three words of the
//! original's language dealer3 did not implement.
//!
//! All three are checked against output captured from the reference binary,
//! not against what the source looked like it would do. `print`'s layout in
//! particular is a line-printer format with counted column padding and a form
//! feed, which is easy to get subtly wrong and impossible to notice by eye.

use std::io::Write;
use std::process::{Command, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_dealer")
}

/// Three hands fully predealt, so the deal is the same every time and the same
/// in any dealer. West takes what is left, which is forced.
///
/// Not all four: the original looks for an unstacked slot to shuffle into, and
/// with all 52 predealt it searches for one forever. dealer3 shuffles only the
/// free cards and returns immediately, but a fixture that hangs the reference
/// is no use for comparing against it.
const FIXED_DEAL: &str = "\
predeal north SAKQJ,HAKQ,DAKQ,CAKQ
predeal east ST987,HJT9,DJT9,CJT9
predeal south S6543,H876,D876,C876
";

fn run(args: &[&str], script: &str) -> (String, i32) {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dealer3-actions-{}-{:?}.dlr",
        std::process::id(),
        std::thread::current().id()
    ));
    let mut file = std::fs::File::create(&path).expect("temp script");
    file.write_all(script.as_bytes()).expect("temp script");
    let output = Command::new(bin())
        .args(args)
        .arg(&path)
        .stdin(Stdio::null())
        .output()
        .expect("dealer should run");
    let _ = std::fs::remove_file(&path);
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

/// Captured from the reference binary, five boards of the fixture deal, both
/// seats. Trailing spaces are part of it: the original writes a space after
/// every card and never trims the line.
const REFERENCE_PRINT: &str = concat!(
    "\n\nNorth hands:\n\n\n\n",
    "   1.                  2.                  3.                  4.               \n",
    "A K Q J             A K Q J             A K Q J             A K Q J \n",
    "A K Q               A K Q               A K Q               A K Q \n",
    "A K Q               A K Q               A K Q               A K Q \n",
    "A K Q               A K Q               A K Q               A K Q \n",
    "\n",
    "   5.               \n",
    "A K Q J \n",
    "A K Q \n",
    "A K Q \n",
    "A K Q \n",
    "\n",
    "\x0c",
    "\n\nSouth hands:\n\n\n\n",
    "   1.                  2.                  3.                  4.               \n",
    "6 5 4 3             6 5 4 3             6 5 4 3             6 5 4 3 \n",
    "8 7 6               8 7 6               8 7 6               8 7 6 \n",
    "8 7 6               8 7 6               8 7 6               8 7 6 \n",
    "8 7 6               8 7 6               8 7 6               8 7 6 \n",
    "\n",
    "   5.               \n",
    "6 5 4 3 \n",
    "8 7 6 \n",
    "8 7 6 \n",
    "8 7 6 \n",
    "\n",
    "\x0c",
);

#[test]
fn print_lays_hands_out_exactly_as_the_original_does() {
    let script = format!("{FIXED_DEAL}condition 1\naction print(north, south)\n");
    let (out, status) = run(&["-q", "-p", "5", "-s", "1"], &script);
    assert_eq!(status, 0);
    assert_eq!(out, REFERENCE_PRINT);
}

/// The original collects the seats into a bitmask, so the order they are named
/// in makes no difference and naming one twice is the same as naming it once.
#[test]
fn the_seats_come_out_in_compass_order_however_they_are_named() {
    let script = format!("{FIXED_DEAL}condition 1\naction print(south, north, south)\n");
    let (out, status) = run(&["-q", "-p", "5", "-s", "1"], &script);
    assert_eq!(status, 0);
    assert_eq!(out, REFERENCE_PRINT);
}

/// Four boards to a page, so the page breaks are what to check.
#[test]
fn a_page_holds_four_boards() {
    let script = format!("{FIXED_DEAL}condition 1\naction print(north)\n");
    for produced in [1, 4, 5, 8, 9, 13] {
        let (out, status) = run(&["-q", "-p", &produced.to_string(), "-s", "1"], &script);
        assert_eq!(status, 0);
        // Every board is numbered, in one run of four columns or another.
        for board in 1..=produced {
            assert!(
                out.contains(&format!("{board:4}.")),
                "board {board} missing from -p {produced}"
            );
        }
        // One spade line per board, and one page header per group of four.
        assert_eq!(out.matches("A K Q J").count(), produced, "-p {produced}");
        assert_eq!(
            out.matches("   1.").count(),
            1,
            "board 1 should head exactly one page at -p {produced}"
        );
        // A form feed ends the seat, not each page.
        assert_eq!(out.matches('\x0c').count(), 1, "-p {produced}");
    }
}

#[test]
fn printes_prints_what_it_is_given_and_nothing_else() {
    let script = format!(
        "{FIXED_DEAL}condition 1\naction printes(\"N=\", hcp(north), \" S=\", hcp(south), \\n)\n"
    );
    let (out, status) = run(&["-q", "-p", "3", "-s", "1"], &script);
    assert_eq!(status, 0);
    assert_eq!(out, "N=37 S=0\nN=37 S=0\nN=37 S=0\n");
}

/// The original's lexer reads no escapes between quotes — a real newline is a
/// bare `\n` in the list. Getting this wrong would be invisible until someone
/// compared output with dealer.exe.
#[test]
fn a_backslash_n_inside_quotes_stays_literal() {
    let script = format!("{FIXED_DEAL}condition 1\naction printes(\"a\\nb\", \\n)\n");
    let (out, status) = run(&["-q", "-p", "2", "-s", "1"], &script);
    assert_eq!(status, 0);
    assert_eq!(out, "a\\nb\na\\nb\n");
}

#[test]
fn printes_runs_only_for_matching_deals() {
    let script = "condition hcp(north) >= 20\naction printes(hcp(north), \\n)\n";
    let (out, status) = run(&["-q", "-p", "4", "-s", "1"], script);
    assert_eq!(status, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4, "{out:?}");
    for line in lines {
        let hcp: i32 = line.parse().expect("a number per line");
        assert!(
            hcp >= 20,
            "printes ran for a deal that did not match: {hcp}"
        );
    }
}

#[test]
fn rnd_is_uniform_and_within_its_bound() {
    let script = "condition 1\naction printes(rnd(10), \\n)\n";
    let (out, status) = run(&["-q", "-p", "4000", "-s", "1"], script);
    assert_eq!(status, 0);
    let mut counts = [0usize; 10];
    for line in out.lines() {
        let value: usize = line.parse().expect("a number per line");
        assert!(value < 10, "rnd(10) returned {value}");
        counts[value] += 1;
    }
    // Expect 400 a bucket. A generous band, so this fails on a broken
    // generator rather than on an unlucky one.
    for (value, count) in counts.iter().enumerate() {
        assert!(
            (250..=550).contains(count),
            "rnd(10) gave {value} {count} times in 4000, which is not uniform"
        );
    }
}

/// The point of drawing from a stream of its own: output cannot depend on how
/// many threads happened to be running.
#[test]
fn rnd_gives_the_same_answers_whatever_the_thread_count() {
    let script = "condition rnd(4) == 1\naction printoneline\n";
    let reference = run(&["-p", "25", "-s", "9"], script).0;
    assert!(!reference.is_empty());
    for extra in [
        vec!["-R", "1"],
        vec!["-R", "8"],
        vec!["--batch-size", "3"],
        vec!["--batch-size", "997"],
    ] {
        let mut args = vec!["-p", "25", "-s", "9"];
        args.extend(extra.iter().copied());
        assert_eq!(run(&args, script).0, reference, "{extra:?}");
    }
}

#[test]
fn the_rnd_seed_switch_shifts_the_stream_without_moving_the_deals() {
    let rnd_script = "condition rnd(4) == 1\naction printoneline\n";
    let plain_script = "condition 1\naction printoneline\n";

    let default = run(&["-p", "20", "-s", "9"], rnd_script).0;
    let shifted = run(&["-p", "20", "-s", "9", "--rnd-seed", "7"], rnd_script).0;
    assert_ne!(default, shifted, "--rnd-seed changed nothing");

    // The deals themselves must be untouched: the switch feeds `rnd()`, not
    // the shuffle.
    let deals = run(&["-p", "20", "-s", "9"], plain_script).0;
    let deals_with_seed = run(&["-p", "20", "-s", "9", "--rnd-seed", "7"], plain_script).0;
    assert_eq!(deals, deals_with_seed);
}

/// `evalcontract` is the one word left that the original accepts and dealer3
/// does not. It has to fail loudly, because being read as a variable is how it
/// used to produce no deals, no error and exit 0.
#[test]
fn evalcontract_is_refused_rather_than_read_as_a_variable() {
    let (_, status) = run(&["-p", "1", "-s", "1"], "condition evalcontract\n");
    assert_eq!(status, 1);
    let (_, status) = run(
        &["-p", "1", "-s", "1"],
        "condition 1\naction evalcontract\n",
    );
    assert_eq!(status, 1);
}

/// A `score` script in the original's own spelling, run through the reference
/// binary and captured. Both binaries were given this file and printed these
/// seven lines.
///
/// The contract and the vulnerability are lexer tokens in the original
/// (`scan.l:47-48` and `scan.l:101`), not expressions, so until dealer3 learned
/// the words this script was a parse error here and its numeric equivalent was
/// a syntax error there — the one word neither dialect could read from the
/// other. The capture is what keeps that shut.
const REFERENCE_SCORES: &str = "\
a_3N_nv_9: 400
b_3N_vul_9: 600
c_4S_nv_10: 420
d_1C_nv_7: 70
e_7N_vul_13: 2220
f_5D_nv_9: -100
g_6H_vul_12: 1430
";

#[test]
fn score_reads_the_originals_contract_and_vulnerability_words() {
    let script = format!(
        "{FIXED_DEAL}condition 1
action average \"a_3N_nv_9\" score(nv, x3N, 9),
       average \"b_3N_vul_9\" score(vul, x3N, 9),
       average \"c_4S_nv_10\" score(nv, x4S, 10),
       average \"d_1C_nv_7\" score(nv, x1C, 7),
       average \"e_7N_vul_13\" score(vul, x7N, 13),
       average \"f_5D_nv_9\" score(nv, x5D, 9),
       average \"g_6H_vul_12\" score(vul, x6H, 12)
"
    );
    let (out, status) = run(&["-q", "-p", "1", "-s", "1"], &script);
    assert_eq!(status, 0);
    assert_eq!(out, REFERENCE_SCORES);
}
