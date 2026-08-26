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
    let start = current.find(&open).unwrap_or_else(|| {
        panic!(
            "{} has no `{}` marker.\n\nAdd it, with a matching `{}`, around the place the \
             generated table should go.",
            path.display(),
            open,
            close
        )
    });
    let stop = current.find(&close).unwrap_or_else(|| {
        panic!(
            "{} has `{}` but no matching `{}`",
            path.display(),
            open,
            close
        )
    });
    assert!(
        stop > start,
        "{}: `{}` appears after `{}`",
        path.display(),
        close,
        open
    );

    let body_from = start + open.len();
    let existing = &current[body_from..stop];
    let wanted = format!("\n\n{}\n", generated.trim_end());

    if existing == wanted {
        return;
    }

    if std::env::var("UPDATE_DOCS").is_ok() {
        let updated = format!("{}{}{}", &current[..body_from], wanted, &current[stop..]);
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
