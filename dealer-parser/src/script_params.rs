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
        (0..10)
            .filter(|i| self.values[*i].is_some() && !mentions(source, *i))
            .collect()
    }
}

/// Whether `$i` appears anywhere the substitution would reach.
fn mentions(source: &str, index: usize) -> bool {
    let chars: Vec<char> = source.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        if let Some(end) = crate::fdshape::skippable(&chars, at) {
            at = end;
            continue;
        }
        if chars[at] == '$' && chars.get(at + 1) == Some(&char::from(b'0' + index as u8)) {
            return true;
        }
        at += 1;
    }
    false
}

/// Replace every `$0`-`$9` with the text the command line gave for it.
///
/// Comments and quoted strings are stepped over, as the reference's lexer does:
/// its `{qstring}` rule matches the whole string before `{scriptvar}` ever sees
/// the `$`, which is why its own example has to pass a parameter *including*
/// the quotes to get a string in — `-2 '"Controls West > 15 hcp"'`.
pub fn substitute(source: &str, params: &ScriptParams) -> Result<String, String> {
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
        let Some(value) = &params.values[index] else {
            return Err(format!(
                "the script uses `${index}` and nothing supplies it{}\n       \
                 Pass it on the command line: --param {index}=<text>. DealerV2_4 spells \
                 this `-{index} <text>`, which is dealer.exe's swapping switch here.",
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
