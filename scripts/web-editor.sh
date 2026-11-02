#!/usr/bin/env bash
# Builds the sprite editor for the browser into target/web-editor, then serves it to the local network.
# The build embeds the tileset, so it must never be published: see "Sprite editor on an iPad" in the README.
# --build-only skips serving; SPRITE_SERVER_PORT sets the port (8080).
set -euo pipefail
cd "$(dirname "$0")/.."
out=target/web-editor
. scripts/lib/wasm.sh
cargo build --release --target wasm32-unknown-unknown -p sprite-editor --bin sprite-editor
rm -rf "$out"
wasm-bindgen --target web --no-typescript --out-dir "$out" target/wasm32-unknown-unknown/release/sprite-editor.wasm
cp crates/editor/web/index.html "$out/"
cp assets/private/icons/editor-32.png "$out/icon.png"
cp assets/private/icons/editor-176.png "$out/apple-touch-icon.png"
cp assets/private/icons/editor-512.png "$out/icon-512.png"
echo "built $out"
if [ "${1:-}" != "--build-only" ]; then
  exec cargo run --release -p sprite-server -- "$out"
fi
