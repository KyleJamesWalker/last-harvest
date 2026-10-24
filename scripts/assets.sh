#!/usr/bin/env bash
# Extracts the Zombie Apocalypse Tileset zip from itch.io into assets/zombie-apocalypse/.
set -euo pipefail
zip="${1:?usage: scripts/assets.sh <path to Zombie Apocalypse Tileset.zip>}"
cd "$(dirname "$0")/.."
dest=assets/zombie-apocalypse
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
unzip -q "$zip" -x '__MACOSX/*' '*.DS_Store' -d "$tmp"
rm -rf "$dest"
mv "$tmp/Zombie Apocalypse Tileset" "$dest"
echo "assets ready: $dest"
