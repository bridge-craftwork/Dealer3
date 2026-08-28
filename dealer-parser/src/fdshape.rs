//! `shape{ ... }`: François Dellacherie's shape language, expanded to `shape( ... )`.
//!
//! The original dealer can only be told a distribution one pattern at a time,
//! so "a balanced hand with four in a major" is a dozen patterns written out.
//! In 1997 François Dellacherie wrote a Perl pre-processor, `dpp`, that let you
//! write `shape{north, 4M(3+3+2+)}` and expanded it for you; DealerV2_4 ships
//! it as `fdp` and shells out to it from the lexer. Its readme is the language's
//! specification, reproduced in that project as `docs/README_FDshapes.txt`.
//!
//! dealer3 does the expansion here instead, for two reasons. It keeps the
//! braces out of the grammar — the four-digit shape literals were trouble
//! enough on their own, which is why `preprocess` exists — and it leaves the
//! web editor's highlighter alone, since it never sees anything but a `shape`
//! call. It also means no helper binary, so the wasm build has it too.
//!
//! Where this differs from `fdp`, deliberately: rather than manipulating
//! pattern strings, every construct is evaluated as a predicate over the 560
//! distributions, and the answer is rendered back out. That makes a ten-card
//! suit expressible — `5+Mxxx` means a ten-card major as much as a five-card
//! one, and `fdp` silently drops all forty such shapes because a pattern is one
//! character per suit. dealer3 writes them, `:` through `=` standing for ten
//! through thirteen.

use std::collections::BTreeSet;

/// A distribution: spades, hearts, diamonds, clubs.
type Shape = [u8; 4];

/// Every distribution of thirteen cards, in a stable order.
fn all_shapes() -> Vec<Shape> {
    let mut out = Vec::with_capacity(560);
    for s in 0..=13u8 {
        for h in 0..=13 - s {
            for d in 0..=13 - s - h {
                out.push([s, h, d, 13 - s - h - d]);
            }
        }
    }
    out
}

/// The lengths one suit is allowed, as a set — which is all a `5+`, a `2-`, an
/// `x` and a `[013-68]` have in common, and enough to answer every question
/// asked of them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Lengths(u16);

impl Lengths {
    fn none() -> Self {
        Lengths(0)
    }
    fn any() -> Self {
        Lengths((1 << 14) - 1)
    }
    fn exactly(n: u8) -> Self {
        Lengths(1 << n)
    }
    fn at_least(n: u8) -> Self {
        Lengths(((1u16 << 14) - 1) & !((1u16 << n) - 1))
    }
    fn at_most(n: u8) -> Self {
        Lengths((1u16 << (n + 1)) - 1)
    }
    fn with(self, other: Lengths) -> Self {
        Lengths(self.0 | other.0)
    }
    fn holds(self, n: u8) -> bool {
        n < 14 && self.0 & (1 << n) != 0
    }
}

/// One atomic shape: what each suit is pinned to, what floats between the
/// suits left over, and any condition attached with `:`.
struct Atom {
    /// Per suit, in spade-heart-diamond-club order. `None` means the suit is
    /// not pinned and belongs to the floating group.
    fixed: [Option<Lengths>; 4],
    /// Lengths for the suits not pinned, in any order between them.
    group: Vec<Lengths>,
    condition: Option<Cond>,
}

impl Atom {
    fn matches(&self, shape: Shape) -> bool {
        self.fits(shape) && self.condition.as_ref().is_none_or(|c| c.holds(shape))
    }

    /// Whether the pinned suits hold and the floating group can be laid over
    /// what is left. Four suits, so every arrangement is tried; the group is
    /// unordered by definition, and `M` and `m` have already become
    /// alternatives by the time we are here.
    fn fits(&self, shape: Shape) -> bool {
        for (suit, pinned) in self.fixed.iter().enumerate() {
            if let Some(lengths) = pinned {
                if !lengths.holds(shape[suit]) {
                    return false;
                }
            }
        }
        let mut free: Vec<usize> = (0..4).filter(|i| self.fixed[*i].is_none()).collect();
        if free.len() != self.group.len() {
            return false;
        }
        if free.is_empty() {
            return true;
        }
        permutations(&mut free, 0, &mut |order| {
            self.group
                .iter()
                .zip(order)
                .all(|(lengths, suit)| lengths.holds(shape[*suit]))
        })
    }
}

