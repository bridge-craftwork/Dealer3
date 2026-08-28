//! What is left to do, and the table of it in the roadmap.
//!
//! The priority matrix in `docs/implementation_roadmap.md` used to list twelve
//! features with no status column at all. Nine of them were finished, so the
//! table read as a plan for work that had already happened — and its priorities
//! were all command-line switches, by which time the switches were the part
//! that was nearly done and the language was the part that was not.
//!
//! So this lists only what remains. A finished item is not ticked; it is
//! deleted, and for anything that delivers a switch the test below notices when
//! that has happened and says so.
//!
//! Effort and value are judgements, which is why they are written down here
//! rather than derived. Priority is not: it falls out of the two, so it cannot
//! disagree with them.

use crate::switches::{support, Support};
use dealer_parser::vocabulary;

/// What would make an item finished, when that is something a test can see.
///
/// Only switches used to be checkable, which left every language row on the
/// honour system — and the language is where the work now is.
#[derive(Clone, Copy)]
pub enum DoneWhen {
    /// This switch is implemented.
    Switch(&'static str),
    /// This function name is in the vocabulary.
    Function(&'static str),
    /// This statement keyword is in the vocabulary.
    Statement(&'static str),
}

impl DoneWhen {
    fn is_done(self) -> bool {
        match self {
            DoneWhen::Switch(flag) => support(flag, "") == Support::Yes,
            DoneWhen::Function(name) => vocabulary::FUNCTIONS.contains(&name),
            DoneWhen::Statement(name) => vocabulary::STATEMENT_KEYWORDS.contains(&name),
        }
    }

    fn describe(self) -> String {
        match self {
            DoneWhen::Switch(f) => format!("the switch `{}`", f),
            DoneWhen::Function(f) => format!("the function `{}`", f),
            DoneWhen::Statement(k) => format!("the statement `{}`", k),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effort {
    Low,
    Medium,
    High,
}

impl Effort {
    fn label(self) -> &'static str {
        match self {
            Effort::Low => "Low",
            Effort::Medium => "Medium",
            Effort::High => "High",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Low,
    Medium,
    High,
}

impl Value {
    fn label(self) -> &'static str {
        match self {
            Value::Low => "Low",
            Value::Medium => "Medium",
            Value::High => "High",
        }
    }
}

/// Derived from effort and value, so the three columns cannot contradict each
/// other the way a hand-written priority column eventually does.
fn priority(item: &WorkItem) -> (u8, &'static str) {
    match (item.value, item.effort) {
        (Value::High, Effort::Low) => (0, "🔴 Do first"),
        (Value::High, _) => (1, "🟡 Worth it"),
        (Value::Medium, Effort::Low) => (1, "🟡 Worth it"),
        (Value::Medium, _) => (2, "🟢 Someday"),
        (Value::Low, _) => (3, "🔵 Unlikely"),
    }
}

pub struct WorkItem {
    pub what: &'static str,
    /// How a test can tell this item is finished, where it can.
    pub done_when: Option<DoneWhen>,
    /// GitHub issue, where there is one.
    pub issue: Option<u32>,
    pub effort: Effort,
    pub value: Value,
    pub note: Option<&'static str>,
}

/// Everything still outstanding, in no particular order — the table sorts it.
pub const REMAINING: &[WorkItem] = &[
    // ---- The language, which is where the real gaps are now ---------------
    WorkItem {
        what: "Two-dimensional `frequency`",
        done_when: None,
        issue: None,
        effort: Effort::Medium,
        value: Value::Low,
        note: Some("The original takes a second expression and range and prints marginals."),
    },
    WorkItem {
        what: "Contract tokens in `score()`, e.g. `3N` for the code 34",
        done_when: None,
        issue: None,
        effort: Effort::Low,
        value: Value::Low,
        note: None,
    },
    WorkItem {
        what: "The length-bias form of `predeal`, `spades(north) == 5`",
        done_when: None,
        issue: None,
        effort: Effort::Medium,
        value: Value::Low,
        note: Some("Rejected loudly today; the same thing can be written in the condition."),
    },
    WorkItem {
        what: "`--bbo-strict`: warn when a script will behave differently on BBO",
        done_when: None,
        issue: Some(13),
        effort: Effort::Medium,
        value: Value::Low,
        note: Some("Rick judged it unlikely to bite."),
    },
    // ---- Switches ---------------------------------------------------------
    WorkItem {
        what: "Library mode: replay deals by index",
        done_when: Some(DoneWhen::Switch("-l")),
        issue: None,
        effort: Effort::High,
        value: Value::Low,
        note: Some("`--input-deals` already covers the common case in dealer3's own way."),
    },
    WorkItem {
        what: "Upper-case the honour cards in output",
        done_when: Some(DoneWhen::Switch("-u")),
        issue: None,
        effort: Effort::Low,
        value: Value::Low,
        note: Some("Cosmetic."),
    },
    WorkItem {
        what: "Export in RP zrd format",
        done_when: Some(DoneWhen::Switch("-Z")),
        issue: None,
        effort: Effort::Medium,
        value: Value::Low,
        note: None,
    },
    WorkItem {
        what: "Export in DL52 format",
        done_when: None,
        issue: None,
        effort: Effort::Medium,
        value: Value::Low,
        note: Some(
            "DealerV2_4 spells it `-l`, which is dealer.exe's library switch — so as with \
             the script parameters, the spelling here would have to differ.",
        ),
    },
    WorkItem {
        what: "Double-dummy solver mode",
        done_when: Some(DoneWhen::Switch("-M")),
        issue: None,
        effort: Effort::Medium,
        value: Value::Low,
        note: Some(
            "DealerV2_4's `-M`, which prints a double-dummy table per deal. The solver \
             behind it is in place; this is the switch and its output format.",
        ),
    },
    WorkItem {
        what: "Script parameters `$0`-`$9`",
        done_when: None,
        issue: None,
        effort: Effort::Medium,
        value: Value::Low,
        note: Some(
            "DealerV2_4 sets them with `-0` to `-9`, which are dealer.exe's swapping \
             switches. dealer.exe wins, so the syntax could be accepted but the switch \
             that fills it would be dealer3's own. `$` is unused in the grammar today, so \
             the parsing is the easy half; the hard half is that `$` is not in the \
             original's lexer either, so a script using it will not run on BBO.",
        ),
    },
    WorkItem {
        what: "Exhaust mode",
        done_when: Some(DoneWhen::Switch("-e")),
        issue: None,
        effort: Effort::High,
        value: Value::Low,
        note: Some("Never finished in the original either; the code is compiled out."),
    },
];

/// The priority matrix, as markdown.
pub fn render_matrix() -> String {
    let mut items: Vec<&WorkItem> = REMAINING.iter().collect();
    items.sort_by_key(|item| (priority(item).0, item.effort, item.what));

    let mut out = String::from(
        "Only what is **left**. A finished item is deleted rather than ticked, and anything \
         that delivers a switch is checked against the argument parser, so this table cannot \
         quietly describe work that has already happened.\n\n\
         Priority is derived from effort and value rather than written down beside them.\n\n\
         | Priority | What | Effort | Value | Issue | Notes |\n|---|---|---|---|---|---|\n",
    );
    for item in items {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            priority(item).1,
            item.what,
            item.effort.label(),
            item.value.label(),
            item.issue
                .map(|n| format!(
                    "[#{}](https://github.com/bridge-craftwork/Dealer3/issues/{})",
                    n, n
                ))
                .unwrap_or_default(),
            item.note.unwrap_or("")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An item that delivers a switch the parser now accepts is an item that is
    /// finished, and a finished item does not belong in a list of what is left.
    #[test]
    fn nothing_in_the_list_is_already_done() {
        for item in REMAINING {
            let Some(check) = item.done_when else {
                continue;
            };
            assert!(
                !check.is_done(),
                "\"{}\" is still listed as outstanding, but {} exists — delete the row rather \
                 than ticking it",
                item.what,
                check.describe()
            );
        }
    }

    /// A switch named here must be one the comparison table knows about, or the
    /// check above silently passes for a switch that does not exist.
    #[test]
    fn every_named_switch_exists_in_the_comparison_table() {
        for item in REMAINING {
            let Some(DoneWhen::Switch(switch)) = item.done_when else {
                continue;
            };
            assert!(
                crate::switches::SWITCH_ROWS
                    .iter()
                    .any(|row| row.short == switch || row.long == switch),
                "\"{}\" names `{}`, which is not in SWITCH_ROWS",
                item.what,
                switch
            );
        }
    }

    /// The guard above is only worth anything if `support` can actually say
    /// "yes" — a helper that always returned `No` would let every finished item
    /// sit in the list forever while the test passed.
    #[test]
    fn the_done_check_can_actually_detect_finished_work() {
        assert!(DoneWhen::Switch("-p").is_done());
        assert!(!DoneWhen::Switch("-M").is_done());
        assert!(DoneWhen::Function("hcp").is_done());
        assert!(!DoneWhen::Function("nosuchfunction").is_done());
        assert!(DoneWhen::Statement("condition").is_done());
        assert!(!DoneWhen::Statement("nosuchstatement").is_done());
    }

    /// Priority is derived rather than written down, so the derivation has to
    /// be exercised across the whole grid — including the ratings no row
    /// currently carries, which is otherwise the first thing to rot when the
    /// list empties out.
    #[test]
    fn priority_follows_from_effort_and_value() {
        let rank = |value, effort| {
            priority(&WorkItem {
                what: "",
                done_when: None,
                issue: None,
                effort,
                value,
                note: None,
            })
        };
        assert_eq!(rank(Value::High, Effort::Low).1, "🔴 Do first");
        assert_eq!(rank(Value::High, Effort::High).1, "🟡 Worth it");
        assert_eq!(rank(Value::Medium, Effort::Low).1, "🟡 Worth it");
        assert_eq!(rank(Value::Medium, Effort::High).1, "🟢 Someday");
        assert_eq!(rank(Value::Low, Effort::Low).1, "🔵 Unlikely");

        // More valuable, or less work, must never sort later.
        for effort in [Effort::Low, Effort::Medium, Effort::High] {
            assert!(rank(Value::High, effort).0 <= rank(Value::Medium, effort).0);
            assert!(rank(Value::Medium, effort).0 <= rank(Value::Low, effort).0);
        }
        for value in [Value::Low, Value::Medium, Value::High] {
            assert!(rank(value, Effort::Low).0 <= rank(value, Effort::High).0);
        }

        // Every label the enums can print is reachable, so a new variant
        // cannot be added without a label.
        for (effort, label) in [
            (Effort::Low, "Low"),
            (Effort::Medium, "Medium"),
            (Effort::High, "High"),
        ] {
            assert_eq!(effort.label(), label);
        }
        for (value, label) in [
            (Value::Low, "Low"),
            (Value::Medium, "Medium"),
            (Value::High, "High"),
        ] {
            assert_eq!(value.label(), label);
        }
    }

    #[test]
    fn rows_are_filled_in() {
        for item in REMAINING {
            assert!(
                !item.what.trim().is_empty(),
                "a work item has no description"
            );
        }
    }

    #[test]
    fn docs_are_up_to_date() {
        crate::generated_docs::check_or_update(
            "implementation_roadmap.md",
            "priority-matrix",
            &render_matrix(),
        );
    }
}
