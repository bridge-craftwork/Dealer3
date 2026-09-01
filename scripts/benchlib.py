#!/usr/bin/env python3
"""
benchlib.py - Shared machinery for the three-way performance comparison.

Used by bench-corpus.py, bench-reference.py, bench-dealer3.py and
bench-report.py. Nothing here runs on its own.

Why the numbers are measured the way they are
---------------------------------------------

**Timing prefers the program's own clock, and falls back when it lies.**
dealer3 and DealerV2_4 print a truthful `Time needed %.3f sec` covering the
generate loop and nothing else, which is the number to compare: it excludes
process startup, and for a target reached over SSH it excludes the round trip
too.

dealer.exe on the Windows VM prints `Time needed 0.000 sec` no matter how long
it ran -- its gettimeofday() does not work there. So a target may be timed by
wall clock instead, with a per-target overhead (SSH round trip plus process
startup, measured by calibrate_overhead() against a near-zero-work run)
subtracted. That overhead is small and steady -- about 0.28s for the VM -- but
it is a tenth of a three-second run, so it is measured rather than ignored, and
bench-reference.py sizes runs long enough for the residual to be noise.

Sample.timing records which clock was used, so a report never silently mixes
the two without saying so.

**Do not pass `-v` to dealer.exe.** Its verbose flag defaults to *on* and the
switch XORs it (dealer.c:1608), so `-v` turns the trailer off and the run looks
like it printed nothing. The VM's build is older than the local source and has
no `-X` to force it on either. It also has a known `-v` bug tied to whether an
odd or even number of PBN rows were written. Verbose_flag is therefore None for
that target: take the default and touch nothing.

**Work is fixed by `-g`, not by `-p`.** The three programs do not deal the same
boards, so "produce 40 matches" is a different amount of work for each of them:
whoever's shuffle happens to hit the condition sooner does less. `-g N` makes
all three evaluate exactly N deals against the condition, which is the thing
being compared. `-p` is then set high enough never to bind.

**The script's own `produce`/`generate` must be stripped first.** In dealer.c
`yyparse()` runs *after* `getopt()`, and `maxgenerate`/`maxproduce` are only
defaulted if still zero, so a `generate 100000` line inside a script silently
beats `-g` on the command line. 120 of the 347 PBS scripts carry one. See
normalize_script(); this is why the corpus is a rewritten copy rather than the
originals.

**Every sample is validated.** run_once() checks that the run really did
generate the number of deals asked for. A short run means something overrode
the limit and the timing describes a different amount of work.
"""
import hashlib
import json
import os
import re
import shutil
import statistics
import subprocess
import time
from dataclasses import dataclass, field, asdict
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CORPUS_DIR = REPO / "bench" / "corpus"
RESULTS_DIR = REPO / "bench" / "results"
CORPUS_INDEX = REPO / "bench" / "corpus.json"

# `-p` is set to this so the produce limit never binds and `-g` alone decides
# how much work happens. Comfortably inside a 32-bit int, which dealer.exe's
# atoi() lands in.
NO_PRODUCE_LIMIT = 100_000_000

# A synthetic corpus entry that isolates deal generation.
#
# dealer3 rewrote the RNG and the shuffle, and dealer.exe's were notably slow,
# so a large part of any measured gap is generation rather than condition
# evaluation. Total deals/sec cannot separate the two: it is
# (shuffle + evaluate) per deal, and for a cheap condition the shuffle
# dominates. This script makes the condition as close to free as the language
# allows -- one hcp() call and a comparison -- so its throughput is essentially
# generation alone. Subtracting its per-deal cost from a real script's leaves
# the evaluation cost, and the two can then be compared separately.
#
# The threshold is chosen to be rare but not impossible. A condition nothing
# ever satisfies would leave `average` with no samples, and it costs nothing to
# avoid finding out what each program prints in that case.
BASELINE_NAME = "_shuffle_baseline"
BASELINE_SCRIPT = """# Synthetic. Not from Practice-Bidding-Scenarios.
#
# Measures deal generation -- RNG plus shuffle -- with as little condition
# evaluation as the language permits, so the generation half of the comparison
# can be read on its own. See BASELINE_NAME in scripts/benchlib.py.
hcp(north) >= 24
action average "bench" hcp(north)
"""