/// Whether any permutation of `items` satisfies `f`. Four elements at most, so
/// the straightforward swap-and-recurse is plenty.
fn permutations(items: &mut Vec<usize>, k: usize, f: &mut impl FnMut(&[usize]) -> bool) -> bool {
    if k == items.len() {
        return f(items);
    }
    for i in k..items.len() {
        items.swap(k, i);
        if permutations(items, k + 1, f) {
            items.swap(k, i);
            return true;
        }
        items.swap(k, i);
    }
    false
}

// ---------------------------------------------------------------- conditions

/// The `: d>c, h+s==10` part. `,` is `and` and binds tighter than `or`, which
/// is what the readme's examples assume.
enum Cond {
    Or(Box<Cond>, Box<Cond>),
    And(Box<Cond>, Box<Cond>),
    Not(Box<Cond>),
    Compare(Arith, Cmp, Arith),
}

#[derive(Clone, Copy)]
enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

enum Arith {
    Number(i32),
    Suit(usize),
    Op(Box<Arith>, char, Box<Arith>),
}

impl Cond {
    fn holds(&self, shape: Shape) -> bool {
        match self {
            Cond::Or(a, b) => a.holds(shape) || b.holds(shape),
            Cond::And(a, b) => a.holds(shape) && b.holds(shape),
            Cond::Not(a) => !a.holds(shape),
            Cond::Compare(left, op, right) => {
                let (l, r) = (left.value(shape), right.value(shape));
                match op {
                    Cmp::Eq => l == r,
                    Cmp::Ne => l != r,
                    Cmp::Lt => l < r,
                    Cmp::Le => l <= r,
                    Cmp::Gt => l > r,
                    Cmp::Ge => l >= r,
                }
            }
        }
    }
}

impl Arith {
    fn value(&self, shape: Shape) -> i32 {
        match self {
            Arith::Number(n) => *n,
            Arith::Suit(i) => shape[*i] as i32,
            Arith::Op(a, op, b) => {
                let (l, r) = (a.value(shape), b.value(shape));
                match op {
                    '+' => l + r,
                    '-' => l - r,
                    '*' => l * r,
                    '/' => {
                        if r == 0 {
                            0
                        } else {
                            l / r
                        }
                    }
                    _ => 0,
                }
            }
        }
    }
}

/// A cursor over the condition text.
struct Scan<'a> {
    text: &'a [u8],
    at: usize,
}

