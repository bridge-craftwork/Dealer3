#!/usr/bin/env python3
"""
bench-verify.py - Check dealer3 agrees with the reference programs, over the
benchmark corpus.

A performance number is worth nothing if the three programs are not doing the
same thing, so the corpus does double duty: the same scripts that measure
throughput also check semantics. For each script and each reference program:

  1. The reference runs with `printpbn` added to its action list, so it emits
     both the deals it produced and its statistics.
  2. Those deals go back into dealer3 through `--input-deals`, script unchanged.
  3. Two things must hold.

     **Acceptance.** Every deal the reference matched, dealer3 must also match.
     Both are looking at identical cards, so anything less than 100% is a real
     disagreement about what the condition means -- never an artefact of the
     shuffle, which is exactly why this works despite the two programs having
     dealt differently since 0.5.0.

     **Statistics.** The `average` over those deals must come out the same.
     Same cards in, same number out.

This is `scripts/test-filter.py`'s check (dealer.exe only, acceptance only)
widened to both references and to the statistics, and run over the whole corpus
in one go. The PBN plumbing is imported from `scripts/compare-stats.py` rather
than reimplemented, so the two cannot drift.

Usage:
    scripts/bench-verify.py [-p 200] [-s 1] [--only dealerv2_4]

Options:
    -p, --produce N   Deals for the reference to produce per script (default: 200)
    -s, --seed SEED   Reference seed (default: 1)
    --only NAME       Just one reference ("dealer.exe" or "dealerv2_4")
    --scripts NAMES   Comma-separated corpus subset
    -1, --quick       One representative script only
    --dealerv2 PATH   DealerV2_4 binary (else $DEALERV2_BIN, else PATH)
    -v, --verbose     Show the differing statistics lines
"""
import argparse
import importlib.util
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchlib as bl

# These runs take minutes and are usually backgrounded into a file, where
# Python would block-buffer and show nothing until the very end.
sys.stdout.reconfigure(line_buffering=True)