# A synthetic entry that checks dealer3 and DealerV2_4 agree about
# double-dummy results, over identical deals.
#
# dealer.exe and dealer-c have no solver at all -- they answer `dds` with a
# syntax error -- so this entry names the targets it applies to and is skipped
# elsewhere. It is verify-only: its cost is dominated by solving the deals it
# produces rather than by filtering the ones it generates, and the two programs
# do not produce the same *number* of deals from a fixed -g, so it would not be
# a comparable throughput measurement.
#
# Every token here has to parse in both. DealerV2_4's lexer is case-sensitive
# and disagrees with itself about case: compasses are lowercase (`north`) but
# sides are uppercase (`NS`). dealer3 accepts that spelling too, so `par(NS)`
# and `dds(north, ...)` is the combination that runs on both unchanged.
#
# `par` is the strongest check available: it is derived from all twenty
# double-dummy results, so agreeing on it means agreeing on the whole table,
# not on one search that happened to match.
SOLVER_NAME = "_solver_agreement"
SOLVER_TARGETS = ["dealer3", "dealerv2_4"]
SOLVER_SCRIPT = """# Synthetic. Not from Practice-Bidding-Scenarios.
#
# Checks that dealer3 and DealerV2_4 return the same double-dummy results for
# the same cards. dealer.exe and dealer-c have no solver and are excluded.
#
# Spellings are the intersection of the two languages: `north` lowercase,
# `NS` uppercase. See SOLVER_NAME in scripts/benchlib.py.
condition hcp(north) + hcp(south) >= 26
action
    average "dds north notrump" dds(north, notrump),
    average "dds south spades" dds(south, spades),
    average "par NS" par(NS)
"""

# The action every corpus script gets, replacing whatever it had. Cheap, runs
# only on produced deals, and prints one line regardless of hit rate -- so the
# measurement is condition-evaluation throughput and not output I/O.
BENCH_ACTION = 'action average "bench" hcp(north)'

TIME_RE = re.compile(r"Time needed\s+([0-9.]+)\s*sec")
GENERATED_RE = re.compile(r"Generated\s+(\d+)\s+hands")
PRODUCED_RE = re.compile(r"Produced\s+(\d+)\s+hands")

BLOCK_COMMENT_RE = re.compile(r"/\*.*?\*/", re.DOTALL)
LIMIT_LINE_RE = re.compile(r"^\s*(produce|generate)\s+\d+\s*$", re.IGNORECASE)
ACTION_LINE_RE = re.compile(r"^\s*action\b", re.IGNORECASE)


class BenchError(RuntimeError):
    """A run that cannot be trusted as a measurement."""


# OpenSSH prints a post-quantum key-exchange advisory on every connection to
# the VM. It is the first thing on stderr, so without this an SSH target's
# error messages would all read as that banner instead of the real failure.
SSH_NOISE = ("WARNING: connection is not using", "store now, decrypt later",
             "may need to be upgraded", "openssh.com/pq")


def _first_real_line(out):
    for ln in out.splitlines():
        ln = ln.strip()
        if ln and not any(n in ln for n in SSH_NOISE):
            return ln
    return "no output"


# --------------------------------------------------------------------------
# Script normalization
# --------------------------------------------------------------------------

def normalize_script(text):
    """Rewrite a PBS script into one that measures condition throughput.

    Three changes, all of them needed for the number to mean anything:

      - Block comments go. dealer.exe has a parsing bug that echoes stray
        characters out of `/* */` (docs/original-dealer-errata.md); harmless
        for output, but noise in a corpus meant to run clean on all three.
      - `produce`/`generate` lines go, because they would override `-g`.
      - The `action` block is replaced with BENCH_ACTION, so the run is not
        also measuring printall or a frequency table.

    Everything else -- `dealer`, `predeal`, `vulnerable`, the variable
    assignments and the condition itself -- is left exactly as written. That
    is the part being benchmarked.

    Returns (normalized_text, notes) where notes records what was dropped.
    """
    notes = []

    if BLOCK_COMMENT_RE.search(text):
        text = BLOCK_COMMENT_RE.sub("", text)
        notes.append("stripped block comments")

    lines = text.splitlines()

    kept = [ln for ln in lines if not LIMIT_LINE_RE.match(ln)]
    if len(kept) != len(lines):
        notes.append(f"dropped {len(lines) - len(kept)} produce/generate line(s)")
    lines = kept

    # Truncate at the last `action`. In the dealer grammar the action list is
    # the final statement, so everything after it belongs to it. One PBS
    # script (Bergen_Thrump_X_after_Preempt) breaks that rule; bench-corpus.py
    # rejects any script where truncating would drop a real statement.
    action_at = None
    for i, ln in enumerate(lines):
        if ACTION_LINE_RE.match(ln):
            action_at = i
    if action_at is not None:
        lines = lines[:action_at]
        notes.append("replaced action block")
    else:
        notes.append("added action block (script had none)")

    while lines and not lines[-1].strip():
        lines.pop()
    lines.append(BENCH_ACTION)

    return "\n".join(lines) + "\n", notes


