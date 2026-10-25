#!/usr/bin/env bash
# Builds and installs the game into the stock menu's Ports list. RGDS_HOST overrides the target.
set -euo pipefail
cd "$(dirname "$0")/.."
host="${RGDS_HOST:-root@anbernic.local}"
ports=/mnt/sdcard/PORTS
scripts/build-device.sh
ssh "$host" "mkdir -p $ports/last-harvest $ports/Imgs"
scp -q target/aarch64-unknown-linux-gnu/release/last-harvest "$host:$ports/last-harvest/last-harvest"
shopt -s nullglob
sounds=(assets/private/sounds/*.ogg assets/private/sounds/*.wav)
if (( ${#sounds[@]} )); then
  ssh "$host" "rm -rf $ports/last-harvest/sounds && mkdir -p $ports/last-harvest/sounds"
  scp -q "${sounds[@]}" "$host:$ports/last-harvest/sounds/"
fi
scp -q assets/LastHarvest.sh "$host:$ports/LastHarvest.sh"
scp -q assets/LastHarvest.png "$host:$ports/Imgs/LastHarvest.png"
echo "installed: Ports > LastHarvest"