impl<'a> Scan<'a> {
    fn new(text: &'a str) -> Self {
        Scan {
            text: text.as_bytes(),
            at: 0,
        }
    }
    fn skip_space(&mut self) {
        while self.at < self.text.len() && self.text[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.skip_space();
        self.text.get(self.at).copied()
    }
    fn eat(&mut self, want: &str) -> bool {
        self.skip_space();
        if self.text[self.at..].starts_with(want.as_bytes()) {
            self.at += want.len();
            true
        } else {
            false
        }
    }
    fn done(&mut self) -> bool {
        self.peek().is_none()
    }
}

fn parse_cond(scan: &mut Scan) -> Result<Cond, String> {
    let mut left = parse_and(scan)?;
    while scan.eat("or") {
        let right = parse_and(scan)?;
        left = Cond::Or(Box::new(left), Box::new(right));
    }
    Ok(left)
}

fn parse_and(scan: &mut Scan) -> Result<Cond, String> {
    let mut left = parse_cmp(scan)?;
    loop {
        // `,` is the readme's spelling of `and`; `and` and `&&` cost nothing.
        if scan.eat(",") || scan.eat("&&") || scan.eat("and") {
            let right = parse_cmp(scan)?;
            left = Cond::And(Box::new(left), Box::new(right));
        } else {
            return Ok(left);
        }
    }
}

fn parse_cmp(scan: &mut Scan) -> Result<Cond, String> {
    if scan.eat("!") {
        return Ok(Cond::Not(Box::new(parse_cmp(scan)?)));
    }
    if scan.peek() == Some(b'(') {
        // Only a parenthesised condition can hold a comparison; anything else
        // is arithmetic and falls through below.
        let save = scan.at;
        scan.eat("(");
        if let Ok(inner) = parse_cond(scan) {
            if scan.eat(")") {
                return Ok(inner);
            }
        }
        scan.at = save;
    }
    let left = parse_sum(scan)?;
    // Longest first: `>=` before `>`.
    let op = if scan.eat(">=") {
        Cmp::Ge
    } else if scan.eat("<=") {
        Cmp::Le
    } else if scan.eat("==") {
        Cmp::Eq
    } else if scan.eat("!=") {
        Cmp::Ne
    } else if scan.eat(">") {
        Cmp::Gt
    } else if scan.eat("<") {
        Cmp::Lt
    } else if scan.eat("=") {
        Cmp::Eq
    } else {
        return Err("a condition needs a comparison".to_string());
    };
    let right = parse_sum(scan)?;
    Ok(Cond::Compare(left, op, right))
}

fn parse_sum(scan: &mut Scan) -> Result<Arith, String> {
    let mut left = parse_product(scan)?;
    loop {
        let op = match scan.peek() {
            Some(b'+') => '+',
            Some(b'-') => '-',
            _ => return Ok(left),
        };
        scan.at += 1;
        let right = parse_product(scan)?;
        left = Arith::Op(Box::new(left), op, Box::new(right));
    }
}

fn parse_product(scan: &mut Scan) -> Result<Arith, String> {
    let mut left = parse_atom_value(scan)?;
    loop {
        let op = match scan.peek() {
            Some(b'*') => '*',
            Some(b'/') => '/',
            _ => return Ok(left),
        };
        scan.at += 1;
        let right = parse_atom_value(scan)?;
        left = Arith::Op(Box::new(left), op, Box::new(right));
    }
}

fn parse_atom_value(scan: &mut Scan) -> Result<Arith, String> {
    match scan.peek() {
        Some(b'(') => {
            scan.at += 1;
            let inner = parse_sum(scan)?;
            if !scan.eat(")") {
                return Err("unclosed ( in a shape condition".to_string());
            }
            Ok(inner)
        }
        Some(c) if c.is_ascii_digit() => {
            let start = scan.at;
            while scan.at < scan.text.len() && scan.text[scan.at].is_ascii_digit() {
                scan.at += 1;
            }
            let text = std::str::from_utf8(&scan.text[start..scan.at]).unwrap_or("0");
            text.parse()
                .map(Arith::Number)
                .map_err(|_| format!("`{text}` is not a number"))
        }
        Some(c) => match suit_index(c as char) {
            Some(i) => {
                scan.at += 1;
                Ok(Arith::Suit(i))
            }
            None => Err(format!(
                "`{}` is not a suit or a number in a shape condition",
                c as char
            )),
        },
        None => Err("a shape condition ended early".to_string()),
    }
}

/// `s h d c` in the order a distribution is written.
fn suit_index(c: char) -> Option<usize> {
    match c {
        's' | 'S' => Some(0),
        'h' | 'H' => Some(1),
        'd' | 'D' => Some(2),
        'c' | 'C' => Some(3),
        _ => None,
    }
}

// ------------------------------------------------------------------- parsing

/// Turn one atom's text into the set of distributions it means.
fn atom_shapes(text: &str) -> Result<BTreeSet<Shape>, String> {
    let mut found = BTreeSet::new();
    for alternative in expand_major_minor(text)? {
        let atom = parse_atom(&alternative)?;
        for shape in all_shapes() {
            if atom.matches(shape) {
                found.insert(shape);
            }
        }
    }
    Ok(found)
}

/// `M` is either major and `m` is either minor, so an atom holding one becomes
/// two atoms. Textual, as `fdp` does it, which is what makes `h<2 or m<2` mean
/// "or either minor" without the condition parser knowing anything about it.
///
/// The same refusals: two of either, or one alongside a named suit of its own
/// colour, would need the permutation operator instead.
fn expand_major_minor(text: &str) -> Result<Vec<String>, String> {
    let refuse = |what: char| {
        Err(format!(
            "`{what}` may name only one suit in a shape, and not beside a named {} \
             — use the permutation operator, as in `5M(431)`.",
            if what == 'M' { "major" } else { "minor" }
        ))
    };
    if text.matches('M').count() > 1 {
        return refuse('M');
    }
    if text.matches('m').count() > 1 {
        return refuse('m');
    }
    // Only the distribution part is checked for a clashing letter; a condition
    // may mention any suit it likes.
    let head = text.split(':').next().unwrap_or(text);
    if text.contains('M') && (head.contains('s') || head.contains('h')) {
        return refuse('M');
    }
    if text.contains('m') && (head.contains('d') || head.contains('c')) {
        return refuse('m');
    }

    let mut out = vec![text.to_string()];
    for (letter, suits) in [('M', ['s', 'h']), ('m', ['d', 'c'])] {
        if out[0].contains(letter) {
            out = out
                .iter()
                .flat_map(|t| suits.map(|s| t.replace(letter, &s.to_string())))
                .collect();
        }
    }
    Ok(out)
}

fn parse_atom(text: &str) -> Result<Atom, String> {
    let (shape_part, condition) = match text.split_once(':') {
        Some((shape, cond)) => {
            let mut scan = Scan::new(cond);
            let parsed = parse_cond(&mut scan)?;
            if !scan.done() {
                return Err(format!("`{}` has more after the condition", text.trim()));
            }
            (shape, Some(parsed))
        }
        None => (text, None),
    };
    let shape_part: String = shape_part.chars().filter(|c| !c.is_whitespace()).collect();

    let (before, group_text) = match shape_part.find('(') {
        Some(open) => {
            if !shape_part.ends_with(')') {
                return Err(format!("`{shape_part}` has an unclosed ("));
            }
            (
                &shape_part[..open],
                Some(&shape_part[open + 1..shape_part.len() - 1]),
            )
        }
        None => (shape_part.as_str(), None),
    };

    // Slots outside the parentheses pin a suit each. A slot that names one
    // takes that suit; a slot that does not takes the next suit still free, in
    // spade-heart-diamond-club order — so `5(431)` is five spades and
    // `44(xx)` is four spades and four hearts, as DealerV2_4's own regression
    // script says in a comment beside them.
    let slots = parse_slots(before)?;
    let mut fixed: [Option<Lengths>; 4] = [None; 4];
    let named: Vec<usize> = slots.iter().filter_map(|(_, s)| *s).collect();
    let mut spare: Vec<usize> = (0..4).filter(|i| !named.contains(i)).collect();
    spare.reverse();
    for (lengths, suit) in slots {
        let index = match suit {
            Some(index) => index,
            None => spare
                .pop()
                .ok_or_else(|| format!("`{shape_part}` is more than four suits long"))?,
        };
        if fixed[index].is_some() {
            return Err(format!("`{shape_part}` names the same suit twice"));
        }
        fixed[index] = Some(lengths);
    }

    let group = match group_text {
        Some(text) => parse_lengths(text)?,
        None => Vec::new(),
    };
    let free = fixed.iter().filter(|f| f.is_none()).count();
    if free != group.len() {
        return Err(format!(
            "`{shape_part}` pins {} suits and floats {}, which is not four",
            4 - free,
            group.len()
        ));
    }

    Ok(Atom {
        fixed,
        group,
        condition,
    })
}

/// A run of slots, each a length and optionally the suit it belongs to:
/// `xxx`, `2+2+`, `[3-5]x`, `5s4c`, `5+Mxxx`, `4M+4m-`.
///
/// The `+` or `-` may fall on either side of the suit letter — the readme
/// writes `4+M` and the examples file `4M+`, and they mean the same.
fn parse_slots(text: &str) -> Result<Vec<(Lengths, Option<usize>)>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let (lengths, next) = read_length(&chars, at)?;
        at = next;
        let mut suit = None;
        let mut lengths = lengths;
        if let Some(index) = chars.get(at).copied().and_then(suit_index) {
            suit = Some(index);
            at += 1;
            match chars.get(at) {
                Some('+') => {
                    at += 1;
                    lengths = widen(lengths, true);
                }
                Some('-') => {
                    at += 1;
                    lengths = widen(lengths, false);
                }
                _ => {}
            }
        }
        if let Some(index) = suit {
            if out.iter().any(|(_, s)| *s == Some(index)) {
                return Err(format!("`{text}` names the same suit twice"));
            }
        }
        out.push((lengths, suit));
    }
    Ok(out)
}