def statements_after_action(text):
    """True if a real statement follows the last `action`, so truncating loses it.

    Used by bench-corpus.py to reject a script rather than silently benchmark
    a different condition than the one that was written.
    """
    lines = BLOCK_COMMENT_RE.sub("", text).splitlines()
    action_at = None
    for i, ln in enumerate(lines):
        if ACTION_LINE_RE.match(ln):
            action_at = i
    if action_at is None:
        return False
    tail = re.compile(
        r"^\s*(dealer|predeal|vulnerable|condition|produce|generate)\b", re.IGNORECASE
    )
    return any(tail.match(ln) for ln in lines[action_at + 1:])


# --------------------------------------------------------------------------
# Redundancy: imports and body signatures
# --------------------------------------------------------------------------

# The PBS `.dlr` files are generated: a precompiler expands shared fragments
# inline and brackets each one with markers. Two spellings are in use, and the
# closing marker is not always spelled the same as its opener
# ("1-Bid North" opens, "1-Bid-North" closes), so only openers are matched.
IMPORT_OPEN_RE = re.compile(
    r"^#{3,}\s*Imported Script\s*(?::|--)\s*(.*?)\s*#{3,}\s*$", re.MULTILINE)
IMPORT_ANY_RE = re.compile(
    r"^#{3,}\s*(?:End of\s+)?Imported Script\s*(?::|--)\s*.*?#{3,}\s*$", re.MULTILINE)


def imported_fragments(text):
    """Names of the shared fragments a generated script pulled in."""
    return sorted({n for n in IMPORT_OPEN_RE.findall(text) if n})


def body_signature(text):
    """A key that is equal for two scripts doing the same evaluation work.

    Expects text that has already been through normalize_script(): the
    signature has to describe what will actually be *run*, and normalization
    removes the `produce`/`generate` lines and the action block, which vary
    across otherwise-identical family members and would otherwise make them
    look distinct.

    Comments and blank lines go, and every integer is masked. The masking is
    what matters: the PBS corpus is full of families that differ only in a
    threshold -- Weak_NT_09-12, _10-13, _13-15 and four more -- and as
    benchmarks those are the same script. Measured across the real corpus,
    members of such a family score 1.000 against each other while genuinely
    different scripts score 0.04-0.22, so the two populations do not overlap
    anywhere near the 0.85 threshold bench-corpus.py uses.
    """
    stripped = IMPORT_ANY_RE.sub("", BLOCK_COMMENT_RE.sub("", text))
    lines = [ln.strip() for ln in stripped.splitlines()
             if ln.strip() and not ln.strip().startswith("#")]
    return re.sub(r"\d+", "N", " ".join(lines))


# --------------------------------------------------------------------------
# Targets
# --------------------------------------------------------------------------

@dataclass
class Target:
    """One program under test, and how to invoke it."""
    name: str
    kind: str                      # "local" or "ssh"
    command: list = field(default_factory=list)
    threaded: bool = False         # accepts -R for worker threads
    note: str = ""
    # Switch that asks for the run trailer carrying `Time needed`, or None to
    # pass nothing because the program is already verbose by default. See the
    # module docstring on why dealer.exe must be left alone here.
    verbose_flag: object = "-v"
    # "self"  - trust the program's own `Time needed`.
    # "wall"  - time the process and subtract calibrated overhead.
    # "auto"  - prefer self-reported, fall back to wall if it reports zero.
    timing: str = "self"
    # Filled in by calibrate_overhead(); seconds of startup/SSH to subtract
    # from wall-clock measurements.
    overhead: float = 0.0

    def available(self):
        if self.kind == "ssh":
            missing = [v for v in ("WINDOWS_HOST", "WINDOWS_USER", "WINDOWS_GITHUB_HOME")
                       if not os.environ.get(v)]
            if missing:
                return False, "unset: " + ", ".join(missing)
            if not (REPO / "scripts" / "win-dealer.sh").exists():
                return False, "scripts/win-dealer.sh missing"
            return True, ""
        exe = Path(self.command[0])
        if not exe.exists():
            return False, f"not found: {exe}"
        if not os.access(exe, os.X_OK):
            return False, f"not executable: {exe}"
        return True, ""


