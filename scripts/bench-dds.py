#!/usr/bin/env python3
"""
bench-dds.py - Compare dealer3 and DealerV2_4 on double-dummy throughput.

Separate from bench-reference.py because a solver run is a different
measurement. There, work is fixed with -g and the answer is deals evaluated per
second. Here nearly all the time goes into solving the small fraction of deals
that pass the filter, and the answer wanted is *cost per solved deal*.

dealer.exe and dealer-c have no solver and are not involved.

How it isolates the solver
--------------------------

Each program runs the same condition twice: once with a double-dummy action,
once with a cheap one. The difference is the solving, and everything else --
acquiring the deals, evaluating the filter, the run's fixed overhead --
cancels, because it is the same program doing the same work in both runs.

That matters most for dealer3, which does not acquire its deals the way
DealerV2_4 does. There is no way to hand both the same boards directly: V2_4
reads its own binary .zrd libraries and dealer3 reads PBN. So V2_4 filters its
own shuffle and writes the deals it matched, and dealer3 solves exactly those.
dealer3's PBN ingestion costs 3255ns a deal against a shuffle's 269, and V2_4
additionally filters thousands of deals dealer3 never sees -- subtracting each
program's own no-solver run removes all of that, rather than hoping it is
small.

Both programs therefore solve the same cards, which also makes this a
correctness check: the statistics must agree, or the throughput comparison is
comparing different answers.

Why the deals are captured with the condition already applied
-------------------------------------------------------------

The obvious arrangement -- have V2_4 write every deal it generates, then let
dealer3 filter the same file -- does not work. **DealerV2_4's generated
sequence depends on the condition in the script.** Measured directly: with an
always-true condition its produced deals are exactly the unfiltered sequence,
while `hcp(north) >= 20`, `shape(north, any 4333)` and `hascard(east, 2C)` each
produce deals that appear nowhere in the unfiltered run of the same seed -- not
under any seat rotation, and not merely renumbered. Two runs that differ only
in their condition are looking at different cards, whatever seed they were
given.

The action does not have this effect: the same condition with `printpbn`,
with an `average`, and with a `dds` average all produce identical deals. So the
capture run uses the timed condition and differs only in its action, which is
sound, while a capture run with no condition would not be.

The mechanism was not chased down; the fact was established and designed
around.

Why DealerV2_4 gets -M 2, and why it cannot be single-threaded
--------------------------------------------------------------

`par` needs all twenty double-dummy results. dealer3 fills the whole table, so
V2_4 must be in table mode (-M 2) or the two are solving different amounts of
work. V2_4 detects this itself and switches -- and then dies inside DDS with
"Memory::GetPtr: 0 vs. 0", because the switch happens after the library has
been given board-mode resources. Passing -M 2 up front avoids both problems.

Table mode will not run single-threaded: setup_dds_mode() overrides nThreads
to TblModeThreads whenever dds_mode is 2 and nThreads is below 2. So --threads
is clamped to at least 2 and the figure is a threaded one for both programs.
A single-threaded double-dummy comparison is not available from V2_4.

Usage:
    scripts/bench-dds.py [-g 20000] [-s 1] [-r 3] [--threads 4]

Options:
    -g, --generate N   Deals each program processes (default: 20000)
    -s, --seed SEED    Seed for DealerV2_4's shuffle (default: 1)
    -r, --repeats N    Runs per measurement, fastest wins (default: 3)
    --threads T        Worker/DDS threads for both, minimum 2 (default: 4)
    --condition EXPR   Override the filter; aim for a few percent acceptance
    --dealerv2 PATH    DealerV2_4 binary (else $DEALERV2_BIN, else PATH)
    --keep             Leave the generated scripts and PBN in place
"""
import argparse
import re
import resource
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchlib as bl

sys.stdout.reconfigure(line_buffering=True)

# About 2.9% of deals, measured. Low enough that solving dominates without the
# run taking all day, high enough to reach a few hundred solves quickly.
# `hascard` takes rank then suit, so the club two is `2C`.
DEFAULT_CONDITION = "hascard(east, 2C) and hcp(north) >= 12 and hcp(south) >= 11"

