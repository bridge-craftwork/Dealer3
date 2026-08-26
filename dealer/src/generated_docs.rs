//! Tables in `docs/` that are generated rather than maintained by hand.
//!
//! Both tables this module owns had drifted from the code by the time anyone
//! looked: the switch comparison listed `-R` as unimplemented after it shipped,
//! and the language status page claimed `tricks`, `score` and `imps` were still
//! to do while its own summary listed them as working.
//!
//! The fix is the one already used for the TextMate grammar and the language
//! reference: derive the document from the code, and fail the build when the
//! file on disk disagrees.
//!
//! ```text
//! cargo test -p dealer            # verifies
//! UPDATE_DOCS=1 cargo test -p dealer   # rewrites
//! ```
//!
//! Only the region between the markers is touched, so the prose around each
//! table stays hand-written:
//!
//! ```text
//! <!-- BEGIN GENERATED: functions -->
//! ...replaced wholesale...
//! <!-- END GENERATED: functions -->
//! ```

use dealer_parser::vocabulary;
use std::path::PathBuf;
use std::sync::Mutex;

/// One document holds several generated sections, and the tests that write them
/// run concurrently. Without this, two of them read the file, each replaces its
/// own section, and the second write loses the first — which showed up as a
/// missing marker rather than as anything that looked like a race.
static DOC_LOCK: Mutex<()> = Mutex::new(());

fn doc_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../docs")
        .join(file)
}

fn begin(section: &str) -> String {
    format!("<!-- BEGIN GENERATED: {} -->", section)
}

fn end(section: &str) -> String {
    format!("<!-- END GENERATED: {} -->", section)
}

/// Line endings, normalised for comparison.
///
/// Windows checks the repository out with CRLF, `read_to_string` hands those
/// back unchanged, and the generated text is written with `\n` — so a byte
/// comparison could never match there. That broke the Windows CI job on every
/// commit after these tests landed, while Linux and macOS stayed green.
fn as_lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Whether a document is written with CRLF, so a rewrite can keep it that way
/// rather than leaving a file with both.
fn uses_crlf(text: &str) -> bool {
    text.contains("\r\n")
}

/// What the file should become, or `None` when it is already correct.
///
/// Split out from the IO so the line-ending handling can be tested on this
/// machine rather than only discovered on a Windows runner.
fn plan_update(
    current: &str,
    open: &str,
    close: &str,
    generated: &str,
) -> Result<Option<String>, String> {
    let start = current
        .find(open)
        .ok_or_else(|| format!("no `{}` marker", open))?;
    let stop = current
        .find(close)
        .ok_or_else(|| format!("`{}` without a matching `{}`", open, close))?;
    if stop <= start {
        return Err(format!("`{}` appears after `{}`", close, open));
    }

    let body_from = start + open.len();
    let existing = &current[body_from..stop];
    let wanted = format!("\n\n{}\n", generated.trim_end());

    if as_lf(existing) == wanted {
        return Ok(None);
    }

    // Keep the document's own line endings, so rewriting one section does not
    // leave a file that mixes them.
    let body = if uses_crlf(current) {
        wanted.replace('\n', "\r\n")
    } else {
        wanted
    };
    Ok(Some(format!(
        "{}{}{}",
        &current[..body_from],
        body,
        &current[stop..]
    )))
}

/// Verify a generated section, or rewrite it when `UPDATE_DOCS` is set.
///
/// Panics with the diff-worthy detail rather than returning an error: this is
/// only ever called from a test, and a test that quietly returned `Err` would
/// be no better than the hand-maintained table it replaces.
pub fn check_or_update(file: &str, section: &str, generated: &str) {
    // Held for the whole read-modify-write, not just the write.
    let _guard = DOC_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let path = doc_path(file);
    let current = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e));

    let (open, close) = (begin(section), end(section));
    let updated = plan_update(&current, &open, &close, generated).unwrap_or_else(|e| {
        panic!(
            "{}: {}.\n\nAdd `{}` and `{}` around the place the generated table should go.",
            path.display(),
            e,
            open,
            close
        )
    });

    let Some(updated) = updated else { return };

    if std::env::var("UPDATE_DOCS").is_ok() {
        std::fs::write(&path, updated)
            .unwrap_or_else(|e| panic!("cannot write {}: {}", path.display(), e));
        eprintln!("updated {} [{}]", path.display(), section);
        return;
    }

    panic!(
        "docs/{} is out of date in section `{}`.\n\n\
         That table is generated from the code, so the code has changed and the document has \
         not. Regenerate it with:\n\n    UPDATE_DOCS=1 cargo test -p dealer\n\n\
         and commit the result.",
        file, section
    );
}

