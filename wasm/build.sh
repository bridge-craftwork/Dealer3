#!/bin/bash
#
# build.sh - Build the dealer3 WebAssembly package.
#
# Usage:
#   ./build.sh [web|nodejs|both]     (default: web)
#
#   web     ES module for the browser / Cloudflare Pages  -> pkg/
#   nodejs  CommonJS for Node, used by the verification tests -> pkg-node/
#
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

command -v wasm-pack >/dev/null || {
    echo "Error: wasm-pack not found. Install: https://rustwasm.github.io/wasm-pack/installer/" >&2
    exit 1
}

target="${1:-web}"
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
    web)    build web pkg ;;
    nodejs) build nodejs pkg-node ;;
    both)   build web pkg; build nodejs pkg-node ;;
    *)      echo "Unknown target '$target'. Use web, nodejs or both." >&2; exit 1 ;;
esac
