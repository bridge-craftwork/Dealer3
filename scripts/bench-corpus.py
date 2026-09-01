#!/usr/bin/env python3
"""
bench-corpus.py - Pick and normalize the benchmark scripts.

Builds `bench/corpus/` from the Practice-Bidding-Scenarios `dlr/` directory,
and writes `bench/corpus.json` describing what got in and why.

Four stages:

  1. **Normalize.** Each candidate is rewritten by benchlib.normalize_script():
     block comments out, `produce`/`generate` out (they override `-g`), action
     block replaced with a cheap one. See benchlib for why each is necessary.

  2. **Verify.** Every candidate is run on every available target at a small
     deal count. A script that any target rejects is dropped, with the reason
     recorded. This is what makes the corpus honestly three-way: nothing gets
     in that dealer.exe cannot parse.

  3. **Calibrate.** Each survivor gets a `deals` count sized so dealer3
     single-threaded takes about --target-seconds. Cheap conditions therefore
     run more deals than expensive ones, and every script contributes a
     comparably-sized measurement instead of some finishing in 20ms.

  4. **Select.** Survivors are sorted by cost per deal and sampled evenly
     across that range, so the corpus spans cheap and expensive conditions
     rather than clustering on whatever is alphabetically first.

Run this only when the script selection should change. The results it feeds
(bench-reference.py, bench-dealer3.py) are what get run routinely.

Usage:
    scripts/bench-corpus.py [-n 12] [--target-seconds 2.0] [--all-targets]

Options:
    -n N               Scripts to keep (default: 12)
    --target-seconds S Calibrate so dealer3 -R1 takes about S seconds (default: 2.0)
    --source DIR       Where to look for .dlr files
                       (default: ../Practice-Bidding-Scenarios/dlr)
    --candidates N     How many source scripts to consider (default: all)
    --all-targets      Verify against dealer.exe and DealerV2_4 too, not just
                       dealer3. Slower (each check is an SSH round trip), but
                       it is the only way to know the corpus runs everywhere.
    --dealerv2 PATH    DealerV2_4 binary
    -v, --verbose      Report every rejection
"""
import argparse
import difflib
import json
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchlib as bl

# These runs take minutes and are usually backgrounded into a file, where
# Python would block-buffer and show nothing until the very end.
sys.stdout.reconfigure(line_buffering=True)

VERIFY_DEALS = 20_000
CALIBRATE_DEALS = 100_000


