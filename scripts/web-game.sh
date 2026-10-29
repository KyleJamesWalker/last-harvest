#!/usr/bin/env bash
# Builds the game for the browser into target/web-game, beside the game_data.pak it fetches at start.
# --serve also serves it on port 8095 for a look; --zip also packs it as target/last-harvest-web.zip for itch.io.
set -euo pipefail
cd "$(dirname "$0")/.."
out=target/web-game
want=$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' Cargo.lock)
have=$(wasm-bindgen --version 2>/dev/null | awk '{ print $2 }' || true)
if [ "$have" != "$want" ]; then
  echo "wasm-bindgen $want is needed (have: ${have:-none}): cargo install wasm-bindgen-cli --version $want" >&2
  exit 1
fi
rustup target list --installed | grep -qx wasm32-unknown-unknown || rustup target add wasm32-unknown-unknown
cargo run -q -p last-harvest-pack
CARGO_PROFILE_RELEASE_STRIP=symbols cargo rustc --release --target wasm32-unknown-unknown -p last-harvest --lib --crate-type cdylib --no-default-features
rm -rf "$out"
wasm-bindgen --target web --no-typescript --out-name last-harvest --out-dir "$out" target/wasm32-unknown-unknown/release/last_harvest.wasm
cp crates/app/web/index.html crates/editor/web/icon.png target/game_data.pak "$out/"
echo "built $out"
case "${1:-}" in
  --zip)
    rm -f target/last-harvest-web.zip
    (cd "$out" && zip -q -X -r ../last-harvest-web.zip .)
    echo "packed target/last-harvest-web.zip"
    ;;
  --serve) exec python3 -m http.server 8095 --bind 127.0.0.1 --directory "$out" ;;
esac
