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
fn a_script_carrying_its_own_defaults_runs_with_no_switches() {
    // The whole point of the declaration: NTscripted.dls handed to someone
    // without the invocation that goes with it used to be unrunnable, and
    // nothing in the file said what `$1` was meant to be.
    let script = "\
# param 0 = west   # the seat that opens
# param 1 = 12     # minimum HCP
# param 2 = 14     # maximum HCP
# param 9 = hcp    # how strength is counted
NTshape = shape($0, any 4333 + any 4432 + any 5332 - 5xxx - x5xx)
condition NTshape and ($9($0) >= $1) and ($9($0) <= $2)
action printoneline
";
    let (out, err, status) = run(&["-p", "20", "-g", "3000000", "-s", "1"], script);
    assert_eq!(status, 0, "stderr was: {err}");
    assert_eq!(out.lines().count(), 20);

    // And a switch still wins, one parameter at a time: this is the same
    // script asking for the strong notrump instead.
    let (stronger, err, status) = run(
        &[
            "-p", "20", "-g", "3000000", "-s", "1", "--param", "1=15", "--param", "2=17",
        ],
        script,
    );
    assert_eq!(status, 0, "stderr was: {err}");
    assert_eq!(stronger.lines().count(), 20);
    assert_ne!(out, stronger);
}

#[test]
fn params_lists_what_a_script_wants_without_running_it() {
    let (out, err, status) = run(
        &["--params"],
        "# param 0 = west   # the seat that opens\ncondition hcp($0) >= $1\naction printoneline\n",
    );
    assert_eq!(status, 0, "stderr was: {err}");
    assert!(out.contains("$0"), "stdout was: {out}");
    assert!(out.contains("west"), "the declared default: {out}");
    assert!(
        out.contains("the seat that opens"),
        "the description: {out}"
    );
    // `$1` is used and undeclared, which is the thing a caller has to supply.
    assert!(out.contains("$1"), "stdout was: {out}");
    assert!(out.contains("--param 1="), "should say how: {out}");
    // Listing is not running.
    assert!(!out.contains(" n "), "should not have dealt: {out}");
}

#[test]
fn params_reports_as_json_for_a_tool() {
    let (out, err, status) = run(
        &["--params", "--stats-json"],
        "# param 0 = west  # the seat\ncondition hcp($0) >= $1\n",
    );
    assert_eq!(status, 0, "stderr was: {err}");
    let json: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let params = json["params"].as_array().expect("an array");
    assert_eq!(params.len(), 2);
    assert_eq!(params[0]["index"], 0);
    assert_eq!(params[0]["default"], "west");
    assert_eq!(params[0]["description"], "the seat");
    assert_eq!(params[1]["index"], 1);
    assert!(params[1]["default"].is_null(), "nothing declares `$1`");
}

#[test]
fn a_declaration_nothing_uses_is_a_warning_not_an_error() {
    // The mirror of `a_parameter_nothing_uses_is_a_warning_not_an_error`: a
    // declaration whose `$7` an edit took away does nothing, and without this
    // nothing would say so.
    let (out, err, status) = run(
        &["-p", "1", "-s", "1"],
        "# param 7 = north  # left over from an edit\ncondition hcp(north) >= 10\n\
         action printoneline\n",
    );
    assert_eq!(status, 0, "stderr was: {err}");
    assert!(err.contains("declares `$7`"), "stderr was: {err}");
    assert_eq!(out.lines().count(), 1);
}

#[test]
fn a_malformed_declaration_is_refused() {
    // Skipping it silently is how a script ends up unrunnable with a line in it
    // that looks as though it should have worked.
    let (_, err, status) = run(
        &["-p", "1", "-s", "1"],
        "# param 0 west\ncondition hcp($0) >= 10\n",
    );
    assert_eq!(status, 1);
    assert!(err.contains("line 1"), "should point at it: {err}");
}

#[test]
fn a_malformed_parameter_is_refused() {
    for spec in ["west", "x=west", "10=west"] {
        let (_, err, status) = run(&["-p", "1", "-s", "1", "--param", spec], "condition 1\n");
        assert_eq!(status, 1, "`{spec}` should be refused");
        assert!(!err.is_empty());
    }
}

#[test]
fn a_call_with_the_wrong_number_of_arguments_is_refused() {
    // #36. The evaluator raised this correctly all along, but the condition
    // read the error as "this deal does not match" and threw it away, so the
    // run dealt everything `-g` allowed, produced nothing, said nothing and
    // exited 0 — the same silent shape as `dealr west` above, one layer down.
    let (out, err, status) = run(
        &["-p", "1", "-s", "1", "-g", "50"],
        "condition hcp(north, spades, clubs) == 1\n",
    );
    assert_eq!(status, 1, "stdout was: {out}");
    assert!(err.contains("hcp"), "the function should be named: {err}");
    assert!(
        err.contains("1 or 2") && err.contains("not 3"),
        "the counts should both be given: {err}"
    );
    assert!(out.is_empty(), "no deals should have been dealt");
}

