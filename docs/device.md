# The RG DS handheld

This page covers deploy, debug mode and screen capture on the ANBERNIC RG DS Plus, and how the game uses its hardware. Do the device steps in [development](development.md#setup) first.

## Requirements

- The stock firmware, with SSH on and the device reachable as `anbernic.local`. Set `RGDS_HOST` for another host.
- Key-based SSH login. The stock root password is `root`, so run `ssh-copy-id root@anbernic.local` once.

## Deploy

`scripts/deploy.sh` builds the game, copies it to `/userdata/last-harvest` and starts it. A second deploy restarts the running game.

```sh
scripts/deploy.sh
ssh root@anbernic.local tail -f /userdata/last-harvest/run.log
```

| Variable | Effect |
|---|---|
| `CAPTURE=1` | Logs every input device and the lid sensor. |
| `POWERLOG=1` | Logs battery draw, temperature and the top CPU users to `power.log` every 15 seconds. |
| `DEBUG=1` | Starts the [debug command pipe](#debug-mode). |
| `TIMEOUT=N` | Stops each run after N seconds. The default, `0`, never stops it. |

The deploy and install scripts also copy the cards, props, maps and mods in `assets/private/` into the game's `data/` folder (`scripts/sync-data.sh`). They replace the files already there.

## Install

`scripts/install.sh` installs the game into the stock menu as **Ports > LastHarvest**, at `/mnt/sdcard/PORTS/last-harvest`. This is a `dev` build, so the launcher never updates it.

```text
PORTS/last-harvest/
  last-harvest                 the program
  data/game_data.pak           the game's data
  data/enemies/*.enemy.png     enemy cards
  data/players/*.player.png    player cards
  data/props/*.prop.png        props
  data/maps/*.map.txt          maps
  data/mods/*.pak              mod packs
  data/no-updates              if present, the launcher does not update the game
  save.txt  log.txt  crash.log  screenshots/
  .update/                     the launcher's downloads in progress
```

The game writes `save.txt` through a synced temp file and a rename, one second after the last change. Each panic appends a backtrace to `crash.log`.

## Debug mode

`DEBUG=1 scripts/deploy.sh` starts the game with a command pipe at `/tmp/last-harvest.ctl`. `scripts/debug.sh` drives it over SSH. On macOS, set `LAST_HARVEST_DEBUG=1` and write commands to the pipe directly.

```sh
scripts/debug.sh scene farm                 # open a scene
scripts/debug.sh press a 120                # hold a button for 120 ms
scripts/debug.sh tap 500 400                # touch the bottom screen
scripts/debug.sh swipe 300 650 650 250 120  # drag over 120 ms
scripts/debug.sh shot out.png               # save both screens, top above bottom
scripts/debug.sh sleep 5                    # suspend, then wake on the RTC alarm
scripts/debug.sh state                      # log the player's position and the cells and props around them
scripts/debug.sh panic                      # crash on purpose, to test crash.log
scripts/debug.sh logs                       # follow the log, with FPS and CPU times each second
```

### Debug scenes

Open a scene with `scripts/debug.sh scene NAME`, or at launch with `LAST_HARVEST_START=NAME`.

| Scene | What it shows |
|---|---|
| `title` | The title menu |
| `harvest` | A run |
| `single` | A run on one screen, whatever Settings says |
| `farm` | A run without zombies. X drops one a few steps ahead. |
| `bench` | A frozen horde of 300 with every effect, turning a full circle every 600 frames, to compare frame rates. `bench-160x120` and similar build a farm of that size and log the flow field time. |
| `art` | Every sprite in the catalog |
| `icon` | One figure close up in 3D and flat in a 512-pixel square, for icons and screenshots. The bottom screen lists its keys. |
| `range` | One weapon against a ring of zombies that rise again, with its damage per second. `LAST_HARVEST_RANGE` names the weapon. |
| `hardware` | A check of every hardware feature. See below. |

In the `hardware` scene, Up and Down set the top brightness, and Left and Right set the bottom. X and B toggle the work and low-power LEDs. L1 and R1 play the left and right speaker, and R2 runs the motor. Any other button plays a tone.

## Recording and streaming

`scripts/capture.sh` records the screens and sound to an MP4 on the SD card, or plays them live on the Mac. It works with any running game and needs nothing deployed. The device's hardware H.264 encoder does the work and takes about a third of one CPU core. On the `bench` scene, a recording of both screens with sound held 60 FPS.

```sh
scripts/capture.sh record              # both screens and sound, to /mnt/sdcard/captures
scripts/capture.sh stop                # finish the MP4
scripts/capture.sh list                # list the recordings on the device
scripts/capture.sh pull                # copy the newest here, top screen above the bottom
scripts/capture.sh stream              # play the top screen live in ffplay
scripts/capture.sh stream both         # both screens, stacked
AUDIO=0 scripts/capture.sh stream      # no sound, less delay
SAVE=1 scripts/capture.sh stream both  # also record the stream to the SD card
```

- `record` and `stream` take `top`, `bottom` or `both`.
- A recording of both screens keeps each screen as its own video track. `pull` stacks them into one video.
- A recording takes 50 to 80 MB a minute. `record` refuses to start with less than 1 GB free. `CAPTURE_DIR` sets another folder.
- The stream reaches the Mac about 240 ms after the device draws it, or 155 ms with `AUDIO=0`. The Mac needs `ffmpeg`, which includes `ffplay`.

## Performance

With 4x MSAA and both screens at 1024x768, the `bench` scene holds 60 FPS. So does a full horde around the player. The busiest fights dip to the high fifties. The 3D view skips faces turned away from the camera and draws only the nearest 160 zombies. See-through cards, such as a fading blast, take one more draw call after the opaque cards, from their own vertex buffer. A frame without them skips it.

## Hardware

The stock firmware runs Weston with one output per panel. DSI-1 is the top screen and DSI-2 is the bottom. The game swaps them when the system's `lcdswap` setting is on. Rendering uses GLES 3.0 on the Mali-G52 through EGL on Wayland.

| Hardware | Interface |
|---|---|
| Buttons, stick, d-pad | evdev `ANBERNIC-rk3568-keys`. Weston does not forward this device. |
| Back key | evdev `adc-keys`, `KEY_BACK` |
| Power key | evdev `rk805 pwrkey`, `KEY_POWER`. A press runs the stock `/mnt/vendor/ctrl/pwr_new.sh` to suspend. |
| Touch | Wayland touch from the `gt9xx` panel, up to 10 fingers. The stock menu sets `/sys/class/anbernic_misc/tpctrl` to `1`, which blocks touches, so the game sets it to `0`. |
| Lid | `/sys/class/anbernic_misc/hallkey`: `3` open, `2` closed |
| Vibration motor | `/sys/class/anbernic_misc/moto`: `1` on, `0` off |
| Speakers | ALSA through PulseAudio, in stereo |
| Headphone jack | `/sys/class/extcon/*/state` for `rk817-sound`, `HEADPHONE=1`. Its evdev switch never sends events. The game moves audio between the sinks `alsa_output.1.stereo-fallback` (speakers) and `alsa_output.0.HiFi__Speaker__sink` (headphones). |
| Backlights | `/sys/class/backlight/backlight` (top) and `backlight1` (bottom), 0 to 255. `bl_power` `4` turns a panel off. |
| LEDs | `/sys/class/anbernic_misc/work_led` and `lowpwr_led`, `1` on. Both on show orange. |
| Battery | `/sys/class/power_supply/cw2015-battery`. The startup scripts pick this fuel gauge over the PMU's `battery`. |
| Temperature | `/sys/class/thermal`, zones `soc-thermal` and `gpu-thermal`, in millidegrees |
| Wake alarm | `/sys/class/rtc/rtc0/wakealarm` |

The button codes do not follow the evdev gamepad layout. For example, Start reports `BTN_TR` and A reports `BTN_SOUTH`. `crates/app/src/device.rs` holds the full map. On exit, the game restores the backlights, LEDs and touch mode.

The stock menu exits its frontend before it launches a port. `scripts/deploy.sh` runs the game over SSH instead, so the on-device `run.sh` pauses the frontend to stop its idle timer. The timer keeps counting while paused, and a resumed frontend can suspend the device at once. For that reason, `run.sh` restarts the game after a crash, and only a clean quit resumes the frontend.
