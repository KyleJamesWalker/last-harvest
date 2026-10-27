#!/usr/bin/env bash
# Builds the public card editor for the browser into target/mod-editor: the sprite editor without the
# tileset, keeping cards in the browser. --serve also serves it on port 8094 for a look.
set -euo pipefail
cd "$(dirname "$0")/.."
out=target/mod-editor
want=$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' Cargo.lock)
have=$(wasm-bindgen --version 2>/dev/null | awk '{ print $2 }' || true)
if [ "$have" != "$want" ]; then
  echo "wasm-bindgen $want is needed (have: ${have:-none}): cargo install wasm-bindgen-cli --version $want" >&2
  exit 1
fi
rustup target list --installed | grep -qx wasm32-unknown-unknown || rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown -p sprite-editor --bin sprite-editor --no-default-features --features modding
rm -rf "$out"
wasm-bindgen --target web --no-typescript --out-dir "$out" target/wasm32-unknown-unknown/release/sprite-editor.wasm
cp crates/editor/web/index.html crates/editor/web/sw.js crates/editor/web/manifest.webmanifest crates/editor/web/*.png "$out/"
# Only the card editor offers itself as an app.
sed -i.bak 's|<link rel="icon" href="icon.png">|<link rel="icon" href="icon.png"><link rel="manifest" href="manifest.webmanifest">|' "$out/index.html" && rm "$out/index.html.bak"
echo "built $out"
if [ "${1:-}" = "--serve" ]; then
  exec python3 -m http.server 8094 --bind 127.0.0.1 --directory "$out"
fi
