//! Every demo the web page bundles runs.
//!
//! The Demos tab (#104) reads each `web/src/demos/*.json` as a document and
//! offers its script to be run. The page's own tests check that each document
//! reads; they cannot check that the script parses, because the engine is wasm
//! there. This does, through the binary, on a handful of shuffled deals.
//!
//! NT Ladder is the one demo not in that directory: it is
//! `examples/NT_Ladder.stock.dlr`, which `write_leveled.rs` already runs.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn demos_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/src/demos")
}

#[test]
fn every_web_demo_parses_and_runs() {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(demos_dir())
        .expect("web/src/demos should exist")
        .map(|entry| entry.expect("readable entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    // An empty directory would pass every assertion below by never making one.
    assert!(
        !paths.is_empty(),
        "no demos found in {}",
        demos_dir().display()
    );

    for path in paths {
        let text = std::fs::read_to_string(&path).expect("readable demo");
        let doc: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()));
        assert_eq!(
            doc["v"],
            1,
            "{} is not a version 1 document",
            path.display()
        );
        let script = doc["script"]
            .as_str()
            .unwrap_or_else(|| panic!("{} has no script", path.display()));
        assert!(
            script.contains("title \""),
            "{} has no title statement, and the Demos tab titles a demo from it",
            path.display()
        );

        let script_path = std::env::temp_dir().join(format!(
            "dealer3-demo-{}-{}.dlr",
            std::process::id(),
            path.file_stem().unwrap_or_default().to_string_lossy()
        ));
        std::fs::write(&script_path, script).expect("temp script");
        let out = Command::new(env!("CARGO_BIN_EXE_dealer"))
            .args(["-p", "3", "-g", "5000", "-s", "1", "-f", "none"])
            .arg(&script_path)
            .stdin(Stdio::null())
            .output()
            .expect("dealer should run");
        let _ = std::fs::remove_file(&script_path);

        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "{} failed to run:\n{stderr}",
            path.display()
        );
        assert!(
            !String::from_utf8_lossy(&out.stdout).trim().is_empty(),
            "{} ran but reported nothing",
            path.display()
        );
    }
}