def main():
    ap = argparse.ArgumentParser(
        description="Build the three-way benchmark corpus from PBS scripts.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument("-n", "--count", type=int, default=10,
                    help="scripts to keep (default: 10)")
    ap.add_argument("--similarity", type=float, default=0.85,
                    help="body-similarity threshold above which two scripts count "
                         "as the same benchmark (default: 0.85)")
    ap.add_argument("--target-seconds", type=float, default=2.0,
                    help="calibrate so dealer3 -R1 takes about this long (default: 2.0)")
    ap.add_argument("--source", default=None, help="directory of source .dlr files")
    ap.add_argument("--candidates", type=int, default=0,
                    help="limit how many source scripts are considered (default: all)")
    ap.add_argument("--all-targets", action="store_true",
                    help="also verify against dealer.exe and DealerV2_4")
    ap.add_argument("--dealer3", default=None, help="dealer3 binary")
    ap.add_argument("--dealerv2", default=None, help="DealerV2_4 binary")
    ap.add_argument("--baseline-only", action="store_true",
                    help="regenerate just the synthetic generation baseline in the "
                         "existing corpus, leaving the selection alone")
    ap.add_argument("-v", "--verbose", action="store_true")
    args = ap.parse_args()

    dealer3 = bl.dealer3_target(args.dealer3)
    ok, why = dealer3.available()
    if not ok:
        sys.exit(f"error: dealer3 unavailable ({why}) -- run ./dev-build.sh build --release")

    if args.baseline_only:
        return _refresh_baseline(dealer3, args)

    source = Path(args.source) if args.source else \
        bl.REPO.parent / "Practice-Bidding-Scenarios" / "dlr"
    if not source.is_dir():
        sys.exit(f"error: no source directory at {source}")

    verifiers = [dealer3]
    if args.all_targets:
        for t in (bl.dealer_exe_target(), bl.dealerv2_target(args.dealerv2)):
            ok, why = t.available()
            if ok:
                verifiers.append(t)
            else:
                print(f"note: skipping {t.name} in verification ({why})")

    sources = sorted(source.glob("*.dlr"))
    if args.candidates:
        random.seed(1)
        sources = sorted(random.sample(sources, min(args.candidates, len(sources))))
    print(f"Considering {len(sources)} scripts from {source}")
    print(f"Verifying against: {', '.join(t.name for t in verifiers)}\n")

    staging = bl.REPO / "bench" / ".staging"
    staging.mkdir(parents=True, exist_ok=True)

    survivors, rejected = [], []

    for src in sources:
        text = src.read_text(errors="replace")

        if bl.statements_after_action(text):
            rejected.append((src.name, "statements follow the action block"))
            continue

        normalized, notes = bl.normalize_script(text)
        staged = staging / src.name
        staged.write_text(normalized)

        entry = None
        failure = None
        for target in verifiers:
            try:
                sample = bl.run_once(target, staged, VERIFY_DEALS, timeout=180)
            except bl.BenchError as exc:
                failure = f"{target.name}: {_brief(exc)}"
                break
            if target is dealer3:
                entry = {
                    "name": src.stem,
                    "file": src.name,
                    "source": str(src.relative_to(source.parent.parent))
                    if source.parent.parent in src.parents else str(src),
                    "notes": notes,
                    "imports": bl.imported_fragments(text),
                    "signature": bl.body_signature(normalized),
                    "produced_at_verify": sample.produced,
                    "hit_rate": sample.produced / VERIFY_DEALS,
                }

        if failure:
            rejected.append((src.name, failure))
            if args.verbose:
                print(f"  reject {src.name}: {failure}")
            continue

        # A condition nothing satisfies is not a useful benchmark: it also
        # never exercises the action or the produced-deal path, and it is
        # usually a sign the script wanted words this corpus stripped.
        if entry["produced_at_verify"] == 0:
            rejected.append((src.name, f"no matches in {VERIFY_DEALS} deals"))
            if args.verbose:
                print(f"  reject {src.name}: no matches")
            continue

        survivors.append((entry, staged))

    print(f"\n{len(survivors)} of {len(sources)} scripts run on every target")
    if rejected:
        print(f"{len(rejected)} rejected"
              + ("" if args.verbose else " (use -v to see why)"))

    if not survivors:
        sys.exit("error: nothing survived verification")

    # --- Calibrate -------------------------------------------------------
    print(f"\nCalibrating to ~{args.target_seconds}s on dealer3 -R1 ...")
    for entry, staged in survivors:
        probe = bl.run_once(dealer3, staged, CALIBRATE_DEALS, threads=1, timeout=300)
        per_deal = probe.seconds / CALIBRATE_DEALS
        entry["seconds_per_deal_r1"] = per_deal
        deals = int(args.target_seconds / per_deal) if per_deal > 0 else 5_000_000
        entry["deals"] = _round_nicely(deals)
        if args.verbose:
            print(f"  {entry['name']:<40} {per_deal*1e9:7.1f} ns/deal"
                  f"  -> {entry['deals']:>9,} deals")

    # --- Deduplicate, then select ----------------------------------------
    clusters = _cluster(survivors, args.similarity)
    collapsed = len(survivors) - len(clusters)
    print(f"\n{len(clusters)} distinct scripts ({collapsed} near-duplicates collapsed)")
    chosen = _select_diverse(clusters, args.count)

    if bl.CORPUS_DIR.exists():
        for old in bl.CORPUS_DIR.glob("*.dlr"):
            old.unlink()
    bl.CORPUS_DIR.mkdir(parents=True, exist_ok=True)

    for entry, staged in chosen:
        (bl.CORPUS_DIR / entry["file"]).write_text(staged.read_text())

    # The generation baseline is synthetic and always present -- it is not
    # selected from PBS and is not subject to the diversity or duplicate rules,
    # because it is not there to represent a scenario. It is calibrated the
    # same way so its deal count is comparable to the rest.
    baseline_entry, baseline_path = _make_baseline(dealer3, args.target_seconds)
    chosen.append((baseline_entry, baseline_path))
    chosen.append(_make_solver_entry())
    print(f"\nGeneration baseline: {baseline_entry['seconds_per_deal_r1']*1e9:.1f} "
          f"ns/deal on dealer3 -R1 ({baseline_entry['deals']:,} deals)")

    for staged in staging.glob("*.dlr"):
        staged.unlink()
    staging.rmdir()

    index = {
        "built_with": bl.git_describe(),
        "corpus_id": None,          # filled in below, once the files are written
        "source": str(source),
        "target_seconds": args.target_seconds,
        "verified_against": [t.name for t in verifiers],
        "considered": len(sources),
        "survived": len(survivors),
        "distinct": len(clusters),
        "similarity_threshold": args.similarity,
        "scripts": [e for e, _ in chosen],
        "rejected": [{"file": f, "reason": r} for f, r in rejected],
    }
    index["corpus_id"] = bl.corpus_fingerprint(index["scripts"])
    bl.CORPUS_INDEX.parent.mkdir(parents=True, exist_ok=True)
    bl.CORPUS_INDEX.write_text(json.dumps(index, indent=2) + "\n")

    print(f"\nWrote {len(chosen)} scripts to {bl.CORPUS_DIR.relative_to(bl.REPO)}/")
    print(f"Wrote index to {bl.CORPUS_INDEX.relative_to(bl.REPO)}\n")
    print(f"{'script':<34} {'ns/deal':>8} {'deals':>10} {'hit':>7} {'==':>4}  imports")
    print("-" * 100)
    for entry, _ in chosen:
        imports = ", ".join(entry["imports"]) or "-"
        print(f"{entry['name']:<34} {entry['seconds_per_deal_r1']*1e9:8.1f}"
              f" {entry['deals']:>10,} {entry['hit_rate']:>6.2%}"
              f" {entry['cluster_size']:>4}  {imports[:44]}")
    print("\n'==' is how many near-duplicate scripts that row stands for.")
    if not args.all_targets:
        print("\nNote: verified against dealer3 only. Re-run with --all-targets to")
        print("confirm every script also runs on dealer.exe and DealerV2_4.")


