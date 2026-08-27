# Levelling a scenario

A practice scenario usually wants its hand types in some deliberate mix — a
quarter of the boards weak, a quarter invitational, and so on — while nature
supplies them in quite another. **Levelling** is discarding some of the common
types so the mix comes out as intended.

This describes how to do that in one measurement and one calculation, why the
older way needed neither but took several passes, and the one construct that
makes it portable.

## The two ways to flip a coin

For a long time dealer's language had no usable randomness, so scripts made
their own out of the deal:

```
c1 = hascard(west, 2C)     # true on a quarter of deals
keep25 = c1
keep06 = c1 and hascard(east, 2D)
```

A named card sits in a named hand exactly a quarter of the time, so this is a
probability ladder built from spot cards. It works, it is portable to every
build of every dealer, and it has one flaw that is easy to miss.

**The spot card is part of the hand being filtered.** `hascard(west, 2C)` is 25%
*unconditionally*. It is not 25% once the script's own condition has a view
about clubs:

| | rate |
|---|---|
| `hascard(west, 2C)` with no condition | 0.2501 |
| the same under `clubs(north) >= 6` | **0.1764** |
| `rnd(4) == 0` under the same condition | 0.2450 |

Worse than the rate being wrong is that the ladder **biases the hands it
keeps**. Selecting on where the ♣2 landed selects for West being long in clubs,
and slightly weak, because one of West's thirteen slots is now a known low card:

| kept by | clubs(west) | hcp(west) |
|---|---|---|
| nothing (baseline) | 2.259 | 10.01 |
| `hascard(west, 2C)` | **2.846** | **9.31** |
| `rnd(4) == 0` | 2.258 | 10.02 |

And because each keep shifts the population the other keeps are measured
against, tuning a script is iterative: change one level, re-measure everything.

`rnd()` has no such coupling. It knows nothing about the deal, so the keeps are
independent of the hands and of each other — which is what turns the tuning into
arithmetic.

## The roll variable

`rnd()` needs one piece of care. Write it like this:

```
roll = (rnd(1000) % 1000 + 1000) % 1000
```

Two reasons, and both matter.

**The modulo is not decoration.** `rnd(n)` is meant to give `0..n-1`, and does on
BBO and in dealer3. On a locally built dealer binary it does not: `rnd` divides
by `RAND_MAX`, which describes `rand()` rather than the generator it actually
calls, so a build without `STD_RAND` returns values far outside the bound —
`rnd(10)` averages 322,000 on a Windows build — or negative ones, 43% of the
time on a macOS build. The inner `% 1000` reduces any magnitude and the
`+ 1000) % 1000` folds negatives back up, so the same expression lands on target
everywhere:

| | `%4==0` want .25 | `%2==0` want .50 | `%10<3` want .30 |
|---|---|---|---|
| BBO | 0.2525 | 0.507 | 0.3096 |
| dealer3 | 0.2455 | 0.4945 | 0.3011 |
| Windows `dealer.exe` | 0.2540 | 0.5096 | 0.3011 |

Scaling cannot do this. Scaling needs the maximum, and the maximum is exactly
what differs between builds. Neither can `abs()`: `r < 0 ? -r : r` re-evaluates
`r` at every mention, so it tests one draw and negates another.

**Every mention draws again.** `roll` is not a stored value; it is an expression
worked out afresh wherever it appears, which is what the original does with all
its variables and what dealer3 now does with any variable that can reach
`rnd()`. So `roll < 88` twice is two independent flips. Use `roll` once per deal,
in a condition whose branches are mutually exclusive, and the point never
arises.

Pick a modulus that divides the bound. With `rnd(1000)`, `% 2`, `% 4`, `% 5`,
`% 100` and `% 1000` are exact; `% 3` carries about a tenth of a percent of bias.

## Levelling in two steps

### Step one: measure

Classify the deals, and run the script with no levelling at all.

```
nt = hcp(north) >= 15 and hcp(north) <= 17 and shape(north, any 4333 + any 4432 + any 5332)
weak     = hcp(south) <= 7
invite   = hcp(south) >= 8 and hcp(south) <= 9
game     = hcp(south) >= 10 and hcp(south) <= 14
slammish = hcp(south) >= 15

condition nt
action average "weak" weak, average "invite" invite,
       average "game" game, average "slam" slammish
```

```
$ dealer -q --stats-json -g 2000000 -p 200000 scenario.dlr
```