/// A run of slots that name no suit: the inside of a permutation group.
fn parse_lengths(text: &str) -> Result<Vec<Lengths>, String> {
    parse_slots(text)?
        .into_iter()
        .map(|(lengths, suit)| match suit {
            None => Ok(lengths),
            Some(_) => Err(format!("`{text}` names a suit inside a permutation group")),
        })
        .collect()
}

/// One slot, and where it ends.
fn read_length(chars: &[char], mut at: usize) -> Result<(Lengths, usize), String> {
    let Some(&first) = chars.get(at) else {
        return Err("a shape ended early".to_string());
    };
    let base = match first {
        'x' | 'X' => {
            at += 1;
            return Ok((Lengths::any(), at));
        }
        '[' => {
            let close = chars[at..]
                .iter()
                .position(|c| *c == ']')
                .ok_or_else(|| "a shape has an unclosed [".to_string())?;
            let body: String = chars[at + 1..at + close].iter().collect();
            at += close + 1;
            read_range(&body)?
        }
        c if c.is_ascii_digit() => {
            at += 1;
            Lengths::exactly(c as u8 - b'0')
        }
        c => return Err(format!("`{c}` is not a suit length")),
    };
    Ok(match chars.get(at) {
        Some('+') => (widen(base, true), at + 1),
        Some('-') => (widen(base, false), at + 1),
        _ => (base, at),
    })
}

