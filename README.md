# last-harvest

A zombie survivors-like for the ANBERNIC RG DS Plus, written in Rust. It is built on the [twinbit-ds](https://github.com/KyleJamesWalker/twinbit-ds) template. Players can add their own zombies and looks with cards, single PNGs made in a browser card editor, and change the rest of the game's data with mod packs: see Cards and mods.

## The game

Choose Play on the title's bottom screen to start a run on a newly generated farm: a road with a gas station, a farmhouse and barn joined to the road by dirt paths, fenced crop and corn fields, a windmill, a graveyard, and scattered trees, wrecks and barrels.

The top screen is the main view, in 2.5D: every sprite's silhouette extruded into depth, lit by the player's lantern and fading into fog, seen from just behind the player. The bottom screen is the same farm top-down at 2x, 512x384 art pixels, with dots marking what the main view faces.

Zombies claw their way out of the ground, at the edge of the fog early on and only a few tiles away by the end of the night, then find their way around buildings and fences. Your weapons fire on their own. Tap the map to lob a grenade there; you start with one, and each recharges in 12 seconds. Kills drop money, and money levels you up: pick one of three powerups on either screen; see [Powerups](#powerups). Shooting a red barrel sets it off; hold X and your guns go for the barrels in view. Ten minutes in, dawn wakes the Harvester behind you and the dead grow 1% stronger every second: run back to the farmhouse to survive. Start pauses. Fall, and you sink into the dirt and climb back out as one of them, leading the dead off to the next farm.

Pick a difficulty under Play with Left and Right, or by tapping its arrows. Easy bites gently, drops health often and starts you with a pistol, a knife and a grenade. Normal makes bites hurt and health scarce, and starts you with a knife and a grenade. Hard makes every bite count and starts you with only a grenade. The choice is remembered.

The title's menu also opens Settings, Stats and Credits; Up and Down choose a row, A or a tap opens it, and B goes back. Settings sets each screen's brightness, the volume, rumble, the frame-rate overlay, the player's look, which zombies come, and resets progress. Its Enemies page shows each kind walking on the top screen with its numbers, and A turns it on or off; nights skip the kinds turned off, and at least one stays on. Choose Settings from the pause menu to change all but the look, the enemies and the reset mid-run. Stats lists the records the save keeps. R1 saves both screens, top above bottom, as `screenshots/shot-NNNN.png` beside the save.

Every sound is synthesized at runtime from a few oscillator voices per cue, with a low drone that swells as the dead close in. A recorded WAV can replace any cue; see [Sounds](#sounds).

| Action | Device | macOS |
|---|---|---|
| Walk forward and back | Up and Down, or the stick | Up and Down arrows |
| Strafe | Left and Right, or the stick | Left and Right arrows |
| Turn | Y and A, or L2 and R2 | A and X, or 1 and 2 |
| Turn around | B | Z |
| Throw a grenade | Tap the map | Click the bottom window |
| Pick an upgrade | Tap it, or Up, Down and X | Click it, or arrows and S |
| Show the whole map | Hold L1 | Hold Q |
| Back to the menu | Back, or B after a run | Tab, or Z after a run |

The art comes from the tileset's reference sheet. `crates/game/art.txt` names each sprite and where it sits on the sheet, and `crates/game/build.rs` packs them into one texture at build time. The `art` debug scene shows every sprite in the catalog. The new weapons and every upgrade icon are original placeholders in `crates/game/placeholder/`. Redraw one in the [sprite editor](#sprite-editor) and the saved version replaces the placeholder.

### Powerups

A run starts with only the knife and one grenade, and the first level-up takes just 2 money. Each level-up offers three cards, drawn from 17 weapons and 17 passives. A run carries at most six weapons and six passives. Once both are full, the cards only offer levels for what you already carry. When nothing is left to level, the cards offer a snack, an ammo crate or pocket change.

Each weapon covers a zone, shown on its card:

- **Ahead** weapons fire into the view the top screen shows. The pistol also shoots anything within 70 pixels, whichever way you face.
- **Around** weapons hit a ring centered on you.
- **Behind** weapons fire or drop things behind you.
- **Map** weapons land on crowds anywhere on the farm.

Eight powerups are open from the first night. Each of the rest has a condition: a number of kills, a long enough night, a level reached, nights played or dawns survived. Meeting it only makes the powerup findable. From then on, a level-up may offer it on a green FOUND card, and taking that card unlocks it for good. A run can find two powerups at most. A find appears only when you have a free slot of its kind, so keep one open to find new weapons. Some finds appear only after a set minute of the night. The biggest appear only in the chest that each horde's gold-lit leader drops. Picking up a chest gives a free level-up that holds a find whenever one is waiting.

The title screen shows how many powerups are unlocked, how many are waiting to be found, and the condition you are closest to. The game-over screen names what the run found. Reset progress, in Settings, clears kills, records and found powerups after a second press. Settings stay.

`crates/game/powerups.txt` defines every powerup: its name, zone, unlock rule, how often it is found, highest level, and its stats at each level. Change a number and rebuild to rebalance. To add a powerup, copy a section under a new id. A `does` line reuses an existing weapon behavior with new numbers. The comments at the top of the file list every key and stat. A test reads the file, so a typo fails `cargo test` and names the line.

### Zombies

`crates/game/enemies.txt` defines every kind of zombie: its sprites, health, speed, bite, money, weight, death sound, and when and how often it arrives. Big is marked `heavy`, which roots it on a hard hit, and it leads hordes and dresses dawn's Harvester. The comments at the top of the file list every key, and a test reads it the same way.

### On every screen

- Select toggles a performance overlay with FPS, frame times and temperatures.
- Volume and the longest night survived persist between launches.
- Closing the lid pauses the game and turns both backlights off.
- The power button suspends the device, and the game resumes where it left off.
- Plugging in headphones moves the audio to them.

The same code also builds and runs on macOS for quick iteration.

## Cards and mods

Players can add to the game without the source: cards change zombies and the player, and mod packs change anything in its data. The game reads them from `data/` beside the binary at start-up.

An enemy card adds a kind of zombie, or replaces one, with a single PNG named `<id>.enemy.png`: a real PNG that previews anywhere, named so the game and the editor know it for a card. The image is the enemy's frames in five rows: walking front, side and back, as many frames across as its `walk` line says (1 to 8, 3 when left out), then the flash and hurt frames with one per facing. The sheet is as wide as its longest row, and every frame is the same size, up to 64 pixels. A compressed `iTXt` chunk named `last-harvest-enemy` holds its `enemies.txt` section, without `look`. A card with the id of an existing kind replaces it; a new kind goes in just before the last one. The game loads cards from `enemies/` inside `game_data.pak` and from `data/enemies/` beside the binary, or the folder `LAST_HARVEST_ENEMIES` names, and draws their frames into the sprite texture's free rows at startup. A card that can't load is left out, and the log says why. The packer leaves cards out of `game_data.pak`: `scripts/deploy.sh` and `scripts/install.sh` copy the cards in `assets/private/enemies/` into `data/enemies/` beside the game on the device, the player cards in `assets/private/players/` into `data/players/`, and the mod packs in `assets/private/mods/` into `data/mods/`, replacing the ones there, the way a player adds them.

A player card changes how the player looks: `<id>.player.png`, the same five-row sheet with three walking frames, and a `last-harvest-player` text chunk holding its `[id]`, a `name` line, and a `start DIFFICULTY WEAPON...` line for any difficulty whose starting weapons it sets, such as `start normal crows knife`; any weapon counts, up to the run's weapon slots, and a difficulty without a line keeps the game's own. The game loads them from `players/` in a pack and from `data/players/` beside the binary (or `LAST_HARVEST_PLAYERS`), and Settings > Player picks one, showing it walking on the top screen; the save keeps the choice by id. Make one with `cargo run -p last-harvest-pack -- player sheet.png jo.txt jo.player.png`.

```sh
cargo run -p last-harvest-pack -- card sheet.png ghoul.txt ghoul.enemy.png   # make a card
cargo run -p last-harvest-pack -- show ghoul.enemy.png                       # its frame size, text and spawn mix
```

A mod pack is a `.pak` in `data/mods/` beside the binary (or the folder `LAST_HARVEST_MODS` names). The game lays each over `game_data.pak` in file-name order, a mod's file replacing the base file at the same path: a sound in `sounds/`, a card in `enemies/`, or `enemies.txt` or `powerups.txt` themselves. A mod with an atlas must be built for this version of the game. That lets a card bring its own death sound: put `enemies/ghoul.enemy.png` and `sounds/kill-ghoul.ogg` in a folder and run `cargo run -p last-harvest-pack -- mod <folder> ghoul.pak`.

The sprite editor, on the Mac or an iPad, lists each card in `assets/private/enemies/` under Enemy cards as five sprites, like the game's own zombies: `CARD_<ID>_FRONT`, `_SIDE` and `_BACK` walking, then `_FLASH` and `_HURT` with one frame per facing, three frames each. Each part edits like any sprite and saves into its row of the card's PNG; resizing one resizes the whole card, and Compare shows all five side by side. A walking part takes 1 to 8 frames, and the other two walking parts follow its count; the flash and hurt parts keep one frame per facing. The game's own sprites hold to the same rules where the game needs them: a zombie's walk 1 to 8 frames, its flash and hurt 3, and the player's sprites 3. Stats (T), from any part, puts the card's name, numbers, death sound and flags on the canvas, as the game reads them. Under them, a chart shows how a night's spawns split between the game's kinds and every card in the folder, minute by minute from dusk to dawn, with this card's unsaved numbers; the legend gives each kind's share and flags any that never come, so a card that crowds out or replaces a kind shows at once. New with a name starting `CARD_` writes a card from a plain figure to paint over, and Copy from a card's part copies the whole card under a new id. Player cards list the same way under Player cards as `PLAYER_CARD_<ID>_FRONT` and so on, from the `players` folder beside `enemies`; every part keeps three frames, and their Stats page sets the name and, for each difficulty, the weapons a run starts with or Default for the game's own. In the card editor, a New name starting `PLAYER_` makes a player card.

The card editor, under Usage, makes both kinds in a browser.

## Requirements

- Rust 1.94 or later, with the `aarch64-unknown-linux-gnu` target.
- [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) and [Zig](https://ziglang.org) for the device build.
- An RG DS Plus on the stock firmware, with SSH turned on and reachable as `anbernic.local`. Set `RGDS_HOST` to use a different host.
- Key-based SSH login to the device. The stock root password is `root`, so run `ssh-copy-id root@anbernic.local` once.

## Setup

Every build needs the tileset at `assets/private/`. That folder is a submodule pointing at a private repository holding the tileset, the edited sprites and recorded sounds, since the tileset's license forbids redistributing it. With access to that repository:

```sh
git submodule update --init   # or clone with --recurse-submodules
```

Without it, buy the [tileset](https://ittaimanero.itch.io/zombie-apocalypse-tileset) and unpack it into the same place; the game builds with the tileset's own sprites:

```sh
scripts/assets.sh <path to the tileset zip>
```

Then:

```sh
rustup target add aarch64-unknown-linux-gnu
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

The game's folder keeps what ships under `data/` and what the game writes beside it:

```
Ports/last-harvest/
  last-harvest                   the program
  data/game_data.pak             the base game's data
  data/enemies/*.enemy.png       enemy cards
  data/players/*.player.png      player cards
  data/mods/*.pak                mod packs, laid over the base game's data
  save.txt  log.txt  screenshots/
```

Build the data pack, `target/game_data.pak`: a ZIP holding the sprite atlas, sounds, font and powerup table, with a `manifest.toml` that names the atlas it was built from. Builds keep the data built in through the default `embed` feature and read a pack if one is present. A build without it, `--no-default-features`, reads its data only from `data/game_data.pak` beside the binary and refuses to start without one, or with one built for a different atlas.

```sh
cargo run -p last-harvest-pack
```

Publishing a GitHub release, such as with `gh release create v0.2.0 --generate-notes`, runs `.github/workflows/release.yml`. It attaches the device build as a Ports-layout tarball and `game_data.pak` as a separate file; extract the tarball into the SD card's `Ports` folder and put the pack in `Ports/last-harvest/data/`. The workflow reads the private assets with a read-only deploy key on that repository, stored as the `ASSETS_DEPLOY_KEY` secret. The same release publishes the card editor to GitHub Pages, after checking the page holds only the editor's own files and no tileset; the repository's Pages source must be set to GitHub Actions.

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
| Screenshot | R1 | W |
| Back to the title | Back | Tab |
| Quit | Hold Home for one second | Esc |

The `hardware` debug scene checks every hardware feature and also sets each screen's brightness: Up and Down set the top brightness, Left and Right the bottom. X and B toggle the work and low-power LEDs. L1 and R1 play the left and right speaker, R2 runs the motor, and any other button plays a tone.

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
scripts/debug.sh logs                       # follow the game log, with a perf line each second: FPS, frame times, and CPU ms in update, frame building and rendering
```

The scene names are `title`, `harvest`, `farm` (the game without zombies, for looking around; X drops one a few steps ahead), `bench` (a frozen horde of 300 with every effect showing, turning a full circle every 600 frames, for comparing frame rates between builds), `art` and `hardware`. On macOS, set `LAST_HARVEST_DEBUG=1` and write commands to the pipe directly.

### Environment variables

| Variable | Effect |
|---|---|
| `LAST_HARVEST_START` | Opens a scene at launch, by debug scene name. |
| `LAST_HARVEST_DEBUG` | Turns on debug mode. `DEBUG=1 scripts/deploy.sh` sets it. |
| `LAST_HARVEST_LOG_INPUT` | Logs every gamepad event. `CAPTURE=1 scripts/deploy.sh` sets it. |
| `LAST_HARVEST_SAVE` | Sets the save file path. |
| `LAST_HARVEST_ENEMIES` | Sets the folder of enemy cards. The default is `data/enemies` beside the binary. |
| `LAST_HARVEST_PLAYERS` | Sets the folder of player cards. The default is `data/players` beside the binary. |
| `LAST_HARVEST_MODS` | Sets the folder of mod packs. The default is `data/mods` beside the binary. |
| `LAST_HARVEST_PAK` | Sets the data pack path. The default is `data/game_data.pak` beside the binary. |
| `LAST_HARVEST_SOUNDS` | Sets the folder of recorded sounds, over the pack's. Without a pack, the default is `sounds` beside the binary, then `assets/private/sounds`. |
| `LAST_HARVEST_TOP` | Names the output for the top screen, such as `DSI-2`. |
| `LAST_HARVEST_UNLOCK_ALL` | Offers every powerup, whatever the save has unlocked. |
| `LAST_HARVEST_KIT` | Starts every run carrying these powerups, as comma-separated `id` or `id:level`, such as `scythe:3,crows,dog`. |
| `LAST_HARVEST_CLOCK` | Starts every night this many seconds in, such as `590` to try the run home at dawn. |
| `RGDS_HOST` | Sets the SSH target for the scripts. The default is `root@anbernic.local`. |

### Sprite editor

```sh
cargo run -p sprite-editor
```

A pixel editor for the game's sprites, on macOS. The left pane lists every sprite in the catalog, with the pixel canvas, tools, color and frames around it. It starts with nothing open: pick a sprite from the list, or press N for a new one. `SPRITE_EDITOR_OPEN=NAME` opens one at launch, as `?open=NAME` does on the web. The right panes show the sprite as the game would: in the 2.5D view (drag to walk around it, scroll to move closer) and on the map (scroll to zoom). Choose how it stands in 3D: a prop, upright with a depth to extrude; a slab, lying flat as a roof at a height with its edges drawn down, for sprites drawn from above like cars; a figure that turns to face the camera; flat on the ground; or carved. A carved sprite gets two more drawings, beside its frames: a south view (the sprite seen from the south, west on the left) and an east view (seen from the east, south on the left). The solid is every voxel all three views agree on, and each face takes its color from the view that looks at it, so a car can have a raised cabin, a lower hood and wheels. Choosing Carve starts both views as a plain block in the sprite's colors, to erase into shape; the height buttons set how tall they are. When the sprite's own outline isn't its outline from above, as with a car drawn at an angle, add a footprint: a copy of the sprite to erase into the true outline. Only whether its pixels are clear counts; the sprite still colors the top. Delete on the footprint tile removes it.

| Action | Input |
|---|---|
| Draw, erase, fill | B, E, G, or the tool buttons |
| Draw a line, box or oval | The Line, Box and Oval buttons, then drag from one end or corner to the other; hold Shift as the drag starts to fill a box or oval, or keep a line straight |
| Pick a color | Hold I or Option and click, then let go to return to the tool; right-click always picks; the Pick button returns to the tool it took over from once it takes a color |
| Draw a straight line | Shift-click with the pencil or eraser, from the last pixel drawn |
| Search the sprite list | /, then type; Enter opens the first match and keeps the filter, Esc clears it |
| Mirror strokes left to right | M |
| See the previous frame faintly under this one | O, or Onion |
| Compare the sprite with its family, such as `KID_FRONT`, `KID_SIDE` and `KID_HURT`, side by side | C, or Compare; tap another to open it |
| Select, then move by dragging inside | S, then drag; arrows nudge 1 pixel, 8 with Shift |
| Copy, cut and paste | Cmd+C, Cmd+X, Cmd+V; a paste floats until Enter, Esc or a click outside, and its clear pixels leave what's under them |
| Delete the selected pixels | Delete or Backspace |
| Undo and redo | Cmd+Z, Cmd+Shift+Z |
| Next and previous sprite | J and K |
| Previous and next frame | H and L, or the Left and Right arrows when nothing is selected |
| Play the animation | Space |
| New sprite, or a copy of this one | New, Copy |
| Resize every frame | W and H - and +, 8 at a time with Shift; the bottom center stays put |
| Browse earlier versions | R, or History: the commits that touched the sprite in the assets repository, newest first. J and K step through them, shown read-only on the canvas and in the previews; Enter or Restore brings one back as unsaved changes, and Esc or Back returns to what was open. The editor only reads from git. |
| Save | Cmd+S; leaving a sprite with unsaved changes asks to save (S or Enter), discard (D) or stay (Esc) |
| Screenshot the window | F12, into the sprites folder |

Save writes each frame to `assets/private/sprites/NAME/0.png`, `1.png` and so on, and a changed stand to the sprite's line in `crates/game/art.txt` as `@prop 12`, `@slab 10`, `@figure` or `@ground`, or to `stand.txt` in its folder for a new sprite. A carved sprite keeps `south.png`, `east.png`, an optional `footprint.png`, and a `stand.txt` saying `carve` in its folder; a build without them, such as a fresh clone, stands it as a slab. The next build of the game packs a saved sprite in place of its cut from the tileset, and a folder with a name not in `crates/game/art.txt` becomes a new sprite, `art::NAME`, for the code to use. Saved sprites go in the private assets submodule, because edits of the tileset's sprites fall under its license: commit and push them there. Delete a sprite's folder to go back to the tileset's version. Set `LAST_HARVEST_SPRITES` to save somewhere else.

### Card editor

```sh
scripts/mod-editor.sh --serve   # build into target/mod-editor and serve it on http://127.0.0.1:8094
```

The public editor for enemy and player cards: the sprite editor built without the tileset (`--no-default-features --features modding`), so the page can be published. It keeps cards in the browser's local storage and lists only cards; New starts one from the plain figure, Import opens `.enemy.png` and `.player.png` files and the cards in a `.pak`, Export downloads the open card's saved PNG, and Export all downloads every card as one mod pack, `my-cards.pak`, for the game's `data/mods`. The palette is PICO-8's 32 colors, and the previews show the card on a plain floor, the ground tiles in flat colors. A player who owns the game can press Game data and pick their `game_data.pak`: the page keeps it in IndexedDB, never sends it anywhere, and from then on previews the card on the farm with the tileset's palette. A pack built for a different version of the game is refused with a note.

### Sprite editor on an iPad

The same editor also runs in a browser, served from the Mac to an iPad on the same network. Saves land in the Mac's `assets/private/sprites`, by the same rules as above, and History reads the same git log.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # the version in Cargo.lock
scripts/web-editor.sh                              # build, then serve on port 8080
```

The server prints the address to open on the iPad, such as `http://192.168.1.20:8080/`; add `?open=CAR_H` to start on a sprite. `SPRITE_SERVER_PORT` sets the port, and `scripts/web-editor.sh --build-only` only builds. Safari's Add to Home Screen gives the editor the whole screen.

| Action | Touch |
|---|---|
| Draw | Apple Pencil, or a finger on the canvas |
| Zoom and pan the canvas | Pinch with two fingers; Fit sets it back |
| Turn the 2.5D view, move closer | Drag it; pinch it |
| Zoom the map | Pinch it |
| Scroll the sprite list | Drag it; tap a name to open it |
| Keyboard shortcuts | The toolbar under the panes: Undo, Redo, Copy, Cut, Paste, Delete, Enter, Esc, Shift (stays on until tapped again), the arrows, sprites, frames, Search, History, New, Play, Mirror, Fit, Save |
| Hold Shift while drawing | Keep a finger on the Shift pad in either bottom corner |
| Name a sprite or search | Type in the field that appears |

A hardware keyboard keeps the macOS shortcuts. The web build embeds the tileset, whose license forbids redistributing it, so the server only answers loopback and private network addresses: never publish `target/web-editor` or expose the server to the internet. Anyone on the local network can open the editor and save sprites while it runs.

### Sounds

Put Ogg Vorbis or WAV files in `assets/private/sounds/`, named after the cue they replace. `scripts/deploy.sh` and `scripts/install.sh` ship them to the device inside `game_data.pak`. A cue with no file keeps its synth voice. Add `-2`, `-3` and so on for variations, such as `shot.wav` and `shot-2.wav`, and each play picks one at random. Ogg Vorbis is about a tenth the size of WAV and decodes once at startup; on the device a stereo track decodes at about 50 times real time. WAV files can be 8, 16, 24 or 32-bit PCM or 32-bit float. Either can be mono or stereo, at any sample rate. Only files named after a cue are loaded. Two kinds of long track loop instead, streamed a little at a time on their own threads: `music.ogg` replaces the synth drone and swells with the danger, and `ambience.ogg`, `ambience-2.ogg` and `ambience-3.ogg` play quietly underneath, fading out while the game is paused.

| File | Plays when |
|---|---|
| `shot`, `blast`, `slash` | The pistol, shotgun or knife fires |
| `thud`, `crit` | A hit lands, or a critical hit |
| `kill-skinny`, `kill-kid`, `kill-big` | A zombie dies |
| `throw`, `boom` | A grenade leaves your hand, or anything explodes |
| `pickup` | Money is collected; quick pickups play it higher, up a scale |
| `heal`, `hurt`, `heartbeat` | Health is picked up, you are hit, or health is low |
| `levelup`, `pick`, `pick-rare` | The XP bar fills, or an upgrade is chosen |
| `rise`, `groan`, `caw` | A zombie breaks out of the ground, groans, or a crow takes off |
| `bell`, `horde`, `dawn`, `gameover` | Each minute, each horde, surviving, or dying |
| `start`, `miss` | A run starts, or a tap with no grenade ready |

Every file also plays a few percent sharp or flat at random. Explosions, rising, groans and crows are panned and quieted by where they happen. They live in the private assets submodule, so check each file's license before sharing it anywhere else.

### Saves and crashes

The game keeps `save.txt` and `crash.log` next to its binary: `/mnt/sdcard/PORTS/last-harvest` when launched from Ports, `/userdata/last-harvest` in a dev run. It writes the save through a synced temp file and a rename, one second after the last change. Each panic appends a backtrace to `crash.log`.

## Performance

Measured on the device with the debug perf log, 4x MSAA, both screens at 1024x768: the `bench` scene, 300 zombies with every effect showing, holds 60 FPS, and so does a full horde swarming the player. The busiest fights, when kills pile up bodies, specks and numbers at once, dip to the high fifties. The 3D view skips faces turned away from the camera, draws only the nearest 160 zombies, and shades most cards with a shader variant that has no tint or flash lighting.

## Layout

| Path | Contents |
|---|---|
| `crates/game` | Scenes, input model, save format, bitmap font. No platform dependencies. |
| `crates/game/src/harvest` | The game: map generation, zombies, weapons, upgrades. |
| `crates/game/art.txt` | The sprite catalog that `build.rs` cuts from the tileset. |
| `crates/render` | OpenGL renderer for shapes, pixel art, TrueType text and the lit, fogged 3D cards: GL 3.3 on macOS, GLES 3.0 on the device. |
| `crates/app` | Windows, input, audio, saves, crash log, debug mode, and the device-only sysfs and evdev code. |
| `crates/editor` | The sprite editor, built on the game's renderer and views: a macOS window, or a web page in `crates/editor/web`. |
| `crates/pack` | Reads and writes data packs; its binary builds `game_data.pak` from the built atlas and the files in `assets`. |
| `crates/sprite-server` | Serves the web editor on the local network, with the sprites folder and its git history. |
| `scripts` | Cross-compile, sysroot, deploy, install and debug scripts, and the on-device run script. |
| `assets` | The font and its license, the Ports launcher with its cover art, and the README a release puts in `data/`. |
| `assets/private` | Submodule of the private assets repository: the tileset, edited sprites and recorded sounds. |

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

The game art is the [Zombie Apocalypse Tileset](https://ittaimanero.itch.io/zombie-apocalypse-tileset) by Ittai Manero. It is not included in this repository and the MIT License does not cover it. The pack's own license applies: you may use and edit it in personal and commercial games, but you may not redistribute or resell it. The `assets/private` submodule points at a private repository that holds it with the sprites edited from it; without access, download the pack from itch.io and run `scripts/assets.sh <path to the zip>`, which extracts it to `assets/private/zombie-apocalypse/`.

The font is Atkinson Hyperlegible, under the SIL Open Font License in `assets/fonts/OFL.txt`.