#[test]
fn a_miscounted_call_is_refused_before_any_deal_is_made() {
    // The condition here matches nothing, which is what used to hide this: a
    // statistic is evaluated only for a deal that matched, so the error was
    // never reached. An argument count is a property of the script, so it is
    // now settled before the first card comes out.
    let (out, err, status) = run(
        &["-p", "1", "-s", "1", "-g", "50"],
        "condition hcp(north) >= 40\naction average \"a\" controls(north, spades, hearts)\n",
    );
    assert_eq!(status, 1, "stdout was: {out}");
    assert!(
        err.contains("average") && err.contains("controls"),
        "the statement and the function should both be named: {err}"
    );
}

#[test]
fn a_function_that_takes_either_count_still_takes_both() {
    // The check reads `Function::arity`, so getting a range wrong would refuse
    // a legal script rather than accept a broken one. `hcp` is the one every
    // script uses.
    for script in [
        "condition hcp(north) >= 10\n",
        "condition hcp(north, spades) >= 3\n",
    ] {
        let (_, err, status) = run(&["-q", "-p", "1", "-s", "1"], script);
        assert_eq!(status, 0, "`{script}` should run: {err}");
    }
}

#[test]
fn a_miscounted_score_is_reported_as_a_count_not_as_unknown_names() {
    // `score` has a grammar rule of its own, because its first two arguments
    // are words rather than expressions. When that rule demanded exactly three
    // arguments, a call with two fell through to the ordinary function rule —
    // where `nv` and `x3N` are nothing but names the script never defined. The
    // error was loud but about the wrong thing.
    for (script, got) in [
        ("condition score(nv, x3N) == 400\n", "not 2"),
        ("condition score(x3N) == 400\n", "not 1"),
        ("condition score(nv, x3N, 9, 1) == 400\n", "not 4"),
        // The numeric spelling reached the right error already; it must keep it.
        ("condition score(0, 19) == 400\n", "not 2"),
    ] {
        let (_, err, status) = run(&["-q", "-p", "1", "-s", "1", "-g", "20"], script);
        assert_eq!(status, 1, "`{script}` should be refused");
        assert!(
            err.contains("score takes 3 arguments") && err.contains(got),
            "`{script}` should name the count, not the words: {err}"
        );
        assert!(
            !err.contains("never defined"),
            "`{script}` should not blame the contract words: {err}"
        );
    }
}

#[test]
fn a_score_call_that_counts_right_still_runs() {
    for script in [
        "condition score(nv, x3N, 9) == 400\n",
        "condition score(0, 19, 9) == 400\n",
        "condition score(vul, x4Sxx, 10) == 1080\n",
    ] {
        let (_, err, status) = run(&["-q", "-p", "1", "-s", "1"], script);
        assert_eq!(status, 0, "`{script}` should run: {err}");
    }
}

#[test]
fn a_decimal_is_a_hundred_times_itself() {
    // DealerV2_4's dotnums: `(int)(100. * atof(yytext))`. Sugar for an integer,
    // not a fraction — nothing downstream tracks a scale.
    for (written, expected) in [
        ("6.25", "625"),
        ("3.0", "300"),
        (".5", "50"),
        (".25", "25"),
        ("6.", "600"),
        ("0.75", "75"),
        ("-2.5", "-250"),
        ("13", "13"),
    ] {
        let (out, err, status) = run(
            &["-q", "-p", "1", "-s", "1"],
            &format!("condition 1\nprintrpt({written})\n"),
        );
        assert_eq!(status, 0, "`{written}` should run: {err}");
        assert_eq!(out.trim(), expected, "{written}");
    }
}

#[test]
fn a_decimal_count_row_is_the_integer_row_it_denotes() {
    // The reason decimals exist: weighting a card at 0.75 in a count row.
    let decimals = run(
        &["-q", "-p", "1", "-s", "1"],
        "altcount 8 6.25 4.25 1.5 0.75 .25\ncondition 1\nprintrpt(pt6(north))\n",
    );
    let integers = run(
        &["-q", "-p", "1", "-s", "1"],
        "altcount 8 625 425 150 75 25\ncondition 1\nprintrpt(pt6(north))\n",
    );
    assert_eq!(decimals.0, integers.0, "the two spellings are one row");
    assert_eq!(decimals.2, 0);
}

#[test]
fn more_than_two_digits_before_the_point_is_not_one_number() {
    // DealerV2_4's limit, and it is load-bearing: its lexer reads `123.45` as
    // `123` then `.45`, which is a syntax error. Being more permissive here
    // would accept scripts that fail there.
    let (_, err, status) = run(
        &["-q", "-p", "1", "-s", "1"],
        "condition 1\nprintrpt(123.45)\n",
    );
    assert_eq!(status, 1, "123.45 should be refused");
    assert!(err.contains("Parse error"), "stderr was: {err}");
}

