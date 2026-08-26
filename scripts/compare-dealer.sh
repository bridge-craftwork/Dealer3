#!/bin/bash
#
# compare-dealer.sh - Compare dealer3 (Rust) vs reference dealer output
#
# SUPERSEDED. This script compared deals one-for-one, which only worked while
# dealer3 could reproduce dealer.exe's exact deal sequence via '--legacy'. That
# mode and its RNG were removed in 0.5.0, so the two programs no longer generate
# the same deals from a given seed and a direct diff is meaningless.
#
# Use instead:
#   scripts/test-filter.py       ad-hoc check that dealer3 accepts every deal
#                                dealer.exe produced for a script
#   scripts/generate-corpus.py   build a committed regression corpus
#                                (see docs/REGRESSION_TESTING.md)
#
# Kept for reference, and in case it is reworked to compare via '--input-deals'.
# Set COMPARE_DEALER_FORCE=1 to run it anyway; the deal diff will not be useful.
#
# Usage:
#   compare-dealer.sh [options] [dealer-options] [input-file]
#   echo "condition" | compare-dealer.sh [options] [dealer-options]
#
# Options:
#   -t, --timeout SECS   Job timeout in seconds (default: 10)
#   -r, --rust PATH      Path to Rust dealer binary (default: target/release/dealer)
#   --ref PATH           Path to reference dealer binary (default: Dealer-cleanup build)
#   -o, --output         Show raw output from both runs after comparison
#   --no-pretest         Skip the quick pretest (pretest is on by default)
#   --pretest-only       Run only the pretest (-p 2), skip the full comparison
#   -R, --threads N      Pass -R N to Rust dealer (parallel threads, 0=auto)
#   --batch-size N       Pass --batch-size N to Rust dealer (work units per batch)
#   -h, --help           Show this help message
#
# Examples:
#   compare-dealer.sh -p 10 -s 1 test.dlr
#   echo "hcp(north) >= 20" | compare-dealer.sh -p 10 -s 1
#   compare-dealer.sh -r ~/.cargo/bin/dealer -p 10 -s 1 test.dlr
#   compare-dealer.sh -o -p 10 -s 1 test.dlr
#   compare-dealer.sh --no-pretest -p 1000 -s 1 test.dlr
#   compare-dealer.sh --pretest-only -s 1 test.dlr
#   compare-dealer.sh -R 0 -p 10 -s 1 test.dlr       # Test parallel mode (auto threads)
#   compare-dealer.sh -R 4 --batch-size 5000 -p 10 -s 1 test.dlr  # 4 threads, batch 5000
#

set -uo pipefail

# dealer3 no longer reproduces dealer.exe's deal sequence, so a deal-by-deal
# diff compares two unrelated sets of deals. Refuse by default rather than
# emit mismatches that look like real failures.
if [[ "${COMPARE_DEALER_FORCE:-0}" != "1" ]]; then
    echo "compare-dealer.sh is superseded and disabled by default." >&2
    echo "" >&2
    echo "It compared deals one-for-one, which required '--legacy' to reproduce" >&2
    echo "dealer.exe's exact sequence. That mode was removed in 0.5.0, so the two" >&2
    echo "programs no longer generate the same deals and a diff proves nothing." >&2
    echo "" >&2
    echo "Use instead:" >&2
    echo "  scripts/test-filter.py       ad-hoc filter check against dealer.exe" >&2
    echo "  scripts/generate-corpus.py   build a committed regression corpus" >&2
    echo "                               (see docs/REGRESSION_TESTING.md)" >&2
    echo "" >&2
    echo "Set COMPARE_DEALER_FORCE=1 to run it anyway." >&2
    exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# Reference dealer binary — defaults to Dealer-cleanup sibling repo
LOCAL_REF_DEALER="${DEALER_REF:-$(cd "$SCRIPT_DIR/../.." 2>/dev/null && pwd)/Dealer-cleanup/dealer}"

TIMEOUT=10
DEALER_ARGS=()
INPUT_FILE=""
SHOW_OUTPUT=false
DEALER3=""
REF_DEALER=""
RUN_PRETEST=true
PRETEST_ONLY=false
RUST_THREADS=""
BATCH_SIZE=""

