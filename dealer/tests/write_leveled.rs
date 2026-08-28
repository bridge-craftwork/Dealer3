//! `--write-leveled`: measure a scenario's hand types, then write a copy with
//! the keeps filled in.
//!
//! Everything it refuses is a way the method fails quietly rather than loudly.
//! The worst is feeding a generated file back in: that measures the
//! already-levelled mix, computes keeps of roughly 1, and writes a scenario
//! with no levelling at all — a file that runs, carries a stamp agreeing with
//! itself, and is wrong only about the thing nobody re-checks.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// A scenario with five bands, the rarest about 1.3% of what it produces.
const STOCK: &str = "\
HandType_12_14 = hcp(south) >= 12 and hcp(south) <= 14
HandType_15_17 = hcp(south) >= 15 and hcp(south) <= 17
HandType_18_19 = hcp(south) >= 18 and hcp(south) <= 19
HandType_20_21 = hcp(south) >= 20 and hcp(south) <= 21
HandType_22_24 = hcp(south) >= 22 and hcp(south) <= 24

### BEGIN GENERATED LEVELING ###
noLeveling = 1
levelTheDeal = noLeveling
### END GENERATED LEVELING ###

condition shape(south, any 4333 + any 4432 + any 5332)
      and hcp(south) >= 12 and hcp(south) <= 24
      and levelTheDeal
action average \"a\" 100 * HandType_12_14, average \"e\" 100 * HandType_22_24
";

fn temp(tag: &str, contents: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dealer3-level-{tag}-{}-{:?}.dlr",
        std::process::id(),
        std::thread::current().id()
    ));
    let mut file = std::fs::File::create(&path).expect("temp file");
    file.write_all(contents.as_bytes()).expect("temp file");
    path
}

struct Run {
    stderr: String,
    status: i32,
    written: Option<String>,
}

fn level(script: &str, extra: &[&str], tag: &str) -> Run {
    let source = temp(tag, script);
    let mut out_path = source.clone();
    out_path.set_extension("out.dlr");
    let mut args: Vec<String> = vec![
        source.display().to_string(),
        "-q".into(),
        "-p".into(),
        "100000".into(),
        "-s".into(),
        "1".into(),
        "--write-leveled".into(),
        out_path.display().to_string(),
    ];
    args.extend(extra.iter().map(|a| (*a).to_string()));
    let output = Command::new(env!("CARGO_BIN_EXE_dealer"))
        .args(&args)
        .stdin(Stdio::null())
        .output()
        .expect("dealer should run");
    let written = std::fs::read_to_string(&out_path).ok();
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&out_path);
    Run {
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        status: output.status.code().unwrap_or(-1),
        written,
    }
}