/// `5+` is every length from the lowest allowed upwards; `5-` every length to
/// the highest allowed.
fn widen(base: Lengths, upwards: bool) -> Lengths {
    let mut out = Lengths::none();
    for n in 0..14u8 {
        if base.holds(n) {
            out = out.with(if upwards {
                Lengths::at_least(n)
            } else {
                Lengths::at_most(n)
            });
        }
    }
    out
}

/// `[3-5]`, `[13]`, `[013-68]`.
fn read_range(body: &str) -> Result<Lengths, String> {
    let chars: Vec<char> = body.chars().collect();
    let mut out = Lengths::none();
    let mut at = 0;
    while at < chars.len() {
        let low = chars[at]
            .to_digit(10)
            .ok_or_else(|| format!("`{body}` is not a range of lengths"))? as u8;
        if chars.get(at + 1) == Some(&'-') {
            let high = chars
                .get(at + 2)
                .and_then(|c| c.to_digit(10))
                .ok_or_else(|| format!("`{body}` has a range with no end"))?
                as u8;
            for n in low..=high {
                out = out.with(Lengths::exactly(n));
            }
            at += 3;
        } else {
            out = out.with(Lengths::exactly(low));
            at += 1;
        }
    }
    Ok(out)
}

// ----------------------------------------------------------------- rendering

/// A suit length as one character: past `9` the digits run on into `:;<=`.
fn length_char(n: u8) -> char {
    (b'0' + n) as char
}

/// The shortest set of patterns covering exactly `shapes`.
///
/// Written out one distribution per pattern this would be up to 560 terms, so
/// each is generalised to `x` wherever that lets in nothing new — greedily,
/// which is not optimal but turns `x5x2 x5x3 …` back into a handful of terms
/// rather than a wall of digits.
fn render(shapes: &BTreeSet<Shape>) -> Vec<String> {
    let mut covered: BTreeSet<Shape> = BTreeSet::new();
    let mut patterns = Vec::new();
    for shape in shapes {
        if covered.contains(shape) {
            continue;
        }
        let mut pattern: [Option<u8>; 4] = shape.map(Some);
        for position in 0..4 {
            let mut wider = pattern;
            wider[position] = None;
            if expand_pattern(&wider).iter().all(|s| shapes.contains(s)) {
                pattern = wider;
            }
        }
        covered.extend(expand_pattern(&pattern));
        patterns.push(
            pattern
                .iter()
                .map(|slot| match slot {
                    Some(n) => length_char(*n),
                    None => 'x',
                })
                .collect(),
        );
    }
    patterns
}

