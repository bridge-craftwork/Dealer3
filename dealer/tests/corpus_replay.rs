//! Tier 1 regression tests: replay committed dealer.exe corpora through dealer3.
//!
//! Each corpus under `test-data/corpus/<name>/` was generated once by
//! `scripts/generate-corpus.sh` against the real dealer.exe and committed. These
//! tests never invoke dealer.exe — they only replay the saved artifacts, so they
//! run anywhere, including CI.
//!
//! Two corpus shapes exist:
//!
//! - `full` — `unfiltered.txt` holds the complete deal sequence dealer.exe saw.
//!   Filtering it with dealer3 must reproduce `expected.txt` exactly. This is
//!   two-sided: it catches dealer3 being either too strict or too lenient.
//!
//! - `one-sided` — only `expected.txt` exists, because the filter is too
//!   selective for a practical generate count. Feeding it back through the
//!   filter must return every deal. This catches dealer3 being too strict, but
//!   NOT too lenient, since no rejected deals are present in the input.
//!
//! See `docs/REGRESSION_TESTING.md`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

/// Minimal field extractor for the generated manifest. The file is written by
/// `generate-corpus.sh` with a fixed shape, so a full JSON parser (and the
/// dependency it would need) is not warranted.
fn manifest_field(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\":", key);
    let rest = json.find(&needle).map(|i| &json[i + needle.len()..])?;
    let rest = rest.trim_start();
    if let Some(stripped) = rest.strip_prefix('"') {
        stripped.find('"').map(|end| stripped[..end].to_string())
    } else {
        let end = rest.find([',', '\n', '}']).unwrap_or(rest.len());
        Some(rest[..end].trim().to_string())
    }
}

fn manifest_usize(json: &str, key: &str, corpus: &str) -> usize {
    manifest_field(json, key)
        .unwrap_or_else(|| panic!("[{}] manifest missing '{}'", corpus, key))
        .parse()
        .unwrap_or_else(|e| panic!("[{}] manifest '{}' is not a number: {}", corpus, key, e))
}

/// Deal lines start with "n " in oneline format; trailing whitespace is stripped
/// so comparisons are not sensitive to it.
fn deal_lines(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| l.starts_with("n "))
        .map(|l| l.trim_end().to_string())
        .collect()
}

struct Failure {
    corpus: String,
    detail: String,
}

fn replay(dir: &Path, failures: &mut Vec<Failure>) {
    let corpus = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("<unnamed>")
        .to_string();

    let mut fail = |detail: String| {
        failures.push(Failure {
            corpus: corpus.clone(),
            detail,
        })
    };

    let manifest = match std::fs::read_to_string(dir.join("manifest.json")) {
        Ok(m) => m,
        Err(e) => return fail(format!("cannot read manifest.json: {}", e)),
    };

    let mode = manifest_field(&manifest, "mode").unwrap_or_default();
    let input_file = manifest_field(&manifest, "input_file").unwrap_or_default();
    let input_deals = manifest_usize(&manifest, "input_deals", &corpus);
    let expected_deals = manifest_usize(&manifest, "expected_deals", &corpus);

    let script = dir.join("script.dlr");
    let input = dir.join(&input_file);
    let expected_path = dir.join("expected.txt");

    for p in [&script, &input, &expected_path] {
        if !p.exists() {
            return fail(format!("missing corpus file: {}", p.display()));
        }
    }

    let expected = match std::fs::read_to_string(&expected_path) {
        Ok(t) => deal_lines(&t),
        Err(e) => return fail(format!("cannot read expected.txt: {}", e)),
    };
    if expected.len() != expected_deals {
        return fail(format!(
            "expected.txt holds {} deals but manifest says {}",
            expected.len(),
            expected_deals
        ));
    }

    // -p and -g are set to the full input size deliberately. Capping them at the
    // expected count would mask dealer3 matching MORE deals than dealer.exe did.
    let out = Command::new(env!("CARGO_BIN_EXE_dealer"))
        .arg(&script)
        .arg("--input-deals")
        .arg(&input)
        .args(["-f", "oneline"])
        .args(["-p", &input_deals.to_string()])
        .args(["-g", &input_deals.to_string()])
        .arg("-X")
        .output()
        .expect("failed to run dealer");

    if !out.status.success() {
        return fail(format!(
            "dealer exited with {}\nstderr:\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);

    // Guard against a silently short read. bridge-encodings' DealReader skips
    // lines it cannot parse rather than erroring, so a truncated or corrupted
    // corpus would otherwise read short and the comparison below could pass for
    // the wrong reason. See bridge-craftwork/bridge-encodings#4.
    let reported = stdout
        .lines()
        .find_map(|l| l.strip_prefix("Generated "))
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.parse::<usize>().ok());
    match reported {
        Some(n) if n == input_deals => {}
        Some(n) => {
            return fail(format!(
                "read {} deals from {} but manifest says {} — corpus may be truncated",
                n, input_file, input_deals
            ))
        }
        None => return fail("could not find 'Generated N hands' in output".to_string()),
    }

    let actual = deal_lines(&stdout);

    if actual == expected {
        return;
    }

    // Report the difference in the terms that matter: which deals dealer3
    // dropped (too strict) and which it added (too lenient).
    let dropped: Vec<_> = expected.iter().filter(|d| !actual.contains(d)).collect();
    let added: Vec<_> = actual.iter().filter(|d| !expected.contains(d)).collect();

    let mut detail = format!(
        "filter mismatch ({} mode): dealer.exe produced {} deals, dealer3 produced {}",
        mode,
        expected.len(),
        actual.len()
    );
    if !dropped.is_empty() {
        detail.push_str(&format!(
            "\n  dealer3 was TOO STRICT — dropped {} deal(s) dealer.exe accepted:",
            dropped.len()
        ));
        for d in dropped.iter().take(3) {
            detail.push_str(&format!("\n    {}", d));
        }
    }
    if !added.is_empty() {
        detail.push_str(&format!(
            "\n  dealer3 was TOO LENIENT — accepted {} deal(s) dealer.exe rejected:",
            added.len()
        ));
        for d in added.iter().take(3) {
            detail.push_str(&format!("\n    {}", d));
        }
    }
    if dropped.is_empty() && added.is_empty() {
        detail.push_str("\n  same deals, different order");
    }
    fail(detail);
}

#[test]
fn corpora_replay_matches_dealer_exe() {
    let root = corpus_root();
    assert!(
        root.is_dir(),
        "corpus directory not found at {} — see docs/REGRESSION_TESTING.md",
        root.display()
    );

    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("failed to read corpus directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    assert!(
        !dirs.is_empty(),
        "no corpora found under {} — see docs/REGRESSION_TESTING.md",
        root.display()
    );

    let mut failures = Vec::new();
    for dir in &dirs {
        replay(dir, &mut failures);
    }

    if !failures.is_empty() {
        let mut msg = format!(
            "\n{} of {} corpora failed to replay:\n",
            failures.len(),
            dirs.len()
        );
        for f in &failures {
            msg.push_str(&format!("\n[{}] {}\n", f.corpus, f.detail));
        }
        panic!("{}", msg);
    }

    println!("replayed {} corpora successfully", dirs.len());
}
