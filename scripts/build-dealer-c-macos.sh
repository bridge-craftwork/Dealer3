#!/usr/bin/env bash
#
# build-dealer-c-macos.sh - Build the original C dealer natively on macOS.
#
# Why this is worth having: the benchmark's dealer.exe leg runs on an ARM64
# Windows VM, and dealer.exe is a PE32/i386 binary, so it goes through x86
# emulation. That number is honest about what running dealer.exe costs in
# practice, but it says nothing about how fast the original C implementation
# is. Built natively here, the same source becomes a like-for-like opponent for
# a native dealer3 -- same silicon, no emulation, no VM.
#
#   scripts/build-dealer-c-macos.sh
#   scripts/bench-reference.py            # picks it up automatically
#
# The source list mirrors upstream's make.bat. Everything needed is already in
# the checkout, including __random.c -- the GNU random() port -- so the RNG is
# the original's rather than a substitute. scan.c and y.tab.c are committed
# pre-generated, so flex and bison are only run if they are missing.
#
# A note on that RNG. Built as a 64-bit Mach-O, __random.c does 64-bit
# arithmetic, whereas dealer.exe is 32-bit and Windows is LLP64. The two data
# models differ only in bit 31, and dealer indexes its card table from bits
# 15..=30 -- so this binary deals *the same boards as dealer.exe*, which was
# checked directly: byte-identical PBN over 400 deals at seed 1 and 200 each at
# seeds 42 and 12345, modulo CRLF.
#
# That makes it a local stand-in for dealer.exe wherever the deals themselves
# matter, at roughly six times the speed and with no SSH round trip. See
# dealer-legacy-shuffle's PROVENANCE.md for the disassembly and the captured
# vectors, and do not re-derive any of it.
set -euo pipefail

SRC="${DEALER_C_SRC:-$(cd "$(dirname "$0")/../.." && pwd)/Dealer-cleanup}"

if [[ ! -f "$SRC/dealer.c" ]]; then
    echo "error: no C dealer source at $SRC" >&2
    exit 1
fi
cd "$SRC"

if [[ ! -f y.tab.c ]]; then
    echo "==> bison defs.y"
    bison -y -d defs.y
fi
if [[ ! -f scan.c ]]; then
    echo "==> flex scan.l"
    flex -o scan.c scan.l          # included by y.tab.c, not compiled on its own
fi

# The source list is upstream's make.bat, and scan.c is deliberately absent
# from it: defs.y ends with `#include "scan.c"`, so the scanner is compiled as
# part of y.tab.c. Passing it separately instead fails with every token
# undeclared, because scan.l has no prologue including y.tab.h -- it relies on
# being textually inside the parser.
#
# -Wno-everything: this is 2003-era C compiled by a 2020s clang, and the
# warnings are all about the era rather than anything worth fixing here.
# -O2 because an unoptimized build would make the comparison meaningless.
echo "==> clang -O2"
clang -O2 -Wno-everything -o dealer \
    dealer.c pbn.c c4.c getopt.c pointcount.c \
    __random.c rand.c srand.c y.tab.c -lm

echo
echo "Built $SRC/dealer  ($(lipo -archs dealer))"
./dealer -V 2>&1 | head -2 || true
