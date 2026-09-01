#!/usr/bin/env python3
"""
bench-reference.py - Measure dealer.exe and DealerV2_4 over the corpus.

The slow half of the comparison, and the half that rarely needs re-running.
dealer.exe lives on the Windows VM and every run costs an SSH round trip;
neither program changes. Re-run this only when the corpus changes -- or when
the VM or the DealerV2_4 build does, since both are part of what the numbers
describe.

Results go to `bench/results/reference-<corpus-rev>.json`, keyed to the corpus
they were measured against. bench-report.py refuses to compare a dealer3 run
against reference numbers taken on a different corpus, because the calibrated
deal counts would differ and the ratio would be meaningless.

Timing differs by target, and the script says which it used. DealerV2_4's own
`Time needed` is trusted if it reports one. dealer.exe's is not: on Windows it
prints `Time needed 0.000 sec` however long the run took, so that target is
wall-clocked and the SSH round trip plus process startup -- measured once at
the top of the run, about 0.28s -- is subtracted. Runs are sized so the
residual error in that subtraction is well under the spread between repeats.

Usage:
    scripts/bench-reference.py [-r 3] [--only dealer.exe]

Options:
    -r, --repeats N    Runs per measurement, fastest wins (default: 3)
    --only NAME        Just one target ("dealer.exe", "dealer-c" or "dealerv2_4")
    --scripts NAMES    Comma-separated corpus subset
    -1, --quick        One representative script only
    --scale F          Multiply every script's calibrated deal count by F
    --dealerv2 PATH    DealerV2_4 binary (else $DEALERV2_BIN, else PATH)
    --dealer-c PATH    Natively-built original C dealer (else $DEALER_C_BIN)
    --ssh-timeout S    Per-run timeout for the Windows VM (default: 600)
    -o, --output PATH  Where to write results
"""
import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchlib as bl

# These runs take minutes and are usually backgrounded into a file, where
# Python would block-buffer and show nothing until the very end.
sys.stdout.reconfigure(line_buffering=True)


