#!/bin/bash
#
# build.sh - Build the dealer3 WebAssembly package.
#
# Usage:
#   ./build.sh [web|nodejs|both|threaded]     (default: web)
#
#   web       ES module for the browser / Cloudflare Pages  -> pkg/
#   nodejs    CommonJS for Node, used by the verification tests -> pkg-node/
#   threaded  the same ES module built for wasm threads       -> pkg/
#
# **`threaded` is what the site ships.** `.github/workflows/pages.yml` runs
# `npm run wasm:threaded`, and then reads the binary it is about to deploy to
# check that is what it got. It needs a pinned nightly and rust-src, and the
# page must be served with COOP/COEP — `web/public/_headers` does that, which is
# why the site is on Cloudflare Pages at all.
#
# One build for everyone, not two with detection. A threads build carries shared
# memory, and the question was whether a browser without `SharedArrayBuffer`
# would refuse to instantiate it — which would be worse than being slow.
# Measured, not assumed: on a page served WITHOUT COOP/COEP, in Chromium 153 and
# in WebKit 26.6, `SharedArrayBuffer` is undefined and
# `new WebAssembly.Memory({shared: true})` succeeds anyway. The module loads,
# `crossOriginIsolated` is false, the pool is not started, and the run produces
# the same deals on one thread. So the bundle degrades rather than failing, and
# a second build with detection would buy nothing.
#
# It used to be slower, and dramatically: 4M deals in six seconds on one thread
# against 290K on twelve, getting worse with every thread added. That was the
# allocator. A `Deal` was four `Vec<Card>` allocations and wasm's dlmalloc
# serialises them, so every worker queued on the same lock. This comment used
# to end "Allocation-free dealing comes first"; that landed, and the shape
# reversed.
#
# Measured 2026-09-08 in Chromium 153 on an M4 Pro (8 performance + 4 efficiency
# cores), `condition hcp(north) >= 20`, 16M-deal budget, median of three:
#
#     threads   1      2      4      6      8     12
#     M deals/s 2.83   4.73   8.05  10.14  11.80  11.12
#     vs one    1.00x  1.67x  2.85x  3.58x  4.17x  3.93x
#
# Sublinear, and flat from eight — four of the cores are efficiency cores and
# some contention remains. Eight and twelve are inside each other's noise
# (repeats: 11.89/11.80/11.31 at eight, 9.93/11.29/11.12 at twelve).
#
# **That script is the best case, and it is not a typical one.** wasm's
# allocator is one dlmalloc behind one lock, so anything a script allocates per
# deal is a queue every worker stands in. One variable assignment is a hash map
# per deal, and the same 12 threads then run at a QUARTER of one thread. A real
# scenario (Jacoby 2NT) goes 269k deals/s to 105k. The full table is at
# `threads_for` in `src/lib.rs`, which is also where the engine decides what to
# do about it: threads are used for a script that searches for double-dummy
# results over deals it shuffled — 321 deals/s to 537, and the case this work
# was asked for — and one thread for everything else, which is faster.
#
# So the threaded build ships, and most runs still deal on one thread. What
# would change that is per-deal allocation going the way `Hand`'s did; until
# then, threading an ordinary filter here loses.
#
# The atomics build costs nothing at one thread: 2.83 against the
# single-threaded build's 2.73 in the same session, which is the wrong way round
# by less than the noise. And `produced` was byte-identical at every thread
# count, against the single-threaded browser build and against the Node build —
# the property that makes any of this safe.
#
# WebKit 26.6, which is the engine an iPad runs, scales too: 2.87 at one thread,
# 9.18 at four, 11.06 at eight, with the same deals. iOS and iPadOS have had
# `SharedArrayBuffer` under COOP/COEP since Safari 15.2.
#
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

command -v wasm-pack >/dev/null || {
    echo "Error: wasm-pack not found. Install: https://rustwasm.github.io/wasm-pack/installer/" >&2
    exit 1
}

target="${1:-web}"