`--stats-json` reports the same numbers as the tables, at full precision and
with the sample size each was measured over, which is what a build script should
read rather than parsing `%g` out of a text table.

```
weak     0.4525
invite   0.2126
game     0.2952
slam     0.0397
```

### Step two: calculate

For an even mix, keep the rarest type always and every other type in inverse
proportion to how common it is:

> **kᵢ = p_min / pᵢ**

For an uneven target mix `q`, it is **kᵢ ∝ qᵢ / pᵢ**, scaled so the largest keep
is 1.

| | p | keep | per 1000 |
|---|---|---|---|
| weak | 0.4525 | 8.78% | 88 |
| invite | 0.2126 | 18.70% | 187 |
| game | 0.2952 | 13.46% | 135 |
| slam | 0.0397 | 100% | 1000 |

Then write the keeps into the condition, one roll, one branch per type:

```
roll = (rnd(1000) % 1000 + 1000) % 1000

condition nt and (
      (weak     and roll <   88)
   or (invite   and roll <  187)
   or (game     and roll <  135)
   or (slammish)
)
```

Level to within half a percent, first attempt, on both engines:

| | dealer3 | BBO |
|---|---|---|
| weak | 0.2415 | 0.2545 |
| invite | 0.2459 | 0.2520 |
| game | 0.2513 | 0.2545 |
| slam | 0.2613 | 0.2390 |

Because the type tests partition the deals, at most one branch's comparison can
matter. That makes the construct correct whether or not the engine
short-circuits `and`, and whether or not `roll` is redrawn at each mention —
worth keeping as a rule when the shape is generalised.

## What it costs, before you run it

The acceptance rate among deals that already pass the base condition is

> **acceptance = 1 / max(qⱼ / pⱼ)**

Not a sum: the single type being stretched furthest from nature sets the price,
and every other type is free. Here slam is 3.97% against a 25% target, a ratio
of 6.29, so 15.9% of qualifying deals survive and roughly 6.3 times as many must
be generated. Measured against a base condition rate of 0.0482, that predicts an
overall yield of 0.00767; the run produced 0.00761.

Read the other way, it is a budget:

> **With an acceptance budget A, no type can be over-represented beyond 1/A
> times its natural rate.**

A 20% budget allows a 5× stretch, so a type occurring 3.97% of the time can be
asked for at most 19.9%. When a target costs more than the budget allows, cap
and redistribute:

1. `R = 1 / A_budget`
2. `q'ⱼ = min(qⱼ, R · pⱼ)` — cap the over-ambitious types
3. Share the shortfall `1 − Σq'` among the uncapped types, in proportion to
   their targets
4. Repeat until stable — at most as many rounds as there are types

On the numbers above with a 20% budget, slam caps at 19.9% and the other three
rise to 26.7% each, landing exactly on the budget. Which is a better answer than
"it is slow, try again": it says what mix is affordable and which type is the
bottleneck.

## Relaxing exactness when the budget will not stretch

A budget can be too tight for a fully level mix. The lever is not to single out
the rarest type — it is usually the one the whole exercise exists to promote —
but to level *less*, moving every type the same fraction of the way from nature
toward the target:

> **qⱼ(λ) = (1 − λ)·pⱼ + λ·tⱼ**

λ = 1 is the target exactly, λ = 0 is no levelling at all. The cost has a closed
form, because the worst ratio is affine in λ:

> **acceptance(λ) = 1 / (1 + λ·(r_max − 1))**, where r_max = max(tⱼ / pⱼ)

so the exactness a budget affords needs no search:

> **λ = (1/A − 1) / (r_max − 1)**

For the NT ladder, whose 22-24 band asks to be stretched 15.6× from natural:

| budget (deals per kept) | λ | resulting mix (%) | spread |
|---|---|---|---|
| 93 | 1.000 | 20.0  20.0  20.0  20.0  20.0 | 0.0 |
| 80 | 0.847 | 25.9  21.4  18.2  17.4  17.1 | 8.7 |
| 60 | 0.618 | 34.7  23.5  15.4  13.6  12.9 | 21.8 |
| 40 | 0.389 | 43.4  25.6  12.7   9.7   8.6 | 34.9 |
| none | 0.000 | 58.4  29.2   8.0   3.2   1.3 | 57.1 |

Every band gives ground together, so the rare types keep as much representation
as the budget allows.

