//! `$0` to `$9`: DealerV2_4's script parameters.
//!
//! A parameter is not a value but a piece of source. DealerV2_4's lexer, on
//! meeting `$n`, pushes a new buffer holding whatever the switch gave it and
//! scans that in place — so a parameter can be a number, a compass, a shape
//! spec, or a function name. Its own `NTscripted.dls`, run with
//! `-0 west -1 12 -2 14 -3 east -4 '5xxx + x5xx'` and `-9 hcp`, is
//!
//! ```text
//! NTshape = shape($0, any 4333 + any 4432 + any 5332 - 5xxx - x5xx)
//! condition NTshape && ($9($0) >= $1) && ($9($0) <= $2) && shape($3, $4)
//! ```
//!
//! where `$9($0)` becomes `hcp(west)`. Nothing but textual substitution can do
//! that, which is why this is a preprocessor pass rather than a name the
//! grammar knows.
//!
//! It runs before the François Dellacherie shapes are expanded, because a
//! parameter can be part of one — `shape{$1, $2:d>c or h>s}` is in the
//! reference's own regression suite, and its lexer likewise fills the
//! parameter into the command buffer before running the expander.
//!
//! **The switches differ.** DealerV2_4 sets these with `-0` to `-9`, which are
//! dealer.exe's swapping switches; dealer.exe wins, so dealer3 spells it
//! `--param 1=west`. The syntax a script is written in is unchanged.
//!
//! One deliberate difference in behaviour. DealerV2_4 zeroes its parameter
//! table and never looks at the lengths again, so a `$n` with no switch behind
//! it scans an empty buffer and vanishes: `average $2 controls(west)` quietly
//! becomes a valid statement that has lost its label. `$` marks the spot too
//! plainly for that — an unfilled parameter is refused.
//!
//! # What a script says about its own parameters
//!
//! Refusing an unfilled `$n` is right, and on its own it means a parameterised
//! script cannot be run at all without knowing, from outside the file, which
//! parameters it wants and what they should be. `NTscripted.dls` handed to
//! someone without its invocation is unrunnable, and nothing in it says what
//! `$3` was meant to be.
//!
//! So a script may declare its own:
//!
//! ```text
//! # param 0 = west          # the seat that opens
//! # param 1 = 15            # minimum HCP
//! # param 4 = 5xxx + x5xx   # the shape responder shows
//! ```
//!
//! A whole-line comment, because the declaration has to survive the trip to BBO
//! and to the original dealer, whose lexers skip a `#` line entirely — the same
//! reason the `HandType_` convention is a variable name rather than a keyword.
//! The description is whatever follows a second `#`, an exact split rather than
//! a run of spaces because `#` cannot appear inside a value the language would
//! accept, where `5xxx + x5xx` is full of single spaces.
//!
//! The precedence is `--param` if given, else the declared default, else the
//! error. Which makes the default documentation that cannot drift from the
//! thing it documents, and gives a front end something to label a field with
//! and somewhere to start it — `dealer --params` on the command line, fields
//! beside the editor in the browser.

/// What the command line supplied for `$0` to `$9`.
#[derive(Debug, Default, Clone)]
pub struct ScriptParams {
    values: [Option<String>; 10],
}

impl ScriptParams {
    /// Read one `N=text` pair, as `--param` takes it.
    pub fn set(&mut self, spec: &str) -> Result<(), String> {
        let (index, value) = spec.split_once('=').ok_or_else(|| {
            format!("`{spec}` is not a script parameter; write it as `--param 1=west`")
        })?;
        let index: usize = index
            .trim()
            .parse()
            .map_err(|_| format!("`{index}` is not a parameter number; they run from 0 to 9"))?;
        if index > 9 {
            return Err(format!(
                "there is no `${index}`; script parameters run from 0 to 9"
            ));
        }
        if self.values[index].is_some() {
            return Err(format!("`${index}` is given more than once"));
        }
        self.values[index] = Some(value.to_string());
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.values.iter().all(Option::is_none)
    }

    /// Which parameters were supplied but never mentioned by the script.
    pub fn unused(&self, source: &str) -> Vec<usize> {
        let used = uses(source);
        (0..10)
            .filter(|i| self.values[*i].is_some() && !used.iter().any(|(j, _)| j == i))
            .collect()
    }
}