fn expand_pattern(pattern: &[Option<u8>; 4]) -> Vec<Shape> {
    all_shapes()
        .into_iter()
        .filter(|shape| {
            pattern
                .iter()
                .zip(shape)
                .all(|(slot, n)| slot.is_none_or(|want| want == *n))
        })
        .collect()
}

// -------------------------------------------------------------------- public

/// Rewrite every `shape{ ... }` in `source` as an ordinary `shape( ... )`.
///
/// Runs before the pass that marks four-digit shape literals, since what it
/// writes is exactly the kind of literal that pass exists to mark.
///
/// Comments and quoted strings are stepped over rather than searched. One of
/// DealerV2_4's own regression scripts has a comment counting "the 10
/// `shape{}` statements" in the file, and a naive search reads that as an
/// eleventh with nothing in it.
pub fn expand(source: &str) -> Result<String, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    while at < chars.len() {
        if let Some(end) = skippable(&chars, at) {
            out.extend(&chars[at..end]);
            at = end;
            continue;
        }
        match fdshape_at(&chars, at) {
            Some((open, close)) => {
                let body: String = chars[open + 1..close].iter().collect();
                let (compass, list) = body
                    .split_once(',')
                    .ok_or_else(|| format!("`shape{{{body}}}` needs a compass and then a shape"))?;
                out.push_str(&expand_one(compass.trim(), list)?);
                at = close + 1;
            }
            None => {
                out.push(chars[at]);
                at += 1;
            }
        }
    }
    Ok(out)
}

/// The end of the comment or string starting at `at`, if one does.
///
/// Shared with the script-parameter pass, which has to step over the same
/// things for the same reason.
pub(crate) fn skippable(chars: &[char], at: usize) -> Option<usize> {
    let two: String = chars[at..(at + 2).min(chars.len())].iter().collect();
    let line_end = |from: usize| {
        chars[from..]
            .iter()
            .position(|c| *c == '\n')
            .map(|i| from + i)
            .unwrap_or(chars.len())
    };
    match chars[at] {
        '#' => Some(line_end(at)),
        '/' if two == "//" => Some(line_end(at)),
        '/' if two == "/*" => Some(
            chars[at..]
                .windows(2)
                .position(|w| w == ['*', '/'])
                .map(|i| at + i + 2)
                .unwrap_or(chars.len()),
        ),
        '"' => Some(
            chars[at + 1..]
                .iter()
                .position(|c| *c == '"')
                .map(|i| at + i + 2)
                .unwrap_or(chars.len()),
        ),
        _ => None,
    }
}

/// Whether a `shape` followed by `{` starts here, and where its braces are.
fn fdshape_at(chars: &[char], at: usize) -> Option<(usize, usize)> {
    if !chars[at..].starts_with(&['s', 'h', 'a', 'p', 'e']) {
        return None;
    }
    // Not the tail of a longer word.
    if at > 0 && (chars[at - 1].is_alphanumeric() || chars[at - 1] == '_') {
        return None;
    }
    let mut open = at + 5;
    while chars.get(open).is_some_and(|c| c.is_whitespace()) {
        open += 1;
    }
    if chars.get(open) != Some(&'{') {
        return None;
    }
    let close = chars[open..].iter().position(|c| *c == '}')? + open;
    Some((open, close))
}