def main():
    ap = argparse.ArgumentParser(description="Benchmark dealer.exe and DealerV2_4.")
    ap.add_argument("-r", "--repeats", type=int, default=3)
    ap.add_argument("--only", default=None)
    ap.add_argument("--scripts", default=None)
    ap.add_argument("-1", "--quick", action="store_true",
                    help="one representative script only")
    ap.add_argument("--scale", type=float, default=1.0)
    ap.add_argument("--dealerv2", default=None)
    ap.add_argument("--dealer-c", default=None,
                    help="natively-built original C dealer (else $DEALER_C_BIN)")
    ap.add_argument("--ssh-timeout", type=int, default=600)
    ap.add_argument("-o", "--output", default=None)
    args = ap.parse_args()

    corpus = bl.load_corpus()
    # Verify-only entries are for bench-verify.py. They are not timed: their
    # cost is dominated by solving produced deals, and the programs do not
    # produce the same number from a fixed -g, so the figure would not compare.
    corpus = [e for e in corpus if not e.get("verify_only")]
    index = json.loads(bl.CORPUS_INDEX.read_text())
    if args.scripts:
        wanted = {s.strip() for s in args.scripts.split(",")}
        corpus = [e for e in corpus if e["name"] in wanted]
        if not corpus:
            sys.exit("error: no corpus scripts matched --scripts")
    elif args.quick:
        corpus = [bl.representative(corpus)]
        print(f"--quick: {corpus[0]['name']} only\n")

    targets = [bl.dealer_exe_target(args.ssh_timeout),
               bl.dealer_c_target(args.dealer_c),
               bl.dealerv2_target(args.dealerv2)]
    if args.only:
        targets = [t for t in targets if t.name == args.only]
        if not targets:
            sys.exit(f"error: unknown target {args.only!r} "
                     "(expected 'dealer.exe', 'dealer-c' or 'dealerv2_4')")

    live = []
    for t in targets:
        ok, why = t.available()
        if ok:
            live.append(t)
        else:
            print(f"skipping {t.name}: {why}")
            if t.name == "dealer-c":
                print("  Build it with scripts/build-dealer-c-macos.sh.")
            elif t.name == "dealerv2_4":
                print("  Build it and point --dealerv2 or $DEALERV2_BIN at the binary.")
                print("  The upstream repo ships only a Linux x86-64 build.")
            elif t.name == "dealer.exe":
                print("  Set WINDOWS_HOST / WINDOWS_USER / WINDOWS_GITHUB_HOME in ~/.zshrc.")
    if not live:
        sys.exit("error: no reference targets available")

    # dealer.exe reads the script off the shared drive, so the corpus has to
    # sit under the directory Parallels maps into the VM. win-dealer.sh raises
    # the same objection later; catching it here saves a slow failure.
    import os
    gh = os.environ.get("WINDOWS_GITHUB_HOME")
    if any(t.kind == "ssh" for t in live) and gh and not str(bl.CORPUS_DIR).startswith(gh):
        sys.exit(f"error: corpus at {bl.CORPUS_DIR} is not under $WINDOWS_GITHUB_HOME "
                 f"({gh}), so the VM cannot read it")

    print(f"Corpus {index.get('corpus_id', '?')}: {len(corpus)} scripts, "
          f"{args.repeats} repeats, fastest run wins")
    print(f"Targets: {', '.join(t.name for t in live)}\n")

    # Anything wall-clocked needs its fixed cost measured before the real runs,
    # so it can be subtracted rather than counted as dealing time.
    for t in live:
        if t.timing != "self":
            overhead = bl.calibrate_overhead(t, corpus[0]["path"])
            print(f"{t.name}: {overhead:.2f}s startup/SSH overhead, "
                  f"subtracted from every wall-clocked run")
    if any(t.timing != "self" for t in live):
        print()

    vm = bl.windows_vm_info() if any(t.kind == "ssh" for t in live) else {}
    if vm:
        print(f"Windows VM: {vm['architecture']}, {vm['cpus']} cpus, {vm['identifier']}")
        if "ARM" in vm["architecture"].upper():
            print("  NOTE: dealer.exe is a 32-bit x86 binary, so on this ARM64 VM it runs")
            print("  under emulation. Its throughput is what you get in practice, but it")
            print("  is not a like-for-like measure of the C implementation. Compare")
            print("  dealer-c -- the same source built natively -- for that.")
        print()

    results = {"tool": "bench-reference", "windows_vm": vm,
               "corpus_id": index.get("corpus_id"),
               "corpus_revision": index.get("built_with"),
               "repeats": args.repeats, "scale": args.scale,
               "targets": {t.name: t.note for t in live}, "scripts": {}}

    for entry in corpus:
        deals = max(1000, int(entry["deals"] * args.scale))
        print(f"{entry['name']}  ({deals:,} deals)")
        record = {"deals": deals}
        for target in live:
            try:
                r = bl.run_repeated(target, entry["path"], deals,
                                    repeats=args.repeats, timeout=args.ssh_timeout + 60)
            except bl.BenchError as exc:
                print(f"    {target.name:<12} FAILED: {exc}")
                record[target.name] = {"error": str(exc)}
                continue
            record[target.name] = r
            overhead = r["wall_best"] - r["seconds_best"]
            print(f"    {target.name:<12} {r['seconds_best']:8.3f}s"
                  f"  {r['deals_per_sec']/1e6:6.3f} M deals/s"
                  f"  (spread {r['spread_pct']:.1f}%, {overhead:.1f}s overhead excluded)")
        results["scripts"][entry["name"]] = record
        print()

    rev = index.get("corpus_id") or index.get("built_with", "unknown")
    # As in bench-dealer3.py: a subset run must not overwrite the full record.
    partial = bool(args.scripts or args.quick or args.only)
    results["partial"] = partial
    if args.output:
        out = Path(args.output)
    else:
        out = bl.RESULTS_DIR / f"reference-{rev}{'-partial' if partial else ''}.json"
    bl.write_results(out, results)
    print(f"Wrote {out.relative_to(bl.REPO) if bl.REPO in out.parents else out}")


if __name__ == "__main__":
    main()
