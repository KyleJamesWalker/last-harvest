#!/usr/bin/env bash
# Builds and installs the game into the stock menu's Ports list. RGDS_HOST overrides the target.
set -euo pipefail
cd "$(dirname "$0")/.."
host="${RGDS_HOST:-root@anbernic.local}"
ports=/mnt/sdcard/PORTS
scripts/build-device.sh
cargo run -q -p last-harvest-pack
game="$ports/last-harvest"
ssh "$host" "mkdir -p $game/data $ports/Imgs && rm -rf $game/sounds $game/enemies $game/game_data.pak"
scp -q target/aarch64-unknown-linux-gnu/release/last-harvest "$host:$game/last-harvest"
scp -q target/game_data.pak "$host:$game/data/game_data.pak"
scripts/sync-data.sh "$host" "$game/data"
scp -q assets/LastHarvest.sh "$host:$ports/LastHarvest.sh"
scp -q assets/LastHarvest.png "$host:$ports/Imgs/LastHarvest.png"
echo "installed: Ports > LastHarvest"