/// One `# param N = VALUE` line: what the script itself says a parameter is.
///
/// A comment rather than syntax, because the declaration has to survive the
/// trip to BBO and to the original dealer, whose lexers skip a `#` line whole.
/// A syntax form would be cleaner to validate and would stop the script running
/// anywhere else, which is the opposite of the point — a scenario that carries
/// its own defaults is one you can hand to someone without the invocation that
/// goes with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDecl {
    /// Which parameter, 0 to 9.
    pub index: usize,
    /// The source to put where `$n` stands when nothing else supplies it.
    pub default: String,
    /// What it is for, in the writer's words. Everything after the second `#`.
    pub description: Option<String>,
    /// The line the declaration is on, for an error that has to point at it.
    pub line: usize,
}

/// A parameter a script has something to say about: one it uses, one it
/// declares, or both.
///
/// This is what a front end needs to ask for the ones that are missing — the
/// `$n` occurrences alone give it nowhere to put a label and no sensible
/// starting value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptParam {
    pub index: usize,
    /// The declared default, if the script carries one.
    pub default: Option<String>,
    pub description: Option<String>,
    /// Where the declaration is, if there is one.
    pub declared_on: Option<usize>,
    /// Where the parameter is first used. `None` means it is declared and never
    /// mentioned, which is usually a typo in one or the other.
    pub used_on: Option<usize>,
}

/// Every `# param n = ...` line the script carries, indexed by parameter.
///
/// Only a whole-line comment counts. A `#` after code on the same line is a
/// remark about that line, and reading a declaration out of one would mean a
/// script's behaviour depended on where a comment happened to sit.
pub fn declarations(source: &str) -> Result<[Option<ParamDecl>; 10], String> {
    let mut found: [Option<ParamDecl>; 10] = Default::default();
    for (offset, raw) in source.lines().enumerate() {
        let line = offset + 1;
        let Some(rest) = pragma_body(raw) else {
            continue;
        };
        let decl = parse_declaration(rest, line, raw)?;
        if let Some(first) = &found[decl.index] {
            return Err(format!(
                "`${}` is declared twice, on lines {} and {}. Leave one.",
                decl.index, first.line, decl.line
            ));
        }
        let index = decl.index;
        found[index] = Some(decl);
    }
    Ok(found)
}

/// What follows `param` on a declaration line, or `None` if this is not one.
///
/// `param` is a magic word, so its case is not part of it. Three things keep an
/// ordinary comment from becoming a declaration, and they matter more than they
/// look — a comment misread as a malformed declaration is a hard error, on a
/// line whose author never meant to declare anything:
///
/// - the word ends there, so `parameters` is not it, or documenting the feature
///   inside a script would change what the script does;
/// - a number follows it, so the `# Param: ...` and `# alias: ...` headers the
///   Practice-Bidding-Scenarios corpus carries stay comments;
/// - and the whole line is the comment, checked by the caller, because a `#`
///   after code is a remark about that line.
///
/// Past those it *is* a declaration, and anything wrong with it is reported
/// rather than skipped: silently ignoring `# param 0 west` is how a script ends
/// up unrunnable with a line in it that looks as though it should have worked.
fn pragma_body(raw: &str) -> Option<&str> {
    let trimmed = raw.trim_start();
    let body = trimmed
        .strip_prefix('#')
        .or_else(|| trimmed.strip_prefix("//"))?
        .trim_start();
    if !body.get(..5)?.eq_ignore_ascii_case("param") {
        return None;
    }
    let rest = &body[5..];
    // Whitespace, then the parameter number, written `$3` or `3`.
    let after_word = rest.strip_prefix(|c: char| c.is_whitespace())?.trim_start();
    let digits = after_word.strip_prefix('$').unwrap_or(after_word);
    digits
        .starts_with(|c: char| c.is_ascii_digit())
        .then_some(rest)
}

fn parse_declaration(rest: &str, line: usize, raw: &str) -> Result<ParamDecl, String> {
    let complain = |what: &str| {
        format!(
            "{what}, on line {line}:\n       {}\n       \
             A declaration reads `# param 1 = 15 # minimum HCP`: the number, `=`, the \
             source to stand in for `$1`, and anything after a second `#` as its \
             description.",
            raw.trim()
        )
    };

    let (head, value) = rest
        .split_once('=')
        .ok_or_else(|| complain("a parameter declaration needs an `=`"))?;
    let head = head.trim().strip_prefix('$').unwrap_or(head.trim());
    let index: usize = head
        .parse()
        .map_err(|_| complain(&format!("`{head}` is not a parameter number")))?;
    if index > 9 {
        return Err(complain(&format!(
            "there is no `${index}`; script parameters run from 0 to 9"
        )));
    }

    // The description is whatever follows a second `#`. An exact split rather
    // than a run of spaces: `#` cannot appear inside a value the language would
    // accept, where a value like `5xxx + x5xx` is full of single spaces and a
    // gap rule would have to guess which one ended it.
    let (default, description) = match value.split_once('#') {
        Some((v, d)) => (v.trim(), Some(d.trim())),
        None => (value.trim(), None),
    };
    if default.is_empty() {
        return Err(complain(&format!("`${index}` is declared with no default")));
    }

    Ok(ParamDecl {
        index,
        default: default.to_string(),
        description: description.filter(|d| !d.is_empty()).map(|d| d.to_string()),
        line,
    })
}