# Every token has to parse in both languages. DealerV2_4's lexer is
# case-sensitive and inconsistent about it -- compasses lowercase, sides
# uppercase -- so `par(NS)` with `dds(north, ...)` is the spelling that runs on
# both unchanged. par is the strongest thing to ask for: it is derived from all
# twenty results, so agreement means the whole table agrees.
DD_ACTION = ('action average "dds N NT" dds(north, notrump), '
             'average "dds S spades" dds(south, spades), '
             'average "par NS" par(NS)')
CHEAP_ACTION = 'action average "hcp N" hcp(north)'

PBN_DEAL_RE = re.compile(r'^\[Deal\s+"', re.MULTILINE)


def main():
    ap = argparse.ArgumentParser(description="Compare dealer3 and DealerV2_4 on "
                                             "double-dummy throughput.")
    ap.add_argument("-g", "--generate", type=int, default=20000)
    ap.add_argument("-s", "--seed", type=int, default=1)
    ap.add_argument("-r", "--repeats", type=int, default=3)
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--condition", default=DEFAULT_CONDITION)
    ap.add_argument("--dealerv2", default=None)
    ap.add_argument("--keep", action="store_true")
    args = ap.parse_args()

    threads = max(2, args.threads)
    if threads != args.threads:
        print(f"note: --threads raised to {threads}; DealerV2_4's table mode "
              "overrides anything below 2\n")

    dealer3 = bl.dealer3_target()
    v2 = bl.dealerv2_target(args.dealerv2)
    for t in (dealer3, v2):
        ok, why = t.available()
        if not ok:
            sys.exit(f"error: {t.name} unavailable ({why})")

    work = bl.REPO / "bench" / ".dds"
    work.mkdir(parents=True, exist_ok=True)

    dd = work / "dd.dlr"
    nodd = work / "nodd.dlr"
    emit = work / "emit.dlr"
    pbn = work / "deals.pbn"
    header = f"# Generated by scripts/bench-dds.py\ncondition {args.condition}\n"
    dd.write_text(header + DD_ACTION + "\n")
    nodd.write_text(header + CHEAP_ACTION + "\n")
    # Same condition as the timed runs -- see the module docstring on why an
    # unfiltered capture would be looking at different cards.
    emit.write_text(header + "action printpbn\n")

    print(f"condition : {args.condition}")
    print(f"deals     : {args.generate:,}   seed {args.seed}   "
          f"threads {threads}   best of {args.repeats}\n")

    # DealerV2_4 shuffles; dealer3 has no way to read its libraries, so the
    # boards are handed over as PBN. Untimed: this run only exists to produce
    # the file, and it prints every deal, which the timed runs do not.
    print(f"Capturing the deals DealerV2_4 matches in {args.generate:,} "
          f"(seed {args.seed}) ...")
    out = subprocess.run(
        v2.command + ["-g", str(args.generate), "-p", str(bl.NO_PRODUCE_LIMIT),
                      "-s", str(args.seed), str(emit)],
        capture_output=True, text=True, timeout=1800)
    n_written = len(PBN_DEAL_RE.findall(out.stdout))
    if n_written == 0:
        sys.exit("error: the condition matched nothing -- loosen it or raise -g")
    pbn.write_text(out.stdout)
    print(f"    {n_written:,} matched ({n_written/args.generate:.1%}), "
          f"{pbn.stat().st_size/1e6:.1f} MB\n")

    results = {}
    for name, argv_for in (
        ("dealerv2_4", lambda script, t=None: v2.command
            + ["-M", "2", "-R", str(t or threads), "-g", str(args.generate),
               "-p", str(bl.NO_PRODUCE_LIMIT), "-s", str(args.seed), str(script)]),
        # dealer3 is handed the deals V2_4 already matched, so it re-applies the
        # same condition to deals that all satisfy it. Anything it rejects is a
        # semantic disagreement, and _verdict says so.
        ("dealer3", lambda script, t=None: dealer3.command
            + ["-v", "-R", str(t or threads), "-p", str(bl.NO_PRODUCE_LIMIT),
               "--input-deals", str(pbn), str(script)]),
    ):
        print(f"{name}")
        with_dd = _timed(argv_for(dd), args.repeats, name, "with solver")
        without = _timed(argv_for(nodd), args.repeats, name, "no solver")
        solved = with_dd["produced"]
        if solved != without["produced"]:
            sys.exit(f"error: {name} produced {solved} deals with the solver and "
                     f"{without['produced']} without -- the condition is not stable")
        dd_seconds = with_dd["seconds"] - without["seconds"]
        dd_cpu = with_dd["cpu"] - without["cpu"]
        results[name] = {
            "with": with_dd, "without": without, "solved": solved,
            "dd_seconds": dd_seconds, "dd_cpu": dd_cpu,
            "parallelism": dd_cpu / dd_seconds if dd_seconds > 0 else float("nan"),
            "ns_per_solve": dd_seconds / solved * 1e9 if solved else float("nan"),
            "cpu_per_solve": dd_cpu / solved * 1e9 if solved else float("nan"),
        }
        print(f"    with solver  {with_dd['seconds']:8.3f}s")
        print(f"    no solver    {without['seconds']:8.3f}s")
        print(f"    solving      {dd_seconds:8.3f}s over {solved:,} deals"
              f"  ->  {results[name]['ns_per_solve']/1e6:.2f} ms per solved deal")
        par = results[name]["parallelism"]
        note = "single-threaded" if par < 1.2 else f"{par:.1f} cores busy"
        print(f"    cpu          {dd_cpu:8.3f}s"
              f"  ->  {results[name]['cpu_per_solve']/1e6:.1f} ms cpu per solve"
              f"  ({note})\n")

    _verdict(results, args, threads)

    if not args.keep:
        for f in (dd, nodd, emit, pbn):
            f.unlink(missing_ok=True)
        try:
            work.rmdir()
        except OSError:
            pass