def dealer3_target(binary=None):
    binary = Path(binary) if binary else REPO / "target" / "release" / "dealer"
    return Target(
        name="dealer3",
        kind="local",
        command=[str(binary)],
        threaded=True,
        note="built from this repo",
        verbose_flag="-v",
        timing="self",
    )


def dealer_exe_target(timeout=600):
    """dealer.exe on the Windows VM, through scripts/win-dealer.sh.

    The wrapper owns the host, the login and the drive mapping -- none of which
    belong in this repository. Its default timeout is 10s, far too short for a
    calibrated benchmark run, so it is raised here.
    """
    return Target(
        name="dealer.exe",
        kind="ssh",
        command=[str(REPO / "scripts" / "win-dealer.sh"), "-t", str(timeout)],
        threaded=False,
        note="Windows VM via SSH; wall-clocked with SSH overhead calibrated out, "
             "because dealer.exe reports Time needed 0.000 there",
        verbose_flag=None,
        timing="wall",
    )


def dealer_c_target(binary=None):
    """The original C dealer, built natively for this machine.

    The point of it is to take emulation out of the comparison. dealer.exe runs
    x86-emulated on an ARM64 VM, so its throughput cannot be read as the C
    implementation's speed; this is the same source compiled for the same
    silicon dealer3 runs on. Built by scripts/build-dealer-c-macos.sh.

    Verbose defaults on in this source too, and -v XORs it, so nothing is
    passed. This source is newer than the VM's and does have -X, but there is
    no reason to use it.
    """
    if binary:
        path = str(binary)
    elif os.environ.get("DEALER_C_BIN"):
        path = os.environ["DEALER_C_BIN"]
    else:
        path = str(REPO.parent / "Dealer-cleanup" / "dealer")
    return Target(
        name="dealer-c",
        kind="local",
        command=[path],
        threaded=False,
        note="original C dealer, built natively -- the emulation-free reference "
             "for dealer.exe's lineage",
        verbose_flag=None,
        timing="auto",
    )


def dealerv2_target(binary=None):
    """DealerV2_4.

    Resolved from --dealerv2, then $DEALERV2_BIN, then PATH. The upstream repo
    ships only a Linux x86-64 build, so this is expected to be a local build;
    bench-reference.py reports it as unavailable rather than failing if it is
    not there yet.
    """
    if binary:
        path = str(binary)
    elif os.environ.get("DEALERV2_BIN"):
        path = os.environ["DEALERV2_BIN"]
    else:
        path = shutil.which("dealerv2") or "/usr/local/bin/dealerv2"
    return Target(
        name="dealerv2_4",
        kind="local",
        command=[path],
        threaded=False,
        note="DealerV2_4",
        # Same lineage as dealer.exe, so assume verbose-by-default and let the
        # timing mode work out for itself whether its clock is trustworthy.
        verbose_flag=None,
        timing="auto",
    )


# --------------------------------------------------------------------------
# Running
# --------------------------------------------------------------------------

@dataclass
class Sample:
    seconds: float          # the number to compare
    wall: float             # process wall clock, including startup/SSH
    generated: int
    produced: int
    timing: str = "self"    # which clock `seconds` came from

    @property
    def deals_per_sec(self):
        return self.generated / self.seconds if self.seconds > 0 else float("nan")