/// Every `$n` the substitution would reach, as `(index, line)`, in order.
fn uses(source: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    let mut line = 1;
    while at < chars.len() {
        if let Some(end) = crate::fdshape::skippable(&chars, at) {
            line += chars[at..end].iter().filter(|c| **c == '\n').count();
            at = end;
            continue;
        }
        if chars[at] == '\n' {
            line += 1;
        } else if chars[at] == '$' {
            if let Some(digit) = chars.get(at + 1).and_then(|c| c.to_digit(10)) {
                out.push((digit as usize, line));
                at += 2;
                continue;
            }
        }
        at += 1;
    }
    out
}

/// What a script wants filled in, ordered by parameter number.
///
/// Both halves of it: the parameters it uses, so a front end knows what to ask
/// for, and the ones it only declares, so a declaration that has lost its `$n`
/// to an edit is visible rather than silently doing nothing.
pub fn script_parameters(source: &str) -> Result<Vec<ScriptParam>, String> {
    let declared = declarations(source)?;
    let used = uses(source);
    Ok((0..10)
        .filter_map(|index| {
            let decl = declared[index].as_ref();
            let used_on = used.iter().find(|(i, _)| *i == index).map(|(_, l)| *l);
            if decl.is_none() && used_on.is_none() {
                return None;
            }
            Some(ScriptParam {
                index,
                default: decl.map(|d| d.default.clone()),
                description: decl.and_then(|d| d.description.clone()),
                declared_on: decl.map(|d| d.line),
                used_on,
            })
        })
        .collect())
}

/// Replace every `$0`-`$9` with the text standing behind it.
///
/// `--param` first, then the script's own `# param n = ...` declaration, and
/// only then the error — so a script carrying its own defaults runs with no
/// switches at all, and one switch overrides one default without disturbing the
/// rest.
///
/// Comments and quoted strings are stepped over, as the reference's lexer does:
/// its `{qstring}` rule matches the whole string before `{scriptvar}` ever sees
/// the `$`, which is why its own example has to pass a parameter *including*
/// the quotes to get a string in — `-2 '"Controls West > 15 hcp"'`.
pub fn substitute(source: &str, params: &ScriptParams) -> Result<String, String> {
    let declared = declarations(source)?;
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    while at < chars.len() {
        if let Some(end) = crate::fdshape::skippable(&chars, at) {
            out.extend(&chars[at..end]);
            at = end;
            continue;
        }
        if chars[at] != '$' {
            out.push(chars[at]);
            at += 1;
            continue;
        }
        let Some(digit) = chars.get(at + 1).and_then(|c| c.to_digit(10)) else {
            return Err(format!(
                "`$` names a script parameter and needs a digit after it, as in `$1`{}",
                near(&chars, at)
            ));
        };
        let index = digit as usize;
        let value = params.values[index]
            .as_deref()
            .or(declared[index].as_ref().map(|d| d.default.as_str()));
        let Some(value) = value else {
            return Err(format!(
                "the script uses `${index}` and nothing supplies it{}\n       \
                 Pass it on the command line: --param {index}=<text>. DealerV2_4 spells \
                 this `-{index} <text>`, which is dealer.exe's swapping switch here.\n       \
                 Or give the script its own default, so it runs without one: \
                 `# param {index} = <text>  # what it means`.",
                near(&chars, at)
            ));
        };
        out.push_str(value);
        at += 2;
    }
    Ok(out)
}

