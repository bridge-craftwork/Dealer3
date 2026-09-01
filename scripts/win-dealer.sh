#!/bin/bash
#
# win-dealer.sh - Run Windows dealer.exe via SSH for comparison testing
#
# Usage:
#   win-dealer.sh [options] [dealer-options] [input-file]
#   echo "condition" | win-dealer.sh [options] [dealer-options]
#
# Options:
#   -t, --timeout SECS   Job timeout in seconds (default: 10, kills if exceeded)
#   --strip-comment-bug  Strip stray chars from dealer.exe block comment bug
#   -h, --help           Show this help message
#
# Note: Use --strip-comment-bug to filter stray characters caused by a dealer.exe
#       block comment parsing bug (see docs/original-dealer-errata.md).
#
# Examples:
#   win-dealer.sh -p 10 -s 1 test.dlr
#   echo "hcp(north) >= 20" | win-dealer.sh -p 10 -s 1
#   win-dealer.sh -t 60 -p 100 -s 42 myfile.dlr
#

set -euo pipefail

# Environment variables (required — set in ~/.zshrc)
if [[ -z "${WINDOWS_HOST:-}" ]] || [[ -z "${WINDOWS_USER:-}" ]] || [[ -z "${WINDOWS_GITHUB_HOME:-}" ]]; then
    echo "Error: Required environment variables not set." >&2
    echo "Add these to your ~/.zshrc:" >&2
    echo '  export WINDOWS_HOST="your-windows-ip"' >&2
    echo '  export WINDOWS_USER="your-windows-username"' >&2
    echo '  export WINDOWS_GITHUB_HOME="/path/to/your/GitHub"' >&2
    exit 1
fi

TIMEOUT=10
DEALER_ARGS=()
INPUT_FILE=""
STRIP_COMMENT_BUG=false

show_help() {
    sed -n '2,20p' "$0" | sed 's/^# \?//'
    exit 0
}

# Filter out stray characters from dealer.exe block comment bug
# The bug echoes <, E, O, F, > characters from inside /* */ comments
# This sed removes any of those characters from the start of line 1
filter_output() {
    if [[ "$STRIP_COMMENT_BUG" == true ]]; then
        sed '1s/^[<EOF>]*//'
    else
        cat
    fi
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -t|--timeout)
            TIMEOUT="$2"
            shift 2
            ;;
        --strip-comment-bug)
            STRIP_COMMENT_BUG=true
            shift
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

# Build the SSH command
SSH_OPTS="-o ConnectTimeout=5 -o BatchMode=yes"

# Map G: drive to GitHub folder (must be done each SSH session)
# Parallels maps \\Mac\Home to macOS $HOME, so strip $HOME prefix
#
# /y matters: if the VM has a *different* path remembered for G: -- which it
# will if $WINDOWS_GITHUB_HOME ever changed -- `net use` stops to ask whether
# to overwrite it. An SSH session has nobody to answer, so the mapping is left
# as it was and every path built below resolves against the wrong root. The
# failure reads as "No such file or directory" for files that plainly exist.
_REL="${WINDOWS_GITHUB_HOME#$HOME}"
_UNC="\\\\Mac\\Home${_REL//\//\\}"
DRIVE_MAP="net use G: \"${_UNC}\" /y >nul 2>&1 & "
SSH_TARGET="${WINDOWS_USER}@${WINDOWS_HOST}"

# Run command with timeout (kills if exceeded)
# Uses gtimeout (GNU coreutils) if available, otherwise perl fallback
run_with_timeout() {
    local exit_code=0
    if command -v gtimeout &>/dev/null; then
        gtimeout "$TIMEOUT" "$@" || exit_code=$?
    elif command -v timeout &>/dev/null; then
        timeout "$TIMEOUT" "$@" || exit_code=$?
    else
        # Perl fallback for macOS without coreutils
        perl -e '
            use strict;
            my $timeout = shift @ARGV;
            my $pid = fork();
            if ($pid == 0) {
                exec @ARGV;
            }
            local $SIG{ALRM} = sub { kill 9, $pid; exit 124; };
            alarm $timeout;
            waitpid $pid, 0;
            alarm 0;
            exit ($? >> 8);
        ' "$TIMEOUT" "$@" || exit_code=$?
    fi
    if [[ $exit_code -eq 124 ]]; then
        echo "Error: Command timed out after ${TIMEOUT}s" >&2
    fi
    return $exit_code
}

if [[ -n "$INPUT_FILE" ]]; then
    # Input file provided - convert path for Windows
    # Get absolute path
    if [[ "$INPUT_FILE" = /* ]]; then
        ABS_PATH="$INPUT_FILE"
    else
        ABS_PATH="$(cd "$(dirname "$INPUT_FILE")" && pwd)/$(basename "$INPUT_FILE")"
    fi

    # Check if file exists
    if [[ ! -f "$ABS_PATH" ]]; then
        echo "Error: File not found: $INPUT_FILE" >&2
        exit 1
    fi

    # Convert Mac path to Windows path via G: drive
    # G: maps to WINDOWS_GITHUB_HOME via Parallels shared folders
    # e.g. $WINDOWS_GITHUB_HOME/dealer3/foo.dlr -> G:\dealer3\foo.dlr
    if [[ "$ABS_PATH" == "$WINDOWS_GITHUB_HOME"/* ]]; then
        REL_PATH="${ABS_PATH#$WINDOWS_GITHUB_HOME/}"
        WIN_PATH="G:\\${REL_PATH//\//\\}"
    else
        echo "Error: File must be under $WINDOWS_GITHUB_HOME" >&2
        echo "       Got: $ABS_PATH" >&2
        exit 1
    fi

    # Build dealer command with file
    DEALER_CMD="dealer ${DEALER_ARGS[*]} \"$WIN_PATH\""

    # Execute via SSH with timeout
    run_with_timeout ssh $SSH_OPTS "$SSH_TARGET" "${DRIVE_MAP}${DEALER_CMD}" | filter_output
else
    # No input file - read from stdin
    if [[ -t 0 ]]; then
        echo "Error: No input file provided and stdin is a terminal" >&2
        echo "Usage: win-dealer.sh [options] [dealer-options] [input-file]" >&2
        echo "   or: echo \"condition\" | win-dealer.sh [options] [dealer-options]" >&2
        exit 1
    fi

    # Read stdin and pass to dealer
    INPUT=$(cat)
    DEALER_CMD="dealer ${DEALER_ARGS[*]}"

    # Execute via SSH with stdin and timeout
    echo "$INPUT" | run_with_timeout ssh $SSH_OPTS "$SSH_TARGET" "${DRIVE_MAP}${DEALER_CMD}" | filter_output
fi