def run_once(target, script, deals, seed=1, threads=None, timeout=900):
    """Run one script once and return a validated Sample.

    Raises BenchError if the run failed, printed no timing, or generated a
    different number of deals than asked for.
    """
    argv = list(target.command)
    if target.verbose_flag:
        argv.append(target.verbose_flag)
    argv += ["-g", str(deals), "-p", str(NO_PRODUCE_LIMIT), "-s", str(seed)]
    if threads is not None and target.threaded:
        argv += ["-R", str(threads)]
    argv.append(str(script))

    started = time.monotonic()
    try:
        proc = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        raise BenchError(f"{target.name}: timed out after {timeout}s on {Path(script).name}")
    wall = time.monotonic() - started

    out = proc.stdout + proc.stderr
    if proc.returncode != 0:
        raise BenchError(f"{target.name}: exit {proc.returncode} on "
                         f"{Path(script).name}: {_first_real_line(out)}")

    m = TIME_RE.search(out)
    reported = float(m.group(1)) if m else 0.0

    # Which clock to believe. A self-reported zero is not a fast run, it is a
    # broken timer (dealer.exe on Windows does this unconditionally), so "auto"
    # treats it as absent rather than as 0.000 seconds.
    if target.timing == "wall" or (target.timing == "auto" and reported <= 0.0):
        seconds = max(wall - target.overhead, 1e-6)
        timing = "wall"
    else:
        if not m:
            raise BenchError(
                f"{target.name}: no 'Time needed' line on {Path(script).name} "
                "-- wrong verbose flag, or the run trailer is switched off"
            )
        seconds = reported
        timing = "self"

    g = GENERATED_RE.search(out)
    p = PRODUCED_RE.search(out)
    if not g:
        raise BenchError(f"{target.name}: no 'Generated' line on {Path(script).name}")
    generated = int(g.group(1))
    produced = int(p.group(1)) if p else 0

    # The whole point of fixing work with -g. A short run means a produce
    # limit or a stray script statement bound first, and the timing describes
    # a different amount of work than every other target's.
    if generated != deals:
        raise BenchError(
            f"{target.name}: asked for {deals} deals, generated {generated} "
            f"on {Path(script).name} -- something overrode -g"
        )

    return Sample(seconds=seconds, wall=wall, generated=generated,
                  produced=produced, timing=timing)


def run_repeated(target, script, deals, seed=1, threads=None, repeats=3, timeout=900):
    """Run repeatedly and summarise.

    `best` is the headline number. The fastest of several runs is the least
    contaminated by whatever else the machine was doing; the median and the
    spread are kept so a noisy result is visible rather than averaged in.
    """
    samples = [run_once(target, script, deals, seed, threads, timeout)
               for _ in range(repeats)]
    times = sorted(s.seconds for s in samples)
    best = times[0]
    return {
        "seconds_best": best,
        "seconds_median": statistics.median(times),
        "seconds_all": times,
        "spread_pct": (times[-1] - best) / best * 100 if best > 0 else 0.0,
        "deals": deals,
        "deals_per_sec": deals / best if best > 0 else float("nan"),
        "produced": samples[0].produced,
        "wall_best": min(s.wall for s in samples),
        "timing": samples[0].timing,
        "overhead_subtracted": target.overhead if samples[0].timing == "wall" else 0.0,
    }


def calibrate_overhead(target, script, repeats=3, deals=1000):
    """Measure and store what a run costs before any dealing happens.

    Only matters for wall-clocked targets. A run of `deals` deals is small
    enough that its generate loop is a rounding error, so what is left is
    process startup plus -- for the Windows VM -- the SSH round trip and the
    drive mapping. The fastest of several attempts is used, on the same
    reasoning as everywhere else here: the floor is the honest estimate of
    fixed cost, and anything above it was the network having a bad moment.

    Under-subtracting is the safe direction. It makes a target look slower than
    it is, never faster, so a favourable comparison is never an artefact of
    this number.
    """
    if target.timing == "self":
        return 0.0
    target.overhead = 0.0
    walls = []
    for _ in range(repeats):
        try:
            walls.append(run_once(target, script, deals).wall)
        except BenchError:
            continue
    target.overhead = min(walls) if walls else 0.0
    return target.overhead


# --------------------------------------------------------------------------
# Corpus + results I/O
# --------------------------------------------------------------------------

def load_corpus():
    if not CORPUS_INDEX.exists():
        raise BenchError(
            f"no corpus at {CORPUS_INDEX.relative_to(REPO)} -- run scripts/bench-corpus.py first"
        )
    entries = json.loads(CORPUS_INDEX.read_text())["scripts"]
    for e in entries:
        e["path"] = CORPUS_DIR / e["file"]
    return entries