/// The line a problem is on, and the line itself, since the substitution has
/// already moved everything around by the time a parser could say.
fn near(chars: &[char], at: usize) -> String {
    let line_no = chars[..at].iter().filter(|c| **c == '\n').count() + 1;
    let start = chars[..at]
        .iter()
        .rposition(|c| *c == '\n')
        .map_or(0, |i| i + 1);
    let end = chars[at..]
        .iter()
        .position(|c| *c == '\n')
        .map_or(chars.len(), |i| at + i);
    let line: String = chars[start..end].iter().collect();
    format!(", on line {line_no}:\n       {}", line.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[&str]) -> ScriptParams {
        let mut p = ScriptParams::default();
        for pair in pairs {
            p.set(pair).expect("a parameter");
        }
        p
    }

    #[test]
    fn a_parameter_is_source_not_a_number() {
        // DealerV2_4's own NTscripted.dls, and the point of the whole feature:
        // `$9($0)` is a function name applied to a compass.
        let script = "condition ($9($0) >= $1) && shape($3, $4)\n";
        let filled = substitute(
            script,
            &params(&["0=west", "1=12", "3=east", "4=5xxx + x5xx", "9=hcp"]),
        )
        .expect("substitutes");
        assert_eq!(
            filled,
            "condition (hcp(west) >= 12) && shape(east, 5xxx + x5xx)\n"
        );
    }

    #[test]
    fn an_unfilled_parameter_is_refused() {
        // Where DealerV2_4 scans an empty buffer and carries on, leaving
        // `average $2 controls(west)` a valid statement with no label.
        let err = substitute("action average $2 controls(west)\n", &params(&["1=x"]))
            .expect_err("should refuse");
        assert!(err.contains("$2"), "got: {err}");
        assert!(err.contains("--param 2="), "should say how: {err}");
    }

    #[test]
    fn a_lone_dollar_is_refused() {
        assert!(substitute("condition $ > 1\n", &params(&[])).is_err());
    }

    #[test]
    fn comments_and_strings_are_left_alone() {
        // The reference's lexer matches a quoted string whole, so `$1` inside
        // one is never a parameter — which is why its own example passes the
        // quotes in the parameter instead.
        for source in [
            "# a $1 in a comment\ncondition 1\n",
            "// a $1 in a comment\ncondition 1\n",
            "/* a $1 in a block */\ncondition 1\n",
            "title \"a $1 in a string\"\ncondition 1\n",
        ] {
            assert_eq!(
                substitute(source, &params(&[])).expect("no parameters to fill"),
                source,
                "in: {source}"
            );
        }
    }

    #[test]
    fn a_parameter_may_carry_its_own_quotes() {
        // How the reference gets a string in: `-2 '"Controls West"'`.
        let filled = substitute(
            "action average $2 controls(west)\n",
            &params(&["2=\"Controls West\""]),
        )
        .expect("substitutes");
        assert_eq!(filled, "action average \"Controls West\" controls(west)\n");
    }

    #[test]
    fn the_spec_has_to_look_like_one() {
        let mut p = ScriptParams::default();
        assert!(p.set("west").is_err());
        assert!(p.set("x=west").is_err());
        assert!(p.set("10=west").is_err());
        assert!(p.set("1=west").is_ok());
        assert!(p.set("1=east").is_err(), "twice over is a mistake");
    }

    #[test]
    fn a_script_can_carry_its_own_defaults() {
        // The point of the whole thing: no switches, and it still runs.
        let script = "# param 0 = west   # the seat that opens\n\
                      # param 1 = 15     # minimum HCP\n\
                      condition hcp($0) >= $1\n";
        assert_eq!(
            substitute(script, &params(&[])).expect("the script supplies them"),
            "# param 0 = west   # the seat that opens\n\
             # param 1 = 15     # minimum HCP\n\
             condition hcp(west) >= 15\n"
        );
    }

    #[test]
    fn a_switch_beats_a_declared_default() {
        // And overriding one leaves the others alone, which is what makes a
        // declared default usable as a starting point rather than all or
        // nothing.
        let script = "# param 0 = west  # the seat\n\
                      # param 1 = 15    # minimum\n\
                      condition hcp($0) >= $1\n";
        let filled = substitute(script, &params(&["1=20"])).expect("substitutes");
        assert!(
            filled.ends_with("condition hcp(west) >= 20\n"),
            "got: {filled}"
        );
    }

    #[test]
    fn a_declaration_is_read_from_either_comment_marker() {
        for marker in ["#", "//"] {
            let script = format!("{marker} PARAM $3 = north\ncondition hcp($3) > 1\n");
            let filled = substitute(&script, &params(&[])).expect("substitutes");
            assert!(
                filled.ends_with("condition hcp(north) > 1\n"),
                "got: {filled}"
            );
        }
    }

    #[test]
    fn a_default_may_be_a_shape_full_of_spaces() {
        // Why the description is split on `#` rather than on a run of spaces:
        // this value has three of them and none of them ends it.
        let decls =
            declarations("# param 4 = 5xxx + x5xx   # what responder shows\n").expect("declares");
        let four = decls[4].as_ref().expect("$4");
        assert_eq!(four.default, "5xxx + x5xx");
        assert_eq!(four.description.as_deref(), Some("what responder shows"));
    }

    #[test]
    fn a_declaration_without_a_description_is_still_a_declaration() {
        let decls = declarations("# param 2 = 14\n").expect("declares");
        let two = decls[2].as_ref().expect("$2");
        assert_eq!(two.default, "14");
        assert_eq!(two.description, None);
        assert_eq!(two.line, 1);
    }

    #[test]
    fn prose_about_parameters_is_not_a_declaration() {
        // `param` is a magic word, but a comment that merely starts with a
        // longer word beginning `param` is not one — otherwise documenting the
        // feature inside a script would change what the script does.
        for line in [
            "# parameters are filled with --param\n",
            "# params: see the reference\n",
            // A number is what makes it a declaration, so the `# Param: ...`
            // headers the Practice-Bidding-Scenarios corpus carries stay
            // comments. A declaration is refused when malformed, and refusing
            // one of these would stop a script that never meant to declare
            // anything.
            "# Param: the seat that opens\n",
            "# param x = west\n",
            "condition hcp(north) > 1 # param 0 = west\n",
        ] {
            let script = format!("{line}condition 1\n");
            let decls = declarations(&script).expect("no declarations");
            assert!(decls.iter().all(Option::is_none), "in: {line}");
        }
    }

    #[test]
    fn a_malformed_declaration_is_refused_rather_than_ignored() {
        // Silently skipping these is how a script ends up unrunnable with a
        // line in it that looks like it should have worked.
        for source in [
            "# param 0 west\n",    // no `=`
            "# param 12 = west\n", // out of range
            "# param 0 =\n",       // nothing to stand in
            "# param 0 =   # only a description\n",
        ] {
            let err = declarations(source).expect_err("should refuse");
            assert!(err.contains("line 1"), "should point at it: {err}");
            assert!(
                err.contains("`# param 1 = 15"),
                "should show the form: {err}"
            );
        }
    }

    #[test]
    fn declaring_the_same_parameter_twice_is_refused() {
        let err = declarations("# param 0 = west\n# param 0 = east\n").expect_err("refuses");
        assert!(err.contains("lines 1 and 2"), "got: {err}");
    }

    #[test]
    fn an_unfilled_parameter_says_both_ways_to_fill_it() {
        let err = substitute("condition hcp($0) > 1\n", &params(&[])).expect_err("refuses");
        assert!(err.contains("--param 0="), "the switch: {err}");
        assert!(err.contains("# param 0 ="), "the declaration: {err}");
    }

    #[test]
    fn what_a_script_wants_is_reported_for_a_front_end() {
        let script = "# param 0 = west   # the seat that opens\n\
                      # param 7 = north  # left over from an edit\n\
                      condition hcp($0) >= $1\n";
        let wanted = script_parameters(script).expect("reads");
        assert_eq!(wanted.len(), 3);

        // Declared and used.
        assert_eq!(wanted[0].index, 0);
        assert_eq!(wanted[0].default.as_deref(), Some("west"));
        assert_eq!(
            wanted[0].description.as_deref(),
            Some("the seat that opens")
        );
        assert_eq!(wanted[0].declared_on, Some(1));
        assert_eq!(wanted[0].used_on, Some(3));

        // Used and undeclared: what a front end has to ask for.
        assert_eq!(wanted[1].index, 1);
        assert_eq!(wanted[1].default, None);
        assert_eq!(wanted[1].used_on, Some(3));

        // Declared and never used: a declaration that has lost its `$7`.
        assert_eq!(wanted[2].index, 7);
        assert_eq!(wanted[2].used_on, None);
    }

    #[test]
    fn a_use_inside_a_comment_is_not_a_use() {
        let wanted = script_parameters("# $3 was here\ncondition 1\n").expect("reads");
        assert!(wanted.is_empty(), "got: {wanted:?}");
    }

    #[test]
    fn unused_parameters_are_noticed() {
        let source = "condition hcp($0) > $1\n";
        assert_eq!(
            params(&["0=north", "1=12"]).unused(source),
            Vec::<usize>::new()
        );
        assert_eq!(params(&["0=north", "5=x"]).unused(source), vec![5]);
        // A mention inside a comment is not a mention.
        assert_eq!(params(&["3=x"]).unused("# $3\ncondition 1\n"), vec![3]);
    }
}
