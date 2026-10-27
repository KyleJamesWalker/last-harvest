#!/usr/bin/env bash
# Builds, copies to the device, and (re)starts the game. RGDS_HOST overrides the target.
set -euo pipefail
cd "$(dirname "$0")/.."
host="${RGDS_HOST:-root@anbernic.local}"
dir=/userdata/last-harvest
scripts/build-device.sh
cargo run -q -p last-harvest-pack
ssh "$host" "mkdir -p $dir"
scp -q scripts/device-run.sh "$host:$dir/run.sh"
scp -q target/game_data.pak "$host:$dir/game_data.pak.new"
scp -q target/aarch64-unknown-linux-gnu/release/last-harvest "$host:$dir/last-harvest.new"
# A run.sh from before packs never swaps game_data.pak.new in.
ssh "$host" "rm -rf $dir/sounds; [ -f $dir/game_data.pak ] || cp $dir/game_data.pak.new $dir/game_data.pak"
ssh "$host" "cd $dir; if pidof last-harvest >/dev/null; then touch /tmp/last-harvest.restart; killall last-harvest; else CAPTURE=${CAPTURE:-} DEBUG=${DEBUG:-} nohup ./run.sh >/dev/null 2>&1 </dev/null & fi"
echo "deployed; logs: ssh $host tail -f $dir/run.log"