show_help() {
    sed -n '2,28p' "$0" | sed 's/^# \?//'
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -t|--timeout)
            TIMEOUT="$2"
            shift 2
            ;;
        -r|--rust)
            DEALER3="$2"
            shift 2
            ;;
        --ref)
            REF_DEALER="$2"
            shift 2
            ;;
        -o|--output)
            SHOW_OUTPUT=true
            shift
            ;;
        --no-pretest)
            RUN_PRETEST=false
            shift
            ;;
        --pretest-only)
            PRETEST_ONLY=true
            shift
            ;;
        -R|--threads)
            RUST_THREADS="$2"
            shift 2
            ;;
        --batch-size)
            BATCH_SIZE="$2"
            shift 2
            ;;
        -h|--help)
            show_help
            ;;
        -*)
            # Dealer option - collect it and its argument if needed
            case "$1" in
                -p|-g|-s|-f|-d|--produce|--generate|--seed|--format|--dealer|--vulnerable)
                    DEALER_ARGS+=("$1" "$2")
                    shift 2
                    ;;
                *)
                    DEALER_ARGS+=("$1")
                    shift
                    ;;
            esac
            ;;
        *)
            # Positional argument - treat as input file
            INPUT_FILE="$1"
            shift
            ;;
    esac
done

# Find dealer3 binary if not specified (prefer development build over installed version)
if [[ -z "$DEALER3" ]]; then
    if [[ -x "$SCRIPT_DIR/../target/release/dealer" ]]; then
        DEALER3="$SCRIPT_DIR/../target/release/dealer"
    elif command -v dealer &>/dev/null; then
        DEALER3="dealer"
    else
        echo "Error: dealer3 binary not found in target/release or PATH" >&2
        exit 1
    fi
fi

# Verify dealer3 exists
if [[ ! -x "$DEALER3" ]]; then
    echo "Error: dealer3 binary not found or not executable: $DEALER3" >&2
    exit 1
fi

# Find reference dealer if not specified
if [[ -z "$REF_DEALER" ]]; then
    if [[ -x "$LOCAL_REF_DEALER" ]]; then
        REF_DEALER="$LOCAL_REF_DEALER"
    else
        echo "Error: Reference dealer not found at $LOCAL_REF_DEALER" >&2
        echo "Build it with: cd $(dirname "$SCRIPT_DIR")/../Dealer-cleanup && make" >&2
        echo "Or set DEALER_REF=/path/to/dealer in your environment" >&2
        exit 1
    fi
fi

# Verify reference dealer exists
if [[ ! -x "$REF_DEALER" ]]; then
    echo "Error: Reference dealer not found or not executable: $REF_DEALER" >&2
    exit 1
fi

# Create temp files for output
RUST_OUT=$(mktemp)
REF_OUT=$(mktemp)
trap "rm -f $RUST_OUT $REF_OUT" EXIT

# Extract deal lines (exclude stats lines and PBN metadata that differs between environments)
# Normalizes: CRLF, trailing whitespace, Event field (paths differ), Date field (runtime difference)
extract_deals() {
    tr -d '\r' < "$1" | \
        grep -v -E '^(Generated|Produced|Initial|Time|$)' | \
        grep -v -E '^\[Event ' | \
        grep -v -E '^\[Date ' | \
        sed 's/[[:space:]]*$//'
}

# Extract statistics
extract_stat() {
    local file="$1"
    local pattern="$2"
    # Match lines starting with the pattern (e.g., "Generated 123 hands")
    # This avoids matching timeout messages like "Timeout after 2 seconds (469572 generated, 0 produced)"
    grep -E "^${pattern}" "$file" 2>/dev/null | grep -oE '[0-9]+' | head -1
}

# Run reference dealer
run_ref_dealer() {
    local args=("$@")

    # Always use -X to force stats on
    if [[ -n "$INPUT_FILE" ]]; then
        "$REF_DEALER" -X "${args[@]}" "$INPUT_FILE" > "$REF_OUT" 2>&1
    else
        echo "$INPUT" | "$REF_DEALER" -X "${args[@]}" > "$REF_OUT" 2>&1
    fi
}

