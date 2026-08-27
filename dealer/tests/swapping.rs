//! Integration tests for the original's `-0`, `-2` and `-3` swapping switches.
//!
//! Swapping is the one feature where the *order and count* of deals is the
//! whole point, and where the original silently does the wrong thing when a
//! predeal is in play. So these drive the real binary and check the shape of
//! the output — which hands repeat, how many deals a shuffle yields, and that
//! the refusals land where they should.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_dealer")
}

fn temp_script(tag: &str, contents: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dealer3-swap-{}-{}-{:?}.dlr",
        tag,
        std::process::id(),
        std::thread::current().id()
    ));
    let mut file = std::fs::File::create(&path).expect("temp script should be creatable");
    file.write_all(contents.as_bytes())
        .expect("temp script should be writable");
    path
}

const PRINT_EVERY_DEAL: &str = "condition 1\naction printoneline\n";

struct Run {
    stdout: String,
    stderr: String,
    status: i32,
}

fn run(args: &[&str], script: &str) -> Run {
    // The tag only has to be legible in a temp filename, so keep it to
    // characters a path is happy with.
    let tag: String = args
        .join("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let path = temp_script(&tag, script);
    let output = Command::new(bin())
        .args(args)
        .arg(&path)
        .stdin(Stdio::null())
        .output()
        .expect("dealer should run");
    let _ = std::fs::remove_file(&path);
    Run {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        status: output.status.code().unwrap_or(-1),
    }
}

/// The deals a run printed, split into per-seat hands.
fn hands(stdout: &str) -> Vec<[String; 4]> {
    stdout
        .lines()
        .filter(|line| line.starts_with("n "))
        .map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // "n <hand> e <hand> s <hand> w <hand>"
            assert_eq!(fields.len(), 8, "unexpected oneline output: {line}");
            [
                fields[1].to_string(),
                fields[3].to_string(),
                fields[5].to_string(),
                fields[7].to_string(),
            ]
        })
        .collect()
}

#[test]
fn no_swapping_is_the_default_and_dash_zero_asks_for_it() {
    let plain = run(&["-p", "20", "-s", "7"], PRINT_EVERY_DEAL);
    let explicit = run(&["-0", "-p", "20", "-s", "7"], PRINT_EVERY_DEAL);
    assert_eq!(explicit.status, 0, "{}", explicit.stderr);
    assert_eq!(plain.stdout, explicit.stdout, "`-0` should change nothing");
}

#[test]
fn two_way_exchanges_east_and_west_and_leaves_the_rest() {
    let result = run(&["-2", "-p", "6", "-s", "7"], PRINT_EVERY_DEAL);
    assert_eq!(result.status, 0, "{}", result.stderr);
    let deals = hands(&result.stdout);
    assert_eq!(deals.len(), 6);
    for pair in deals.chunks(2) {
        let (dealt, swapped) = (&pair[0], &pair[1]);
        assert_eq!(dealt[0], swapped[0], "North should not move");
        assert_eq!(dealt[2], swapped[2], "South should not move");
        assert_eq!(dealt[1], swapped[3], "East's cards should go to West");
        assert_eq!(dealt[3], swapped[1], "West's cards should go to East");
    }
    // Consecutive shuffles must be different deals, or the swap is all there is.
    assert_ne!(deals[0], deals[2]);
}

#[test]
fn three_way_gives_six_arrangements_of_one_shuffle() {
    let result = run(&["-3", "-p", "12", "-s", "7"], PRINT_EVERY_DEAL);
    assert_eq!(result.status, 0, "{}", result.stderr);
    let deals = hands(&result.stdout);
    assert_eq!(deals.len(), 12);
    for shuffle in deals.chunks(6) {
        let north = &shuffle[0][0];
        let mut arrangements = Vec::new();
        for deal in shuffle {
            assert_eq!(&deal[0], north, "North should be the same all six times");
            let mut others = [&deal[1], &deal[2], &deal[3]];
            arrangements.push(deal[1..].to_vec());
            others.sort();
            let mut expected = [&shuffle[0][1], &shuffle[0][2], &shuffle[0][3]];
            expected.sort();
            assert_eq!(others, expected, "the same three hands every time");
        }
        arrangements.sort();
        arrangements.dedup();
        assert_eq!(arrangements.len(), 6, "all six arrangements, none repeated");
    }
    assert_ne!(
        deals[0][0], deals[6][0],
        "the seventh deal is a new shuffle"
    );
}

/// The underlying shuffle sequence must be untouched by swapping, so a script
/// can be re-run with and without the switch and still recognise its deals.
#[test]
fn every_nth_deal_is_the_unswapped_sequence() {
    let plain = run(&["-p", "4", "-s", "7"], PRINT_EVERY_DEAL);
    for (switch, width) in [("-2", 2), ("-3", 6)] {
        let swapped = run(
            &[switch, "-p", &(4 * width).to_string(), "-s", "7"],
            PRINT_EVERY_DEAL,
        );
        let firsts: Vec<[String; 4]> = hands(&swapped.stdout).into_iter().step_by(width).collect();
        assert_eq!(firsts, hands(&plain.stdout), "{switch} moved the shuffles");
    }
}