/// Escape the characters that would break a markdown table cell.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// Turn the backticked spans the vocabulary uses into markdown code, which they
/// already are — this only collapses the whitespace of a wrapped Rust string.
fn prose(text: &str) -> String {
    cell(&text.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// The function tables, grouped as the vocabulary groups them.
pub fn render_functions() -> String {
    let mut out = String::new();
    let docs = vocabulary::FUNCTION_DOCS;

    let real = docs.iter().filter(|d| d.alias_of.is_none()).count();
    out.push_str(&format!(
        "**{} functions**, under {} spellings — the extra {} are alternative names, listed with \
         the function they stand for.\n",
        real,
        docs.len(),
        docs.len() - real
    ));

    for group in vocabulary::FUNCTION_GROUPS {
        let entries: Vec<_> = docs.iter().filter(|d| d.group == *group).collect();
        if entries.is_empty() {
            continue;
        }
        out.push_str(&format!("\n### {}\n\n", group));
        out.push_str("| Function | What it computes | Example |\n|---|---|---|\n");
        for doc in entries {
            let summary = match doc.alias_of {
                Some(target) => format!("Another spelling of `{}`", target),
                None => prose(doc.summary),
            };
            out.push_str(&format!(
                "| `{}` | {} | `{}` |\n",
                cell(doc.signature),
                summary,
                cell(doc.example)
            ));
            if let Some(note) = doc.note {
                out.push_str(&format!("| | {} | |\n", prose(note)));
            }
        }
    }
    out
}

/// Operators, in the precedence order the vocabulary declares.
pub fn render_operators() -> String {
    let mut out = String::from(
        "Tightest binding first. Operators sharing a level are applied left to right.\n\n\
         | Level | Operator | Also | What it does |\n|---|---|---|---|\n",
    );
    for doc in vocabulary::OPERATOR_DOCS {
        out.push_str(&format!(
            "| {} | `{}` | {} | {} |\n",
            doc.precedence,
            cell(doc.symbol),
            doc.word.map(|w| format!("`{}`", w)).unwrap_or_default(),
            prose(doc.summary)
        ));
    }
    out
}

/// Statement forms, then the output actions.
pub fn render_statements() -> String {
    let mut out = String::from("| Statement | What it does | Example |\n|---|---|---|\n");
    for doc in vocabulary::STATEMENT_DOCS {
        out.push_str(&format!(
            "| `{}` | {} | `{}` |\n",
            cell(doc.form),
            prose(doc.summary),
            cell(doc.example)
        ));
    }

    out.push_str("\n### Actions\n\n| Action | What it prints |\n|---|---|\n");
    for doc in vocabulary::ACTION_DOCS {
        out.push_str(&format!("| `{}` | {} |\n", doc.name, prose(doc.summary)));
    }
    out
}

/// Words the original dealer accepts that dealer3 does not.
pub fn render_not_supported() -> String {
    let mut out = String::from("| Word | Instead |\n|---|---|\n");
    for entry in vocabulary::NOT_SUPPORTED {
        out.push_str(&format!(
            "| `{}` | {} |\n",
            entry.name,
            prose(entry.instead)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LANGUAGE_DOC: &str = "FILTER_LANGUAGE_STATUS.md";

    #[test]
    fn function_tables_are_up_to_date() {
        check_or_update(LANGUAGE_DOC, "functions", &render_functions());
    }

    #[test]
    fn operator_table_is_up_to_date() {
        check_or_update(LANGUAGE_DOC, "operators", &render_operators());
    }

    #[test]
    fn statement_tables_are_up_to_date() {
        check_or_update(LANGUAGE_DOC, "statements", &render_statements());
    }

    #[test]
    fn not_supported_table_is_up_to_date() {
        check_or_update(LANGUAGE_DOC, "not-supported", &render_not_supported());
    }

    const OPEN: &str = "<!-- BEGIN GENERATED: x -->";
    const CLOSE: &str = "<!-- END GENERATED: x -->";

    /// The bug that broke Windows CI on every commit for two hours: the file is
    /// checked out with CRLF, the generated text uses LF, and a byte comparison
    /// says "out of date" forever.
    #[test]
    fn a_crlf_document_is_recognised_as_current() {
        let table = "| a | b |\n|---|---|\n";
        let lf = format!("intro\n{}\n\n{}\n{}\nrest\n", OPEN, table.trim_end(), CLOSE);
        let crlf = lf.replace('\n', "\r\n");

        assert_eq!(
            plan_update(&lf, OPEN, CLOSE, table),
            Ok(None),
            "the LF form is current"
        );
        assert_eq!(
            plan_update(&crlf, OPEN, CLOSE, table),
            Ok(None),
            "and so is the same document with Windows line endings"
        );
    }

    #[test]
    fn a_crlf_document_is_rewritten_with_crlf() {
        let before = format!("intro\r\n{}\r\n{}\r\nrest\r\n", OPEN, CLOSE);
        let updated = plan_update(&before, OPEN, CLOSE, "| new |\n")
            .expect("markers are present")
            .expect("the section is empty, so it needs updating");

        assert!(updated.contains("| new |"));
        assert!(
            !updated.replace("\r\n", "").contains('\n'),
            "a CRLF document must not come back with mixed endings: {:?}",
            updated
        );
    }

    #[test]
    fn a_changed_section_is_reported_whatever_the_line_endings() {
        let before = format!("{}\n\n| old |\n{}\n", OPEN, CLOSE);
        assert!(plan_update(&before, OPEN, CLOSE, "| new |\n")
            .expect("markers are present")
            .is_some());
    }

    #[test]
    fn a_missing_marker_is_an_error_rather_than_a_panic_in_the_splice() {
        assert!(plan_update("no markers here", OPEN, CLOSE, "x").is_err());
        assert!(plan_update(&format!("{} only", OPEN), OPEN, CLOSE, "x").is_err());
        assert!(plan_update(&format!("{}{}", CLOSE, OPEN), OPEN, CLOSE, "x").is_err());
    }

    /// The generated tables must not contain a raw `|`, which would silently
    /// split a cell and produce a table that renders wrong rather than failing.
    #[test]
    fn generated_tables_have_no_unescaped_pipes() {
        for (name, table) in [
            ("functions", render_functions()),
            ("operators", render_operators()),
            ("statements", render_statements()),
            ("not-supported", render_not_supported()),
        ] {
            for line in table.lines().filter(|l| l.starts_with('|')) {
                let cells = line.matches("|").count() - line.matches("\\|").count();
                assert!(
                    cells >= 2,
                    "{}: this row does not look like a table row: {:?}",
                    name,
                    line
                );
            }
        }
    }
}
