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
# `threaded` needs a pinned nightly and rust-src, and the page must be served
# with COOP/COEP. It works and it is correct — the same deals, whatever the
# thread count — but it is **not** what the site ships, because today it is
# slower: 4M deals in six seconds on one thread against 290K on twelve, getting
# worse with every thread added. That shape is lock contention, and the lock is
# almost certainly the allocator: a `Deal` is four `Vec<Card>` allocations and
# wasm's dlmalloc serialises them all. Allocation-free dealing comes first.
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
    # wasm-pack keeps a matching wasm-bindgen of its own; using that one rather
    # than asking for a separate install keeps the two versions in step, which
    # is a thing they must be.
    local bindgen
    # `|| true` because `ls` reports failure for whichever of the two cache
    # locations this machine does not have, and `set -e` would take that as the
    # assignment failing.
    bindgen=$(ls -t "$HOME"/Library/Caches/.wasm-pack/wasm-bindgen-*/wasm-bindgen \
                     "$HOME"/.cache/.wasm-pack/wasm-bindgen-*/wasm-bindgen 2>/dev/null \
              | head -1 || true)
    [ -n "$bindgen" ] || bindgen=$(command -v wasm-bindgen) || {
        echo "Error: no wasm-bindgen. Run './build.sh web' once to have wasm-pack fetch one." >&2
        exit 1
    }
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
