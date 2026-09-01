#!/usr/bin/env bash
#
# build-dealerv2-macos.sh - Build DealerV2_4 natively on macOS/arm64.
#
# The upstream repo ships a Linux x86-64 binary and a Linux libdds.a, neither
# of which runs here, so the third leg of the performance comparison has to be
# built. Everything this needs is in the upstream tree already -- including the
# DDS 2.9.0 source tarball -- so there is nothing to fetch beyond the clone.
#
#   git clone https://github.com/dealerv2/Dealer-Version-2-.git ../Dealer-Version-2-
#   scripts/build-dealerv2-macos.sh
#   export DEALERV2_BIN=../Dealer-Version-2-/macos-build/dealerv2
#
# Four things upstream assumes that macOS does not provide, and what is done
# about each:
#
#   1. DDS is built with -Werror and trips over deprecated sprintf() and a
#      non-UTF-8 comment. -Werror is dropped; both warnings are cosmetic.
#   2. DDS's Mac makefile wants Boost for threading. DDS supports several
#      backends, so it is built with GCD + STL instead. Irrelevant to the
#      benchmark either way, which calls no double-dummy functions.
#   3. -mtune=corei7 is x86-only, and -fopenmp needs libomp. Both are dropped;
#      DealerV2_4 contains no OpenMP pragmas or API calls at all, so the flag
#      is vestigial.
#
#      That is *not* the same as DealerV2_4 being single-threaded. Its own
#      deal-and-filter loop is single-threaded, but its double-dummy solving is
#      not: -R sets nThreads, which is handed to the DDS library through
#      SetResources(maxMemoryMB, maxThreads), and table mode (-M 2) silently
#      defaults it if you did not ask. So the benchmark corpus -- which calls
#      no solver function -- measures it single-threaded, while anything using
#      dds() or par() does not. Which is why DDS is built here with a real
#      threading backend rather than none.
#   4. <malloc.h> and getrandom() are glibc-isms. Both get a shim in
#      macos-build/include/. getrandom() is reached only when no -s seed is
#      given, which the benchmark never does.
#
# The result is a native arm64 binary that is a fair opponent for a native
# arm64 dealer3. A cross-architecture or emulated build would not be.
set -euo pipefail

V2="${DEALERV2_SRC:-$(cd "$(dirname "$0")/../.." && pwd)/Dealer-Version-2-}"
OUT="$V2/macos-build"

if [[ ! -d "$V2/src" ]]; then
    echo "error: no DealerV2 checkout at $V2" >&2
    echo "  git clone https://github.com/dealerv2/Dealer-Version-2-.git \"$V2\"" >&2
    exit 1
fi

# Homebrew bison; Apple's is 2.3 and too old for dealyacc.y.
export PATH="/opt/homebrew/opt/bison/bin:${PATH}"
if ! bison --version 2>/dev/null | head -1 | grep -qE '3\.[0-9]+'; then
    echo "error: need bison 3.x (brew install bison)" >&2
    exit 1
fi

echo "==> Preparing $OUT"
rm -rf "$OUT"
mkdir -p "$OUT/lib"
cp -R "$V2/src" "$V2/include" "$V2/Prod" "$OUT/"
rm -f "$OUT/Prod/dealerv2" "$OUT/Prod"/*.o          # upstream's Linux artifacts

echo "==> Building DDS 2.9.0 for arm64"
tar xzf "$V2/lib/dds290-src_2022.tar.gz" -C "$OUT"
DDS="$OUT/dds290-src/src"
rm -f "$DDS"/*.o "$DDS/libdds.a"                     # ditto, the tarball ships Linux .o
cp "$DDS/Makefiles/Makefile_Mac_clang_static" "$DDS/Makefile.mac"
sed -i '' \
    -e 's/-Werror//g' \
    -e 's/-mtune=[a-z0-9]*//g; s/-march=[a-z0-9]*//g' \
    -e 's/^THREADING	= .*/THREADING	= $(THR_GCD) $(THR_STL)/' \
    -e 's/^THREAD_LINK	= .*/THREAD_LINK	=/' \
    "$DDS/Makefile.mac"
make -C "$DDS" -f Makefile.mac -j"$(sysctl -n hw.ncpu)" >/dev/null
cp "$DDS/libdds.a" "$OUT/lib/"
cp "$OUT/dds290-src/include/"*.h "$OUT/include/" 2>/dev/null || true
echo "    libdds.a: $(lipo -archs "$OUT/lib/libdds.a")"

echo "==> Writing glibc shims"
printf '#pragma once\n#include <stdlib.h>\n#include <malloc/malloc.h>\n' \
    > "$OUT/include/malloc.h"
cat > "$OUT/include/macos_compat.h" <<'EOF'
/* macOS shims for DealerV2_4, which is written against glibc.
 *
 * getrandom(2) is Linux-only. DealerV2_4 calls it in one place, init_rand48(),
 * and only when no -s seed was supplied. getentropy(3) has the same contract
 * for buffers up to 256 bytes; the call site asks for 6.
 */
#pragma once
#if defined(__APPLE__)
#include <sys/random.h>
#include <sys/types.h>
static inline ssize_t getrandom(void *buf, size_t len, unsigned int flags) {
    (void)flags;
    return getentropy(buf, len) == 0 ? (ssize_t)len : -1;
}
#endif
EOF

echo "==> Building dealerv2"
sed -i '' \
    -e 's/^CC      = gcc/CC      = clang/' \
    -e 's/^CXX = g++/CXX = clang++/' \
    -e 's/-mtune=corei7//g; s/-flto//g; s/-fopenmp//g' \
    -e 's/-Wall -pedantic/-Wall -Wno-everything/' \
    -e 's|-I\.\./include|-I../include -include macos_compat.h|g' \
    "$OUT/Prod/Makefile"
make -C "$OUT/Prod" >/dev/null
cp "$OUT/Prod/dealerv2" "$OUT/dealerv2"

echo
echo "Built $OUT/dealerv2  ($(lipo -archs "$OUT/dealerv2"))"
"$OUT/dealerv2" -h 2>&1 | head -3 || true
echo
echo "Point the benchmark at it with:"
echo "  export DEALERV2_BIN=\"$OUT/dealerv2\""
