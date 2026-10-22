#!/usr/bin/env sh
# Builds the site into dist/.
#   BASE      path the site is served under (default "/")
#   SITE_URL  absolute origin, for feeds, sitemap and canonical links
set -eu
cd "$(dirname "$0")"
cargo build --release --lib --target wasm32-unknown-unknown
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Os --strip-debug target/wasm32-unknown-unknown/release/field.wasm -o target/field.opt.wasm
  WASM=target/field.opt.wasm
else
  WASM=target/wasm32-unknown-unknown/release/field.wasm
fi
cargo run --release --quiet --bin generate -- \
  --out dist --wasm "$WASM" --base "${BASE:-/}" --site-url "${SITE_URL:-}"