def _timed(argv, repeats, name, label):
    """Best of `repeats`: wall seconds, CPU seconds, and the produced count.

    CPU comes from getrusage(RUSAGE_CHILDREN), differenced across the call.
    Measuring it is the whole point: a thread count on the command line says
    what was *asked for*, not what was used, and the two programs disagree about
    what the switch even means. CPU divided by wall says how many cores actually
    worked, and needs no assumption at all.
    """
    best, produced, stats, best_cpu = None, None, None, None
    for _ in range(repeats):
        before = resource.getrusage(resource.RUSAGE_CHILDREN)
        started = time.monotonic()
        proc = subprocess.run(argv, capture_output=True, text=True, timeout=7200)
        wall = time.monotonic() - started
        after = resource.getrusage(resource.RUSAGE_CHILDREN)
        cpu = ((after.ru_utime - before.ru_utime)
               + (after.ru_stime - before.ru_stime))
        if proc.returncode != 0:
            sys.exit(f"error: {name} ({label}) exit {proc.returncode}: "
                     f"{bl._first_real_line(proc.stdout + proc.stderr)}")
        text = proc.stdout + proc.stderr
        m = bl.TIME_RE.search(text)
        # DealerV2_4 reports honestly; if a program ever does not, wall time is
        # the fallback. Both runs of a pair use the same clock, so the
        # subtraction stays valid either way.
        seconds = float(m.group(1)) if m and float(m.group(1)) > 0 else wall
        p = bl.PRODUCED_RE.search(text)
        produced = int(p.group(1)) if p else 0
        if best is None or seconds < best:
            best, stats, best_cpu = seconds, _statistics(text), cpu
    return {"seconds": best, "cpu": best_cpu, "produced": produced, "stats": stats}


def _statistics(text):
    """The `average` lines, by label and value, tolerating each program's format.

    DealerV2_4 writes `label: Mean=   10.6600, Std Dev= ...` where dealer3
    writes `label: 10.66`. Same answer, different spelling.
    """
    out = {}
    for line in text.splitlines():
        m = re.match(r"^([^:]*?):\s*(?:Mean=\s*)?(-?\d+(?:\.\d+)?)", line.strip())
        if m and "Time needed" not in line:
            out[m.group(1).strip()] = float(m.group(2))
    return out


def _x(dealer3, other):
    """dealer3's cost against another program's, in words.

    Takes the two costs rather than a ratio, because a bare ratio invites
    getting the direction backwards -- which is exactly what happened here
    once. Costs, so larger is worse.
    """
    if other <= 0:
        return "not comparable"
    ratio = dealer3 / other
    return f"{ratio:.2f}x slower" if ratio >= 1 else f"{1/ratio:.2f}x faster"