/// Run a scenario and report each band's share, to check the keeps deliver.
fn mix(script: &str, tag: &str) -> Vec<f64> {
    let path = temp(tag, script);
    let out = Command::new(env!("CARGO_BIN_EXE_dealer"))
        .args([
            &path.display().to_string(),
            "-q",
            "--stats-json",
            "-p",
            "20000",
            "-s",
            "2",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("dealer should run");
    let _ = std::fs::remove_file(&path);
    let text = String::from_utf8_lossy(&out.stdout);
    text.split("\"value\": ")
        .skip(1)
        .map(|piece| {
            piece
                .split(',')
                .next()
                .expect("a value")
                .trim()
                .parse()
                .expect("a number")
        })
        .collect()
}

#[test]
fn the_generated_scenario_delivers_the_target_mix() {
    let run = level(STOCK, &[], "deliver");
    assert_eq!(run.status, 0, "{}", run.stderr);
    let generated = run.written.expect("a file");
    assert!(generated.contains("roll = (rnd(1000) % 1000 + 1000) % 1000"));
    assert!(generated.contains("levelTheDeal = level_12_14 or"));
    // The rarest band is kept always; the commonest hardly ever.
    assert!(generated.contains("level_22_24 = HandType_22_24\n"));
    assert!(generated.contains("level_12_14 = HandType_12_14 and roll <"));

    let shares = mix(&generated, "delivered");
    assert_eq!(shares.len(), 2);
    for share in shares {
        assert!(
            (18.0..22.0).contains(&share),
            "a band came out at {share}%, wanted about 20"
        );
    }
}

/// Seeded, so a build can regenerate and diff rather than trusting a timestamp.
#[test]
fn the_same_seed_writes_the_same_file() {
    let first = level(STOCK, &[], "repeat1").written.expect("a file");
    let second = level(STOCK, &[], "repeat2").written.expect("a file");
    assert_eq!(first, second);
}

/// A budget too tight relaxes exactness rather than sacrificing the rarest
/// type, which is usually the one the exercise exists to promote.
#[test]
fn a_tight_budget_relaxes_exactness_instead_of_dropping_the_rare_type() {
    let run = level(STOCK, &["--level-budget", "40"], "budget");
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(
        run.stderr.contains("relaxed to fit the budget"),
        "{}",
        run.stderr
    );
    let generated = run.written.expect("a file");
    // Still keeps every one of the rarest, and still discards most of the
    // commonest — just less of the way toward even.
    assert!(generated.contains("level_22_24 = HandType_22_24\n"));
    let shares = mix(&generated, "budgeted");
    assert!(
        shares[0] > 35.0,
        "the commonest band should stay common at a tight budget, got {}",
        shares[0]
    );
}

#[test]
fn an_uneven_target_is_honoured() {
    let run = level(STOCK, &["--level-target", "40,30,15,10,5"], "uneven");
    assert_eq!(run.status, 0, "{}", run.stderr);
    let shares = mix(&run.written.expect("a file"), "unevenmix");
    assert!((37.0..43.0).contains(&shares[0]), "got {}", shares[0]);
    assert!((3.5..6.5).contains(&shares[1]), "got {}", shares[1]);
}

/// The `{{level-mix}}` markers, so the text a student reads cannot drift from
/// the keeps — which is exactly what had happened to the scenario this was
/// built from.
#[test]
fn the_player_facing_text_is_filled_from_the_same_numbers() {
    let script = STOCK.replace(
        "### BEGIN GENERATED LEVELING ###",
        "# 12-14 is {{level-mix:12_14}} and 22-24 is {{level-mix:22_24}}\n\
         ### BEGIN GENERATED LEVELING ###",
    );
    let run = level(&script, &[], "markers");
    assert_eq!(run.status, 0, "{}", run.stderr);
    let generated = run.written.expect("a file");
    assert!(
        generated.contains("# 12-14 is 20% and 22-24 is 20%"),
        "markers not filled:\n{generated}"
    );

    let typo = script.replace("{{level-mix:22_24}}", "{{level-mix:22_25}}");
    let run = level(&typo, &[], "markertypo");
    assert_eq!(run.status, 1);
    assert!(run.stderr.contains("does not declare"), "{}", run.stderr);
}

/// The worst failure this can have, and the only one that leaves a file that
/// looks right.
#[test]
fn a_generated_file_is_refused_as_input() {
    let generated = level(STOCK, &[], "again").written.expect("a file");
    let run = level(&generated, &[], "again2");
    assert_eq!(run.status, 1);
    assert!(run.stderr.contains("not written by hand"), "{}", run.stderr);
    assert!(run.written.is_none(), "nothing should have been written");
}

#[test]
fn a_scenario_that_cannot_take_the_block_is_refused_before_dealing() {
    let no_marker = STOCK
        .replace("### BEGIN GENERATED LEVELING ###\n", "")
        .replace("### END GENERATED LEVELING ###\n", "");
    let run = level(&no_marker, &[], "nomarker");
    assert_eq!(run.status, 1);
    assert!(run.stderr.contains("needs a placeholder"), "{}", run.stderr);

    let unused = STOCK.replace("      and levelTheDeal\n", "");
    let run = level(&unused, &[], "unused");
    assert_eq!(run.status, 1);
    assert!(run.stderr.contains("would\nhave no effect") || run.stderr.contains("no effect"));
}

/// A scenario may share `roll` through an include. It is used as it stands if
/// it is the safe form, and refused otherwise, because the keeps are read
/// against a draw assumed uniform over 0..N-1.
#[test]
fn a_roll_the_scenario_already_defines_is_reused_or_refused() {
    let shared = STOCK.replacen(
        "HandType_12_14",
        "roll = (rnd(100) % 100 + 100) % 100\nHandType_12_14",
        1,
    );
    let run = level(&shared, &[], "sharedroll");
    assert_eq!(run.status, 0, "{}", run.stderr);
    let generated = run.written.expect("a file");
    assert_eq!(
        generated.matches("roll = (").count(),
        1,
        "should not write a second roll"
    );
    assert!(generated.contains("comes from the scenario, drawing over 0..99"));
    // Thresholds read against the draw that actually happens.
    assert!(generated.contains("and roll < 2\n"), "{generated}");

    let unsafe_roll = STOCK.replacen("HandType_12_14", "roll = rnd(1000)\nHandType_12_14", 1);
    let run = level(&unsafe_roll, &[], "badroll");
    assert_eq!(run.status, 1);
    assert!(run.stderr.contains("not in the form"), "{}", run.stderr);
}

/// A rate that is divided by has to be worth dividing by.
#[test]
fn types_measured_on_too_few_deals_are_refused() {
    let source = temp("thin", STOCK);
    let mut out_path = source.clone();
    out_path.set_extension("out.dlr");
    let output = Command::new(env!("CARGO_BIN_EXE_dealer"))
        .args([
            &source.display().to_string(),
            "-q",
            "-p",
            "2000",
            "-s",
            "1",
            "--write-leveled",
            &out_path.display().to_string(),
        ])
        .stdin(Stdio::null())
        .output()
        .expect("dealer should run");
    let _ = std::fs::remove_file(&source);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("too few deals to divide by"), "{stderr}");
    assert!(!out_path.exists(), "nothing should have been written");
}

/// The types have to partition what the scenario produces, or the keeps do not
/// add up to the mix they claim.
#[test]
fn a_deal_matching_no_type_is_refused() {
    let gappy = STOCK.replace(
        "HandType_12_14 = hcp(south) >= 12 and hcp(south) <= 14\n",
        "HandType_12_14 = hcp(south) >= 13 and hcp(south) <= 14\n",
    );
    let run = level(&gappy, &[], "gap");
    assert_eq!(run.status, 1);
    assert!(
        run.stderr.contains("matched no hand type"),
        "{}",
        run.stderr
    );
}

#[test]
fn it_needs_the_scenario_as_a_file() {
    let output = Command::new(env!("CARGO_BIN_EXE_dealer"))
        .args(["-p", "10", "--write-leveled", "/tmp/never-written.dlr"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .as_mut()
                .expect("stdin")
                .write_all(b"condition 1\n")?;
            child.wait_with_output()
        })
        .expect("dealer should run");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("file argument"));
}

