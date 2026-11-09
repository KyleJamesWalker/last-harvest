#!/usr/bin/env bash
# Builds the public card editor for the browser into target/mod-editor: the sprite editor without the
# tileset, keeping cards in the browser. --serve also serves it on port 8094 for a look; --remote serves it
# with the remote door on SPRITE_EDITOR_REMOTE's port (8792) for an agent, at http://127.0.0.1:8094/?remote.
set -euo pipefail
cd "$(dirname "$0")/.."
out=target/mod-editor
. scripts/lib/wasm.sh
CARGO_PROFILE_RELEASE_STRIP=symbols CARGO_PROFILE_RELEASE_OPT_LEVEL=s cargo build --release --target wasm32-unknown-unknown -p sprite-editor --bin sprite-editor --no-default-features --features modding
rm -rf "$out"
wasm-bindgen --target web --no-typescript --out-dir "$out" target/wasm32-unknown-unknown/release/sprite-editor.wasm
cp crates/editor/web/index.html crates/editor/web/sw.js crates/editor/web/manifest.webmanifest "$out/"
cp -R crates/editor/web/vendor "$out/"
cp assets/private/icons/editor-32.png "$out/icon.png"
cp assets/private/icons/editor-176.png "$out/apple-touch-icon.png"
cp assets/private/icons/editor-512.png "$out/icon-512.png"
# Only the card editor offers itself as an app.
sed -i.bak 's|<link rel="icon" href="icon.png">|<link rel="icon" href="icon.png"><link rel="manifest" href="manifest.webmanifest">|' "$out/index.html" && rm "$out/index.html.bak"
echo "built $out"
if [ "${1:-}" = "--remote" ]; then
  exec python3 tools/editor_relay.py "$out" --port 8094 --door "${SPRITE_EDITOR_REMOTE:-8792}"
fi
if [ "${1:-}" = "--serve" ]; then
  exec python3 -m http.server 8094 --bind 127.0.0.1 --directory "$out"
fi
