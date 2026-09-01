#!/usr/bin/env python3
"""
bench-report.py - Join the reference and dealer3 results into one table.

Reads the newest `bench/results/reference-*.json` and `dealer3-*.json` (or the
files named on the command line) and prints the comparison:

  - deals/sec for dealer.exe, DealerV2_4, dealer3 single-threaded and dealer3
    with threads on;
  - the speedup of dealer3 over each reference, single-threaded, which is the
    only like-for-like comparison since neither reference threads;
  - the thread-scaling summary from the sweep.

Both halves must have been measured against the same corpus revision. The deal
counts are calibrated per script, so numbers taken against a different corpus
describe different amounts of work and cannot be divided by each other.

Usage:
    scripts/bench-report.py [--reference FILE] [--dealer3 FILE] [--markdown]
"""
import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchlib as bl


def newest(pattern):
    """Newest full run, preferring it over any partial one.

    A partial run is a spot check, not a record of a revision, so it is only
    used if there is nothing else -- and _plain() says so when it happens.
    """
    files = sorted(bl.RESULTS_DIR.glob(pattern), key=lambda p: p.stat().st_mtime)
    full = [f for f in files if "-partial" not in f.name]
    return (full or files)[-1] if files else None


def rate(record, key, sub=None):
    """deals/sec out of a result record, or None if it is missing or failed."""
    r = record.get(key)
    if not isinstance(r, dict) or "error" in r or "deals_per_sec" not in r:
        return None
    return r["deals_per_sec"]


def main():
    ap = argparse.ArgumentParser(description="Compare dealer.exe, DealerV2_4 and dealer3.")
    ap.add_argument("--reference", default=None)
    ap.add_argument("--dealer3", default=None)
    ap.add_argument("--markdown", action="store_true", help="emit a markdown table")
    ap.add_argument("--against", default=None,
                    help="an earlier dealer3-*.json; adds a before/after delta")
    args = ap.parse_args()

    ref_path = Path(args.reference) if args.reference else newest("reference-*.json")
    d3_path = Path(args.dealer3) if args.dealer3 else newest("dealer3-*.json")
    if not d3_path:
        sys.exit("error: no dealer3 results -- run scripts/bench-dealer3.py")

    d3 = json.loads(d3_path.read_text())
    ref = json.loads(ref_path.read_text()) if ref_path else None

    if ref:
        corpus_id = json.loads(bl.CORPUS_INDEX.read_text()).get("corpus_id")
        for label, payload in (("reference", ref), ("dealer3", d3)):
            got = payload.get("corpus_id")
            if got and corpus_id and got != corpus_id:
                print(f"warning: the {label} numbers were measured against corpus "
                      f"{got}, but the corpus is now {corpus_id}.")
                print("         Deal counts are calibrated per script, so those rows")
                print("         describe different amounts of work. Re-measure.\n")

    print(f"dealer3 results : {d3_path.name}")
    print(f"                  source {d3.get('source_id', '?')}, "
          f"repo {d3.get('revision', '?')}")
    print(f"reference       : {ref_path.name if ref_path else 'none -- run bench-reference.py'}")
    m = d3.get("machine", {})
    print(f"machine         : {m.get('cpu') or m.get('model')}  "
          f"{m.get('logical_cpus')} logical ({m.get('performance_cpus')}P+{m.get('efficiency_cpus')}E)\n")

    rows = []
    baseline = {}
    for name, rec in d3["scripts"].items():
        if name != bl.BASELINE_NAME:
            continue
        refrec = (ref or {}).get("scripts", {}).get(name, {})
        baseline = {"exe": rate(refrec, "dealer.exe"), "c": rate(refrec, "dealer-c"),
                    "v2": rate(refrec, "dealerv2_4"), "d3_1": rate(rec, "single"),
                    "d3_auto": rate(rec, "auto")}

    for name, rec in d3["scripts"].items():
        if name == bl.BASELINE_NAME:
            continue
        refrec = (ref or {}).get("scripts", {}).get(name, {})
        rows.append({
            "name": name,
            "exe": rate(refrec, "dealer.exe"),
            "c": rate(refrec, "dealer-c"),
            "v2": rate(refrec, "dealerv2_4"),
            "d3_1": rate(rec, "single"),
            "d3_auto": rate(rec, "auto"),
        })

    if args.markdown:
        _markdown(rows)
    else:
        _plain(rows)

    _generation(baseline, rows)
    if args.against:
        _delta(json.loads(Path(args.against).read_text()), d3, Path(args.against).name)
    _scaling(d3)


def _fmt(v, width=9):
    return f"{v/1e6:{width}.3f}" if v else " " * (width - 1) + "-"


def _ratio(a, b):
    return f"{a/b:6.1f}x" if a and b else "     -"