def corpus_fingerprint(entries):
    """A short hash of what the corpus actually is.

    Results are keyed to this rather than to a git revision. A revision is the
    wrong identity for two reasons: it changes when anything in the repo
    changes, so reference numbers would look stale after an unrelated commit;
    and it cannot be written into the corpus before the commit that contains
    the corpus exists, which is a regress with no fixed point.

    A content hash has neither problem. It is stable across unrelated commits
    and changes exactly when the scripts or their calibrated deal counts do --
    which is precisely when a measurement stops being comparable.
    """
    h = hashlib.sha256()
    for e in sorted(entries, key=lambda e: e["name"]):
        h.update(e["name"].encode())
        h.update(str(e["deals"]).encode())
        path = CORPUS_DIR / e["file"]
        if path.exists():
            h.update(path.read_bytes())
    return h.hexdigest()[:12]


def representative(corpus):
    """The single script to use when one has to stand for the corpus.

    The median cost per deal, so it is neither the cheapest condition (where
    dealing dominates and evaluation changes barely show) nor the most
    expensive outlier. Deterministic, so two runs of --quick are comparable to
    each other -- which is the whole point when iterating on a change.
    """
    ranked = sorted(corpus, key=lambda e: e.get("seconds_per_deal_r1", 0))
    return ranked[len(ranked) // 2]


def git_describe():
    for args in (["git", "describe", "--always", "--dirty"], ["git", "rev-parse", "--short", "HEAD"]):
        try:
            out = subprocess.run(args, cwd=REPO, capture_output=True, text=True, timeout=10)
            if out.returncode == 0 and out.stdout.strip():
                return out.stdout.strip()
        except (OSError, subprocess.SubprocessError):
            pass
    return "unknown"


def machine_info():
    def sysctl(key):
        try:
            out = subprocess.run(["sysctl", "-n", key], capture_output=True, text=True, timeout=5)
            return out.stdout.strip() if out.returncode == 0 else ""
        except (OSError, subprocess.SubprocessError):
            return ""
    return {
        "model": sysctl("hw.model"),
        "cpu": sysctl("machdep.cpu.brand_string"),
        "logical_cpus": sysctl("hw.ncpu"),
        "performance_cpus": sysctl("hw.perflevel0.logicalcpu"),
        "efficiency_cpus": sysctl("hw.perflevel1.logicalcpu"),
    }


def windows_vm_info():
    """What the VM actually is, recorded so the dealer.exe number can be read right.

    It matters more than it looks. dealer.exe is a PE32/i386 binary, and if the
    VM is ARM64 Windows -- which it is on an Apple Silicon host -- then it runs
    under x86 emulation. Its throughput then describes "dealer.exe as run on
    this VM", which is a real thing to know, but not the speed of the C
    implementation. DealerV2_4 built natively is the like-for-like reference.

    Host and login come from the environment, never from a file in this repo.
    """
    host, user = os.environ.get("WINDOWS_HOST"), os.environ.get("WINDOWS_USER")
    if not host or not user:
        return {}
    try:
        out = subprocess.run(
            ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", f"{user}@{host}",
             "echo %PROCESSOR_ARCHITECTURE% & echo %PROCESSOR_IDENTIFIER% "
             "& echo %NUMBER_OF_PROCESSORS%"],
            capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.SubprocessError):
        return {}
    lines = [ln.strip() for ln in out.stdout.splitlines() if ln.strip()]
    if len(lines) < 3:
        return {}
    return {"architecture": lines[0], "identifier": lines[1], "cpus": lines[2],
            "note": "dealer.exe is PE32/i386; on an ARM64 VM it runs emulated"}


def write_results(path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    payload.setdefault("written", time.strftime("%Y-%m-%d %H:%M:%S"))
    payload.setdefault("machine", machine_info())
    path.write_text(json.dumps(payload, indent=2, default=_json_default) + "\n")
    return path


def _json_default(obj):
    if isinstance(obj, Path):
        return str(obj)
    if hasattr(obj, "__dataclass_fields__"):
        return asdict(obj)
    raise TypeError(f"not JSON serialisable: {type(obj)}")
