# Levelling a scenario: a guide

A practice scenario deals the hands nature offers. A balanced South is weak far
more often than strong, so a script asking for "the whole notrump range" spends
most of a session on 12-14 and shows 22-24 about once a fortnight.

**Levelling** discards some of the common hands so the mix comes out as
intended. This is how to use it, from either front end.

- [`docs/leveling-strategy.md`](leveling-strategy.md) is the method and the
  measurements behind it — the arithmetic, the cost formula, why `rnd()` beats
  the old spot-card ladder. Read it when you want to know *why*. This page is
  the shorter companion, and says *how*.
- [`examples/`](../examples/) holds a real scenario and its generated pair, if
  you would rather read one than a description.

---

## Contents

1. [The one thing to get right](#the-one-thing-to-get-right)
2. [Naming the hand types](#naming-the-hand-types)
3. [Levelling in the browser](#levelling-in-the-browser)
4. [Levelling from the command line](#levelling-from-the-command-line)
5. [Keeping the player-facing text honest](#keeping-the-player-facing-text-honest)
6. [Choosing a target mix, and what it costs](#choosing-a-target-mix-and-what-it-costs)
7. [Ordering the boards](#ordering-the-boards)
8. [What it refuses to do, and what to do about it](#what-it-refuses-to-do-and-what-to-do-about-it)
9. [When not to level](#when-not-to-level)

---

## The one thing to get right

**Measure over enough deals.** Everything else on this page is recoverable; this
is not.

A keep rate is `mix / natural` — the share you want, divided by the share nature
supplies. That divisor is *measured*, so an error in the measurement passes
straight into the keep, and the keep is then written into a file and used
forever. Producing more deals afterwards does not average it out, because there
is nothing random left to average: the wrong number is baked in.

Here is what that looks like, on this repo's own example. `22_24` is the rarest
band, about 1.29% of qualifying deals:

| type | natural @ 10k measured | natural @ 100k measured | keep from the 10k pass | what that band then delivers |
|---|---|---|---|---|
| 12_14 | 0.57430 | 0.58367 | 0.0277 | 21.0% |
| 15_17 | 0.29970 | 0.29189 | 0.0531 | 20.1% |
| 18_19 | 0.08010 | 0.08006 | 0.1985 | 20.6% |
| 20_21 | 0.03000 | 0.03153 | 0.5300 | 21.7% |
| **22_24** | **0.01590** | **0.01285** | **1.0000** | **16.7%** |

The ten-thousand-deal pass saw `22_24` 159 times where the true rate predicts
129 — a 24% overestimate, which is an ordinary result at 129 expected sightings.
Its keep comes out too small, and the band under-delivers by three and a third
points against a 20% target.

**And it stays wrong.** Observed in the browser at 18.4% over 10,000 produced
and 17.8% over 30,000. It converged rather than scattering, which is the
signature of a systematic error: sampling noise gets *smaller* with more deals
and a mis-measured keep does not move at all.

So the two front ends draw the line in different places:

|  | measures over | refuses below | why |
|---|---|---|---|
| **command line** | whatever `-p` says | **500 sightings** of any type | a build step can simply be told to measure over more, so a hard stop costs nothing and prevents a bad file |
| **browser** | **10,000 produced deals** | **50 sightings** | a page that refused would teach nothing; it goes ahead and reports the count it managed |

500 sightings is about ±4.5% on that band's rate, which is roughly a point on a
20% target. That is the bar for a file you are going to keep.

The browser's 10,000 is a second or two, and it is enough to *look* at a
levelling. It is not enough to *ship* one when a band is rare — 10,000 produced
deals is only 129 sightings of a 1.29% band. **The Hand types panel prints how
many times the rarest band was seen; read it.** Under a few hundred, generate
the file from the command line with a larger `-p` before it goes anywhere near a
class.

To measure a band of a given natural rate to a given relative precision:

| rarest band | ±5% | ±2% | ±1% |
|---|---|---|---|
| 5% | 7,600 | 47,500 | 190,000 |
| 2% | 19,600 | 122,500 | 490,000 |
| 1% | 39,600 | 247,500 | 990,000 |
| 0.5% | 79,600 | 497,500 | 1,990,000 |

`--write-leveled` reports where it landed, so this need not be worked out by
hand:

```
keeps pinned down by `22_24`, the rarest, seen 1285 times: +-2.8%
```

### And a short set is lumpy however even the keeps are

The second thing worth knowing before promising anything to a class. Levelling
sets the **long-run** mix. It does not make twelve boards come out even, and it
cannot.

A 24-board set drawn from a *perfectly* level five-band distribution has a
standard deviation of about **8 points per band**. Four boards of each is the
average; 6/1/5/4/4 is unremarkable. That lumpiness is in the sample the students
actually see, and no amount of measuring precision removes it.

What levelling does buy is coverage, and it buys a lot of it. For the NT ladder,
the chance a band appears at least once in a 12-board set:

| band | natural | levelled |
|---|---|---|
| 12-14 | 100.0% | 93.1% |
| 15-17 | 98.4% | 93.1% |
| 18-19 | 63.3% | 93.1% |
| 20-21 | 31.9% | 93.1% |
| **22-24** | **14.4%** | **93.1%** |

Unlevelled, a student practising twelve boards meets the 22-24 hand about one
session in seven. Levelled, six sessions in seven. That last row is the whole
argument for doing this.

If you need a *fixed* set to come out exactly even — a PBN generated once and
handed to a class — levelling is the wrong tool; see
[When not to level](#when-not-to-level).

---

## Naming the hand types

Levelling needs to know what the categories are. You say so by naming variables
with a `HandType_` prefix:

```
HandType_12_14 = hcp(south) >= 12 and hcp(south) <= 14
HandType_15_17 = hcp(south) >= 15 and hcp(south) <= 17
HandType_18_19 = hcp(south) >= 18 and hcp(south) <= 19
HandType_20_21 = hcp(south) >= 20 and hcp(south) <= 21
HandType_22_24 = hcp(south) >= 22 and hcp(south) <= 24
```

That is the entire interface. The rest is worked out.

**It is a naming convention, not syntax.** Nothing in the grammar treats these
variables specially, which is deliberate: a script using them still parses on
BBO and on the original dealer, and these scenarios have to run there. The cost
is that the parser cannot catch a misspelling — `HandTpye_20_21` is simply an
ordinary variable — so the types found are always reported back, in the
statistics, in `--stats-json`, and in the browser's Hand types panel. **Check
the count is the one you expected.**

The name after the prefix is the label used everywhere else: `HandType_12_14` is
the type `12_14`. Declaration order is kept, because it is the order you thought
in and the order an interleaved set walks through.

Two rules the types have to obey:

- **No overlaps.** Two types matching one deal is refused.
- **No gaps.** For levelling, the types must cover every deal the scenario
  produces, or the keeps will not add up — the tool checks the measured rates
  sum to 1.

Outside levelling, an untyped deal is fine: `[HandType "..."]` in PBN output and
the hand-type statistics both accept a deal that matched nothing.

### The generated block

The levelling itself is a block of generated lines. It is **optional to leave a
placeholder for it** — naming the hand types has already said everything the
levelling needs — but if you want to control where it lands, write one:

```
### BEGIN GENERATED LEVELING ###
noLeveling = 1
levelTheDeal = noLeveling
### END GENERATED LEVELING ###

condition
balanced and hcp(south) >= 12 and hcp(south) <= 24
and levelTheDeal
```

`levelTheDeal = noLeveling` rather than `levelTheDeal = 1` on purpose: `1` reads
as "yes, level this deal" when it means exactly the opposite, and a comment
saying so is read past. The stock file also *runs* as it stands, at its natural
mix, which is what the tool measures first. (`keepTheDeal` is accepted as the
verdict's name too, if you prefer the predicate reading.)

Without a placeholder, the block is written in above the condition and
`and levelTheDeal` is added to the condition itself. The condition is found with
the parser rather than by hunting for the keyword, because the original's grammar
lets a condition be a bare expression and most scenarios in the wild write it
that way.

What gets generated is one roll and one threshold per type:

```
roll = (rnd(1000) % 1000 + 1000) % 1000

level_12_14 = HandType_12_14 and roll < 22
level_15_17 = HandType_15_17 and roll < 44
level_18_19 = HandType_18_19 and roll < 161
level_20_21 = HandType_20_21 and roll < 408
level_22_24 = HandType_22_24
levelTheDeal = level_12_14 or level_15_17 or level_18_19 or level_20_21 or level_22_24
```

The double modulo in `roll` is not decoration — it is what makes the same
thresholds land on target on BBO, in dealer3 and on a locally built dealer
binary, whose `rnd()` returns values outside the bound or negative ones.
[The strategy document](leveling-strategy.md#the-roll-variable) has the
measurements. If your scenario already defines `roll` — from an include, say —
it is used as it stands, but only in exactly that shape.

---

## Levelling in the browser

At <https://dealer.bridge-classroom.org>.

**Auto-level** is a checkbox in the row above the editor, beside Run.

- It **ticks itself** the moment your script names hand types, because that is
  the only thing levelling needs and the only reason to want it.
- Untick it and it stays unticked. Turning it off is a choice, and re-ticking it
  on your next edit would take that away.
- It is greyed out when there are no hand types to level, with a tooltip saying
  what to add.

Press **Run** and the engine does both passes: 10,000 deals to measure the
natural mix, then your requested number through the levelled scenario.

### What you get back

**A Hand types panel**, above the averages, because when a scenario is levelled
this is the thing it was levelled for. Each type gets a bar showing natural
against delivered — orange for what nature offers, the accent colour for what
this run gave you — with the delivered percentage and a `was N%` beside it.

Above the bars, a line worth reading every time:

```
Levelled over 10,000 measured deals · 75 dealt per deal kept ·
keeps pinned down by 22_24, seen 159 times
```

That last figure is [the one that matters](#the-one-thing-to-get-right). A few
hundred or more and the levelling is sound. A few dozen and you are looking at a
sketch.

And that particular line is the worked example from
[the first section](#the-one-thing-to-get-right), caught live: 159 sightings
where the true rate predicts 129. Everything on the page looks healthy — the
bars land near 20%, nothing is refused — and the `22_24` keep is nonetheless
24% too small. **A levelling reports its own precision and not its own
accuracy.** 159 is the only thing on screen that says so.

**A second editor tab, "Leveled"**, holding the generated scenario: the keeps,
the header recording what they were measured over, and the player-facing text
with its shares filled in. This is the file you would copy out and give to BBO.

**Run on the Leveled tab resamples without re-levelling.** The generated
scenario runs as it stands, so pressing Run again is another sample of *one*
levelling rather than a fresh one — the keeps stay put and only the deals
change. That is what you want for judging whether a mix is really landing where
it should. The Auto-level box is greyed while that tab is showing, because there
is nothing left for it to decide.

The natural rates on the bars still come from the levelling that produced the
script, not from the re-run — a re-run of a levelled scenario measures nothing,
so its own idea of "natural" would just be what it delivered.

**Levelled runs are interleaved automatically**, so the deals on screen walk
through the types rather than arriving in whatever order they fell. See
[Ordering the boards](#ordering-the-boards).

### Browser limits worth knowing

- The measuring pass is fixed at 10,000 produced deals (or your Max generate, if
  that is smaller). There is no control for it — for a bigger measurement, use
  the command line.
- The target is always **even**. Weights and budgets are command-line only.
- It goes ahead from 50 sightings rather than refusing at 500, and tells you the
  count instead.

---

## Levelling from the command line

Two files per scenario: a **stock** one written by hand, and a **levelled** one
generated from it that should never be edited.

```bash
dealer stock.dlr -q -p 100000 -s 1 --write-leveled leveled.dlr
```

`-p` sets how many deals to measure over and `-s` the seed, so the result is
reproducible — regenerating the same pair gives the same file byte for byte,
which is how CI checks the committed example has not drifted from the tool.

It prints where it landed:

```
measured over 100000 deals
  type     natural    target       mix     keep      seen
  12_14    0.58367   0.20000   0.20000   0.0220     58367
  15_17    0.29189   0.20000   0.20000   0.0440     29189
  18_19    0.08006   0.20000   0.20000   0.1605      8006
  20_21    0.03153   0.20000   0.20000   0.4075      3153
  22_24    0.01285   0.20000   0.20000   1.0000      1285

  exactness 1.000
  acceptance 0.0643 of qualifying deals
  about 93 deals dealt per deal kept
  keeps pinned down by `22_24`, the rarest, seen 1285 times: +-2.8%
```

Column by column: **natural** is what the scenario deals unlevelled, **target**
what you asked for, **mix** what the keeps will actually deliver (the target,
unless a budget relaxed it), **keep** how often that type survives, and **seen**
the sightings the rate was measured over.

The same block is stamped into the generated file, along with the source's hash
— so a stale generated file is detectable rather than merely wrong. The keeps
are only valid for the base condition that was in force when they were measured.
**Check that hash in your build.**

### The switches

| switch | what it does |
|---|---|
| `--write-leveled FILE` | measure this scenario and write the levelled copy to `FILE` |
| `--level-target MIX` | `even` (the default), or one weight per hand type |
| `--level-budget N` | cap the cost at `N` deals dealt per deal kept, relaxing exactness to fit |
| `--interleave` | order the output so each type appears before any repeats |
| `--stats-json` | report the statistics as JSON instead of tables |
| `--param N=TEXT` | fill a `$0`–`$9` script parameter |

`--stats-json` is what a build step should read rather than parsing `%g` out of
a text table. Its `hand_types` gives each type's count and share directly, so
verifying a levelled run needs no `average` statement per type:

```json
"hand_types": [
  { "name": "12_14", "produced": 40213, "share": 0.201065 },
  { "name": "15_17", "produced": 39902, "share": 0.19951 }
]
```

Pair it with `-q` for a stdout that is nothing but JSON.

### Verifying the result

Run the *generated* file and check what came out:

```bash
dealer leveled.dlr -q -p 10000 --stats-json | jq .hand_types
```

**10,000 produced is a good default** for this check. It buys about ±1 point for
anything from three bands to ten, and costs a second or two:

| bands | target | ±2.0 pts | ±1.0 pts | ±0.5 pts | ±0.2 pts |
|---|---|---|---|---|---|
| 3 | 33.3% | 3,200 | 12,700 | 51,000 | 318,000 |
| 4 | 25.0% | 2,900 | 11,700 | 46,800 | 292,000 |
| 5 | 20.0% | 2,700 | 10,600 | 42,500 | 265,000 |
| 8 | 12.5% | 2,000 | 8,200 | 32,700 | 204,000 |
| 10 | 10.0% | 1,800 | 7,100 | 28,400 | 177,000 |

Read as: produce this many, and 95% of the time *every* band lands within the
tolerance.

Note that this is a **different and usually smaller** number than the measuring
pass wants. Verification sees each band at its *target* share, so one number
serves every script with the same band count. The measuring pass sees each band
at its *natural* share, and the rarest is by definition measured on fewest
deals. That asymmetry is the whole reason
[the first section](#the-one-thing-to-get-right) exists.

Do not chase ±0.2. Tightening from ±1 point costs 25 times the deals, and it is
measuring something nobody will ever experience — the set a class actually plays
has [an SD of 8 points](#and-a-short-set-is-lumpy-however-even-the-keeps-are)
whatever you do.

---

## Keeping the player-facing text honest

The `@chat` block a student reads is written by hand, and drifts. Before this
existed, the example scenario advertised `15-17 HCP (~23%)` for a band that
delivered 19.3%, and `20-21 HCP (~19%)` for one delivering 22.0% — near enough
swapped, because the text was written once and the keeps were tuned afterwards.

So write a marker instead, and let it be filled from the same numbers as the
keeps:

```
Five HCP ranges, leveled to:
• 12-14 HCP ({{level-mix:12_14}}) - Open 1 of a suit, then rebid 1NT
• 15-17 HCP ({{level-mix:15_17}}) - Standard 1NT opening
```

The generated file reads:

```
Five HCP ranges, leveled to:
• 12-14 HCP (20%) - Open 1 of a suit, then rebid 1NT
• 15-17 HCP (20%) - Standard 1NT opening
```

A bare `{{level-mix}}` writes every type and its share instead, for a scenario
that wants a block rather than prose.

Either way the figure follows the budget: generate the same file with
`--level-budget 40` and it reads `43.4%`, `25.6%` and so on, because that is
what it now delivers. And a marker naming a type the scenario does not declare
is an error, so a typo surfaces rather than leaving `{{level-mix:22_25}}` in the
text a student reads.

---

## Choosing a target mix, and what it costs

### An even mix

The default. `--level-target even`, or say nothing.

### Weights

One per hand type, in declaration order:

```bash
dealer stock.dlr -p 100000 --level-target 1,1,2,2,3 --write-leveled leveled.dlr
```

They are weights, not percentages — they are normalised — so `1,1,2,2,3` and
`10,10,20,20,30` are the same request.

**A weight of `0` excludes its type exactly.** It is written `level_X = 0`,
never kept, rather than rounded up to the one-deal-in-a-thousand a threshold can
express.

### What it costs before you run it

The acceptance rate among deals that already pass the base condition is

> **acceptance = 1 / max(qⱼ / pⱼ)**

Not a sum. **The single type being stretched furthest from nature sets the
price, and every other type is free.** For the NT ladder, `22_24` is 1.29%
natural against a 20% target — a 15.6× stretch — so about 6.4% of qualifying
deals survive, and roughly 93 deals must be dealt per deal kept.

Read the other way, it is a budget:

> With an acceptance budget A, no type can be over-represented beyond **1/A**
> times its natural rate.

### When the target costs more than you want to pay

`--level-budget N` caps the cost at `N` deals dealt per deal kept. Rather than
sacrificing the rarest type — usually the one the whole exercise exists to
promote — every type moves the same fraction `λ` of the way from nature toward
the target:

| `--level-budget` | λ | resulting mix (%) | spread |
|---|---|---|---|
| 93 | 1.000 | 20.0  20.0  20.0  20.0  20.0 | 0.0 |
| 80 | 0.847 | 25.9  21.4  18.2  17.4  17.1 | 8.7 |
| 60 | 0.618 | 34.7  23.5  15.4  13.6  12.9 | 21.8 |
| 40 | 0.389 | 43.4  25.6  12.7   9.7   8.6 | 34.9 |
| none | 0.000 | 58.4  29.2   8.0   3.2   1.3 | 57.1 |

λ = 1 is the target exactly; λ = 0 is no levelling at all. Everything gives
ground together, so the rare types keep as much representation as the budget
allows. The summary says `exactness 0.847  (relaxed to fit the budget)` when
this has happened, and `{{level-mix}}` reports the mix you will actually get.

A budget is worth reaching for when an instructor is redealing to skip past
repeats: coverage of the pool matters more than the exact mix, and at λ = 0.7
the generation cost falls by a third while a 24-board pool still holds all five
types 94% of the time.

---

## Ordering the boards

Levelling fixes the proportions. It says nothing about the sequence, so a
levelled run still deals its types in whatever order they happened to fall — a
twenty-board set can open with four `22-24` hands and finish with none.

```bash
dealer leveled.dlr -p 20 -f pbn --interleave
```

`--interleave` reorders the produced deals so every type appears before any type
repeats, each spread evenly across the whole run rather than round-robined until
the small buckets run dry. **In the browser this happens automatically on a
levelled run** — there is no control for it.

It is a reordering and nothing more. The same deals come out and the mix is
untouched; what changes is that any prefix of the file is a fair walk through
the types, which is what matters when a table plays the first twelve of
twenty-four.

Three things to know:

- **Within a round the order is shuffled**, derived from the seed and the round
  number. Dealing the types in declaration order every round reads as the
  natural frequency the levelling exists to remove — and a student meeting the
  same sequence every round learns the sequence instead of the hands. A seed
  still reproduces its set exactly.
- **Boards are numbered by where they land, not by when they were dealt.** The
  order lives in the file and `[Board]` is what a reader sorts on; numbering in
  production order would let any such reader silently undo the ordering. Dealer
  and vulnerability rotate with the number, so they follow the emitted sequence.
- **It does not combine with `--write-leveled`.** That run measures the scenario
  as it stands, at its natural mix, so there is no practice set to walk through.
  Asking for both is refused rather than ignored. Level first, then interleave
  the generated file.

Deals matching no type come out last, in the order they were produced.

---

## What it refuses to do, and what to do about it

Each of these is a way the method goes quietly wrong, so each is an error rather
than a warning.

| refusal | what to do |
|---|---|
| **a type seen fewer than 500 times** | raise `-p`. The message names the type. See [the first section](#the-one-thing-to-get-right) |
| **types that overlap** | two types matched one deal — tighten the boundaries |
| **types that leave a gap** | the measured rates did not sum to 1. Add a catch-all type, or narrow the condition so the declared types cover it |
| **a generated file given as the stock one** | use the stock file. Levelling an already-levelled scenario measures the levelled mix, computes keeps of roughly 1, and writes a scenario with no levelling at all. The stamp is what catches this |
| **no condition to gate** | add one, or a `### BEGIN GENERATED LEVELING ###` placeholder saying where the block belongs |
| **a `levelTheDeal` nothing uses** | the placeholder is there but the condition ignores the verdict, so the file would look levelled and not be. Add `and levelTheDeal` to the condition |
| **a `roll` that is not the safe form** | your scenario defines its own `roll` in some other shape. It must be `(rnd(N) % N + N) % N` — `roll = rnd(1000)` is refused, because a bare `rnd` is not uniform on every build |
| **a keep too small for the roll to express** | under half of one in the roll's range there is no threshold that says it. Either use `--level-budget` to relax the target, or set that type's weight to `0` if you meant to exclude it |
| **`--interleave` with `--write-leveled`** | level first, then interleave the generated file |

---

## When not to level

Three ways a scenario reaches a student, and only one of them wants full
levelling.

**Randomly, in order, with no way to skip** — a BBO practice table. Levelling is
what makes the rare types appear at all. Level fully.

**An instructor redealing to skip past repeats.** Coverage matters more than the
exact mix, so trade exactness for generation cost with `--level-budget`.

**Software asking for one variation at a time.** Do not level at all — narrow
the condition instead:

```
condition balanced and hcp(south) >= 12 and hcp(south) <= 24 and HandType_22_24
```

which deals that type and nothing else, on demand, with no roll and no
thresholds to keep up to date.

Be clear about what this saves, because it is not generation. A rare hand costs
the same either way — 465 deals dealt per rare hand taking one in five from a
levelled stream, 467 asking for it directly. The same number, because it is the
same rarity underneath. **Levelling does not make a rare hand cheaper. It makes
it arrive on schedule instead of at random, and selecting makes it arrive on
demand.**

### A fixed set that must come out exactly even

Levelling is the wrong tool. It gets the long-run proportions right and cannot
make a *finite* set come out even — that is
[the 8-point SD](#and-a-short-set-is-lumpy-however-even-the-keeps-are) again.

For a PBN generated once and handed to a class, what you want is **quotas**:
deal, classify, take the deal if its bin has room, stop when every bin is full.
The bins are then exact by construction rather than in expectation, and it needs
no measuring pass, no keeps, no `roll` and no `{{level-mix}}` — so it sidesteps
[the one error levelling cannot recover from](#the-one-thing-to-get-right)
entirely, by having nothing to measure.

That is [issue #16](https://github.com/bridge-craftwork/Dealer3/issues/16), and
it is not implemented yet. Levelling is for the stream; quotas are for the
finite set.

---

## See also

- [`docs/leveling-strategy.md`](leveling-strategy.md) — the method, the maths and
  the measurements behind every number here
- [`examples/README.md`](../examples/README.md) — a real scenario and its
  generated pair, and what the pair is worth looking at for
- [`docs/FILTER_LANGUAGE_STATUS.md`](FILTER_LANGUAGE_STATUS.md) — the script
  language itself, generated from the parser's vocabulary
- [`docs/command_line_comparison.md`](command_line_comparison.md) — every switch,
  generated from the CLI definition