def _verdict(results, args, threads):
    a, b = results["dealerv2_4"], results["dealer3"]

    print("=" * 64)
    if a["solved"] != b["solved"]:
        print(f"WARNING: DealerV2_4 solved {a['solved']:,} deals and dealer3 "
              f"{b['solved']:,}.")
        print("dealer3 was handed the very deals DealerV2_4 matched, so it should")
        print("accept every one. A shortfall is a real disagreement about what the")
        print("condition means. The figures below are not comparing the same work.")
    else:
        print(f"Both solved the same {a['solved']:,} deals "
              f"({a['solved']/args.generate:.1%} of {args.generate:,} generated).")

    shared = set(a["with"]["stats"]) & set(b["with"]["stats"])
    disagree = [k for k in sorted(shared)
                if round(a["with"]["stats"][k], 2) != round(b["with"]["stats"][k], 2)]
    if disagree:
        print("\nWARNING: the two disagree on double-dummy results over identical")
        print("cards, so this is not a like-for-like comparison:")
        for k in disagree:
            print(f"  {k}: DealerV2_4 {a['with']['stats'][k]}, "
                  f"dealer3 {b['with']['stats'][k]}")
    elif shared:
        print(f"Statistics agree on all {len(shared)} measures, so both solved the "
              "same cards to the same answers.")

    print()
    print(f"{'':<12} {'wall/solve':>11} {'cpu/solve':>11} {'cores busy':>11}"
          f" {'solves/s':>9}")
    print("-" * 68)
    for name, r in (("DealerV2_4", a), ("dealer3", b)):
        rate = r["solved"] / r["dd_seconds"] if r["dd_seconds"] > 0 else float("nan")
        print(f"{name:<12} {r['ns_per_solve']/1e6:>10.1f}ms"
              f" {r['cpu_per_solve']/1e6:>10.1f}ms {r['parallelism']:>10.1f}x"
              f" {rate:>9.1f}")
    print("-" * 68)

    # Cores busy is CPU divided by wall, measured rather than asked for. It is
    # the only honest way to compare here: --threads is passed to both, but
    # DealerV2_4 hands it to DDS while dealer3's -R threads deal generation,
    # which a solver-bound run barely does. And DealerV2_4 cannot be made
    # single-threaded in table mode at all -- setup_dds_mode() overrides
    # anything below 2 up to TblModeThreads, which is 9 -- so asking it for one
    # thread and believing the answer is how the first version of this script
    # got the comparison wrong.
    solo = [n for n, r in (("DealerV2_4", a), ("dealer3", b))
            if r["parallelism"] < 1.2]
    if solo and len(solo) < 2:
        print(f"{' and '.join(solo)} solved single-threaded while the other did not.")
        print("Read cpu/solve for the solvers themselves and wall/solve for what a")
        print("user waits; the difference between the two columns is parallelism,")
        print("not solver quality.")
        print()

    if a["ns_per_solve"] > 0 and b["ns_per_solve"] > 0:
        print(f"Wall clock:  dealer3 is "
              f"{_x(b['ns_per_solve'], a['ns_per_solve'])} than DealerV2_4.")
        print(f"Per core:    dealer3 is "
              f"{_x(b['cpu_per_solve'], a['cpu_per_solve'])} than DealerV2_4.")
        wall = b["ns_per_solve"] / a["ns_per_solve"]
        cpu = b["cpu_per_solve"] / a["cpu_per_solve"] if a["cpu_per_solve"] else 0
        if wall > 1 and cpu > 0:
            print(f"\nOf the {wall:.2f}x wall-clock gap, about {wall/cpu:.2f}x is "
                  f"parallelism DealerV2_4\nuses and dealer3 does not, and about "
                  f"{cpu:.2f}x is the cost of one solve on one core.")
            print("Those multiply rather than add.")

    print("\nSolving time is the with-solver run minus the no-solver run, per")
    print("program, so deal acquisition and filtering cancel out. DealerV2_4's")
    print("table mode will not run below two threads, so --threads is at least 2.")


if __name__ == "__main__":
    main()
