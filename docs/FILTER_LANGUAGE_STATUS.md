# The dealer3 script language

What dealer3's script language accepts, and where it still differs from the
original dealer.

## The tables are generated

Every table below is rendered from `dealer-parser/src/vocabulary.rs`, which is
itself checked against `grammar.pest`. So this document cannot claim a function
the parser rejects, or miss one it accepts.

That guarantee is here because the hand-maintained version of this page had
drifted badly: its summary listed `tricks`, `score` and `imps` as working while
its "Evaluator Limitations" section said "No double-dummy analysis" and "No
scoring functions", and its "Next Steps" listed all three as still to do. They
had been implemented for months.

```bash
cargo test -p dealer                  # verifies this file
UPDATE_DOCS=1 cargo test -p dealer    # rewrites the tables
```

The same tables drive https://dealer.bridge-classroom.org/reference.html, which
reads them out of the WebAssembly build at runtime.

Descriptions were written from the evaluator and checked against the original C
sources — `c4.c` for `quality` and `cccc`, `pointcount.c` for the honour counts,
`dealer.c` for losers — and against
[Henk Uijterwaal's manual](https://www.bridgebase.com/tools/dealer/Manual/input.html).
`quality`, `cccc`, `losers`, `c13`, `controls` and `top5` are pinned in
`dealer-eval/tests/doc_examples_evaluate.rs` to the values dealer.exe produces
for the same predealt deal.

## Functions

<!-- BEGIN GENERATED: functions -->

**24 functions**, under 39 spellings — the extra 15 are alternative names, listed with the function they stand for.

### Hand evaluation

| Function | What it computes | Example |
|---|---|---|
| `hcp(compass)  ·  hcp(compass, suit)` | High card points on the 4-3-2-1 scale: ace 4, king 3, queen 2, jack 1. With a suit, only that suit's cards are counted. | `hcp(north) >= 12 && hcp(north, spades) >= 4` |
| | The original dealer can re-scale this with a `pointcount` statement. dealer3 has no such statement, so the 4-3-2-1 scale is fixed. | |
| `controls(compass)  ·  controls(compass, suit)` | Controls: each ace counts 2 and each king 1. | `controls(north) >= 5` |
| | The original dealer also accepts the singular `control`. dealer3 does not. | |
| `losers(compass)  ·  losers(compass, suit)` | Losing trick count: a void is 0; a singleton is 0 holding the ace and 1 otherwise; a doubleton is 0 holding A-K, 1 holding the ace or the king and 2 otherwise; three cards or more is 3 minus the number of A, K and Q held. | `losers(south) <= 6` |
| `loser(compass)  ·  loser(compass, suit)` | Another spelling of `losers` | `loser(south) <= 6` |
| `quality(compass, suit)` | Quality of one suit, by the algorithm published in The Bridge World, October 1982, multiplied by 100 — so 450 means 4.50. | `quality(north, spades) >= 400` |
| | Each honour is worth a multiple of ten times the suit length — ace 4×, king 3×, queen 2×, jack 1× — with an extra allowance for length beyond six cards, and for the ten and nine when they are supported. dealer3's implementation follows the original `c4.c` line for line, and was checked against dealer.exe's own output. | |
| `cccc(compass)` | Whole-hand evaluation by the algorithm published in The Bridge World, October 1982, multiplied by 100 — a minimum opening bid is around 1200. | `cccc(north) >= 1200` |
| | Honours are valued by suit with penalties for short or unsupported ones, each suit's `quality` is added, and short suits contribute shape points. dealer3's implementation follows the original `c4.c` line for line, and was checked against dealer.exe's own output. | |

### Suit length

| Function | What it computes | Example |
|---|---|---|
| `spades(compass)` | Number of spades held. | `spades(north) + spades(south) >= 8` |
| `hearts(compass)` | Number of hearts held. | `hearts(north) + hearts(south) >= 8` |
| `diamonds(compass)` | Number of diamonds held. | `diamonds(west) >= 6` |
| `clubs(compass)` | Number of clubs held. | `clubs(east) <= 2` |
| `spade(compass)` | Another spelling of `spades` | `spade(north) >= 5` |
| `heart(compass)` | Another spelling of `hearts` | `heart(north) >= 5` |
| `diamond(compass)` | Another spelling of `diamonds` | `diamond(north) >= 5` |
| `club(compass)` | Another spelling of `clubs` | `club(north) >= 5` |

### Shape and cards

| Function | What it computes | Example |
|---|---|---|
| `shape(compass, pattern)` | True when the hand matches the pattern. Four digits are lengths in spades, hearts, diamonds, clubs order; `x` matches any length; `any` allows the suits in any order; `+` adds a pattern and `-` excludes one. | `shape(north, any 4333 + any 4432 + any 5332)` |
| | Matching is a table lookup over all 560 shapes, so a long pattern list costs no more than a short one. | |
| `hascard(compass, card)` | True when the hand holds exactly that card, written rank then suit — `TC` is the ten of clubs. | `hascard(east, TC) && hascard(east, AS)` |

### Honour counts

| Function | What it computes | Example |
|---|---|---|
| `tens(compass)  ·  tens(compass, suit)` | Number of tens held. | `tens(north) >= 2` |
| `jacks(compass)  ·  jacks(compass, suit)` | Number of jacks held. | `jacks(north) >= 2` |
| `queens(compass)  ·  queens(compass, suit)` | Number of queens held. | `queens(north) >= 2` |
| `kings(compass)  ·  kings(compass, suit)` | Number of kings held. | `kings(north) >= 2` |
| `aces(compass)  ·  aces(compass, suit)` | Number of aces held. | `aces(north) >= 2` |
| `top2(compass)  ·  top2(compass, suit)` | Number of the top two honours held: ace, king. | `top2(north, spades) == 2` |
| `top3(compass)  ·  top3(compass, suit)` | Number of the top three honours held: ace, king, queen. | `top3(north, hearts) >= 2` |
| `top4(compass)  ·  top4(compass, suit)` | Number of the top four honours held: ace, king, queen, jack. | `top4(north, hearts) >= 3` |
| `top5(compass)  ·  top5(compass, suit)` | Number of the top five honours held: ace, king, queen, jack, ten. | `top5(east, spades) >= 3` |
| `c13(compass)  ·  c13(compass, suit)` | C13 points: ace 6, king 4, queen 2, jack 1. | `c13(north) >= 18` |
| `pt0(compass)  ·  pt0(compass, suit)` | Another spelling of `tens` | `pt0(north) >= 2` |
| `pt1(compass)  ·  pt1(compass, suit)` | Another spelling of `jacks` | `pt1(north) >= 2` |
| `pt2(compass)  ·  pt2(compass, suit)` | Another spelling of `queens` | `pt2(north) >= 2` |
| `pt3(compass)  ·  pt3(compass, suit)` | Another spelling of `kings` | `pt3(north) >= 2` |
| `pt4(compass)  ·  pt4(compass, suit)` | Another spelling of `aces` | `pt4(north) >= 2` |
| `pt5(compass)  ·  pt5(compass, suit)` | Another spelling of `top2` | `pt5(north, spades) == 2` |
| `pt6(compass)  ·  pt6(compass, suit)` | Another spelling of `top3` | `pt6(north, spades) >= 2` |
| `pt7(compass)  ·  pt7(compass, suit)` | Another spelling of `top4` | `pt7(north, spades) >= 3` |
| `pt8(compass)  ·  pt8(compass, suit)` | Another spelling of `top5` | `pt8(north, spades) >= 3` |
| `pt9(compass)  ·  pt9(compass, suit)` | Another spelling of `c13` | `pt9(north) >= 18` |

### Double-dummy and scoring

| Function | What it computes | Example |
|---|---|---|
| `tricks(compass, strain)` | Tricks that compass takes as declarer in that strain with every hand seen — the double-dummy result. Strain is a suit name, or a number: 0 clubs, 1 diamonds, 2 hearts, 3 spades, 4 notrump. | `tricks(south, spades) >= 10` |
| | Notrump has to be written as the number 4. The original dealer's `notrumps` keyword is not part of dealer3's grammar and is rejected outright — it used to be read as a variable, and since an unset variable is 0 and 0 means clubs, the script quietly asked about the wrong strain. Solving a deal is far slower than any other function here, so a script using `tricks` wants a tight `condition` ahead of it. | |
| `score(vulnerable, contract, tricks)` | Declarer's score for a contract played at that vulnerability and making that many tricks. `vulnerable` is 0 or 1; `contract` is level × 10 + strain, plus 100 if doubled or 200 if redoubled; `tricks` is 0 to 13. | `score(0, 34, 9) == 400` |
| | Strain digits match `tricks`: 0 clubs, 1 diamonds, 2 hearts, 3 spades, 4 notrump. So 34 is 3NT, 43 is four spades, 143 is four spades doubled and 243 redoubled. The original dealer writes the contract as a token such as `3N`; dealer3 requires the number. | |
| `imps(scoredifference)` | Converts a difference between two scores into IMPs, by the standard table. | `imps(score(0, 43, 10) - score(0, 34, 9)) >= 1` |
<!-- END GENERATED: functions -->

## Operators

<!-- BEGIN GENERATED: operators -->

Tightest binding first. Operators sharing a level are applied left to right.

| Level | Operator | Also | What it does |
|---|---|---|---|
| 1 | `!` | `not` | True when its operand is zero, and false otherwise. |
| 2 | `*` |  | Multiplication. |
| 2 | `/` |  | Division. Whole numbers throughout, so the remainder is discarded. |
| 2 | `%` |  | Remainder after division. |
| 3 | `+` |  | Addition. |
| 3 | `-` |  | Subtraction, and negation when written in front of a single value. |
| 4 | `<` |  | Less than. |
| 4 | `<=` |  | Less than or equal to. |
| 4 | `>` |  | Greater than. |
| 4 | `>=` |  | Greater than or equal to. |
| 4 | `==` |  | Equal to. Note the two signs: a single `=` assigns a variable instead. |
| 4 | `!=` |  | Not equal to. |
| 5 | `&&` | `and` | True when both sides are true. The right side is skipped when the left is false. |
| 6 | `\|\|` | `or` | True when either side is true. The right side is skipped when the left is true. |
| 7 | `?` |  | First half of the three-way choice `test ? when_true : when_false`. |
| 7 | `:` |  | Second half of the three-way choice `test ? when_true : when_false`. |
| 8 | `=` |  | Gives a name to an expression. This is a statement, not something that can appear inside a larger expression. |
<!-- END GENERATED: operators -->

## Statements

<!-- BEGIN GENERATED: statements -->

| Statement | What it does | Example |
|---|---|---|
| `condition <expression>` | Keep a deal when the expression is anything other than zero. | `condition hcp(north) >= 15 && shape(north, any 5332)` |
| `produce <number>` | Stop once this many deals have matched. | `produce 25` |
| `generate <number>` | Stop after dealing this many hands, however few matched. | `generate 100000` |
| `action <action>, <action>, ...` | What to do with each matching deal: a print format, and any averages or frequencies to accumulate. | `action printoneline, average "hcp" hcp(north)` |
| `average ["label"] <expression>` | Report the mean of the expression over the deals that matched. | `average "north hcp" hcp(north)` |
| `frequency ["label"] (<expression>, <low>, <high>)` | Report a histogram of the expression over the deals that matched, counting from low to high inclusive. | `frequency "north hcp" (hcp(north), 10, 20)` |
| `dealer <compass>` | Records who dealt. Affects the output only, never which deals are produced. | `dealer south` |
| `vulnerable none \| ns \| ew \| all` | Records the vulnerability. Affects the output only, never which deals are produced. | `vulnerable ns` |
| `predeal <compass> <holding>, <holding>, ...` | Places cards in a hand before shuffling; the rest of the deal is dealt around them. A holding is a suit letter followed by its ranks, using T for the ten. | `predeal north SAKQ,HT98` |
| `csvrpt(<term>, <term>, ...)` | Writes one comma-separated row per matching deal. A term is an expression, a quoted string, a compass for that hand, `ns` or `ew` for a partnership's two hands, or the word `deal` for all four. | `csvrpt(deal, hcp(north), "north")` |
| `<name> = <expression>` | Names an expression so a long condition can be written in pieces. The name stands for the expression and is worked out afresh for every deal. | `fit = spades(north) + spades(south)` |
| `<expression>` | An expression on its own is the condition, so the `condition` keyword can be left off. | `hcp(north) >= 20` |

### Actions

| Action | What it prints |
|---|---|
| `printall` | All four hands, laid out around the compass. This is what happens with no action given. |
| `printew` | East and West only, West on the left. |
| `printpbn` | PBN, the record format other bridge programs read. |
| `printcompact` | Four lines per deal. |
| `printoneline` | One line per deal. |
<!-- END GENERATED: statements -->

## Not supported

Words the original dealer accepts that dealer3 does not. Each is **reserved in
the grammar**, so using one is a syntax error rather than something that quietly
changes what a script means — see the note on #15 below.

<!-- BEGIN GENERATED: not-supported -->

| Word | Instead |
|---|---|
| `notrump` | In `tricks`, write notrump as the number 4. |
| `notrumps` | In `tricks`, write notrump as the number 4. |
| `control` | Write `controls`. |
| `hcps` | Write `hcp`. |
| `ten` | Write `tens`, or `pt0`. |
| `jack` | Write `jacks`, or `pt1`. |
| `queen` | Write `queens`, or `pt2`. |
| `king` | Write `kings`, or `pt3`. |
| `ace` | Write `aces`, or `pt4`. |
| `trick` | Write `tricks`. |
| `imp` | Write `imps`. |
| `pointcount` | There is no way to re-scale the high card points; `hcp` is always 4-3-2-1. |
| `altcount` | The `pt0`..`pt9` counts are fixed and cannot be redefined. |
| `print` | Use `printall`, or one of the other print actions. |
| `printes` | There is no expression-printing action; use `average` or `csvrpt`. |
| `rnd` | There is no random function inside the language. |
<!-- END GENERATED: not-supported -->

## Where dealer3 still differs

Beyond the words above, these are the known behavioural differences. None is
tracked by a generated table, so this section is maintained by hand.

| Difference | Status |
|---|---|
| Singular/plural spellings (`control`, `hcps`, `jack`, `trick`, …) and `notrump` are not accepted | Rejected loudly; implementing them is [#15](https://github.com/bridge-craftwork/Dealer3/issues/15) |
| `pointcount` and `altcount` cannot re-scale the point counts | Rejected loudly; implementing them is [#15](https://github.com/bridge-craftwork/Dealer3/issues/15) |
| `score` takes a numeric contract code, not a token like `3N` | Documented, no issue |
| `frequency` has no two-dimensional form | Documented, no issue |
| `predeal` has no length-bias form (`spades(north) == 5`) | Documented, no issue |
| Variables whose name begins with a statement keyword are rejected | [#12](https://github.com/bridge-craftwork/Dealer3/issues/12) |
| `tricks()` is unusably slow — minutes per solve | [#14](https://github.com/bridge-craftwork/Dealer3/issues/14) |

## Variables

Variables are **evaluated per deal, not expanded as text**. A name stands for an
expression, and that expression is worked out afresh for every deal, so it
responds to the hand in front of it:

```
nt_opener = hcp(north) >= 15 && hcp(north) <= 17 && shape(north, any 4333 + any 4432 + any 5332)
weak = hcp(south) <= 8
condition nt_opener && weak
```

Variables may refer to other variables. A result is cached for the duration of
one deal, so referring to the same variable twice costs one evaluation — with
the exception noted in [#14](https://github.com/bridge-craftwork/Dealer3/issues/14),
where a variable holding a `tricks()` call is re-solved per reference.

## Comments

`#` and `//` to end of line, `/* */` across lines. A `# key: value` line at the
start of a file is treated as a header by the web editor's highlighting, which
is how the Practice-Bidding-Scenarios scripts carry their metadata.

## Related documents

- `command_line_comparison.md` — the switch table, generated the same way
- `WASM.md` — the `language_info()` payload these tables come from
- `../web/README.md` — how the reference page is built
- `CHANGELOG.md` — the 0.2.0 breaking change to `-v`