def _plain(rows):
    print(f"{'script':<28} {'exe*':>8} {'dealer-c':>9} {'V2_4':>8} {'d3 -R1':>8} "
          f"{'d3 auto':>8} {'vs C':>7} {'vs V2':>7}")
    print(f"{'':<28} {'M/s':>8} {'M/s':>9} {'M/s':>8} {'M/s':>8} {'M/s':>8} "
          f"{'(-R1)':>7} {'(-R1)':>7}")
    print("-" * 94)
    for r in rows:
        print(f"{r['name']:<28} {_fmt(r['exe'],8)} {_fmt(r['c'],9)} {_fmt(r['v2'],8)}"
              f" {_fmt(r['d3_1'],8)} {_fmt(r['d3_auto'],8)}"
              f" {_ratio(r['d3_1'], r['c']):>7} {_ratio(r['d3_1'], r['v2']):>7}")
    print("-" * 94)
    print("* dealer.exe runs x86-emulated on an ARM64 VM; dealer-c is the same")
    print("  source built natively, and is the fair comparison for its lineage.")
    _totals(rows)


def _markdown(rows):
    print("| script | dealer.exe* | dealer-c | DealerV2_4 | dealer3 `-R1` | dealer3 auto "
          "| vs dealer-c | vs V2_4 |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|")
    for r in rows:
        print(f"| {r['name']} | {_fmt(r['exe'],1).strip()} | {_fmt(r['c'],1).strip()} "
              f"| {_fmt(r['v2'],1).strip()} | {_fmt(r['d3_1'],1).strip()} "
              f"| {_fmt(r['d3_auto'],1).strip()} "
              f"| {_ratio(r['d3_1'], r['c']).strip()} | {_ratio(r['d3_1'], r['v2']).strip()} |")
    print("\nAll rates in millions of deals evaluated per second; higher is better.")
    print("\n\\* dealer.exe runs x86-emulated on an ARM64 VM. `dealer-c` is the same")
    print("source built natively and is the like-for-like comparison.")
    _totals(rows)


def _totals(rows):
    def gmean(vals):
        vals = [v for v in vals if v]
        if not vals:
            return None
        prod = 1.0
        for v in vals:
            prod *= v
        return prod ** (1 / len(vals))

    print()
    pairs = [(r["d3_1"] / r["c"]) for r in rows if r["d3_1"] and r["c"]]
    if pairs:
        print(f"dealer3 -R1 is {gmean(pairs):.1f}x the original C dealer, built natively "
              "(geometric mean).")
    pairs = [(r["d3_1"] / r["exe"]) for r in rows if r["d3_1"] and r["exe"]]
    if pairs:
        print(f"dealer3 -R1 is {gmean(pairs):.1f}x dealer.exe as run on the VM "
              "(emulated -- not a like-for-like figure).")
    pairs = [(r["d3_1"] / r["v2"]) for r in rows if r["d3_1"] and r["v2"]]
    if pairs:
        print(f"dealer3 -R1 is {gmean(pairs):.1f}x DealerV2_4 across the corpus (geometric mean).")
    pairs = [(r["d3_auto"] / r["d3_1"]) for r in rows if r["d3_auto"] and r["d3_1"]]
    if pairs:
        print(f"Threading (auto) buys {gmean(pairs):.2f}x over single-threaded.")


def _times(x):
    """Render a ratio without the "0.3x faster" trap, which reads as a speedup."""
    if x >= 1.0:
        return f"is {x:.1f}x faster"
    return f"is {1/x:.1f}x SLOWER" if x > 0 else "is immeasurably slower"


