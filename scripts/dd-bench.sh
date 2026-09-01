#!/usr/bin/env bash
# Regression benchmark for tricks() / double-dummy throughput.
#
# Usage: ./scripts/dd-bench.sh [path-to-dealer-binary]
#
# Exits non-zero if any stage exceeds its budget. The budgets are the ones
# issue #14 set as the acceptance criteria for routing tricks() through
# bridge-solver; they are generous against what the solver actually does, so a
# stage going over means something regressed rather than that the machine is
# busy. For reference, on an M-series Mac the four stages take roughly
# 0.1s, 0.4s, 1s and 30s.
#
# The legacy in-crate alpha-beta this replaced blew the very first solving
# stage by orders of magnitude: one solve took over fifteen minutes.

set -uo pipefail

DEALER="${1:-./target/release/dealer}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# South holds a fixed 5-2-4-2: S AJT97  H Ax  D Qxxx  C Jx
cat > "$WORK/base.dlr" <<'EOF'
predeal south SAJT97,HA,DQ,CJ

southhand = shape(south, 5242) and
            top4(south, hearts)   == 1 and
            top4(south, diamonds) == 1 and
            top4(south, clubs)    == 1 and
            tens(south) == 1

condition southhand
EOF

# Stage 1: one solve, single denomination.
cat "$WORK/base.dlr" > "$WORK/one.dlr"
echo 'action average "S spades" tricks(south, spades)' >> "$WORK/one.dlr"

# Stage 2: the 1NT-vs-1S comparison. Four written-out tricks() calls asking
# about two (denomination, declarer) pairs, so it also checks that the per-deal
# memo collapses them to two searches instead of four.
cat "$WORK/base.dlr" > "$WORK/two.dlr"
cat >> "$WORK/two.dlr" <<'EOF'
action average "Tricks in spades"   tricks(south, spades),
       average "Tricks in notrump"  tricks(south, 4),
       frequency "Tricks in spades"  (tricks(south, spades), 0, 13),
       frequency "Tricks in notrump" (tricks(south, 4),      0, 13)
EOF

# Stage 3: no double-dummy at all - establishes the generator-only baseline.
cat "$WORK/base.dlr" > "$WORK/none.dlr"
echo 'action average "North HCP" hcp(north)' >> "$WORK/none.dlr"

# Stage 4: the whole 20-entry table, through `trix(deal)`. This is the stage
# that catches a table solved in the wrong order. `DealAnalysis` keeps the
# solver's caches per denomination and drops them when the denomination
# changes, so a table filled seat-outermost rebuilds them twenty times instead
# of five and takes about twice as long -- which is exactly what the first cut
# of `dealer_dds::table` did. The budget is set just above the correct order,
# so the wrong one fails rather than merely being slower.
cat "$WORK/base.dlr" > "$WORK/table.dlr"
echo 'printrpt(trix(deal))' >> "$WORK/table.dlr"

fail=0

run_stage() {
    local name="$1" file="$2" produce="$3" budget="$4"
    local start end elapsed
    start=$(date +%s)
    perl -e "alarm $budget; exec @ARGV" \
        "$DEALER" -q -p "$produce" -s 42 "$file" > "$WORK/out.txt" 2>&1
    local status=$?
    end=$(date +%s)
    if [ "$status" -eq 142 ]; then
        echo "FAIL  $name: exceeded ${budget}s budget at -p $produce"
        fail=1
        return
    elif [ "$status" -ne 0 ]; then
        echo "ERROR $name: dealer exited $status"
        sed -n 1,5p "$WORK/out.txt"
        fail=1
        return
    fi
    elapsed=$(( end - start ))
    echo "ok    $name: -p $produce in ${elapsed}s (budget ${budget}s)"
}

echo "dealer binary: $DEALER"
run_stage "no-dd baseline"       "$WORK/none.dlr"   1000  10
run_stage "1 solve  x 1  deal"   "$WORK/one.dlr"       1  10
run_stage "1 solve  x 100 deals" "$WORK/one.dlr"     100  30
run_stage "2 solves x 1000 deals (the real workload)" "$WORK/two.dlr" 1000 300
run_stage "20 solves x 40 deals (the whole table)"    "$WORK/table.dlr"  40  10

exit "$fail"
