#!/usr/bin/env bash
# Copies the enemy cards, player cards, props, maps and mod packs in assets/private/enemies, players, props, maps
# and mods into a game's data folder on the device, the way a player adds them, replacing the ones already there.
# Usage: sync-data.sh <host> <data folder>
set -euo pipefail
cd "$(dirname "$0")/.."
host="${1:?host}" data="${2:?data folder}"
shopt -s nullglob
for kind in "enemies:*.enemy.png" "players:*.player.png" "props:*.prop.png" "maps:*.map.txt" "mods:*.pak"; do
  dir="${kind%%:*}" pattern="${kind#*:}"
  files=(assets/private/$dir/$pattern)
  ssh "$host" "mkdir -p '$data/$dir' && rm -f '$data/$dir'/$pattern"
  if (( ${#files[@]} )); then
    scp -q "${files[@]}" "$host:$data/$dir/"
  fi
  echo "$dir: ${#files[@]} in $data/$dir"
done