def _generation(baseline, rows):
    """Split the win into generating deals and evaluating conditions.

    dealer3 rewrote the RNG and the shuffle, and the originals were slow, so a
    lot of any headline ratio is generation rather than evaluation. Those are
    different pieces of work and improve for different reasons, and a single
    deals/sec number cannot tell them apart -- on a cheap condition the shuffle
    dominates, on an expensive one it barely registers.

    The `_shuffle_baseline` corpus entry runs a near-free condition, so its
    cost per deal is essentially generation. Subtracting it from a real
    script's cost per deal leaves evaluation. Both are then per-deal times, in
    nanoseconds, which subtract honestly -- unlike rates, which do not.
    """
    if not baseline.get("d3_1"):
        return
    print("\nWhere the time goes, per deal (ns)")
    print(f"{'':<12} {'generate':>10} {'evaluate':>10} {'total':>10}"
          f"  {'gen share':>10}")
    print("-" * 58)

    labels = {"exe": "dealer.exe", "c": "dealer-c", "v2": "DealerV2_4",
              "d3_1": "dealer3 -R1", "d3_auto": "dealer3 auto"}
    summary = {}
    for key, label in labels.items():
        gen_rate = baseline.get(key)
        if not gen_rate:
            continue
        totals = [1e9 / r[key] for r in rows if r.get(key)]
        if not totals:
            continue
        gen = 1e9 / gen_rate
        total = sum(totals) / len(totals)
        # A script cannot evaluate in less than no time; if the baseline comes
        # out slower than a real script the difference is noise, not a negative
        # cost, so it is floored rather than reported as one.
        evaluate = max(total - gen, 0.0)
        summary[key] = (gen, evaluate)
        print(f"{label:<12} {gen:>10.0f} {evaluate:>10.0f} {total:>10.0f}"
              f"  {gen/total:>9.0%}")
    print("-" * 58)
    print("Mean over the corpus. 'generate' is the _shuffle_baseline entry:")
    print("RNG and shuffle with a near-free condition.")

    if "d3_auto" in summary:
        print("\n'dealer3 auto' is wall time per deal with every core working, so it is\n"
              "throughput per deal and not work per deal -- the only row here that is\n"
              "not one core's effort. It is also the noisiest: repeats vary by tens of\n"
              "percent where the single-threaded rows vary by one or two.")

    for key, label in (("c", "the original C dealer"), ("v2", "DealerV2_4"),
                       ("exe", "dealer.exe (emulated)")):
        if key in summary and "d3_1" in summary:
            gen_ref, eval_ref = summary[key]
            gen_d3, eval_d3 = summary["d3_1"]
            gen_x = gen_ref / gen_d3 if gen_d3 else 0
            eval_x = eval_ref / eval_d3 if eval_d3 else 0
            print(f"\nAgainst {label}: dealer3 {_times(gen_x)} at generating "
                  f"and {_times(eval_x)} at evaluating.")


def _delta(old, new, old_name):
    """Before/after on the same corpus, which is the point of keeping results.

    Only comparable when both sides ran the same corpus: the deal counts are
    calibrated per script, so a row measured against a different corpus is
    describing a different amount of work. Rows present on one side only are
    listed rather than silently dropped, because a script disappearing is
    usually a corpus change rather than a result.
    """
    print(f"\nChange against {old_name}")
    if old.get("source_id") and new.get("source_id"):
        if old["source_id"] == new["source_id"]:
            print(f"  NOTE: both sides have source {old['source_id']} -- the code did "
                  "not change, so this is run-to-run noise.")
        else:
            print(f"  source {old['source_id']} -> {new['source_id']}")
    print(f"{'script':<28} {'before':>10} {'after':>10} {'change':>10}")
    print("-" * 62)

    ratios = []
    for name, rec in new["scripts"].items():
        before = old.get("scripts", {}).get(name, {}).get("single", {}).get("deals_per_sec")
        after = rec.get("single", {}).get("deals_per_sec")
        if not before or not after:
            continue
        ratio = after / before
        ratios.append(ratio)
        arrow = "faster" if ratio >= 1 else "SLOWER"
        shown = ratio if ratio >= 1 else 1 / ratio
        print(f"{name:<28} {before/1e6:>9.3f} {after/1e6:>9.3f} {shown:>8.2f}x {arrow}")
    print("-" * 62)
    print("M deals/s, single-threaded (-R 1).")

    missing = set(old.get("scripts", {})) ^ set(new["scripts"])
    if missing:
        print(f"Not compared (present on one side only): {', '.join(sorted(missing))}")

    if ratios:
        prod = 1.0
        for r in ratios:
            prod *= r
        g = prod ** (1 / len(ratios))
        verdict = f"{g:.2f}x faster" if g >= 1 else f"{1/g:.2f}x slower"
        print(f"\nOverall: {verdict} across {len(ratios)} scripts (geometric mean).")


def _scaling(d3):
    per_thread = {}
    for rec in d3["scripts"].values():
        rows = rec.get("sweep", {}).get("default", {})
        base = rows.get("1", {}).get("seconds_best")
        if not base:
            continue
        for n_str, r in rows.items():
            per_thread.setdefault(int(n_str), []).append(base / r["seconds_best"])
    if not per_thread:
        return

    print("\nThread scaling (corpus mean)")
    print(f"{'threads':>7} {'speedup':>9} {'efficiency':>11}  {'':<24}")
    print("-" * 56)
    peak_n, peak_sp = None, 0.0
    scale = {}
    for n in sorted(per_thread):
        sp = sum(per_thread[n]) / len(per_thread[n])
        scale[n] = sp
        if sp > peak_sp:
            peak_n, peak_sp = n, sp
    for n in sorted(scale):
        bar = "#" * int(scale[n] * 4)
        print(f"{n:>7} {scale[n]:>8.2f}x {scale[n]/n:>10.0%}  {bar}")
    print("-" * 56)
    print(f"Peak {peak_sp:.2f}x at {peak_n} threads.")
    tail = [n for n in sorted(scale) if n > peak_n]
    if tail and min(scale[n] for n in tail) < peak_sp * 0.92:
        print(f"Falls to {min(scale[n] for n in tail):.2f}x beyond that -- more workers,")
        print("less throughput. That is contention, not saturation.")


if __name__ == "__main__":
    main()
