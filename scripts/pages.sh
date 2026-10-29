#!/usr/bin/env bash
# Builds the GitHub Pages site into target/pages: the landing page, the game in play/ and the card editor in editor/.
# --serve also serves it on port 8093 for a look.
set -euo pipefail
cd "$(dirname "$0")/.."
out=target/pages
scripts/mod-editor.sh
scripts/web-game.sh
rm -rf "$out"
mkdir -p "$out"
cp assets/site/index.html assets/site/sw.js crates/editor/web/icon.png crates/editor/web/apple-touch-icon.png target/game_data.pak "$out/"
cp -R target/mod-editor "$out/editor"
cp -R target/web-game "$out/play"
# The game and the editor share the one pack at the root.
rm "$out/play/game_data.pak"
sed -i.bak 's|data-pak="game_data.pak"|data-pak="../game_data.pak"|' "$out/play/index.html" && rm "$out/play/index.html.bak"
grep -q 'data-pak="../game_data.pak"' "$out/play/index.html"
echo "built $out"
if [ "${1:-}" = "--serve" ]; then
  exec python3 -m http.server 8093 --bind 127.0.0.1 --directory "$out"
fi
