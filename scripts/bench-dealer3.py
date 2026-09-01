#!/usr/bin/env python3
"""
bench-dealer3.py - Measure dealer3 over the benchmark corpus.

This is the one to run whenever dealer3 changes substantively. It records
three things:

  - **Single-threaded** (`-R 1`), the number that compares like-for-like
    against dealer.exe and DealerV2_4, which have no threading.
  - **Auto** (`-R 0`), what a user actually gets by default.
  - **A thread sweep**, one run per worker count, which is what turns "our
    multithreading peaks somewhere around 4-5" into a curve with a number on it.

The sweep is the diagnostic. For each script it reports speedup against the
single-threaded time and marks where the curve turns over. A curve that
*plateaus* past the physical core count is ordinary saturation; one that
*regresses* -- gets slower with more workers -- is contention, and that is a
bug rather than a limit. The two look identical if you only ever measure at one
workload size, so --scale exists to check the shape holds:

    scripts/bench-dealer3.py --sweep-only --scale 0.2     # short runs
    scripts/bench-dealer3.py --sweep-only --scale 4       # long runs

If the turnover moves when the workload does, the cost is per-run (thread
startup, the final merge) rather than per-deal. If it stays put, it is
contention in the generate loop.

--batch-sizes adds a second dimension, since the work-unit size is the obvious
suspect for a shared-state bottleneck.

Usage:
    scripts/bench-dealer3.py [-r 3] [--sweep-only | --no-sweep]

Options:
    -r, --repeats N     Runs per measurement, fastest wins (default: 3)
    --threads LIST      Sweep these worker counts (default: derived from CPU count)
    --batch-sizes LIST  Also sweep --batch-size at each thread count
    --scale F           Multiply every script's calibrated deal count by F
    --scripts NAMES     Comma-separated subset of the corpus
    -1, --quick         One representative script only. For iterating on a
                        performance change, where the whole corpus is too slow
                        a loop and the absolute number matters less than
                        whether it moved.
    --sweep-only        Only the thread sweep
    --no-sweep          Skip the thread sweep
    --binary PATH       dealer3 binary (default: target/release/dealer)
    -o, --output PATH   Where to write results (default: bench/results/dealer3-<rev>.json)
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchlib as bl

# These runs take minutes and are usually backgrounded into a file, where
# Python would block-buffer and show nothing until the very end.
sys.stdout.reconfigure(line_buffering=True)


def default_thread_list():
    """1..physical cores in steps of 1, then coarser out to every logical CPU.

    The interesting region is at and just past the physical core count, so it
    is sampled densely; beyond that the question is only whether the curve
    keeps falling, which needs fewer points.
    """
    try:
        ncpu = int(subprocess.run(["sysctl", "-n", "hw.ncpu"], capture_output=True,
                                  text=True, timeout=5).stdout.strip())
    except (OSError, ValueError, subprocess.SubprocessError):
        ncpu = 8
    try:
        perf = int(subprocess.run(["sysctl", "-n", "hw.perflevel0.logicalcpu"],
                                  capture_output=True, text=True, timeout=5).stdout.strip())
    except (OSError, ValueError, subprocess.SubprocessError):
        perf = ncpu
    dense = list(range(1, min(perf, ncpu) + 1))
    coarse = [n for n in range(perf + 2, ncpu + 1, 2)]
    return sorted(set(dense + coarse + [ncpu]))


def main():
    ap = argparse.ArgumentParser(description="Benchmark dealer3 over the corpus.")
    ap.add_argument("-r", "--repeats", type=int, default=3)
    ap.add_argument("--threads", default=None, help="comma-separated worker counts")
    ap.add_argument("--batch-sizes", default=None, help="comma-separated --batch-size values")
    ap.add_argument("--scale", type=float, default=1.0)
    ap.add_argument("--min-sweep-seconds", type=float, default=0.75,
                    help="scale the sweep up until the fastest thread count runs "
                         "at least this long (default: 0.75; 0 disables)")
    ap.add_argument("--scripts", default=None, help="comma-separated corpus subset")
    ap.add_argument("-1", "--quick", action="store_true",
                    help="one representative script only, for iterating on a change")
    ap.add_argument("--sweep-only", action="store_true")
    ap.add_argument("--no-sweep", action="store_true")
    ap.add_argument("--binary", default=None)
    ap.add_argument("-o", "--output", default=None)
    args = ap.parse_args()

    target = bl.dealer3_target(args.binary)
    ok, why = target.available()
    if not ok:
        sys.exit(f"error: {why} -- run ./dev-build.sh build --release")

    corpus = bl.load_corpus()
    # Verify-only entries are for bench-verify.py. They are not timed: their
    # cost is dominated by solving produced deals, and the programs do not
    # produce the same number from a fixed -g, so the figure would not compare.
    corpus = [e for e in corpus if not e.get("verify_only")]
    if args.scripts:
        wanted = {s.strip() for s in args.scripts.split(",")}
        corpus = [e for e in corpus if e["name"] in wanted]
        if not corpus:
            sys.exit("error: no corpus scripts matched --scripts")
    elif args.quick:
        corpus = [bl.representative(corpus)]
        print(f"--quick: {corpus[0]['name']} only\n")

    threads = ([int(t) for t in args.threads.split(",")] if args.threads
               else default_thread_list())
    batch_sizes = [int(b) for b in args.batch_sizes.split(",")] if args.batch_sizes else [None]

    rev = bl.git_describe()
    machine = bl.machine_info()
    print(f"dealer3 {rev} on {machine.get('cpu') or machine.get('model') or 'unknown'} "
          f"({machine.get('logical_cpus', '?')} logical, "
          f"{machine.get('performance_cpus', '?')}P + {machine.get('efficiency_cpus', '?')}E)")
    print(f"source {bl.source_fingerprint()}, {len(corpus)} scripts, "
          f"{args.repeats} repeats, fastest run wins\n")

    results = {"tool": "bench-dealer3", "revision": rev,
               "source_id": bl.source_fingerprint(),
               "corpus_id": json.loads(bl.CORPUS_INDEX.read_text()).get("corpus_id"),
               "repeats": args.repeats,
               "scale": args.scale, "scripts": {}}

    for entry in corpus:
        deals = max(1000, int(entry["deals"] * args.scale))
        name = entry["name"]
        record = {"deals": deals, "hit_rate": entry.get("hit_rate")}
        print(f"{name}  ({deals:,} deals)")

        if not args.sweep_only:
            for label, nthreads in (("single", 1), ("auto", 0)):
                try:
                    r = bl.run_repeated(target, entry["path"], deals,
                                        threads=nthreads, repeats=args.repeats)
                except bl.BenchError as exc:
                    print(f"    {label:<8} FAILED: {exc}")
                    continue
                record[label] = r
                print(f"    {label:<8} {r['seconds_best']:7.3f}s"
                      f"  {r['deals_per_sec']/1e6:6.2f} M deals/s"
                      f"  (spread {r['spread_pct']:.1f}%)")
                _warn_spread(label, r)

        if not args.no_sweep:
            sweep_deals = _size_sweep(target, entry["path"], deals, max(threads),
                                      args.min_sweep_seconds)
            if sweep_deals != deals:
                print(f"    sweep scaled to {sweep_deals:,} deals so the fastest"
                      f" thread count still runs >{args.min_sweep_seconds}s")
            record["sweep_deals"] = sweep_deals
            sweep = {}
            for batch in batch_sizes:
                key = "default" if batch is None else str(batch)
                rows = {}
                for n in threads:
                    try:
                        r = _run_with_batch(target, entry["path"], sweep_deals, n, batch,
                                            args.repeats)
                    except bl.BenchError as exc:
                        print(f"    -R {n:<3} FAILED: {exc}")
                        continue
                    rows[str(n)] = r
                sweep[key] = rows
                _print_sweep(rows, threads, prefix=(f"    batch={key}  " if batch else "    "))
            record["sweep"] = sweep

        results["scripts"][name] = record
        print()

    # A partial run must never land on the canonical path. Otherwise a quick
    # --quick or --scripts check silently overwrites a full corpus run measured
    # against the same revision, and the record for that revision is gone with
    # nothing to say it happened. (Which is exactly how this was found.)
    partial = bool(args.scripts or args.quick or args.no_sweep or args.sweep_only)
    if args.output:
        out = Path(args.output)
    else:
        suffix = "-partial" if partial else ""
        # Named for the source that produced the binary, not the repo state --
        # see benchlib.source_fingerprint(). Mirrors reference-<corpus_id>.json.
        out = bl.RESULTS_DIR / f"dealer3-{results['source_id']}{suffix}.json"
    results["partial"] = partial
    if partial:
        results["partial_reason"] = {
            "scripts": args.scripts, "quick": args.quick,
            "no_sweep": args.no_sweep, "sweep_only": args.sweep_only,
        }
    bl.write_results(out, results)
    print(f"Wrote {out.relative_to(bl.REPO) if bl.REPO in out.parents else out}")

    if not args.no_sweep:
        _summarise_scaling(results, threads)


def _size_sweep(target, script, deals, max_threads, floor):
    """Grow the deal count until even the fastest configuration runs long enough.

    A sweep is only worth reading if every point in it is a real measurement.
    The calibration in bench-corpus.py sizes runs against `-R 1`; at the top of
    the sweep the same work finishes several times faster, and a run of a
    couple hundred milliseconds is mostly thread startup and the final merge.
    Measured that way the curve comes out monotone and the interesting part --
    where throughput turns over -- is buried in noise.

    So: pilot the highest thread count, and if it lands under `floor`, scale
    the whole sweep up by the shortfall. Every thread count then runs the same
    (larger) number of deals, which is what keeps the speedup ratios valid.
    """
    if floor <= 0:
        return deals
    try:
        pilot = bl.run_once(target, script, deals, threads=max_threads)
    except bl.BenchError:
        return deals
    if pilot.seconds >= floor:
        return deals
    factor = floor / pilot.seconds if pilot.seconds > 0 else 8.0
    return int(deals * min(factor * 1.2, 50))


def _warn_spread(label, r, limit=15.0):
    """A wide spread means the machine was busy, not that the code got slower."""
    if r.get("spread_pct", 0) > limit:
        print(f"    {'':<8} note: {label} varied {r['spread_pct']:.0f}% across runs; "
              "treat it as indicative only")


def _run_with_batch(target, script, deals, nthreads, batch, repeats):
    """run_repeated, with --batch-size appended when one is being swept."""
    if batch is None:
        return bl.run_repeated(target, script, deals, threads=nthreads, repeats=repeats)
    patched = bl.Target(name=target.name, kind=target.kind,
                        command=target.command + ["--batch-size", str(batch)],
                        threaded=True, note=target.note,
                        verbose_flag=target.verbose_flag)
    return bl.run_repeated(patched, script, deals, threads=nthreads, repeats=repeats)


def _print_sweep(rows, threads, prefix="    "):
    base = rows.get("1", {}).get("seconds_best")
    cells = []
    best_n, best_speedup = None, 0.0
    for n in threads:
        r = rows.get(str(n))
        if not r:
            continue
        if base:
            sp = base / r["seconds_best"]
            if sp > best_speedup:
                best_n, best_speedup = n, sp
            cells.append(f"{n}:{sp:.2f}x")
        else:
            cells.append(f"{n}:{r['seconds_best']:.3f}s")
    print(prefix + "  ".join(cells))
    if best_n is not None:
        tail = [n for n in threads if n > best_n and str(n) in rows]
        verdict = ""
        if tail:
            worst_tail = min(base / rows[str(n)]["seconds_best"] for n in tail)
            if worst_tail < best_speedup * 0.92:
                verdict = "  <- REGRESSES past peak"
        print(f"{prefix}peak {best_speedup:.2f}x at {best_n} threads{verdict}")


def _summarise_scaling(results, threads):
    """Aggregate the sweep across scripts -- the headline for the threading work."""
    print("\n" + "=" * 66)
    print("Thread scaling, averaged over the corpus")
    print("=" * 66)
    print(f"{'threads':>7} {'speedup':>9} {'efficiency':>11}")
    print("-" * 66)

    per_thread = {}
    for record in results["scripts"].values():
        rows = record.get("sweep", {}).get("default", {})
        base = rows.get("1", {}).get("seconds_best")
        if not base:
            continue
        for n_str, r in rows.items():
            per_thread.setdefault(int(n_str), []).append(base / r["seconds_best"])

    if not per_thread:
        return
    peak_n, peak_sp = None, 0.0
    for n in sorted(per_thread):
        sp = sum(per_thread[n]) / len(per_thread[n])
        if sp > peak_sp:
            peak_n, peak_sp = n, sp
        print(f"{n:>7} {sp:>8.2f}x {sp/n:>10.0%}")

    print("-" * 66)
    print(f"Peak {peak_sp:.2f}x at {peak_n} threads.")
    tail = [n for n in sorted(per_thread) if n > peak_n]
    if tail:
        worst = min(sum(per_thread[n]) / len(per_thread[n]) for n in tail)
        if worst < peak_sp * 0.92:
            print(f"Past the peak it falls back to {worst:.2f}x. Adding workers makes")
            print("it slower, which is contention rather than saturation -- re-run with")
            print("--scale to see whether the turnover moves with the workload size.")


if __name__ == "__main__":
    main()
