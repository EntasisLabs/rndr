#!/usr/bin/env bash
# Build examples/web for the browser: compile for wasm32, then run
# wasm-bindgen into examples/web/pkg. Serve examples/web over HTTP and open it
# in a browser with WebGPU.
#
#   scripts/web-build.sh            # release
#   PROFILE=debug scripts/web-build.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
profile="${PROFILE:-release}"
cd "$root"

need="$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p' | head -n1)"
have="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)"
if [ "$have" != "$need" ]; then
  echo "wasm-bindgen $need is required (found: ${have:-none})." >&2
  echo "Install it with: cargo install wasm-bindgen-cli --version $need --locked" >&2
  exit 1
fi

flags=()
if [ "$profile" = release ]; then
  flags+=(--release)
fi
cargo build -p web-hello --lib --target wasm32-unknown-unknown "${flags[@]}"
wasm-bindgen --target web --no-typescript \
  --out-dir examples/web/pkg \
  "target/wasm32-unknown-unknown/$profile/web_hello.wasm"
echo "Built examples/web/pkg. Serve with: python3 -m http.server -d examples/web 8000"
