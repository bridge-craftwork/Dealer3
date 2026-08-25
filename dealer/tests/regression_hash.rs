//! Tier 2 regression tests: pin dealer3's own output across changes.
//!
//! Each case runs the real binary at a fixed seed and hashes the deals it
//! produces, comparing against a committed hash in `regression_hashes.txt`.
//!
//! # What this protects
//!
//! Generation and filtering together. A failure means dealer3's behaviour
//! changed: the xoshiro256++ sequence, the shuffle, predeal placement, or
//! constraint evaluation.
//!
//! # What this does NOT protect
//!
//! Agreement with dealer.exe. These hashes pin *our* sequence, not the
//! reference implementation's — see `corpus_replay.rs` for that. A change to
//! the RNG or its seeding is a deliberate breaking change and will fail here by
//! design; it does not mean the new behaviour is wrong.
//!
//! # Updating hashes
//!
//! ```sh
//! UPDATE_REGRESSION_HASHES=1 cargo test -p dealer --test regression_hash
//! ```
//!
//! Only do this when the change in output is understood and intended. Review
//! the resulting diff — every changed line is a behaviour change.
//!
//! # Determinism
//!
//! Output is independent of thread count: `-R 1`, `-R 2` and `-R 8` all produce
//! identical results, as do repeated runs. `deal_order` below pins that.
//!
//! # Serialisation
//!
//! Cases run with `-f oneline`, which emits one deal per line in a canonical
//! form: positions always in N/E/S/W order, suits in S.H.D.C order, and ranks
//! descending within each suit. Deal order is generation order. Trailing
//! whitespace is stripped before hashing so it cannot silently affect the hash.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A pinned case: name, script body, seed, and how many deals to produce.
///
/// Scripts are inline rather than fixture files so a case and the behaviour it
/// pins stay visible together.
const CASES: &[(&str, &str, u32, usize)] = &[
    // --- Pure generation: no meaningful filter, so these pin the RNG, the
    // shuffle and the deal-to-hand distribution on their own. -------------------
    ("generation_seed1", "hcp(north) >= 0\n", 1, 50),
    ("generation_seed42", "hcp(north) >= 0\n", 42, 50),
    ("generation_seed2024", "hcp(north) >= 0\n", 2024, 50),
    // --- Generation + filtering ------------------------------------------------
    ("hcp_basic", "condition hcp(north) >= 15\n", 1, 25),
    (
        "suit_lengths",
        "condition spades(north) >= 5 && hearts(north) <= 3 && clubs(south) >= 4\n",
        42,
        25,
    ),
    (
        "shape_patterns",
        "condition shape(north, any 4333 + any 4432 + any 5332)\n",
        7,
        25,
    ),
    (
        "arithmetic",
        "condition hcp(north) + hcp(south) >= 25 && hcp(north) - hcp(south) <= 6\n",
        123,
        25,
    ),
    (
        "variables",
        "opener = hcp(north) >= 15 && hcp(north) <= 17\n\
         balanced = shape(north, any 4333 + any 4432 + any 5332)\n\
         condition opener && balanced\n",
        99,
        25,
    ),
    (
        "controls_hascard",
        "condition controls(north) >= 5 && hascard(north, AS)\n",
        2024,
        20,
    ),
    (
        "ternary",
        "condition (hcp(north) >= 12 ? spades(north) >= 5 : spades(north) >= 6)\n",
        555,
        20,
    ),
    (
        "negation_or",
        "condition not (hcp(north) < 10) and (spades(north) >= 5 or hearts(north) >= 5)\n",
        31,
        20,
    ),
    // Predeal is not reachable from Tier 1 at all: `--input-deals` rejects it,
    // since predeal only applies to generation. This is its only coverage.
    (
        "predeal",
        "predeal north SAKQ,HAK\ncondition hcp(south) >= 8\n",
        5,
        20,
    ),
];

fn hashes_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/regression_hashes.txt")
}

/// FNV-1a, 64-bit.
///
/// Deliberately hand-rolled: `DefaultHasher` is explicitly not guaranteed
/// stable across Rust releases, so committed hashes would rot on a toolchain
/// bump. FNV-1a is fixed by its specification and needs no dependency.
fn fnv1a(data: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

struct CaseResult {
    deals: usize,
    hash: u64,
}

fn run_case(script: &str, seed: u32, produce: usize, threads: Option<u32>) -> CaseResult {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dealer3-regression-{}-{:?}.dlr",
        std::process::id(),
        std::thread::current().id()
    ));
    let mut file = std::fs::File::create(&path).expect("failed to write script");
    file.write_all(script.as_bytes()).expect("failed to write");
    drop(file);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dealer"));
    cmd.arg(&path)
        .args(["-f", "oneline"])
        .args(["-s", &seed.to_string()])
        .args(["-p", &produce.to_string()]);
    if let Some(t) = threads {
        cmd.args(["-R", &t.to_string()]);
    }
    let out = cmd.output().expect("failed to run dealer");

    let _ = std::fs::remove_file(&path);

    assert!(
        out.status.success(),
        "dealer exited with {}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let deals: Vec<&str> = stdout
        .lines()
        .filter(|l| l.starts_with("n "))
        .map(|l| l.trim_end())
        .collect();

    CaseResult {
        deals: deals.len(),
        hash: fnv1a(&deals.join("\n")),
    }
}