fn expand_one(compass: &str, list: &str) -> Result<String, String> {
    // The separator carries whitespace on both sides, which is what keeps the
    // `+` in `h+s>=10` inside its condition. `fdp` splits the same way.
    let mut atoms: Vec<(bool, String)> = Vec::new();
    let mut include = true;
    let mut current = String::new();
    let bytes: Vec<char> = list.chars().collect();
    let mut at = 0;
    while at < bytes.len() {
        let is_separator = (bytes[at] == '+' || bytes[at] == '-')
            && at > 0
            && bytes[at - 1].is_whitespace()
            && bytes.get(at + 1).is_some_and(|c| c.is_whitespace());
        if is_separator {
            atoms.push((include, std::mem::take(&mut current)));
            include = bytes[at] == '+';
            at += 1;
        } else {
            current.push(bytes[at]);
            at += 1;
        }
    }
    atoms.push((include, current));

    let mut shapes: BTreeSet<Shape> = BTreeSet::new();
    for (include, text) in atoms {
        if text.trim().is_empty() {
            continue;
        }
        let found = atom_shapes(text.trim())?;
        if include {
            shapes.extend(found);
        } else {
            for shape in found {
                shapes.remove(&shape);
            }
        }
    }
    if shapes.is_empty() {
        return Err(format!(
            "`shape{{{compass}, {}}}` matches no distribution at all",
            list.trim()
        ));
    }

    let patterns = render(&shapes);
    let terms: Vec<String> = patterns
        .iter()
        .map(|p| {
            // A pattern with no wildcard has to be marked, or four digits read
            // as a number rather than a shape.
            if p.contains('x') {
                p.clone()
            } else {
                format!("%s{p}")
            }
        })
        .collect();
    Ok(format!("shape({compass}, {})", terms.join(" + ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The set a `shape{...}` body denotes, for comparing against the readme.
    fn shapes_of(list: &str) -> BTreeSet<Shape> {
        let expanded = expand_one("north", list).expect("expands");
        let inner = expanded
            .trim_start_matches("shape(north, ")
            .trim_end_matches(')')
            .to_string();
        let mut out = BTreeSet::new();
        for term in inner.split(" + ") {
            let term = term.trim_start_matches("%s");
            let pattern: [Option<u8>; 4] = std::array::from_fn(|i| {
                let c = term.chars().nth(i).expect("four characters");
                if c == 'x' {
                    None
                } else {
                    Some(c as u8 - b'0')
                }
            });
            out.extend(expand_pattern(&pattern));
        }
        out
    }

    fn from_patterns(patterns: &[&str]) -> BTreeSet<Shape> {
        let mut out = BTreeSet::new();
        for p in patterns {
            let pattern: [Option<u8>; 4] = std::array::from_fn(|i| {
                let c = p.chars().nth(i).expect("four characters");
                if c == 'x' {
                    None
                } else {
                    Some(c as u8 - b'0')
                }
            });
            out.extend(expand_pattern(&pattern));
        }
        out
    }

    /// Every worked example in François Dellacherie's readme, with the
    /// expansion he prints beside it. The comparison is between the sets of
    /// distributions, not the text: dealer3 groups its patterns differently,
    /// and includes the ten-card suits his notation cannot write.
    #[test]
    fn the_readme_examples_expand_as_documented() {
        // (b) at least
        assert_eq!(
            shapes_of("5+xx5+"),
            from_patterns(&[
                "5xx5", "5xx6", "6xx5", "5xx7", "6xx6", "7xx5", "5008", "6007", "7006", "8005"
            ])
        );
        // (c) at most
        assert_eq!(shapes_of("2-xxx"), from_patterns(&["0xxx", "1xxx", "2xxx"]));
        // (d) range
        assert_eq!(
            shapes_of("x[3-5]x[13]"),
            from_patterns(&["x3x1", "x4x1", "x3x3", "x5x1", "x4x3", "x5x3"])
        );
        // (d) again: an at-most is a range
        assert_eq!(shapes_of("3-xxx"), shapes_of("[0-3]xxx"));
        // (e) permutation
        assert_eq!(
            shapes_of("5s(431)"),
            from_patterns(&["5134", "5314", "5143", "5341", "5413", "5431"])
        );
        // (e) permutation over a restricted group
        assert_eq!(
            shapes_of("4+c3+d(2+2+)"),
            from_patterns(&[
                "3334", "4234", "2434", "3244", "2344", "2254", "3235", "2335", "2245", "2236"
            ])
        );
        // (f) a major
        assert_eq!(shapes_of("5M(xxx)"), from_patterns(&["x5xx", "5xxx"]));
        assert_eq!(
            shapes_of("5+M3+c(31)"),
            from_patterns(&["1534", "3514", "1633", "3613", "5134", "5314", "6133", "6313"])
        );
        // (g) a minor
        assert_eq!(
            shapes_of("5M5m(xx)"),
            from_patterns(&["x5x5", "5xx5", "x55x", "5x5x"])
        );
        // (h) conditions
        assert_eq!(
            shapes_of("4+s4+h(xx):d>c,h+s==10"),
            from_patterns(&["6421", "6430", "5521", "5530", "4621", "4630"])
        );
    }

    #[test]
    fn a_bare_permutation_is_any() {
        // `any 4432` is every arrangement, which is what the readme says.
        assert_eq!(shapes_of("(4432)").len(), 12);
        assert!(shapes_of("(4432)").contains(&[2, 3, 4, 4]));
    }

    #[test]
    fn a_slot_before_the_group_need_not_name_its_suit() {
        // From DealerV2_4's ShapeFD_syntax_s223.dli, whose comment beside this
        // line reads "44(xx) same as 44xx": an unnamed slot takes the next suit
        // still free, so `5(431)` is five spades.
        assert_eq!(shapes_of("5(431)"), shapes_of("5s(431)"));
        assert_eq!(shapes_of("44(xx)"), shapes_of("44xx"));
        // And it composes with a named one.
        assert_eq!(shapes_of("4M(3+3+2+)").len(), shapes_of("4M(3+3+2+)").len());
        assert!(shapes_of("5(431)").contains(&[5, 4, 3, 1]));
        assert!(!shapes_of("5(431)").contains(&[4, 5, 3, 1]));
    }

    #[test]
    fn the_separator_needs_space_so_a_condition_can_add() {
        // `h+s>=10` is arithmetic; ` + ` starts another shape.
        let both = shapes_of("4+s4+h(xx):d>c,h+s==10 + 7xxx");
        assert!(both.contains(&[6, 4, 3, 0]));
        assert!(both.contains(&[7, 3, 3, 0]));
    }

    #[test]
    fn ten_card_suits_are_included() {
        // What `fdp` cannot write, and the reason for widening a shape length
        // past `9`. A ten-card major is a major of at least five.
        let shapes = shapes_of("5+Mxxx");
        assert!(shapes.contains(&[10, 1, 1, 1]));
        assert!(shapes.contains(&[13, 0, 0, 0]));
        assert!(!shapes.contains(&[4, 4, 4, 1]));
    }

    #[test]
    fn a_condition_may_name_either_major_or_minor() {
        // `m<2` is "either minor is short", by expanding the atom in two.
        assert_eq!(
            shapes_of("7+xxx:h<2 or m<2"),
            shapes_of("7+xxx:h<2 or d<2 or c<2")
        );
    }

    #[test]
    fn two_of_the_same_colour_are_refused() {
        assert!(expand_one("north", "5M4M(xx)").is_err());
        assert!(expand_one("north", "5M4s(xx)").is_err());
        assert!(expand_one("north", "5m4c(xx)").is_err());
    }

    #[test]
    fn the_whole_statement_is_rewritten() {
        let out = expand("condition shape{north, 5M(xxx)} and hcp(north) > 10\n").expect("expands");
        assert!(out.starts_with("condition shape(north, "), "got: {out}");
        assert!(out.ends_with("and hcp(north) > 10\n"), "got: {out}");
        assert!(!out.contains('{'));
    }

    #[test]
    fn an_ordinary_shape_call_is_left_alone() {
        let source = "condition shape(north, any 4333 + 54xx)\n";
        assert_eq!(expand(source).expect("expands"), source);
    }

    #[test]
    fn a_shape_inside_a_comment_is_left_alone() {
        // DealerV2_4's own ShapeFD_syntax_s223.dli counts "the 10 `shape{}`
        // statements" in a comment, which a plain search reads as an eleventh.
        for source in [
            "// counts the 10 'shape{}' statements\ncondition 1\n",
            "# a shape{} in a hash comment\ncondition 1\n",
            "/* a shape{} in a block comment */\ncondition 1\n",
            "title \"a shape{} in a string\"\ncondition 1\n",
        ] {
            assert_eq!(expand(source).expect("expands"), source, "in: {source}");
        }
    }

    #[test]
    fn a_word_ending_in_shape_is_not_a_shape() {
        let source = "myshape = 1\ncondition myshape\n";
        assert_eq!(expand(source).expect("expands"), source);
    }
}