def _make_baseline(dealer3, target_seconds):
    """Write the synthetic generation baseline and calibrate it."""
    path = bl.CORPUS_DIR / f"{bl.BASELINE_NAME}.dlr"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(bl.BASELINE_SCRIPT)
    probe = bl.run_once(dealer3, path, CALIBRATE_DEALS, threads=1, timeout=300)
    per_deal = probe.seconds / CALIBRATE_DEALS
    entry = {
        "name": bl.BASELINE_NAME,
        "file": path.name,
        "source": "synthetic",
        "synthetic": True,
        "notes": ["generation baseline: RNG and shuffle, minimal evaluation"],
        "imports": [],
        "signature": "",
        "cluster_size": 1,
        "cluster_members": [],
        "produced_at_verify": probe.produced,
        "hit_rate": probe.produced / CALIBRATE_DEALS,
        "seconds_per_deal_r1": per_deal,
        "deals": _round_nicely(int(target_seconds / per_deal) if per_deal else 5_000_000),
    }
    return entry, path


def _make_solver_entry():
    """Write the solver-agreement entry. Verify-only, and not calibrated.

    Nothing to calibrate: it is never timed. Its `deals` is a nominal figure so
    the shape of a corpus entry stays uniform.
    """
    path = bl.CORPUS_DIR / f"{bl.SOLVER_NAME}.dlr"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(bl.SOLVER_SCRIPT)
    return {
        "name": bl.SOLVER_NAME,
        "file": path.name,
        "source": "synthetic",
        "synthetic": True,
        "verify_only": True,
        "targets": list(bl.SOLVER_TARGETS),
        "verify_produce": 50,
        "notes": ["double-dummy agreement between dealer3 and DealerV2_4"],
        "imports": [],
        "signature": "",
        "cluster_size": 1,
        "cluster_members": [],
        "produced_at_verify": 0,
        "hit_rate": 0.0,
        "seconds_per_deal_r1": 0.0,
        "deals": 100_000,
    }, path


def _refresh_baseline(dealer3, args):
    """Update only the baseline in an existing corpus.

    Selecting the corpus means verifying every candidate on every target, which
    is minutes of SSH round trips, and it is deterministic -- rerunning it after
    a change that only concerns the synthetic baseline would reproduce exactly
    the same ten scripts. This path exists so changing the baseline does not
    cost that.
    """
    if not bl.CORPUS_INDEX.exists():
        sys.exit("error: no corpus to update -- run without --baseline-only first")
    index = json.loads(bl.CORPUS_INDEX.read_text())
    target_seconds = index.get("target_seconds", args.target_seconds)
    entry, _ = _make_baseline(dealer3, target_seconds)
    solver, _ = _make_solver_entry()
    index["scripts"] = [e for e in index["scripts"]
                        if e["name"] not in (bl.BASELINE_NAME, bl.SOLVER_NAME)]
    index["scripts"].extend([entry, solver])
    index["corpus_id"] = bl.corpus_fingerprint(index["scripts"])
    bl.CORPUS_INDEX.write_text(json.dumps(index, indent=2) + "\n")
    print(f"Generation baseline: {entry['seconds_per_deal_r1']*1e9:.1f} ns/deal "
          f"on dealer3 -R1 ({entry['deals']:,} deals)")
    print(f"Updated {bl.CORPUS_INDEX.relative_to(bl.REPO)}; the other "
          f"{len(index['scripts']) - 1} scripts are unchanged.")
    return None


