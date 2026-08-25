#!/usr/bin/env python3
"""
generate-corpus.py - Build a Tier 1 regression corpus from dealer.exe

Usage:
    generate-corpus.py [options] <script.dlr>

Options:
    -s SEED      Random seed (default: 1)
    -p PRODUCE   Deals to produce (default: 20)
    -g MAXGEN    Ceiling on deals generated (default: 1000)
    -n NAME      Corpus name (default: script basename without extension)
    -1           One-sided corpus: save only the filtered output, skip the
                 unfiltered sequence. Use when the filter is too selective for
                 a practical generate count. See the caveat below.
    -t TIMEOUT   Per-invocation timeout in seconds (default: 300)

What this does

    1. Runs dealer.exe with the script at SEED, producing PRODUCE deals with a
       ceiling of MAXGEN. Records the resulting generate count G, and saves the
       filtered deals as expected.txt.
    2. Runs dealer.exe again at the same SEED with no condition and -g G -p G,
       yielding the same first G deals unfiltered. Saves them as unfiltered.txt.
    3. Writes manifest.json recording seed, counts and provenance.

    The replay test then feeds unfiltered.txt through dealer3 with the same
    script and asserts the result matches expected.txt. This checks parsing and
    filter semantics against dealer.exe without depending on RNG compatibility.

One-sided corpora (-1)

    Only expected.txt is saved, and the replay test feeds it back through the
    filter asserting nothing is dropped. This catches dealer3 being too strict,
    but NOT too lenient, since no rejected deals are present. Prefer the full
    form wherever G is practical.

Output format

    Deals are stored in oneline format. dealer.exe has no output-format switch,
    so the harness appends `action printoneline` to a derived copy of the
    script; dealer.exe honours the LAST action block, so this overrides any
    action the original script declares. Averages and frequencies declared by
    the original script are therefore not exercised by the corpus.

Windows access

    All VM commands go through Practice-Bidding-Scenarios' ssh_runner, which
    re-issues the drive mappings on every invocation. Do not hand-roll ssh or
    `net use` here: a drive letter already mapped to a different root is kept
    silently, which produces confusing "No such file or directory" failures.
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CORPUS_ROOT = os.path.join(REPO_ROOT, "dealer", "tests", "corpus")

# Add Practice-Bidding-Scenarios build-scripts-mac to path for ssh_runner
PBS_BUILD_SCRIPTS = os.path.normpath(
    os.path.join(REPO_ROOT, "..", "Practice-Bidding-Scenarios", "build-scripts-mac")
)
if not os.path.isdir(PBS_BUILD_SCRIPTS):
    sys.exit(
        f"Error: cannot find PBS build-scripts-mac at {PBS_BUILD_SCRIPTS}\n"
        "Expected at: ../Practice-Bidding-Scenarios/build-scripts-mac/"
    )
sys.path.insert(0, PBS_BUILD_SCRIPTS)

from ssh_runner import mac_to_windows_path, run_windows_command  # noqa: E402


def die(msg: str) -> None:
    sys.exit(f"Error: {msg}")


def run_dealer(dlr_path: str, seed: int, produce: int, generate: int, timeout: int) -> str:
    """Run dealer.exe on the VM against a .dlr file and return its stdout."""
    win_path = mac_to_windows_path(os.path.abspath(dlr_path))
    cmd = f"dealer -s {seed} -p {produce} -g {generate} {win_path}"
    rc, stdout, stderr = run_windows_command(
        cmd, timeout=timeout, check=False, verbose=False
    )
    if rc != 0:
        die(f"dealer.exe failed (exit {rc}) running {win_path}\n{stderr.strip()}")
    return stdout


def extract_deals(text: str) -> list:
    """Deal lines in oneline format start with 'n '; strip trailing whitespace
    so committed corpora are stable and diff cleanly."""
    return [ln.rstrip() for ln in text.splitlines() if ln.startswith("n ")]


def stat_value(text: str, label: str) -> int:
    m = re.search(rf"^{label}\s+(\d+)", text, re.MULTILINE)
    if not m:
        die(f"could not parse '{label}' from dealer.exe output; is the script valid?")
    return int(m.group(1))


def dealer_version(timeout: int) -> str:
    rc, stdout, _ = run_windows_command(
        "dealer -V", timeout=timeout, check=False, verbose=False
    )
    for line in stdout.splitlines():
        if "Revision" in line:
            return line.strip()
    return "unknown"


def main() -> None:
    ap = argparse.ArgumentParser(add_help=False)
    ap.add_argument("-s", type=int, default=1, dest="seed")
    ap.add_argument("-p", type=int, default=20, dest="produce")
    ap.add_argument("-g", type=int, default=1000, dest="maxgen")
    ap.add_argument("-n", default=None, dest="name")
    ap.add_argument("-t", type=int, default=300, dest="timeout")
    ap.add_argument("-1", action="store_true", dest="one_sided")
    ap.add_argument("-h", "--help", action="store_true", dest="help")
    ap.add_argument("script", nargs="?")
    args = ap.parse_args()

    if args.help or not args.script:
        print(__doc__)
        sys.exit(0 if args.help else 1)

    if not os.path.isfile(args.script):
        die(f"script not found: {args.script}")

    name = args.name or os.path.splitext(os.path.basename(args.script))[0]
    out_dir = os.path.join(CORPUS_ROOT, name)
    os.makedirs(out_dir, exist_ok=True)

    # Derived scripts must live under a mapped drive for the VM to read them.
    tmp_dir = os.path.join(REPO_ROOT, ".corpus-tmp")
    os.makedirs(tmp_dir, exist_ok=True)
    try:
        filter_dlr = os.path.join(tmp_dir, f"{name}.filter.dlr")
        unfilt_dlr = os.path.join(tmp_dir, f"{name}.unfiltered.dlr")

        # Force oneline output. dealer.exe honours the last action block, so
        # appending overrides whatever the source script declared.
        with open(args.script) as f:
            source = f.read()
        with open(filter_dlr, "w") as f:
            f.write(source + "\naction printoneline\n")
        with open(unfilt_dlr, "w") as f:
            f.write("action printoneline\n")

        print(
            f"==> [{name}] filtered run: seed={args.seed} "
            f"produce={args.produce} generate<={args.maxgen}"
        )
        filtered = run_dealer(
            filter_dlr, args.seed, args.produce, args.maxgen, args.timeout
        )
        generated = stat_value(filtered, "Generated")
        produced = stat_value(filtered, "Produced")

        expected = extract_deals(filtered)
        if len(expected) != produced:
            die(
                f"produced count ({produced}) does not match saved deals "
                f"({len(expected)})"
            )
        print(f"    generated={generated} produced={produced}")

        if produced < args.produce:
            print(
                f"    NOTE: hit the generate ceiling before producing "
                f"{args.produce} deals.\n"
                f"          Raise -g, or use -1 for a one-sided corpus."
            )

        with open(os.path.join(out_dir, "expected.txt"), "w") as f:
            f.write("\n".join(expected) + "\n")

        if args.one_sided:
            mode, input_file, input_count = "one-sided", "expected.txt", len(expected)
            stale = os.path.join(out_dir, "unfiltered.txt")
            if os.path.exists(stale):
                os.remove(stale)
        else:
            mode, input_file = "full", "unfiltered.txt"
            print(f"==> [{name}] unfiltered run: seed={args.seed} generate={generated}")
            unfiltered_out = run_dealer(
                unfilt_dlr, args.seed, generated, generated, args.timeout
            )
            unfiltered = extract_deals(unfiltered_out)
            if len(unfiltered) != generated:
                die(f"expected {generated} unfiltered deals, got {len(unfiltered)}")

            # Every deal dealer.exe produced must appear in the unfiltered
            # sequence; otherwise the seed or generate count did not line up and
            # the corpus would be silently wrong.
            missing = [d for d in expected if d not in set(unfiltered)]
            if missing:
                die(
                    "expected deal missing from unfiltered sequence; "
                    f"seed/generate mismatch:\n       {missing[0]}"
                )

            input_count = len(unfiltered)
            with open(os.path.join(out_dir, "unfiltered.txt"), "w") as f:
                f.write("\n".join(unfiltered) + "\n")

        shutil.copy(args.script, os.path.join(out_dir, "script.dlr"))

        manifest = {
            "name": name,
            "mode": mode,
            "seed": args.seed,
            "produce_target": args.produce,
            "generate_limit": args.maxgen,
            "generated": generated,
            "produced": produced,
            "input_file": input_file,
            "input_deals": input_count,
            "expected_deals": len(expected),
            "dealer_version": dealer_version(args.timeout),
            "generated_on": datetime.now(timezone.utc).strftime("%Y-%m-%d"),
        }
        with open(os.path.join(out_dir, "manifest.json"), "w") as f:
            json.dump(manifest, f, indent=2)
            f.write("\n")

        print(
            f"==> [{name}] wrote {out_dir} ({mode}, input={input_count} deals, "
            f"expected={len(expected)})"
        )
    finally:
        shutil.rmtree(tmp_dir, ignore_errors=True)


if __name__ == "__main__":
    try:
        main()
    except (subprocess.CalledProcessError, TimeoutError) as e:
        die(str(e))
