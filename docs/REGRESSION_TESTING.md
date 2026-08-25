# Regression Testing

dealer3 has two independent regression tiers. Neither runs `dealer.exe` in CI —
Tier 1 replays artifacts that were generated once, by hand, and committed.

| Tier | What it protects | Needs dealer.exe? |
|------|------------------|-------------------|
| 1 — corpus replay | Script parsing and filter semantics, checked against the reference implementation | Only to *create* a corpus |
| 2 — self-regression | Deal generation and filtering staying stable across changes | No |

## Why Tier 1 works without RNG compatibility

Historically the only way to check our filter against `dealer.exe` was to
reproduce its exact deal sequence, which is what `--legacy` and the ported GNU
`random()` existed for.

Tier 1 removes that dependency. Instead of *reproducing* dealer.exe's deals, we
*capture* them and feed them back in:

1. Run `dealer.exe` with a script at seed `S`, producing `P` deals with a
   ceiling of `MAXGEN`. It reports the generate count `G` it actually consumed.
2. Run `dealer.exe` again at the same seed `S` with **no condition** and
   `-g G -p G`. Because the seed and generate count match, this yields exactly
   the same first `G` deals, unfiltered.
3. Feed those `G` deals to dealer3 via `--input-deals`, applying the same
   script. The result must match step 1's output exactly.

The comparison is now purely about constraint evaluation. The RNG never enters
into it.

## Corpus layout

```
dealer/tests/corpus-scripts/    # source scripts, curated by feature
dealer/tests/corpus/<name>/
├── script.dlr                  # copy of the script used
├── unfiltered.txt              # the G deals dealer.exe saw (full mode only)
├── expected.txt                # the deals dealer.exe produced
└── manifest.json               # seed, counts, dealer.exe version, date
```

`manifest.json` records everything needed to regenerate the corpus exactly, and
carries `input_deals` — the count the replay test asserts against before
comparing anything. See "Guarding against short reads" below.

## Full vs one-sided corpora

**Full** (default) commits `unfiltered.txt`, so the replay is two-sided: it
detects dealer3 being either too strict (dropping deals dealer.exe accepted) or
too lenient (accepting deals it rejected).

**One-sided** (`-1`) commits only `expected.txt`, which is fed back through the
filter asserting nothing is dropped. Use it when a filter is so selective that
`G` would be impractically large — `selective_slam` needs 6,877 deals to produce
8.

> **One-sided corpora cannot detect dealer3 being too lenient.** No rejected
> deals are present in the input, so there is nothing for an over-permissive
> filter to wrongly accept. Prefer the full form wherever `G` is reasonable.

## Creating a corpus

Requires the Windows VM (see `CLAUDE.md`). All VM access goes through
Practice-Bidding-Scenarios' `build-scripts-mac/ssh_runner.py`, which owns the
drive mappings and Mac→Windows path translation — do not hand-roll `ssh` or
`net use` here.

> If a run fails with "No such file or directory" for a path that clearly
> exists, check `net use` on the VM. Windows persists drive mappings across
> sessions and silently keeps an existing one if the letter is already taken,
> so a `G:` left pointing at the wrong root survives remapping. Clear it with
> `net use G: /delete /y`.

```bash
# Full corpus
./scripts/generate-corpus.py -s 1 -p 20 -g 1000 dealer/tests/corpus-scripts/hcp_basic.dlr

# One-sided, for a very selective filter
./scripts/generate-corpus.py -s 17 -p 8 -g 20000 -1 dealer/tests/corpus-scripts/selective_slam.dlr
```

Then commit the generated directory and run `cargo test -p dealer --test corpus_replay`.

### Guidance

- **Vary the seed across corpora** so coverage does not depend on one seed's
  particular deals. The committed set uses seeds 1, 7, 17, 31, 42, 99, 123, 555
  and 2024.
- **Prefer many scripts with small `G`** over few with large `G`. The whole
  committed set is ~220 KB; keep it that way.
- **One feature per script.** These test the language, not bidding theory.
- If a run reports hitting the generate ceiling before producing `-p` deals,
  either raise `-g` or switch to `-1`.

### Output format caveat

`dealer.exe` has no output-format switch, so the harness appends
`action printoneline` to a derived copy of the script. dealer.exe honours the
**last** action block, so this overrides whatever the source declared — which
means `average` and `frequency` blocks in a source script are **not** exercised
by its corpus. Those need separate coverage.

## Guarding against short reads

`bridge_encodings::DealReader` skips lines it cannot parse rather than erroring,
which is what lets PBN metadata pipe through. The side effect: a truncated or
corrupted corpus reads short **silently**, and the replay could then pass for
the wrong reason — the filter looks correct because the deals that would have
failed it were never read.

The replay test therefore asserts the reported deal count matches
`manifest.input_deals` *before* comparing any output:

```
[hcp_basic] read 100 deals from unfiltered.txt but manifest says 156 — corpus may be truncated
```

Tracked upstream as bridge-craftwork/bridge-encodings#4.

## When to regenerate

Regeneration is **event-driven, not periodic**. Re-running the same scripts at
the same seeds produces byte-identical output, so there is no value in a
schedule. Add or regenerate a corpus when:

- the filter language gains a feature the existing set does not exercise
- a specific bug needs pinning with a targeted case
- a corpus is found to be wrong (in which case, work out why before replacing it)

Never regenerate a corpus simply because it started failing. A failing replay
means dealer3's behaviour changed relative to dealer.exe; establish which side is
wrong first. Regenerating to make the failure go away destroys the evidence.

## Tier 2 — self-regression

Covered separately: dealer3 generates deals at fixed seeds and compares a hash
of the result against a committed value. This pins the xoshiro256++ sequence and
the filtering path together, and is cheap to store and fast to run.

A Tier 2 failure means deal generation changed. That is sometimes intended — a
deliberate RNG or seeding change is a breaking change — but it should never
happen by accident.