def _brief(exc):
    return str(exc).split(": ", 1)[-1][:120]


def _round_nicely(n):
    """Round to two significant figures, so deal counts read as round numbers."""
    if n < 1000:
        return max(1000, n)
    mag = 10 ** (len(str(n)) - 2)
    return max(1000, (n // mag) * mag)


def _cluster(survivors, threshold):
    """Collapse scripts that are the same benchmark into one entry each.

    The PBS `.dlr` files are generated, and a precompiler expands shared
    fragments inline -- 155 of the 347 scripts pull in at least one, and 46 of
    them share the exact same set. On top of that, whole families differ only
    in a threshold (`Weak_NT_09-12` against `Weak_NT_10-13`). Vendoring ten of
    those would cost ten files and measure one thing.

    Two passes. Exact signature matches group instantly, which is what most of
    the redundancy is. The survivors of that are then compared pairwise, and
    merged when they share an import set *and* their bodies exceed `threshold`.
    Requiring the import set to match as well means two scripts are never
    merged just because they both inline the same large shared fragment; what
    has to be similar is the part that is actually theirs.

    Returns one (entry, path) per cluster, the member of median cost, with
    `cluster_size` recorded on it.
    """
    by_signature = {}
    for entry, path in survivors:
        by_signature.setdefault(entry["signature"], []).append((entry, path))

    groups = list(by_signature.values())

    merged = []
    for group in groups:
        head = group[0][0]
        for existing in merged:
            other = existing[0][0][0]
            if other["imports"] != head["imports"]:
                continue
            ratio = difflib.SequenceMatcher(
                None, other["signature"], head["signature"]).quick_ratio()
            if ratio >= threshold and difflib.SequenceMatcher(
                    None, other["signature"], head["signature"]).ratio() >= threshold:
                existing.append(group)
                break
        else:
            merged.append([group])

    clusters = []
    for bundle in merged:
        members = [m for group in bundle for m in group]
        members.sort(key=lambda m: m[0]["seconds_per_deal_r1"])
        entry, path = members[len(members) // 2]          # median cost, deterministic
        entry = dict(entry, cluster_size=len(members),
                     cluster_members=sorted(m[0]["name"] for m in members)[:8])
        clusters.append((entry, path))
    return clusters


def _select_diverse(clusters, count):
    """Pick `count` clusters spanning both import sets and cost.

    Two axes, because they answer different questions. Different import sets
    mean genuinely different shared machinery is exercised. Different cost per
    deal means cheap and expensive conditions are both represented, and a
    change that only helps one of them cannot hide.

    Round-robin over import sets, taking from each the entry that most extends
    the cost range covered so far.
    """
    if count >= len(clusters):
        return sorted(clusters, key=lambda c: c[0]["seconds_per_deal_r1"])

    buckets = {}
    for cluster in clusters:
        buckets.setdefault(tuple(cluster[0]["imports"]), []).append(cluster)
    for items in buckets.values():
        items.sort(key=lambda c: c[0]["seconds_per_deal_r1"])

    # Largest bucket first, so the most-represented shared machinery is not
    # crowded out by a long tail of one-off import sets.
    order = sorted(buckets, key=lambda k: (-len(buckets[k]), k))
    chosen, taken = [], set()
    while len(chosen) < count:
        progressed = False
        for key in order:
            if len(chosen) >= count:
                break
            pool = [c for c in buckets[key] if c[0]["name"] not in taken]
            if not pool:
                continue
            if not chosen:
                pick = pool[len(pool) // 2]
            else:
                have = [c[0]["seconds_per_deal_r1"] for c in chosen]
                pick = max(pool, key=lambda c: min(
                    abs(c[0]["seconds_per_deal_r1"] - h) for h in have))
            chosen.append(pick)
            taken.add(pick[0]["name"])
            progressed = True
        if not progressed:
            break
    return sorted(chosen, key=lambda c: c[0]["seconds_per_deal_r1"])


if __name__ == "__main__":
    main()
