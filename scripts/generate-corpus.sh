#!/bin/bash
#
# generate-corpus.sh - Build a Tier 1 regression corpus from dealer.exe
#
# Usage:
#   generate-corpus.sh [options] <script.dlr>
#
# Options:
#   -s SEED      Random seed (default: 1)
#   -p PRODUCE   Deals to produce (default: 20)
#   -g MAXGEN    Ceiling on deals generated (default: 1000)
#   -n NAME      Corpus name (default: script basename without extension)
#   -1           One-sided corpus: save only the filtered output, skip the
#                unfiltered sequence. Use when the filter is too selective for
#                a practical generate count. See the caveat below.
#   -t TIMEOUT   Per-invocation timeout in seconds (default: 60)
#   -h           Show this help
#
# What this does
#
#   1. Runs dealer.exe with the script at SEED, producing PRODUCE deals with a
#      ceiling of MAXGEN. Records the resulting generate count G, and saves the
#      filtered deals as expected.txt.
#   2. Runs dealer.exe again at the same SEED with no condition and -g G -p G,
#      yielding the same first G deals unfiltered. Saves them as unfiltered.txt.
#   3. Writes manifest.json recording seed, counts and provenance.
#
#   The replay test then feeds unfiltered.txt through dealer3 with the same
#   script and asserts the result matches expected.txt. This checks parsing and
#   filter semantics against dealer.exe without depending on RNG compatibility.
#
# One-sided corpora (-1)
#
#   Only expected.txt is saved, and the replay test feeds it back through the
#   filter asserting nothing is dropped. This catches dealer3 being too strict,
#   but NOT too lenient, since no rejected deals are present. Prefer the full
#   form wherever G is practical.
#
# Output format
#
#   Deals are stored in oneline format. dealer.exe has no output-format switch,
#   so the harness appends `action printoneline` to a derived copy of the
#   script; dealer.exe honours the LAST action block, so this overrides any
#   action the original script declares. Averages and frequencies declared by
#   the original script are therefore not exercised by the corpus.
#
# Requires: WINDOWS_HOST, WINDOWS_USER, WINDOWS_GITHUB_HOME (see win-dealer.sh)
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WIN_DEALER="$SCRIPT_DIR/win-dealer.sh"
CORPUS_ROOT="$REPO_ROOT/dealer/tests/corpus"

SEED=1
PRODUCE=20
MAXGEN=1000
NAME=""
ONE_SIDED=false
TIMEOUT=60

show_help() { sed -n '2,45p' "$0" | sed 's/^# \?//'; exit 0; }
die() { echo "Error: $*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        -s) SEED="$2"; shift 2 ;;
        -p) PRODUCE="$2"; shift 2 ;;
        -g) MAXGEN="$2"; shift 2 ;;
        -n) NAME="$2"; shift 2 ;;
        -t) TIMEOUT="$2"; shift 2 ;;
        -1) ONE_SIDED=true; shift ;;
        -h|--help) show_help ;;
        -*) die "Unknown option: $1" ;;
        *) SCRIPT_FILE="$1"; shift ;;
    esac
done

[[ -n "${SCRIPT_FILE:-}" ]] || die "No script file given. Try -h."
[[ -f "$SCRIPT_FILE" ]] || die "Script not found: $SCRIPT_FILE"
[[ -x "$WIN_DEALER" ]] || die "win-dealer.sh not found or not executable: $WIN_DEALER"

[[ -n "$NAME" ]] || NAME="$(basename "$SCRIPT_FILE" .dlr)"
OUT_DIR="$CORPUS_ROOT/$NAME"

# Derived scripts must live under the shared drive for the VM to read them.
TMP_DIR="$REPO_ROOT/.corpus-tmp"
mkdir -p "$TMP_DIR" "$OUT_DIR"
cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT

FILTER_TMP="$TMP_DIR/${NAME}.filter.dlr"
UNFILT_TMP="$TMP_DIR/${NAME}.unfiltered.dlr"

# Force oneline output. dealer.exe honours the last action block, so appending
# overrides whatever the source script declared.
cat "$SCRIPT_FILE" > "$FILTER_TMP"
printf '\naction printoneline\n' >> "$FILTER_TMP"
printf 'action printoneline\n' > "$UNFILT_TMP"

