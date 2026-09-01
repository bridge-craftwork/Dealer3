#!/usr/bin/env python3
"""
compare-stats.py - Compare dealer3 and dealer.exe over the *same* deals.

The two programs have not dealt alike since legacy mode was removed in 0.5.0,
so diffing their output directly proves nothing: different boards, different
numbers, no information. This compares what a script *means* instead.

    1. Run the script on dealer.exe, with `printpbn` added to its action list
       so the run emits both the deals it produced and its statistics.
    2. Feed those deals back to dealer3 through `--input-deals`, with the
       script unchanged.
    3. Diff the statistics.

Both programs are then looking at identical cards, so every number is
comparable: `average`, `frequency` in one dimension or two, and the deal
layouts themselves. A difference is a real difference in what a word means,
not an artefact of the shuffle.

This is the rework `scripts/compare-dealer.sh` asked for in its own header.
That script diffed boards one for one, which only worked while dealer3 could
reproduce dealer.exe's exact sequence.

What it cannot compare, and skips:
  - The run trailer (`Generated`, `Produced`, `Initial random seed`,
    `Time needed`), which differs by construction: dealer3 read deals where
    dealer.exe dealt them.
  - Words dealer.exe does not have. `printrpt`, `csvrpt`, `par`, `trix`,
    `printns` and the rest are DealerV2_4's or dealer3's own, and dealer.exe
    answers them with a syntax error. Compare those against DealerV2_4.

Usage:
    compare-stats.py [-p N] [-s SEED] [-v] <script.dlr>

Options:
    -p N        Deals to produce (default: 200)
    -s SEED     Seed for dealer.exe (default: 1)
    -v          Show both outputs in full, not just the difference
    -h, --help  Show this help message
"""
import argparse
import difflib
import os
import re
import subprocess
import sys
import tempfile

PBS_BUILD_SCRIPTS = os.path.normpath(os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "..", "Practice-Bidding-Scenarios", "build-scripts-mac",
))
if not os.path.isdir(PBS_BUILD_SCRIPTS):
    sys.exit(f"Error: cannot find PBS build-scripts-mac at {PBS_BUILD_SCRIPTS}")
sys.path.insert(0, PBS_BUILD_SCRIPTS)

from ssh_runner import run_windows_command, mac_to_windows_path  # noqa: E402

PRINT_ACTIONS = r"printall|printew|printpbn|printcompact|printoneline"

# Lines that cannot match, because dealer3 read the deals rather than dealing
# them. Everything else is fair game.
TRAILER = re.compile(r"^(Generated |Produced |Initial random seed|Time needed)")


def with_printpbn(script: str) -> tuple:
    """The script emitting PBN, and whether that had to be added.

    `printpbn` rather than another format because `--input-deals` reads it back
    exactly, including the seat each hand belongs to.

    The flag matters for the comparison. A script that already printed is
    compared including its blank lines, because those are part of the layout
    and a missing one is a real difference. A script that did not print gets
    PBN injected, and PBN separates its records with blank lines that only one
    side will have — so those are dropped rather than reported.
    """
    if re.search(rf"\b({PRINT_ACTIONS})\b", script):
        return re.sub(rf"\b({PRINT_ACTIONS})\b", "printpbn", script, count=1), False
    if re.search(r"^\s*action\b", script, re.MULTILINE):
        return re.sub(r"^(\s*action\b)", r"\1 printpbn,", script, count=1,
                      flags=re.MULTILINE), True
    return script.rstrip("\n") + "\naction printpbn\n", True


def statistics(text: str, drop_blanks: bool) -> list:
    """The lines worth comparing: not deals, not the run trailer."""
    lines = []
    for line in text.splitlines():
        line = line.rstrip("\r")
        if line.startswith("[") or TRAILER.match(line):
            continue
        if drop_blanks and not line.strip():
            continue
        lines.append(line)
    # Trailing blank lines carry no information either way.
    while lines and not lines[-1].strip():
        lines.pop()
    return lines


def main() -> int:
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("script")
    parser.add_argument("-p", "--produce", type=int, default=200)
    parser.add_argument("-s", "--seed", type=int, default=1)
    parser.add_argument("-v", "--verbose", action="store_true")
    parser.add_argument("-h", "--help", action="store_true")
    args = parser.parse_args()
    if args.help:
        print(__doc__)
        return 0

    with open(args.script) as f:
        script = f.read()

    # dealer.exe reads the script from a path the VM can see, so the temporary
    # copy has to live under the shared directory rather than in /tmp.
    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    handle, remote_script = tempfile.mkstemp(suffix=".dlr", dir=here)
    try:
        with os.fdopen(handle, "w") as f:
            reference_script, injected = with_printpbn(script)
            f.write(reference_script)
        command = (f'dealer -p {args.produce} -s {args.seed} '
                   f'"{mac_to_windows_path(remote_script)}"')
        code, reference, stderr = run_windows_command(command, timeout=300,
                                                      verbose=False)
    finally:
        os.unlink(remote_script)

    if code != 0 or "[Deal " not in reference:
        print("dealer.exe did not produce deals.", file=sys.stderr)
        print((reference or stderr).strip()[:500], file=sys.stderr)
        return 2

    with tempfile.NamedTemporaryFile("w", suffix=".pbn", delete=False) as f:
        deals = f.name
        f.write(reference)
    with tempfile.NamedTemporaryFile("w", suffix=".dlr", delete=False) as f:
        local_script = f.name
        f.write(script)

    binary = os.path.join(here, "target", "release", "dealer")
    if not os.path.exists(binary):
        sys.exit(f"Error: {binary} not built. Run ./dev-build.sh build --release")
    try:
        run = subprocess.run(
            [binary, "-p", str(args.produce), "--input-deals", deals,
             local_script],
            capture_output=True, text=True, timeout=300,
        )
    finally:
        os.unlink(deals)
        os.unlink(local_script)

    # Said aloud rather than showing up as "everything differs": a dealer3 that
    # refused the script is a different result from one that disagreed with it.
    if run.returncode != 0:
        print(f"dealer3 exited {run.returncode}:", file=sys.stderr)
        print((run.stderr or run.stdout).strip()[:500], file=sys.stderr)
        return 2
    mine = run.stdout

    theirs = statistics(reference, injected)
    ours = statistics(mine, injected)

    boards = reference.count("[Deal ")
    print(f"{boards} deals from dealer.exe, seed {args.seed}")

    if args.verbose:
        print("\n--- dealer.exe ---")
        print("\n".join(theirs))
        print("\n--- dealer3 ---")
        print("\n".join(ours))

    if theirs == ours:
        print(f"MATCH: {len(ours)} lines identical over the same deals")
        return 0

    print("DIFFER:")
    for line in difflib.unified_diff(theirs, ours, "dealer.exe", "dealer3",
                                     lineterm="", n=2):
        print("  " + line)
    return 1


if __name__ == "__main__":
    sys.exit(main())
