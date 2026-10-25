#!/usr/bin/env bash
# Extracts the Zombie Apocalypse Tileset zip from itch.io into assets/private/zombie-apocalypse/,
# for a checkout without access to the private assets submodule.
set -euo pipefail
zip="${1:?usage: scripts/assets.sh <path to Zombie Apocalypse Tileset.zip>}"
cd "$(dirname "$0")/.."
dest=assets/private/zombie-apocalypse
mkdir -p assets/private
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
unzip -q "$zip" -x '__MACOSX/*' '*.DS_Store' -d "$tmp"
rm -rf "$dest"
mv "$tmp/Zombie Apocalypse Tileset" "$dest"
echo "assets ready: $dest"