#[test]
fn a_type_targeted_at_nothing_is_written_as_nothing() {
    // A zero weight asks for none of the type. Rounding its keep up to one in
    // a thousand — the least a threshold can express — would leave the file
    // delivering a share the header above it calls zero, which is the one
    // thing this whole arrangement exists to prevent.
    let run = level(STOCK, &["--level-target", "1,1,1,1,0"], "zero-weight");
    assert_eq!(run.status, 0, "stderr was: {}", run.stderr);
    let written = run.written.expect("a generated file");
    assert!(
        written.contains("level_22_24 = 0"),
        "the excluded band should be written as nothing, got:\n{written}"
    );
    assert!(
        !written.contains("level_22_24 = HandType_22_24 and roll < 1"),
        "the excluded band must not be kept one time in a thousand"
    );
    // And the header agrees.
    assert!(written.contains("# 22_24"));

    let shares = mix(&written, "zero-weight-mix");
    assert_eq!(shares.len(), 2, "the fixture reports two bands");
    assert_eq!(
        shares[1], 0.0,
        "the excluded band should never appear, got {shares:?}"
    );
}

#[test]
fn a_keep_too_small_for_the_roll_is_refused() {
    // Under half of one in the roll's range there is no threshold to write.
    // Rounding either way makes the file disagree with its own header, so it
    // is an error and says what to do about it.
    let script = STOCK.replace(
        "### BEGIN GENERATED LEVELING ###",
        "roll = (rnd(10) % 10 + 10) % 10\n\n### BEGIN GENERATED LEVELING ###",
    );
    let run = level(&script, &[], "coarse-roll");
    assert_eq!(run.status, 1, "stderr was: {}", run.stderr);
    assert!(
        run.stderr.contains("less than one deal in 10"),
        "stderr was: {}",
        run.stderr
    );
    assert!(
        run.stderr.contains("finer roll"),
        "the message should say what to do: {}",
        run.stderr
    );
    assert!(run.written.is_none(), "nothing should have been written");
}
