# Examples

## `NT_Ladder.stock.dlr` and `NT_Ladder.leveled.dlr`

A real scenario from the [Practice-Bidding-Scenarios][pbs] corpus, and the
levelled version generated from it. See `docs/leveling-strategy.md` for the
method; these are what it produces.

[pbs]: https://github.com/bridge-craftwork/Practice-Bidding-Scenarios

**The stock file** is the one written by hand. Its hand types are named by the
`HandType_` prefix, it leaves a placeholder where the levelling goes, and it
marks the places in the player-facing text where the resulting mix belongs:

```
HandType_12_14 = hcp(south) >= 12 and hcp(south) <= 14

• 12-14 HCP ({{level-mix:12_14}}) - Open 1 of a suit, then rebid 1NT

### BEGIN GENERATED LEVELING ###
noLeveling = 1
levelTheDeal = noLeveling
### END GENERATED LEVELING ###
```

It runs as it stands, dealing the natural mix — 58% / 29% / 8% / 3% / 1.3% —
which is what the tool measures.

**The levelled file** is generated and should not be edited:

```bash
dealer examples/NT_Ladder.stock.dlr -q -p 100000 -s 1 \
    --write-leveled examples/NT_Ladder.leveled.dlr
```

The measuring run is seeded, so this reproduces the committed file byte for byte
— which is how CI checks the pair has not drifted from the tool.

## What the pair is worth looking at for

The original hand-tuned version of this scenario used the spot-card ladder: 31
lines of imported `keep06 = c1 and c2` arithmetic built out of where the ♣2 and
♦2 landed. It delivered 20.8 / 19.3 / 20.8 / 22.0 / 17.1 after three years of
iteration, and its own header claimed a different mix again.

The generated version is one `roll` and five thresholds, delivers
19.9 / 19.5 / 20.3 / 19.8 / 20.6 first time, and its header cannot disagree with
its keeps because both come from the same calculation.

| | hand-tuned ladder | generated |
|---|---|---|
| spread across bands | 4.8 points | 1.1 points |
| deals dealt per deal kept | 77 | 94 |
| wall clock, 5000 deals | 0.18s | 0.18s |
| length | 93 lines | 62 lines |

The extra deals buy the exactness, not the mechanism: aimed at the ladder's own
uneven mix, the formula asks for 80 per kept against the ladder's measured 77.
