#!/bin/sh
# Runs on the device: pauses the stock frontend (and its idle auto-sleep) while the game runs.
# A deploy drops in last-harvest.new and game_data.pak.new and touches /tmp/last-harvest.restart to relaunch without resuming the frontend.
# A panic (exit 101) also relaunches; any other exit ends the session.
# Set CAPTURE=1 to also log every input device and the lid sensor, and DEBUG=1 for remote control.
cd "$(dirname "$0")"
. /etc/profile.d/myenv.sh
front=$(pidof muos2.bin dmenu.bin)
[ -n "$front" ] && kill -STOP $front
# The paused frontend's idle timer kept counting, so resuming it would suspend the device at once.
# Ending it instead makes dmenu_ln start a fresh one.
trap '[ -n "$front" ] && kill -CONT $front && kill $front; kill $(jobs -p) 2>/dev/null' EXIT INT TERM
if [ -n "$CAPTURE" ]; then
  for e in 0 2 4 6; do evtest /dev/input/event$e > ev$e.log 2>&1 & done
  (while :; do echo "$(date +%s) $(cat /sys/class/anbernic_misc/hallkey)"; sleep 0.5; done) > hall.log &
  export LAST_HARVEST_LOG_INPUT=1
fi
[ -n "$DEBUG" ] && export LAST_HARVEST_DEBUG=1
while :; do
  [ -f last-harvest.new ] && mv last-harvest.new last-harvest
  [ -f game_data.pak.new ] && mv game_data.pak.new game_data.pak
  rm -f /tmp/last-harvest.restart
  timeout "${TIMEOUT:-1800}" ./last-harvest > run.log 2>&1
  status=$?
  echo "run.sh: game exited with $status" >> run.log
  [ -f /tmp/last-harvest.restart ] && continue
  [ $status -eq 101 ] || break
  cp run.log crash-run.log
  sleep 1
done