/// #43. A bare `"-"` in a non-atomic pest rule produces no pair, so the
/// negation was dropped and `-2` evaluated to 2. Checked against the reference,
/// which prints -2 and 0 for these.
#[test]
fn unary_minus_negates() {
    for (written, expected) in [
        ("-2", "-2"),
        ("-(2)", "-2"),
        ("-2 == 2", "0"),
        ("0-2 == -2", "1"),
        ("5 - -2", "7"),
        ("-2.5 == 0 - 2.5", "1"),
    ] {
        let (out, err, status) = run(
            &["-q", "-p", "1", "-s", "1"],
            &format!("condition 1\nprintrpt({written})\n"),
        );
        assert_eq!(status, 0, "`{written}` should run: {err}");
        assert_eq!(out.trim(), expected, "{written}");
    }
}

/// #49. An `action` list replaces the default action, and `printall` is the
/// default — so a list naming only statistics prints no deals. The original
/// settles it with `will_print` in `defs.y`, incremented by every printing
/// action and by nothing else.
#[test]
fn an_action_that_only_measures_prints_no_deals() {
    let boards = |out: &str| out.lines().filter(|l| l.trim_end().ends_with('.')).count();

    let (measuring, err, status) = run(
        &["-p", "3", "-s", "1"],
        "condition 1\naction average \"hcp\" hcp(north)\n",
    );
    assert_eq!(status, 0, "stderr was: {err}");
    assert_eq!(
        boards(&measuring),
        0,
        "no deals were asked for: {measuring:?}"
    );
    assert!(measuring.contains("hcp:"), "the average is still reported");

    // The two cases that already agreed with the original must keep agreeing.
    let (no_action, _, _) = run(&["-p", "3", "-s", "1"], "condition 1\n");
    assert_eq!(boards(&no_action), 3, "no action at all still prints deals");

    let (explicit, _, _) = run(
        &["-p", "3", "-s", "1"],
        "condition 1\naction printall, average \"hcp\" hcp(north)\n",
    );
    assert_eq!(
        boards(&explicit),
        3,
        "an explicit format still prints deals"
    );
}

/// The silent half: a measuring script takes every deal it generates, rather
/// than the first forty. `dealer.c:1656` decides both with one expression.
#[test]
fn an_action_that_only_measures_takes_every_deal_generated() {
    let (out, err, status) = run(
        &["-s", "1", "-v"],
        "generate 1000\ncondition 1\naction average \"hcp\" hcp(north)\n",
    );
    assert_eq!(status, 0, "stderr was: {err}");
    assert!(
        out.contains("Produced 1000 hands"),
        "a measuring run samples everything it generates: {out:?}"
    );

    // And `-p` still wins when it is given.
    let (capped, _, _) = run(
        &["-s", "1", "-v", "-p", "25"],
        "generate 1000\ncondition 1\naction average \"hcp\" hcp(north)\n",
    );
    assert!(capped.contains("Produced 25 hands"), "got: {capped:?}");
}

/// Asking for a format on the command line is asking for deals, whatever the
/// script's action list says.
#[test]
fn an_explicit_format_shows_deals_even_when_only_measuring() {
    let (out, _, status) = run(
        &["-p", "3", "-s", "1", "-f", "printoneline"],
        "condition 1\naction average \"hcp\" hcp(north)\n",
    );
    assert_eq!(status, 0);
    assert_eq!(
        out.lines().filter(|l| l.starts_with('n')).count(),
        3,
        "got: {out:?}"
    );
}

/// `opener west` is DealerV2_4's statement, and neither dealer3 nor dealer.exe
/// has it — the word appears nowhere in the original's `scan.l`. Both refuse
/// it, which is what the roadmap's `opc()` row claims; this keeps the claim
/// honest.
///
/// The nuance is that `opener` must stay usable as an ordinary variable name.
/// It is a natural word for a bridge script and this suite's own fixtures use
/// it that way, so refusing the bare word must not cost the assignment.
#[test]
fn the_dealerv2_opener_statement_is_refused_but_the_name_is_not() {
    let (out, err, status) = run(
        &["-q", "-p", "2", "-s", "1"],
        "opener west\ncondition hcp(north) >= 15\n",
    );
    assert_eq!(status, 1, "stdout was: {out}");
    assert!(
        err.contains("never defined") && err.contains("opener"),
        "the undefined name should be named: {err}"
    );

    // Defined, it is just a variable, and the script runs.
    let (_, err, status) = run(
        &["-q", "-p", "1", "-s", "1"],
        "opener = hcp(north) >= 12\ncondition opener\n",
    );
    assert_eq!(status, 0, "`opener` is still a usable name: {err}");
}
