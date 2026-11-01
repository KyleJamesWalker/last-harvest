#!/bin/bash
# HELP: Last Harvest - survive the night on a farm overrun by the dead
# ICON: lastharvest

. /opt/muos/script/var/func.sh

if command -v SETUP_APP >/dev/null 2>&1; then
	SETUP_APP lastharvest ""
else
	echo app >/tmp/act_go
	SETUP_SDL_ENVIRONMENT
fi

# Derived from $0, not GET_VAR: the app may be installed on either card.
APP_DIR="$(cd "$(dirname "$0")" && pwd)"
export SDL_AUDIODRIVER=alsa
cd "$APP_DIR" || exit 1
chmod +x ./last-harvest 2>/dev/null

# Only muOS's virtual pad when it exists, else every node the kernel also gives a jsN handler.
if [ -z "$SDL_JOYSTICK_DEVICE" ]; then
	PADS="$(awk '/^N: / { name = $0 } /^H: / && (/js[0-9]/ || name ~ /"muOS-Keys"/) && match($0, /event[0-9]+/) { node = "/dev/input/" substr($0, RSTART, RLENGTH); if (name ~ /"muOS-Keys"/) first = first (first ? ":" : "") node; else rest = rest (rest ? ":" : "") node } END { print first ? first : rest }' /proc/bus/input/devices)"
	[ -n "$PADS" ] && export SDL_JOYSTICK_DEVICE="$PADS"
fi

# The 3D view needs the CPU at full speed; put the governor back after.
CPU=/sys/devices/system/cpu/cpu0/cpufreq
OLD_GOVERNOR="$(cat "$CPU/scaling_governor" 2>/dev/null)"
echo performance >"$CPU/scaling_governor" 2>/dev/null

./last-harvest >"$APP_DIR/log.txt" 2>&1

[ -n "$OLD_GOVERNOR" ] && echo "$OLD_GOVERNOR" >"$CPU/scaling_governor" 2>/dev/null
SCREEN_TYPE="internal"
[ "$(GET_VAR config boot/device_mode)" = "1" ] && SCREEN_TYPE="external"
FB_SWITCH "$(GET_VAR device screen/${SCREEN_TYPE}/width)" "$(GET_VAR device screen/${SCREEN_TYPE}/height)" 32
sync