# Deal lines in oneline format start with "n "; trailing whitespace is stripped
# so committed corpora are stable and diff cleanly.
extract_deals() { grep '^n ' "$1" | sed 's/[[:space:]]*$//'; }
stat_value() { grep -m1 "^$2 " "$1" | awk '{print $2}'; }

echo "==> [$NAME] filtered run: seed=$SEED produce=$PRODUCE generate<=$MAXGEN"
RAW_FILTERED="$TMP_DIR/filtered.out"
"$WIN_DEALER" -t "$TIMEOUT" -s "$SEED" -p "$PRODUCE" -g "$MAXGEN" "$FILTER_TMP" \
    > "$RAW_FILTERED" 2>/dev/null \
    || die "dealer.exe failed on the filtered run"

GENERATED="$(stat_value "$RAW_FILTERED" Generated)"
PRODUCED="$(stat_value "$RAW_FILTERED" Produced)"
[[ -n "$GENERATED" && -n "$PRODUCED" ]] \
    || die "could not parse dealer.exe stats; is the script valid?"

extract_deals "$RAW_FILTERED" > "$OUT_DIR/expected.txt"
EXPECTED_COUNT="$(wc -l < "$OUT_DIR/expected.txt" | tr -d ' ')"
[[ "$EXPECTED_COUNT" -eq "$PRODUCED" ]] \
    || die "produced count ($PRODUCED) does not match saved deals ($EXPECTED_COUNT)"

echo "    generated=$GENERATED produced=$PRODUCED"

if [[ "$PRODUCED" -lt "$PRODUCE" ]]; then
    echo "    NOTE: hit the generate ceiling before producing $PRODUCE deals."
    echo "          Raise -g, or use -1 for a one-sided corpus."
fi

if $ONE_SIDED; then
    MODE="one-sided"
    INPUT_FILE="expected.txt"
    INPUT_COUNT="$EXPECTED_COUNT"
    rm -f "$OUT_DIR/unfiltered.txt"
else
    MODE="full"
    INPUT_FILE="unfiltered.txt"
    echo "==> [$NAME] unfiltered run: seed=$SEED generate=$GENERATED"
    RAW_UNFILT="$TMP_DIR/unfiltered.out"
    "$WIN_DEALER" -t "$TIMEOUT" -s "$SEED" -p "$GENERATED" -g "$GENERATED" "$UNFILT_TMP" \
        > "$RAW_UNFILT" 2>/dev/null \
        || die "dealer.exe failed on the unfiltered run"

    extract_deals "$RAW_UNFILT" > "$OUT_DIR/unfiltered.txt"
    INPUT_COUNT="$(wc -l < "$OUT_DIR/unfiltered.txt" | tr -d ' ')"
    [[ "$INPUT_COUNT" -eq "$GENERATED" ]] \
        || die "expected $GENERATED unfiltered deals, got $INPUT_COUNT"

    # The unfiltered sequence must start with the same deal as the filtered run
    # only when the very first deal happened to match; instead assert the
    # stronger invariant that every expected deal appears in the input.
    while IFS= read -r deal; do
        grep -qxF "$deal" "$OUT_DIR/unfiltered.txt" \
            || die "expected deal missing from unfiltered sequence; seed/generate mismatch:
       $deal"
    done < "$OUT_DIR/expected.txt"
fi

cp "$SCRIPT_FILE" "$OUT_DIR/script.dlr"

# `dealer -V` exits non-zero, so capture first and default afterwards rather
# than using `|| echo`, which would concatenate the version and the fallback.
DEALER_VERSION="$("$WIN_DEALER" -t 15 -V 2>/dev/null | grep -m1 Revision | tr -d '\r\n' || true)"
[[ -n "$DEALER_VERSION" ]] || DEALER_VERSION="unknown"
GENERATED_ON="$(date -u +%Y-%m-%d)"

cat > "$OUT_DIR/manifest.json" <<JSON
{
  "name": "$NAME",
  "mode": "$MODE",
  "seed": $SEED,
  "produce_target": $PRODUCE,
  "generate_limit": $MAXGEN,
  "generated": $GENERATED,
  "produced": $PRODUCED,
  "input_file": "$INPUT_FILE",
  "input_deals": $INPUT_COUNT,
  "expected_deals": $EXPECTED_COUNT,
  "dealer_version": "$DEALER_VERSION",
  "generated_on": "$GENERATED_ON"
}
JSON

echo "==> [$NAME] wrote $OUT_DIR ($MODE, input=$INPUT_COUNT deals, expected=$EXPECTED_COUNT)"