def _load_compare_stats():
    """Import compare-stats.py despite the hyphen in its name.

    Reused rather than copied: with_printpbn() knows the three shapes an
    `action` line can take, and statistics() knows which trailer lines are not
    comparable. Duplicating either would guarantee they drift apart.
    """
    path = Path(__file__).resolve().parent / "compare-stats.py"
    spec = importlib.util.spec_from_file_location("compare_stats", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PBN_DEAL_RE = re.compile(r'^\[Deal\s+"', re.MULTILINE)


def main():
    ap = argparse.ArgumentParser(description="Check dealer3 agrees with the references.")
    ap.add_argument("-p", "--produce", type=int, default=200)
    ap.add_argument("-s", "--seed", type=int, default=1)
    ap.add_argument("--only", default=None)
    ap.add_argument("--scripts", default=None)
    ap.add_argument("-1", "--quick", action="store_true",
                    help="one representative script only")
    ap.add_argument("--dealerv2", default=None)
    ap.add_argument("--ssh-timeout", type=int, default=300)
    ap.add_argument("-v", "--verbose", action="store_true")
    args = ap.parse_args()

    cs = _load_compare_stats()

    dealer3 = bl.dealer3_target()
    ok, why = dealer3.available()
    if not ok:
        sys.exit(f"error: dealer3 unavailable ({why})")

    corpus = bl.load_corpus()
    if args.scripts:
        wanted = {s.strip() for s in args.scripts.split(",")}
        corpus = [e for e in corpus if e["name"] in wanted]
        if not corpus:
            sys.exit("error: no corpus scripts matched --scripts")
    elif args.quick:
        corpus = [bl.representative(corpus)]
        print(f"--quick: {corpus[0]['name']} only\n")

    refs = [bl.dealer_exe_target(args.ssh_timeout), bl.dealerv2_target(args.dealerv2)]
    if args.only:
        refs = [t for t in refs if t.name == args.only]
        if not refs:
            sys.exit(f"error: unknown target {args.only!r}")
    live = []
    for t in refs:
        good, reason = t.available()
        if good:
            live.append(t)
        else:
            print(f"skipping {t.name}: {reason}")
    if not live:
        sys.exit("error: no reference programs available")

    # Temp scripts and PBN files must live under the directory Parallels maps
    # into the VM, or dealer.exe cannot read them.
    work = bl.REPO / "bench" / ".verify"
    work.mkdir(parents=True, exist_ok=True)

    print(f"{len(corpus)} scripts, {args.produce} deals each, seed {args.seed}")
    print(f"References: {', '.join(t.name for t in live)}\n")

    failures = []
    for entry in corpus:
        script = entry["path"].read_text()
        ref_script, _ = cs.with_printpbn(script)
        staged = work / f"{entry['name']}.dlr"
        staged.write_text(ref_script)

        print(f"{entry['name']}")
        # An entry may name the targets it applies to: the solver-agreement
        # script uses words dealer.exe and dealer-c do not have, and would
        # otherwise be reported as a failure rather than as inapplicable.
        allowed = entry.get("targets")
        for target in live:
            if allowed and target.name not in allowed:
                print(f"    {target.name:<12} n/a (script needs a double-dummy solver)")
                continue
            verdict = check(target, dealer3, staged, args, cs, work, entry["name"],
                            produce=entry.get("verify_produce"))
            status = verdict["status"]
            print(f"    {target.name:<12} {status}")
            if verdict.get("detail") and (args.verbose or status.startswith("MISMATCH")):
                for line in verdict["detail"]:
                    print(f"        {line}")
            if not status.startswith("ok"):
                failures.append((entry["name"], target.name, status))

    print()
    if failures:
        print(f"{len(failures)} check(s) failed:")
        for name, tgt, status in failures:
            print(f"  {name} / {tgt}: {status}")
        return 1
    print("All scripts agree on every deal the references produced.")
    return 0


def check(target, dealer3, staged, args, cs, work, name, produce=None):
    """Run one reference, replay its deals through dealer3, compare.

    `produce` overrides the run-wide count for entries that are expensive per
    deal -- a script calling the solver spends milliseconds on each produced
    deal rather than microseconds.
    """
    produce = produce or args.produce
    try:
        argv = list(target.command)
        if target.verbose_flag:
            argv.append(target.verbose_flag)
        argv += ["-p", str(produce), "-s", str(args.seed), str(staged)]
        ref = subprocess.run(argv, capture_output=True, text=True,
                             timeout=args.ssh_timeout + 60)
    except subprocess.TimeoutExpired:
        return {"status": "ERROR: reference timed out"}
    if ref.returncode != 0:
        return {"status": f"ERROR: reference exit {ref.returncode}: "
                          f"{bl._first_real_line(ref.stdout + ref.stderr)}"}

    ref_out = ref.stdout
    n_deals = len(PBN_DEAL_RE.findall(ref_out))
    if n_deals == 0:
        return {"status": "ERROR: reference emitted no PBN deals"}

    pbn = work / f"{name}.{target.name}.pbn"
    pbn.write_text(ref_out)

    try:
        rep = subprocess.run(
            list(dealer3.command) + ["-p", str(max(n_deals, produce)),
                                     "--input-deals", str(pbn), str(staged)],
            capture_output=True, text=True, timeout=300)
    except subprocess.TimeoutExpired:
        return {"status": "ERROR: dealer3 timed out replaying"}
    if rep.returncode != 0:
        return {"status": f"ERROR: dealer3 exit {rep.returncode}: "
                          f"{bl._first_real_line(rep.stdout + rep.stderr)}"}

    accepted = len(PBN_DEAL_RE.findall(rep.stdout))
    if accepted != n_deals:
        return {"status": f"MISMATCH: reference matched {n_deals} deals, "
                          f"dealer3 accepted {accepted}"}

    # Same cards in, so every statistic must come out to the same *value*.
    # `statistics()` already drops the run trailer, which differs by
    # construction: one program dealt these boards and the other read them.
    ref_stats = cs.statistics(ref_out, True)
    rep_stats = cs.statistics(rep.stdout, True)
    diffs = compare_statistics(ref_stats, rep_stats, target.name)
    if diffs:
        return {"status": f"MISMATCH: statistics differ over {n_deals} identical deals",
                "detail": diffs[:8]}

    return {"status": f"ok ({n_deals} deals, statistics agree)"}


# An `average` line does not have one spelling across the three programs.
# dealer.exe and dealer3 print `label: 10.66`; DealerV2_4 prints
# `label: Mean=   10.6600, Std Dev= ..., Var= ..., Sample Size=50`. Those are
# the same answer, so comparing the text would report a difference that is
# purely cosmetic -- and, worse, would hide the real ones in noise. What is
# compared is the label and the value.
STAT_RE = re.compile(
    r"^(?P<label>[^:]*?):\s*(?:Mean=\s*)?(?P<value>-?\d+(?:\.\d+)?)\s*(?:,|$)"
)


def _parsed(lines):
    out = []
    for line in lines:
        m = STAT_RE.match(line.strip())
        if m:
            out.append((m.group("label").strip(), m.group("value")))
    return out


def compare_statistics(ref_lines, rep_lines, ref_name):
    """Compare statistics by value, tolerating each program's formatting.

    The two are compared to the precision of whichever printed fewer decimals:
    dealer3's `10.66` and DealerV2_4's `10.6600` are the same measurement
    reported to different widths, and demanding they match as strings would
    fail every time while telling you nothing.
    """
    ref, rep = _parsed(ref_lines), _parsed(rep_lines)
    diffs = []
    if len(ref) != len(rep):
        diffs.append(f"{ref_name} reported {len(ref)} statistic(s), dealer3 {len(rep)}")
        return diffs
    for (rlabel, rval), (plabel, pval) in zip(ref, rep):
        if rlabel != plabel:
            diffs.append(f"label differs: {ref_name} {rlabel!r} vs dealer3 {plabel!r}")
            continue
        decimals = min(len(rval.partition(".")[2]), len(pval.partition(".")[2]))
        if round(float(rval), decimals) != round(float(pval), decimals):
            diffs.append(f"{rlabel}: {ref_name} {rval} vs dealer3 {pval}")
    return diffs


if __name__ == "__main__":
    sys.exit(main())
