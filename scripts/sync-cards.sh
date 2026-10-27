#!/usr/bin/env bash
# Copies the enemy cards in assets/private/enemies into a folder on the device, the way a player adds one,
# replacing the cards already there. Usage: sync-cards.sh <host> <folder>
set -euo pipefail
cd "$(dirname "$0")/.."
host="${1:?host}" dir="${2:?folder}"
shopt -s nullglob
cards=(assets/private/enemies/*.enemy.png)
ssh "$host" "mkdir -p '$dir' && rm -f '$dir'/*.enemy.png"
if (( ${#cards[@]} )); then
  scp -q "${cards[@]}" "$host:$dir/"
fi
echo "cards: ${#cards[@]} in $dir"