# A threaded build is a different build, not a flag on the same one: wasm
# threads need atomics and shared memory, which means rebuilding the standard
# library, which means nightly. Everything else stays on stable.
#
# The page it lands on must also be served with COOP and COEP, or
# `SharedArrayBuffer` does not exist and the workers cannot share memory. Those
# headers are in `web/public/_headers`.
# The nightly a threaded build is known to work with.
#
# Pinned rather than tracking `nightly`, and not because nightly is unstable in
# the usual sense. wasm-bindgen 0.2.127 looks for a `__wasm_init_tls` symbol
# that newer LLVM no longer emits — it emits `__wasm_apply_tls_relocs` instead —
# so a current nightly builds fine and then fails at the bindgen step with
# "failed to find `__wasm_init_tls`". Until wasm-bindgen catches up, the version
# that still emits it is the version to use.
#
# `wasm-bindgen-rayon` pins `nightly-2024-08-02`, which is too old for this
# codebase: `usize::is_multiple_of` arrived in 1.87.
THREADED_TOOLCHAIN=nightly-2025-06-01

# A threaded build is a different build, not a flag on the same one: wasm
# threads need atomics and shared memory, which means rebuilding the standard
# library, which means nightly. Everything else stays on stable.
#
# The page it lands on must also be served with COOP and COEP, or
# `SharedArrayBuffer` does not exist and the workers cannot share memory. Those
# headers are in `web/public/_headers`, and `vite.config.js` sends them in dev.
build_threaded() {
    echo "==> building web (threaded, $THREADED_TOOLCHAIN) -> $1"
    command -v rustup >/dev/null || {
        echo "Error: threaded builds need rustup, for a pinned nightly toolchain." >&2
        exit 1
    }
    rustup run "$THREADED_TOOLCHAIN" rustc --version >/dev/null 2>&1 || {
        echo "Error: threaded builds need $THREADED_TOOLCHAIN:" >&2
        echo "    rustup toolchain install $THREADED_TOOLCHAIN --profile minimal \\" >&2
        echo "        --component rust-src --target wasm32-unknown-unknown" >&2
        exit 1
    }
    # `-Z build-std` before the subcommand, and through `cargo` directly rather
    # than `wasm-pack`, which passes trailing arguments where `-Z` is not
    # accepted. The bindgen and optimise steps are then run by hand.
    RUSTFLAGS='-C target-feature=+atomics,+bulk-memory,+mutable-globals' \
        rustup run "$THREADED_TOOLCHAIN" cargo -Z build-std=panic_abort,std \
            build --target wasm32-unknown-unknown --release --features parallel

    local built="target/wasm32-unknown-unknown/release/dealer3_wasm.wasm"
    # `__wasm_init_tls` is what wasm-bindgen needs and what pins the toolchain.
    # Checked here so a toolchain bump fails with the reason rather than with
    # bindgen's message about a symbol nobody has heard of.
    # `grep -c`, not `grep -q`: under `pipefail` an early-exiting `grep -q`
    # leaves `strings` killed by SIGPIPE, and the pipeline then reports failure
    # on the very binaries that pass.
    [ "$(strings "$built" | grep -c '__wasm_init_tls')" -gt 0 ] || {
        echo "Error: $built has no __wasm_init_tls, so wasm-bindgen will refuse it." >&2
        echo "       The toolchain's LLVM no longer emits it; see THREADED_TOOLCHAIN above." >&2
        exit 1
    }

    rm -rf "$1"

    # The `wasm-bindgen` BINARY must be the same version as the `wasm-bindgen`
    # CRATE the module was compiled against. The two share a private format, and
    # a mismatch fails with "version of wasm-bindgen that uses a different
    # bindgen format than this binary" — which names neither version and reads
    # like a broken install.
    #
    # So the version is read from the lockfile the build above just wrote, and
    # the binary is chosen BY THAT VERSION. It used to be whichever binary in
    # wasm-pack's cache was newest, on the reasoning that wasm-pack keeps one in
    # step with the crate — which it does, right up until the crate moves and
    # the cache does not. Then "newest" is simply the wrong one, and that is how
    # this build broke without a line of it changing.
    local wanted
    wanted=$(sed -n '/^name = "wasm-bindgen"$/{n;s/^version = "\(.*\)"/\1/p;}' Cargo.lock)
    [ -n "$wanted" ] || {
        echo "Error: Cargo.lock names no wasm-bindgen version, so nothing can be matched to it." >&2
        exit 1
    }

    local bindgen="" candidate found=""
    for candidate in "$HOME"/Library/Caches/.wasm-pack/wasm-bindgen-*/wasm-bindgen \
                     "$HOME"/.cache/.wasm-pack/wasm-bindgen-*/wasm-bindgen \
                     "$(command -v wasm-bindgen || true)"; do
        # An unmatched glob arrives as its own pattern, and `command -v` as an
        # empty string; both fail this and are skipped.
        [ -x "$candidate" ] || continue
        local version
        version=$("$candidate" --version 2>/dev/null || true)
        found="$found
      $version  ($candidate)"
        [ "$version" = "wasm-bindgen $wanted" ] || continue
        bindgen="$candidate"
        break
    done
    [ -n "$bindgen" ] || {
        echo "Error: no wasm-bindgen $wanted, which is the version this module was built against." >&2
        echo "    Found:${found:-  none}" >&2
        echo "    Fix with either of:" >&2
        echo "        cargo install -f wasm-bindgen-cli --version $wanted" >&2
        echo "        ./build.sh web   # wasm-pack fetches the matching one into its cache" >&2
        exit 1
    }
    echo "    wasm-bindgen $wanted"
    "$bindgen" "$built" --out-dir "$1" --target web --no-typescript
    # wasm-bindgen, unlike wasm-pack, writes no package.json — and the pool's
    # worker helper needs one. It reaches the glue with `await import('../../..')`
    # from inside `snippets/<hash>/src/`, which lands on this directory and only
    # resolves if something here names the entry point. Without it Vite fails
    # with "Failed to resolve import ../../.." and replaces the whole app with
    # an error overlay — which from a headless browser looks like a page that
    # simply never finished loading.
    cat > "$1/package.json" <<JSON
{
  "name": "dealer3-wasm",
  "version": "1.0.0",
  "type": "module",
  "main": "dealer3_wasm.js",
  "module": "dealer3_wasm.js",
  "sideEffects": ["./snippets/*"]
}
JSON

    # Threads carry features wasm-opt will otherwise refuse. Skipped rather than
    # failed if it is not installed: the binary is merely larger.
    if command -v wasm-opt >/dev/null; then
        wasm-opt -O --enable-threads --enable-bulk-memory --enable-mutable-globals \
            "$1"/dealer3_wasm_bg.wasm -o "$1"/dealer3_wasm_bg.wasm
    fi
    local wasm
    wasm=$(ls "$1"/*_bg.wasm)
    printf "    %.0f KB raw, %.0f KB gzipped\n" \
        "$(( $(wc -c < "$wasm") / 1024 ))" \
        "$(( $(gzip -c "$wasm" | wc -c) / 1024 ))"
}
build() {
    echo "==> building $1 -> $2"
    wasm-pack build --target "$1" --release --out-dir "$2"
    local wasm
    wasm=$(ls "$2"/*_bg.wasm)
    printf "    %.0f KB raw, %.0f KB gzipped\n" \
        "$(( $(wc -c < "$wasm") / 1024 ))" \
        "$(( $(gzip -c "$wasm" | wc -c) / 1024 ))"
}

case "$target" in
    web)      build web pkg ;;
    nodejs)   build nodejs pkg-node ;;
    both)     build web pkg; build nodejs pkg-node ;;
    threaded) build_threaded "${2:-pkg}" ;;
    *)        echo "Unknown target '$target'. Use web, nodejs, both or threaded." >&2; exit 1 ;;
esac