## Generating levelled scripts

`scripts/level-scenario.py` does all of it. Two files per scenario: a *stock*
one written by hand, and a *levelled* one generated from it that should never be
edited.

The stock file declares its types in the header the corpus already uses, and
leaves a placeholder where the levelling goes:

```
# level-types: hcp12_14, hcp15_17, hcp18_19, hcp20_21, hcp22_24
# level-target: even            # or a weighting: 30, 30, 20, 10, 10
# level-budget: 150             # deals dealt per deal kept; optional

### BEGIN GENERATED LEVELING ###
levelTheDeal = 1
### END GENERATED LEVELING ###

condition ... and levelTheDeal
```

`levelTheDeal = 1` means "no levelling", so the stock file runs and can be
measured exactly as it stands — which is what the tool does first.

```
$ scripts/level-scenario.py stock.dlr -o leveled.dlr
measured over 100,000 deals of 599,930 dealt
  base condition accepts 16.669% of deals

  type        natural    target       mix     keep      seen
  hcp12_14    0.58367   0.20000   0.20000   0.0220    58,367
  hcp15_17    0.29189   0.20000   0.20000   0.0440    29,189
  hcp18_19    0.08006   0.20000   0.20000   0.1610     8,006
  hcp20_21    0.03153   0.20000   0.20000   0.4080     3,153
  hcp22_24    0.01285   0.20000   0.20000   1.0000     1,285

  exactness 1.000
  acceptance 0.0643 of qualifying deals
  about 93 deals dealt per deal kept
```

`--dry-run` reports the numbers without writing. `--budget` overrides the
header. `--deals` sets how many to measure over.

### What it refuses to do

Each of these is a way the method goes quietly wrong, so each is an error rather
than a warning.

- **Types measured on too few deals.** A rate that is divided by has to be worth
  dividing by; fewer than 500 sightings of a type is refused, naming the type
  and suggesting a larger measuring run.
- **Types that overlap**, or that **leave a gap**. They have to partition the
  produced deals or the keeps will not add up, and the tool checks that the
  measured rates sum to 1.
- **A missing placeholder or an unused `levelTheDeal`**, either of which would
  produce a file that looks levelled and is not.

### What it stamps into the generated file

The source's hash, the measured natural rates, the target, the keeps, the
exactness, the acceptance and the cost. The keeps are only valid for the base
condition that was in force when they were measured, so a stale generated file
is detectable rather than merely wrong — check the hash in the build.

## A note on where scripts run

`rnd()` is in the original dealer's lexer, grammar and implementation, and it is
absent from BBO's own language manual — which is probably why it went unused for
so long. It is *broken* on locally built dealer binaries, in the two different
ways described above, and that is what the roll variable's modulo absorbs.

It works correctly on BBO. Verified on both paths that matter: the public script
tester at <https://www.bridgebase.com/tools/dealer/dealer.php>, and a live
practice table fed by the PBS BBO extension.

The levelling itself is engine-independent: the same thresholds produce the same
mix on dealer3 and on BBO, because both draw uniformly within the bound.

### Checking a new deployment

A practice table shows hands, not text, so the check has to be visible in a
hand. This makes the South hand — always visible, unlike North before the
auction ends — hold `AKQ` in one suit or the other, on a coin flip:

```
produce 20
condition (rnd(2) == 0 and hascard(south, AS) and hascard(south, KS) and hascard(south, QS))
       or (rnd(2) != 0 and hascard(south, AH) and hascard(south, KH) and hascard(south, QH))
```

| what the boards show | what it means |
|---|---|
| some ♠AKQ, some ♥AKQ | `rnd()` works |
| every board ♥AKQ, never ♠AKQ | broken the way a Windows build is |
| about a third ♠AKQ | broken the way a macOS build is |
| a board with neither | the script never reached the engine |

**One ♠AKQ board is the whole test.** A broken engine produces none at all, so
this is a question of presence, not proportion — with a working `rnd()`, seeing
zero in twenty boards has a probability of about one in a million.

The two branches have to be equally rare, which is why it is `AKQ` in two suits
rather than something simpler. South holds `♠AKQ` about 1.3% of the time and
holds no top spade honour about 41% of the time, so pairing those two would
produce a 3%/97% split whatever `rnd()` did — indistinguishable from broken.
Spades against hearts is 1.29% against 1.296%, so the coin is the only thing
separating them.
