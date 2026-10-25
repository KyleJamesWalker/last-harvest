# last-harvest

A zombie survivors-like for the ANBERNIC RG DS Plus, written in Rust. It is built on the [twinbit-ds](https://github.com/KyleJamesWalker/twinbit-ds) template.

## The game

Press A or tap the title screen to start a run on a newly generated farm: a road with a gas station, a farmhouse and barn joined to the road by dirt paths, fenced crop and corn fields, a windmill, a graveyard, and scattered trees, wrecks and barrels.

The top screen is the main view, in 2.5D: every sprite's silhouette extruded into depth, lit by the player's lantern and fading into fog, seen from just behind the player. The bottom screen is the same farm top-down at 2x, 512x384 art pixels, with dots marking what the main view faces.

Zombies claw their way out of the ground, at the edge of the fog early on and only a few tiles away by the end of the night, then find their way around buildings and fences. Your weapons fire on their own at the nearest one. Tap the map to lob a grenade there; you hold three, and each recharges in 12 seconds. Kills drop money, and money levels you up: pick one of three upgrades on either screen. Shooting a red barrel sets it off. Survive until dawn, ten minutes in.

Every sound is synthesized at runtime from a few oscillator voices per cue, with a low drone that swells as the dead close in. They are placeholders until the game has recorded sound.

| Action | Device | macOS |
|---|---|---|
| Walk forward and back | Up and Down, or the stick | Up and Down arrows |
| Strafe | Left and Right, or the stick | Left and Right arrows |
| Turn | Y and A, or L2 and R2 | A and X, or 1 and 2 |
| Turn around | B | Z |
| Throw a grenade | Tap the map | Click the bottom window |
| Pick an upgrade | Tap it, or Up, Down and A | Click it, or arrows and X |
| Show the whole map | Hold L1 | Hold Q |
| Back to the menu | Back, or B after a run | Tab, or Z after a run |

The art comes from the tileset's reference sheet. `crates/game/art.txt` names each sprite and where it sits on the sheet, and `crates/game/build.rs` packs them into one texture at build time. The `art` debug scene shows every sprite in the catalog.

Also on every screen:

- Select toggles a performance overlay with FPS, frame times and temperatures.
- Volume and the longest night survived persist between launches.
- Closing the lid pauses the game and turns both backlights off.
- The power button suspends the device, and the game resumes where it left off.
- Plugging in headphones moves the audio to them.

The same code also builds and runs on macOS for quick iteration.

## Requirements

- Rust 1.94 or later, with the `aarch64-unknown-linux-gnu` target.
- [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) and [Zig](https://ziglang.org) for the device build.
- An RG DS Plus on the stock firmware, with SSH turned on and reachable as `anbernic.local`. Set `RGDS_HOST` to use a different host.
- Key-based SSH login to the device. The stock root password is `root`, so run `ssh-copy-id root@anbernic.local` once.

## Setup

```sh
rustup target add aarch64-unknown-linux-gnu
scripts/assets.sh <path to the tileset zip>   # unpack the art; every build needs it
scripts/sysroot.sh        # copy the device's libasound for linking
```

## Usage

Run on macOS:

```sh
cargo run -p last-harvest
```

Build, copy and start on the device:

```sh
scripts/deploy.sh
CAPTURE=1 scripts/deploy.sh   # also log every input device and the lid sensor
ssh root@anbernic.local tail -f /userdata/last-harvest/run.log
```

Install into the stock menu as Ports > LastHarvest:

```sh
scripts/install.sh
```

Run the tests:

```sh
cargo test
```

### Controls

| Action | Device | macOS |
|---|---|---|
| D-pad | D-pad or stick | Arrow keys |
| A / B / X / Y | A / B / X / Y | X / Z / S / A |
| L1 / R1 / L2 / R2 | Shoulders | Q / W / 1 / 2 |
| Start / Select | Start / Select | Enter / Backspace |
| Touch | Stylus or finger on the bottom screen | Mouse on the bottom window |
| Volume | V+ / V- | = / - |
| Lid | Close the lid | C toggles |
| Sleep and wake | Power button | None |
| Performance overlay | Select | Backspace |
| Back to the title | Back | Tab |
| Quit | Hold Home for one second | Esc |

The `hardware` debug scene checks every hardware feature and sets each screen's brightness: Up and Down set the top brightness, Left and Right the bottom. X and B toggle the work and low-power LEDs. L1 and R1 play the left and right speaker, R2 runs the motor, and any other button plays a tone.

### Debug mode

`DEBUG=1 scripts/deploy.sh` starts the game with a control pipe at `/tmp/last-harvest.ctl`. `scripts/debug.sh` drives it over SSH:

```sh
scripts/debug.sh scene farm                 # open a scene
scripts/debug.sh press a 120                # hold a button for 120 ms
scripts/debug.sh tap 500 400                # touch the bottom screen
scripts/debug.sh swipe 300 650 650 250 120  # drag over 120 ms
scripts/debug.sh shot out.png               # save both screens, top above bottom
scripts/debug.sh sleep 5                    # suspend, then wake on the RTC alarm
scripts/debug.sh panic                      # crash on purpose to test crash.log
scripts/debug.sh logs                       # follow the game log, with a perf line each second
```

The scene names are `title`, `harvest`, `farm` (the game without zombies, for looking around; X drops one a few steps ahead), `art` and `hardware`. On macOS, set `LAST_HARVEST_DEBUG=1` and write commands to the pipe directly.

### Environment variables

| Variable | Effect |
|---|---|
| `LAST_HARVEST_START` | Opens a scene at launch, by debug scene name. |
| `LAST_HARVEST_DEBUG` | Turns on debug mode. `DEBUG=1 scripts/deploy.sh` sets it. |
| `LAST_HARVEST_LOG_INPUT` | Logs every gamepad event. `CAPTURE=1 scripts/deploy.sh` sets it. |
| `LAST_HARVEST_SAVE` | Sets the save file path. |
| `LAST_HARVEST_TOP` | Names the output for the top screen, such as `DSI-2`. |
| `RGDS_HOST` | Sets the SSH target for the scripts. The default is `root@anbernic.local`. |

### Saves and crashes

The game keeps `save.txt` and `crash.log` next to its binary: `/mnt/sdcard/PORTS/last-harvest` when launched from Ports, `/userdata/last-harvest` in a dev run. It writes the save through a synced temp file and a rename, one second after the last change. Each panic appends a backtrace to `crash.log`.

## Performance

Measured on the device with the debug perf log, 4x MSAA, both screens at 1024x768: a run holds 60 FPS with the full horde of 300 zombies swarming the player. The 3D view skips faces turned away from the camera and draws only the nearest 160 zombies.

## Layout

| Path | Contents |
|---|---|
| `crates/game` | Scenes, input model, save format, bitmap font. No platform dependencies. |
| `crates/game/src/harvest` | The game: map generation, zombies, weapons, upgrades. |
| `crates/game/art.txt` | The sprite catalog that `build.rs` cuts from the tileset. |
| `crates/render` | OpenGL renderer for shapes, pixel art, TrueType text and the lit, fogged 3D cards: GL 3.3 on macOS, GLES 3.0 on the device. |
| `crates/app` | Windows, input, audio, saves, crash log, debug mode, and the device-only sysfs and evdev code. |
| `scripts` | Cross-compile, sysroot, deploy, install and debug scripts, and the on-device run script. |
| `assets` | The font and its license, the Ports launcher with its cover art, and the gitignored tileset. |

## Device notes

The stock firmware runs Weston with one output per panel. DSI-1 is the top screen and DSI-2 is the bottom screen, and each takes a fullscreen window. The game swaps them when the system screen-swap setting (`lcdswap`) is on. Rendering uses GLES 3.0 on the Mali-G52 through EGL on Wayland.

| Hardware | Interface |
|---|---|
| Buttons, stick, d-pad | evdev `ANBERNIC-rk3568-keys`. Weston does not forward this device. |
| Back key | evdev `adc-keys`, `KEY_BACK` |
| Power key | evdev `rk805 pwrkey`, `KEY_POWER`. A press runs the stock `/mnt/vendor/ctrl/pwr_new.sh` to suspend. |
| Touch | Wayland touch events from the `gt9xx` panel, up to 10 fingers. The stock menu sets `/sys/class/anbernic_misc/tpctrl` to `1` when it launches a port, which stops Weston from delivering touches, so the game sets it to `0`. |
| Lid | `/sys/class/anbernic_misc/hallkey`: `3` open, `2` closed |
| Vibration motor | `/sys/class/anbernic_misc/moto`: `1` on, `0` off |
| Speakers | ALSA through PulseAudio. The game plays stereo. |
| Headphone jack | `/sys/class/extcon/*/state` for `rk817-sound`, `HEADPHONE=1`. Its evdev switch device never sends events. The game moves audio between the PulseAudio sinks `alsa_output.1.stereo-fallback` (speakers) and `alsa_output.0.HiFi__Speaker__sink` (headphones). |
| Backlights | `/sys/class/backlight/backlight` (top) and `backlight1` (bottom), 0 to 255. `bl_power` `4` turns a panel off. |
| LEDs | `/sys/class/anbernic_misc/work_led` and `lowpwr_led`, `1` on, `0` off. Both on shows orange. |
| Battery | `/sys/class/power_supply/cw2015-battery`. The startup scripts select this fuel gauge over the PMU's `battery`. |
| Temperature | `/sys/class/thermal`, zones `soc-thermal` and `gpu-thermal`, in millidegrees |
| Wake alarm | `/sys/class/rtc/rtc0/wakealarm` |

The button codes do not follow the evdev gamepad layout. For example, Start reports `BTN_TR` and A reports `BTN_SOUTH`. `crates/app/src/device.rs` holds the full map.

On exit, the game hands the backlights, LEDs and touch mode back the way it found them.

When the stock menu launches a port, the frontend exits first and restarts afterwards. `scripts/deploy.sh` runs the game over SSH instead, so the on-device `run.sh` pauses the frontend to stop its idle timer from suspending the device. The timer keeps counting while paused, so resuming the frontend can suspend the device at once. For that reason a crash restarts the game, and only a clean quit hands control back.

## License

The code is under the [MIT License](LICENSE).

The game art is the [Zombie Apocalypse Tileset](https://ittaimanero.itch.io/zombie-apocalypse-tileset) by Ittai Manero. It is not included in this repository and the MIT License does not cover it. The pack's own license applies: you may use and edit it in personal and commercial games, but you may not redistribute or resell it. To build the game, download the pack from itch.io and run `scripts/assets.sh <path to the zip>`. It extracts to `assets/zombie-apocalypse/`, which git ignores.

The font is Atkinson Hyperlegible, under the SIL Open Font License in `assets/fonts/OFL.txt`.