# Run comparison with given args, returns 0 on match, 1 on mismatch
# Arguments: label args...
run_comparison() {
    local label="$1"
    shift
    local args=("$@")

    # Build Rust-specific args
    local rust_args=(-t "$TIMEOUT" -X)
    if [[ -n "$RUST_THREADS" ]]; then
        rust_args+=(-R "$RUST_THREADS")
    fi
    if [[ -n "$BATCH_SIZE" ]]; then
        rust_args+=(--batch-size "$BATCH_SIZE")
    fi

    # Run dealer3 (with timeout, -X, and optional threads)
    if [[ -n "$INPUT_FILE" ]]; then
        "$DEALER3" "${rust_args[@]}" "${args[@]}" "$INPUT_FILE" > "$RUST_OUT" 2>&1
    else
        echo "$INPUT" | "$DEALER3" "${rust_args[@]}" "${args[@]}" > "$RUST_OUT" 2>&1
    fi

    # Run reference dealer
    run_ref_dealer "${args[@]}"

    local rust_deals=$(extract_deals "$RUST_OUT")
    local ref_deals=$(extract_deals "$REF_OUT")
    local rust_produced=$(extract_stat "$RUST_OUT" "Produced")
    local ref_produced=$(extract_stat "$REF_OUT" "Produced")
    local rust_generated=$(extract_stat "$RUST_OUT" "Generated")
    local ref_generated=$(extract_stat "$REF_OUT" "Generated")

    if [[ "$rust_deals" == "$ref_deals" ]] && [[ "$rust_produced" == "$ref_produced" ]] && [[ "$rust_generated" == "$ref_generated" ]]; then
        return 0
    else
        return 1
    fi
}

# Save stdin if needed (before pretest consumes it)
INPUT=""
if [[ -z "$INPUT_FILE" ]] && [[ ! -t 0 ]]; then
    INPUT=$(cat)
fi

# Reference name for display
REF_NAME="Local dealer ($(basename "$REF_DEALER"))"