/// Parse the committed hash file: `name  deals  hash`, `#` comments ignored.
fn read_hashes() -> BTreeMap<String, (usize, u64)> {
    let text = match std::fs::read_to_string(hashes_path()) {
        Ok(t) => t,
        Err(_) => return BTreeMap::new(),
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_string();
            let deals = parts.next()?.parse().ok()?;
            let hash = u64::from_str_radix(parts.next()?.trim_start_matches("0x"), 16).ok()?;
            Some((name, (deals, hash)))
        })
        .collect()
}

fn write_hashes(results: &BTreeMap<String, (usize, u64)>) {
    let mut out = String::new();
    out.push_str(
        "# Tier 2 regression hashes — dealer3's own output, pinned.\n\
         #\n\
         # Regenerate with:\n\
         #   UPDATE_REGRESSION_HASHES=1 cargo test -p dealer --test regression_hash\n\
         #\n\
         # A changed line here is a behaviour change in generation or filtering.\n\
         # These pin OUR sequence, not dealer.exe's — see corpus_replay.rs for that.\n\
         #\n\
         # name  deals  fnv1a-64\n",
    );
    for (name, (deals, hash)) in results {
        out.push_str(&format!("{:<22} {:<6} 0x{:016x}\n", name, deals, hash));
    }
    std::fs::write(hashes_path(), out).expect("failed to write hashes");
}

#[test]
fn output_matches_committed_hashes() {
    let updating = std::env::var_os("UPDATE_REGRESSION_HASHES").is_some();
    let committed = read_hashes();

    let mut current = BTreeMap::new();
    for (name, script, seed, produce) in CASES {
        let r = run_case(script, *seed, *produce, None);
        current.insert(name.to_string(), (r.deals, r.hash));
    }

    if updating {
        write_hashes(&current);
        eprintln!(
            "Updated {} with {} cases. Review the diff — every change is a \
             behaviour change.",
            hashes_path().display(),
            current.len()
        );
        return;
    }

    assert!(
        !committed.is_empty(),
        "no committed hashes found at {}\nGenerate them with:\n  \
         UPDATE_REGRESSION_HASHES=1 cargo test -p dealer --test regression_hash",
        hashes_path().display()
    );

    let mut problems = Vec::new();

    for (name, (deals, hash)) in &current {
        match committed.get(name) {
            None => problems.push(format!(
                "[{}] new case with no committed hash (deals={}, hash=0x{:016x})",
                name, deals, hash
            )),
            Some((exp_deals, exp_hash)) if exp_hash != hash => problems.push(format!(
                "[{}] output changed\n     expected {} deals, hash 0x{:016x}\n     \
                 got      {} deals, hash 0x{:016x}",
                name, exp_deals, exp_hash, deals, hash
            )),
            Some(_) => {}
        }
    }
    for name in committed.keys() {
        if !current.contains_key(name) {
            problems.push(format!("[{}] committed hash has no matching case", name));
        }
    }

    assert!(
        problems.is_empty(),
        "\n{} of {} regression cases changed:\n\n{}\n\n\
         This means dealer3's generation or filtering behaviour changed. If that \
         was intended (an RNG change, a seeding change, or a deliberate filter \
         fix), regenerate with:\n  \
         UPDATE_REGRESSION_HASHES=1 cargo test -p dealer --test regression_hash\n\
         Otherwise it is a regression — these hashes are not agreement with \
         dealer.exe, so a change here is dealer3 moving on its own.\n",
        problems.len(),
        CASES.len(),
        problems.join("\n\n")
    );
}

/// Output must not depend on how many threads are used, or the hashes above
/// would be machine-dependent and the whole tier would be unreliable.
#[test]
fn output_is_independent_of_thread_count() {
    let (name, script, seed, produce) = CASES[0];
    let single = run_case(script, seed, produce, Some(1));
    let dual = run_case(script, seed, produce, Some(2));
    let many = run_case(script, seed, produce, Some(8));

    assert_eq!(
        single.hash, dual.hash,
        "[{}] output differs between -R 1 and -R 2",
        name
    );
    assert_eq!(
        single.hash, many.hash,
        "[{}] output differs between -R 1 and -R 8",
        name
    );
}

/// Repeated runs at the same seed must be identical.
#[test]
fn output_is_reproducible() {
    let (name, script, seed, produce) = CASES[3];
    let first = run_case(script, seed, produce, None);
    let second = run_case(script, seed, produce, None);
    assert_eq!(first.hash, second.hash, "[{}] output is not stable", name);
}

/// Different seeds must give different deals, or the seed is being ignored and
/// every other test here would still pass.
#[test]
fn different_seeds_differ() {
    let (_, script, _, produce) = CASES[0];
    let a = run_case(script, 1, produce, None);
    let b = run_case(script, 2, produce, None);
    assert_ne!(a.hash, b.hash, "seeds 1 and 2 produced identical output");
}
