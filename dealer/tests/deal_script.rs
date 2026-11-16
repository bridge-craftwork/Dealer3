//! `dealer_run::deal_script` deals what the command line deals.
//!
//! The entry point exists so that a caller wanting only the deals does not
//! copy the command line's reading of a script. That is only worth having if
//! the two agree, so each test here asks both and compares.

use std::io::Write;
use std::process::{Command, Stdio};

use dealer_run::{deal_script, ScriptOptions};

/// The command line's deals for `script`, one line each.
fn cli(script: &str, args: &[&str]) -> Vec<String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dealer"))
        .args(args)
        .args(["-f", "oneline"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("dealer should run");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(script.as_bytes())
        .expect("write the script");
    let out = child.wait_with_output().expect("output");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.starts_with("n "))
        .map(str::to_string)
        .collect()
}

fn library(script: &str, opts: ScriptOptions) -> Vec<String> {
    deal_script(script, opts)
        .expect("the script deals")
        .deals
        .iter()
        .map(|(deal, _)| dealer_pbn::format_oneline(deal).trim_end().to_string())
        .collect()
}

#[test]
fn the_same_deals_as_the_command_line() {
    let script = "condition hcp(north) >= 15 && spades(south) >= 4\nproduce 12\n";
    let from_cli = cli(script, &["-s", "42"]);
    assert_eq!(from_cli.len(), 12);
    assert_eq!(
        library(
            script,
            ScriptOptions {
                seed: 42,
                threads: 1,
                ..Default::default()
            }
        ),
        from_cli
    );
}

#[test]
fn with_a_predeal_and_the_caller_s_own_count() {
    let script = "predeal north SAK, HAK\ncondition hcp(south) >= 10\nproduce 20\n";
    let from_cli = cli(script, &["-s", "7", "-p", "6"]);
    assert_eq!(from_cli.len(), 6);
    assert_eq!(
        library(
            script,
            ScriptOptions {
                seed: 7,
                produce: Some(6),
                threads: 1,
                ..Default::default()
            }
        ),
        from_cli
    );
}