# Run pretest if enabled (quick test with -p 2)
if [[ "$RUN_PRETEST" == true ]] || [[ "$PRETEST_ONLY" == true ]]; then
    echo "=== Pretest (quick -p 2 check) ==="
    echo "Reference: $REF_NAME"
    echo ""

    # Build pretest args: replace any -p value with -p 2, or add -p 2 if not present
    PRETEST_ARGS=()
    FOUND_P=false
    i=0
    while [[ $i -lt ${#DEALER_ARGS[@]} ]]; do
        if [[ "${DEALER_ARGS[$i]}" == "-p" ]] || [[ "${DEALER_ARGS[$i]}" == "--produce" ]]; then
            PRETEST_ARGS+=("-p" "2")
            ((i+=2))
            FOUND_P=true
        else
            PRETEST_ARGS+=("${DEALER_ARGS[$i]}")
            ((i++))
        fi
    done
    if [[ "$FOUND_P" == false ]]; then
        PRETEST_ARGS+=("-p" "2")
    fi

    if run_comparison "Pretest" "${PRETEST_ARGS[@]}"; then
        echo "Pretest:     ✅ PASS"
        echo ""
    else
        echo "Pretest:     ❌ FAIL"
        echo ""
        echo "Pretest failed - skipping full test."
        echo "Run with --no-pretest to force full test, or -o to see output."

        if [[ "$SHOW_OUTPUT" == true ]]; then
            echo ""
            echo "=== Rust Output (pretest) ==="
            cat "$RUST_OUT"
            echo ""
            echo "=== Reference Output (pretest) ==="
            tr -d '\r' < "$REF_OUT"
        fi
        exit 1
    fi
fi

# Exit early if pretest-only mode
if [[ "$PRETEST_ONLY" == true ]]; then
    exit 0
fi

# Run full test
echo "=== Full Comparison ==="
echo "Using: $DEALER3"
echo "Reference: $REF_NAME"
if [[ -n "$RUST_THREADS" ]]; then
    if [[ "$RUST_THREADS" == "0" ]]; then
        echo "Threads: auto-detect (Rust only)"
    else
        echo "Threads: $RUST_THREADS (Rust only)"
    fi
fi
echo ""

# Build Rust-specific args for full test
RUST_SPECIFIC_ARGS=(-t "$TIMEOUT" -X)
if [[ -n "$RUST_THREADS" ]]; then
    RUST_SPECIFIC_ARGS+=(-R "$RUST_THREADS")
fi
if [[ -n "$BATCH_SIZE" ]]; then
    RUST_SPECIFIC_ARGS+=(--batch-size "$BATCH_SIZE")
fi

# Run dealer3 with full args
if [[ -n "$INPUT_FILE" ]]; then
    "$DEALER3" "${RUST_SPECIFIC_ARGS[@]}" "${DEALER_ARGS[@]}" "$INPUT_FILE" > "$RUST_OUT" 2>&1
    RUST_EXIT=$?
else
    echo "$INPUT" | "$DEALER3" "${RUST_SPECIFIC_ARGS[@]}" "${DEALER_ARGS[@]}" > "$RUST_OUT" 2>&1
    RUST_EXIT=$?
fi

# Run reference dealer
run_ref_dealer "${DEALER_ARGS[@]}"
REF_EXIT=$?

# Compare results
RUST_DEALS=$(extract_deals "$RUST_OUT")
REF_DEALS=$(extract_deals "$REF_OUT")

RUST_PRODUCED=$(extract_stat "$RUST_OUT" "Produced")
REF_PRODUCED=$(extract_stat "$REF_OUT" "Produced")

RUST_GENERATED=$(extract_stat "$RUST_OUT" "Generated")
REF_GENERATED=$(extract_stat "$REF_OUT" "Generated")

# Extract time (handle decimal)
RUST_TIME=$(grep -i "Time" "$RUST_OUT" 2>/dev/null | grep -oE '[0-9]+\.[0-9]+' | head -1)
REF_TIME=$(grep -i "Time" "$REF_OUT" 2>/dev/null | grep -oE '[0-9]+\.[0-9]+' | head -1)

# Deals comparison (text diff)
if [[ "$RUST_DEALS" == "$REF_DEALS" ]]; then
    DEAL_COUNT=$(echo "$RUST_DEALS" | grep -c '^' 2>/dev/null || echo 0)
    echo "Deals:       ✅ MATCH ($DEAL_COUNT lines)"
else
    echo "Deals:       ❌ FAIL"
    echo "  Rust lines: $(echo "$RUST_DEALS" | wc -l | tr -d ' ')"
    echo "  Ref lines:  $(echo "$REF_DEALS" | wc -l | tr -d ' ')"
fi

# Produced comparison
if [[ "$RUST_PRODUCED" == "$REF_PRODUCED" ]]; then
    echo "Produced:    ✅ MATCH ($RUST_PRODUCED)"
else
    echo "Produced:    ❌ FAIL (Rust: $RUST_PRODUCED, Ref: $REF_PRODUCED)"
fi

# Generated comparison
if [[ "$RUST_GENERATED" == "$REF_GENERATED" ]]; then
    echo "Generated:   ✅ MATCH ($RUST_GENERATED)"
else
    echo "Generated:   ❌ FAIL (Rust: $RUST_GENERATED, Ref: $REF_GENERATED)"
fi

# Time comparison - show times and warn if Rust is significantly slower (>1s difference)
TIME_WARNING=""
if [[ -n "$RUST_TIME" ]] && [[ -n "$REF_TIME" ]]; then
    # Calculate difference using awk (Rust - Ref)
    TIME_DIFF=$(awk "BEGIN {printf \"%.3f\", $RUST_TIME - $REF_TIME}")
    if awk "BEGIN {exit !($TIME_DIFF > 1.0)}"; then
        TIME_WARNING=" ⚠️  Rust is ${TIME_DIFF}s slower"
    fi
fi
echo "Time:        Rust: ${RUST_TIME:-N/A}s, Ref: ${REF_TIME:-N/A}s${TIME_WARNING}"

# Exit codes
if [[ $RUST_EXIT -ne 0 ]] || [[ $REF_EXIT -ne 0 ]]; then
    echo ""
    echo "Exit codes:  Rust: $RUST_EXIT, Ref: $REF_EXIT"
fi

# Overall result
echo ""
if [[ "$RUST_DEALS" == "$REF_DEALS" ]] && [[ "$RUST_PRODUCED" == "$REF_PRODUCED" ]] && [[ "$RUST_GENERATED" == "$REF_GENERATED" ]]; then
    echo "Overall:     ✅ PASS"
    RESULT=0
else
    echo "Overall:     ❌ FAIL"
    RESULT=1
fi

# Show raw output if requested
if [[ "$SHOW_OUTPUT" == true ]]; then
    echo ""
    echo "=== Rust Output ==="
    cat "$RUST_OUT"
    echo ""
    echo "=== Reference Output ==="
    tr -d '\r' < "$REF_OUT"
fi

exit $RESULT