/// `-g` counts every deal a swap produces, and has to stop on the right one
/// even though deals arrive six at a time and batches do not divide by six.
#[test]
fn generate_limit_is_exact_whatever_the_batch_size() {
    for batch in ["1", "5", "6", "7", "1000"] {
        for limit in ["1", "2", "6", "7", "13", "100"] {
            let result = run(
                &[
                    "-3",
                    "-g",
                    limit,
                    "-p",
                    "1000000",
                    "-s",
                    "7",
                    "--batch-size",
                    batch,
                ],
                PRINT_EVERY_DEAL,
            );
            assert_eq!(result.status, 0, "{}", result.stderr);
            assert_eq!(
                hands(&result.stdout).len(),
                limit.parse::<usize>().unwrap(),
                "-g {limit} with --batch-size {batch}"
            );
        }
    }
}

#[test]
fn the_output_does_not_depend_on_batch_size_or_thread_count() {
    let reference = run(&["-3", "-p", "30", "-s", "7"], PRINT_EVERY_DEAL).stdout;
    for extra in [
        vec!["--batch-size", "1"],
        vec!["--batch-size", "5"],
        vec!["--batch-size", "6"],
        vec!["-R", "1"],
        vec!["-R", "4"],
    ] {
        let mut args = vec!["-3", "-p", "30", "-s", "7"];
        args.extend(extra.iter().copied());
        assert_eq!(
            run(&args, PRINT_EVERY_DEAL).stdout,
            reference,
            "{extra:?} changed the output"
        );
    }
}

#[test]
fn the_last_switch_written_wins() {
    // As under getopt, where all three land in one case.
    let three_then_two = run(&["-3", "-2", "-p", "4", "-s", "7"], PRINT_EVERY_DEAL);
    assert_eq!(
        three_then_two.stdout,
        run(&["-2", "-p", "4", "-s", "7"], PRINT_EVERY_DEAL).stdout
    );

    let two_then_three = run(&["-2", "-3", "-p", "12", "-s", "7"], PRINT_EVERY_DEAL);
    assert_eq!(
        two_then_three.stdout,
        run(&["-3", "-p", "12", "-s", "7"], PRINT_EVERY_DEAL).stdout
    );

    let three_then_off = run(&["-3", "-0", "-p", "4", "-s", "7"], PRINT_EVERY_DEAL);
    assert_eq!(
        three_then_off.stdout,
        run(&["-p", "4", "-s", "7"], PRINT_EVERY_DEAL).stdout
    );
}

/// A predeal to a seat the swap moves is refused, because the original honours
/// it on the first deal of each shuffle and silently loses it on the rest.
#[test]
fn a_predeal_to_a_moved_seat_is_refused() {
    for (switch, seat) in [
        ("-2", "east"),
        ("-2", "west"),
        ("-3", "east"),
        ("-3", "south"),
        ("-3", "west"),
    ] {
        let script = format!("predeal {seat} SAKQ\ncondition 1\n");
        let result = run(&[switch, "-p", "1", "-s", "7"], &script);
        assert_eq!(result.status, 1, "{switch} with predeal {seat} should fail");
        assert!(
            result.stderr.contains("swapping moves the cards of"),
            "unhelpful message for {switch} + predeal {seat}: {}",
            result.stderr
        );
    }
    // The compass switches are predeal too, and must be caught the same way.
    let result = run(&["-2", "-E", "SAKQ", "-p", "1", "-s", "7"], "condition 1\n");
    assert_eq!(result.status, 1);
    assert!(
        result.stderr.contains("predeal to East"),
        "{}",
        result.stderr
    );
}

/// The combination worth having: a fixed North against every arrangement of
/// the other three. The original cannot do this — it loses the predeal.
#[test]
fn a_predeal_to_a_seat_the_swap_leaves_alone_still_works() {
    let script = "predeal north SAKQJT,HAK\ncondition 1\naction printoneline\n";
    let result = run(&["-3", "-p", "12", "-s", "7"], script);
    assert_eq!(result.status, 0, "{}", result.stderr);
    let deals = hands(&result.stdout);
    assert_eq!(deals.len(), 12);
    for deal in &deals {
        assert!(
            deal[0].starts_with("AKQJT") && deal[0].contains(".AK"),
            "North lost its predealt cards: {}",
            deal[0]
        );
    }
    // South is predealable under `-2`, which does not move it.
    let script = "predeal south SAKQ\ncondition 1\naction printoneline\n";
    let result = run(&["-2", "-p", "6", "-s", "7"], script);
    assert_eq!(result.status, 0, "{}", result.stderr);
    for deal in hands(&result.stdout) {
        assert!(
            deal[2].starts_with("AKQ"),
            "South lost its cards: {}",
            deal[2]
        );
    }
}

#[test]
fn swapping_is_refused_with_input_deals() {
    let result = run(
        &["-2", "--input-deals", "/dev/null", "-p", "1"],
        "condition 1\n",
    );
    assert_eq!(result.status, 1);
    assert!(
        result.stderr.contains("--input-deals"),
        "unhelpful message: {}",
        result.stderr
    );
}
