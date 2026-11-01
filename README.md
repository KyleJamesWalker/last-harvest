# last-harvest

A zombie survivors-like for the ANBERNIC RG DS Plus, written in Rust. It is built on the [twinbit-ds](https://github.com/KyleJamesWalker/twinbit-ds) template. Players can add their own zombies and looks with cards, single PNGs made in a browser card editor, and change the rest of the game's data with mod packs: see Cards and mods. The [manual](docs/guides/GAME_MANUAL.md) covers playing and making things for the game.

## The game

Choose Play on the title's bottom screen to start a run on a newly generated farm: a road with a gas station, a farmhouse and barn joined to the road by dirt paths, fenced crop and corn fields, a windmill, a graveyard, and scattered trees, wrecks and barrels.

The top screen is the main view, in 2.5D: every sprite's silhouette extruded into depth, lit by the player's lantern and fading into fog, seen from just behind the player. The bottom screen is the same farm top-down at 2x, 512x384 art pixels, with dots marking what the main view faces.

Zombies claw their way out of the ground, at the edge of the fog early on and only a few tiles away by the end of the night, then find their way around buildings, or chew through a fence when that's quicker. Shots fly past bushes, fences and signs but stop at buildings, vehicles and trees. Your weapons fire on their own. Tap the map to lob a grenade there; you start with one, and each recharges in 12 seconds. Kills drop money, and money levels you up: pick one of three powerups on either screen; see [Powerups](#powerups). Shooting a red barrel sets it off; hold X and your guns go for the barrels in view. Ten minutes in, dawn wakes the Harvester behind you and the dead grow 1% stronger every second: run back to the farmhouse to survive. Start pauses. Fall, and you sink into the dirt and climb back out as one of them, leading the dead off to the next farm.

Pick a difficulty under Play with Left and Right, or by tapping its arrows. Easy bites gently, drops health often and starts you with a pistol, a knife and a grenade. Normal makes bites hurt and health scarce, and starts you with a knife and a grenade. Hard makes every bite count and starts you with only a grenade. A new save starts on Easy, and the choice is remembered.

The title's menu also opens Settings, Stats and Credits; Up and Down choose a row, A or a tap opens it, and B goes back. Settings sets each screen's brightness, the volume, rumble, the frame-rate overlay, the player's look, which zombies come, and resets progress. Its Enemies page shows each kind walking on the top screen with its numbers, and A turns it on or off; nights skip the kinds turned off, and at least one stays on. Choose Settings from the pause menu to change all but the look, the enemies and the reset mid-run. Stats lists the records the save keeps. The top screen shows the game's version in its corner on every title page. R1 saves both screens, top above bottom, as `screenshots/shot-NNNN.png` beside the save.

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

On Normal a run starts with only the knife and one grenade; see the difficulties above. The first level-up takes just 2 money. Each level-up offers three cards, drawn from 17 weapons and 17 passives. A run carries at most six weapons and six passives. Once both are full, the cards only offer levels for what you already carry. When nothing is left to level, the cards offer a snack, an ammo crate or pocket change.

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

An enemy card adds a kind of zombie, or replaces one, with a single PNG named `<id>.enemy.png`: a real PNG that previews anywhere, named so the game and the editor know it for a card. The image is the enemy's frames in five rows: walking front, side and back, as many frames across as its `walk` line says (1 to 8, 3 when left out), then the flash and hurt frames with one per facing. A flash or hurt frame left fully clear is drawn by the game from that facing's first walking frame: the flash as the figure's shape in one pale colour, the hurt in reds with the outline kept. An optional sixth row holds dead frames, one per facing, marked by an uncompressed `last-harvest-rows` text chunk of `6`; a corpse tips its dead frame over, or the front hurt frame when the card has none. The sheet is as wide as its longest row, and every frame is the same size, up to 64 pixels. A compressed `iTXt` chunk named `last-harvest-enemy` holds its `enemies.txt` section, without `look`. Its optional `about` line is one sentence on what to expect, up to 80 printable ASCII characters, that Settings > Enemies shows above the kind; a card without one still loads. A card with the id of an existing kind replaces it; a new kind goes in just before the last one. The game loads cards from `enemies/` inside `game_data.pak` and from `data/enemies/` beside the binary, or the folder `LAST_HARVEST_ENEMIES` names, and draws their frames into the sprite texture's free rows at startup. A card that can't load is left out; the title screen shows the first reason, and the log lists them all. The packer leaves cards out of `game_data.pak`: `scripts/deploy.sh` and `scripts/install.sh` copy the cards in `assets/private/enemies/` into `data/enemies/` beside the game on the device, the player cards in `assets/private/players/` into `data/players/`, and the mod packs in `assets/private/mods/` into `data/mods/`, replacing the ones there, the way a player adds them.

A player card changes how the player looks: `<id>.player.png`, the same five-row sheet with three walking frames, and a `last-harvest-player` text chunk holding its `[id]`, a `name` line, an optional `about` line (one sentence, up to 80 printable ASCII characters, shown under the name in Settings > Player), and a `start DIFFICULTY WEAPON...` line for any difficulty whose starting weapons it sets, such as `start normal crows knife`; any weapon counts, up to the run's weapon slots, and a difficulty without a line keeps the game's own. Optional `hp`, `speed` and `magnet` lines set what the player starts a run with before upgrades; the rest keep `tuning.txt`'s. The game loads them from `players/` in a pack and from `data/players/` beside the binary (or `LAST_HARVEST_PLAYERS`), and Settings > Player picks one, showing it walking on the top screen; the save keeps the choice by id. Make one with `cargo run -p last-harvest-pack -- player sheet.png jo.txt jo.player.png`.

```sh
cargo run -p last-harvest-pack -- card sheet.png ghoul.txt ghoul.enemy.png   # make a card
cargo run -p last-harvest-pack -- show ghoul.enemy.png                       # its frame size, text and spawn mix
cargo run -p last-harvest-pack -- show ghoul.pak                             # a pack's files, its cards, and whether the game takes it
cargo run -p last-harvest-pack -- show old_farm.map.txt                      # whether a map reads, or its first mistake
cargo run -p last-harvest-pack -- prop sheet.png tank.txt rusty_tank.prop.png # make a prop
```

A map is a hand-made farm for a run, `<id>.map.txt`: the id is 1 to 24 lowercase letters, digits and `_`. The game reads maps from `maps/` in packs and from `data/maps/` beside the binary (or the folder `LAST_HARVEST_MAPS` names), and the title screen's Map row picks one, or a random farm, for every run after; the choice is saved. A map reads as a header, then its ground, then its props:

```text
name Old Farm
size 40 30          # 16x12 to 160x120 tiles
spawn 12 17         # the player's tile
home 9 16           # the tile they run back to at dawn
scatter             # optional: trees, bushes and wrecks over the rest of the open ground, new each run
ground              # `size` rows of one code per tile
..........vvv...........,,,......ccccc..
props
HOUSE 6 9 open 1    # a sprite by name and its top-left tile; `open` top rows don't block, `frame` picks one, `fps` animates
SIGN 14 3 hp 60     # `hp` hit points before it breaks, 0 to 1000, 0 for never; `cover yes` or `no` for whether it stops shots
fence rail 25 4 10 8 gate 4 cover yes
```

`ground.txt` lists the ground codes. The game's own are `.` dirt, `,` dirt with tufts, `g` grass, `s` soil, `p` sprouts, `c` corn, `v` and `h` roads running north-south and east-west, and `=` a path. Roads and paths join up with their neighbours, and corn grows a top and bottom at a field's edges. A fence is `rail` or `picket` round a rectangle, with an optional two-tile gate that many tiles from its left. Each sprite has its own hit points and cover, which `hp` and `cover` on its line override: fences, skull posts, signs, mailboxes and scarecrows break and let shots through, road barriers break and stop them, and buildings, vehicles, trees, tanks and tombstones stop shots and never break. Zombies chew through what breaks, shots and blasts break what is cover, and a broken prop stops blocking. Old maps read as before. A ring of trees and bushes always closes the edge, and the spawn and home must be open, off the edge, and joined by open ground, without breaking anything. A map that doesn't read is left out with its first mistake on the title screen; `sync-data.sh` copies `assets/private/maps/` to the device.

A prop is a player's own sprite for maps, `<id>.prop.png`: its frames side by side, one row high, and a compressed `last-harvest-prop` text chunk holding its `[id]` and, when they aren't the defaults, `frames N` (1 to 16 across), `fps N`, `open N` (top rows a figure walks through), `stand prop N`, `fold N`, `slab N` or `slab N float N`, `figure`, `ground`, `carve` or `turned [N]`, `views N` and `footprint yes` for a prop that carves or turns, `roof yes`, `north yes` and `west yes` for a carving's own top, north and west colors, `clear 5A6E3C ...` for colors the 3D view leaves out, `paint` and `coat` lines for coats as below, `cover no` (it stops shots otherwise), `hp N` (0 to 1000; it never breaks otherwise) and `broken yes` (its last frame shows once it breaks and the rest animate; otherwise it goes). Frames are up to 256 pixels a side. A prop that carves or turns keeps its south and east views under its frames, `views` pixels tall, then any north and west views, and a carving's footprint and top after its last frame. The game reads props from `props/` in packs and from `data/props/` beside the binary (or the folder `LAST_HARVEST_PROPS` names), draws them into the sprite texture after the cards, and a map places one as `PROP_<ID>`, upper case. In the editors a prop lists under Props as `PROP_<ID>` and edits like any sprite; its stand, fps, Cover, Broken frame and HP buttons set what its text says, and New with a name starting `PROP_` starts one.

The sprite editor makes maps too: they list under Maps as `MAP_<ID>`, and New with a name starting `MAP_` starts one of dirt in `assets/private/maps/`. The canvas shows the map from above as the game draws it, and the previews show it around the last tile touched. Ground paints a ground code, a road three lanes wide, and Shift-click paints a straight line from the last tile; Prop places a sprite from the picker, the game's own then the props kept beside the maps, Move drags a prop or fence and picks it, showing its hit points and cover to set for this map (Defaults gives it its sprite's again), Erase takes one off, Fence drags a fence with its gate mid-bottom, and Spawn and Home set those tiles. Width and Height grow or shrink the map 4 tiles at a time, or 16 with Shift, and Randomize lays out a farm as the random generator does, every piece of it a prop to edit; Undo takes it back. A line under the canvas says whether the game would load the map, or why not. In the card editor, Export on an open map gives `<id>.pak`, the map and every prop it places, for the game's `data/mods`; Export on a prop gives its PNG; Import takes a `.map.txt`, a `.prop.png` or a `.pak`'s maps and props as well as its cards; and Export all packs maps and props too. Scroll or pinch to zoom, right-drag or the arrows to move.

A mod pack is a `.pak` in `data/mods/` beside the binary (or the folder `LAST_HARVEST_MODS` names). The game lays each over `game_data.pak` in file-name order, a mod's file replacing the base file at the same path: a sound in `sounds/`, a card in `enemies/`, or one of the data files below. A mod with an atlas must be built for this version of the game. A card in `data/enemies/` or `data/players/` replaces a mod's card of the same file name. A mod that can't load is left out, like a card; the packer refuses one whose tables the game would refuse. Stats counts the cards and mods that loaded. That lets a card bring its own death sound: put `enemies/ghoul.enemy.png` and `sounds/kill-ghoul.ogg` in a folder and run `cargo run -p last-harvest-pack -- mod <folder> ghoul.pak`.

### Data files

The game reads its rules from seven text files in `crates/game/`. The packer puts them in `game_data.pak`, and a mod pack can replace any of them. Each holds today's values, so an unchanged file plays as before.

| File | What it sets |
|---|---|
| `tuning.txt` | Numbers no card sets: the player's base health, speed, footprint and turn speed, each difficulty, zombies, the Harvester, the night's length and spawn rings, grenades, barrels, blasts and weapon reach |
| `props.txt` | The game's own props a map can place, with walk-through rows, cover and hit points, and the defaults for fences and other sprites |
| `ground.txt` | Each ground code: its label, tile, how a run joins, what the cell is, and any crop on it |
| `farm.txt` | How the random farm lays itself out: the ground codes it is laid on, tufts, grass, corpses, fields, the props it strews and how often, and its buildings, where they stand and what goes near each |
| `effects.txt` | The sprite and speed of each effect, swing, bullet, thrown item, pickup and animal, plus effects a mod names, and the particle colors in `[colors]` |
| `powerups.txt` | Every weapon and passive |
| `enemies.txt` | Every kind of zombie, including a `charge` that speeds it up near the player |

A mod's `tuning.txt`, and the `[colors]` of its `effects.txt`, need only the lines they change. The other files replace the game's whole file. The packer and the game check a mod's files as the game reads them, and leave out a mod that would break the game.

A weapon in `powerups.txt` picks a behavior with `does`. Two behaviors take their shape from data. `does gun` fires `count` shots at what its `aim` finds (`nearest`, `ahead`, `behind` or `around`), spread by its `pattern` (`fan`, `ring` or `spray`). `does blade` cuts everything within `area` across `spread` degrees, toward what its `aim` finds. `does lob` throws `count` at a crowd within `range`, and `lands` says what it does there: `blast`, `fire`, `flare` or `tacks`.

Any weapon can name an `effect` and a `bullet` from `effects.txt`, and a `sound`. Its `kick`, `punch` and `shake` move the camera on each attack, and `flash` lights the ground: a color, its reach in tiles and its seconds. An `icon` can be a player's prop, `PROP_<ID>`. So a new weapon needs no code.

Both editors edit these files. Each file is a folder under one Data heading, and each section a page in it, such as `tuning.txt`'s `[player]` under Data > tuning. Its page shows one line a row, with the comment under it. Numbers step with - and +, and Edit types any value.

Save writes the file only when the game would read it. The sprite editor saves into `crates/game/`. The card editor keeps a changed file in the browser, and Export all carries it to the mod pack. A sprite that `props.txt` or `effects.txt` names shows a Data button that opens its section.

The sprite editor, on the Mac or an iPad, lists each card in `assets/private/enemies/` under Enemy cards as six sprites, like the game's own zombies: `CARD_<ID>_FRONT`, `_SIDE` and `_BACK` walking, then `_FLASH`, `_HURT` and `_DEAD` with one frame per facing, three frames each. A new card leaves the last three clear for the game to draw; Generate draws them in to edit from and Clear empties them again. Hurt and Kill play a hit or the game's death on the figure in the previews, and an open dead part shows its corpse lying as the game tips it over. Each part edits like any sprite and saves into its row of the card's PNG; resizing one resizes the whole card, and Compare shows all six side by side. A walking part takes 1 to 8 frames, and the other two walking parts follow its count; the flash, hurt and dead parts keep one frame per facing. The game's own sprites hold to the same rules where the game needs them: a zombie's walk 1 to 8 frames, its flash, hurt and dead 3, and the player's sprites 3. Stats (T), from any part, puts the card's name, about, numbers, death sound and flags on the canvas, as the game reads them; Rename and About type the name and the about line. Under them, a chart shows how a night's spawns split between the game's kinds and every card in the folder, minute by minute from dusk to dawn, with this card's unsaved numbers; the legend gives each kind's share and flags any that never come, so a card that crowds out or replaces a kind shows at once. New with a name starting `CARD_` writes a card from a plain figure to paint over, and Copy from a card's part copies the whole card under a new id. Player cards list the same way under Player cards as `PLAYER_CARD_<ID>_FRONT` and so on, from the `players` folder beside `enemies`; every part keeps three frames, and their Stats page sets the name, the about line and, for each difficulty, the weapons a run starts with or Default for the game's own. In the card editor, a New name starting `PLAYER_` makes a player card.

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
  data/props/*.prop.png          props for maps
  data/maps/*.map.txt            hand-made farms
  data/mods/*.pak                mod packs, laid over the base game's data
  data/no-updates                if present, the launcher doesn't update the game
  save.txt  log.txt  crash.log  screenshots/
  .update/                       the launcher's downloads in progress
```

Build the data pack, `target/game_data.pak`: a ZIP holding the sprite atlas, sounds, font and powerup table, with a `manifest.toml` that names the atlas it was built from and the release, `LAST_HARVEST_VERSION` when the packer was built, else `dev`. Builds keep the data built in through the default `embed` feature and read a pack if one is present. A build without it, `--no-default-features`, reads its data only from `data/game_data.pak` beside the binary and treats one that won't open, such as one built for a different atlas, as missing: it starts with no sprites, the title says which release's pack to copy where, and the log says why. A pack table this build refuses is left out for the game's own, with a line on the title.

The binary reports its release, `LAST_HARVEST_VERSION` when it was built, else `dev-<commit>`, with `--version`, at the top of `log.txt`, and in a corner of Stats, which adds the pack's when it differs.

```sh
cargo run -p last-harvest-pack
```

Publishing a GitHub release, such as with `gh release create v0.2.0 --generate-notes`, runs `.github/workflows/release.yml`. It attaches the device build as a Ports-layout tarball and `game_data.pak` as a separate file; extract the tarball into the SD card's `Ports` folder and put the pack in `Ports/last-harvest/data/`. Both are built with the tag as `LAST_HARVEST_VERSION`.

From then on the launcher, `LastHarvest.sh`, keeps the game current before each start, never during a run. It asks the GitHub API for the latest release; a newer tag downloads this platform's tarball and swaps in its binary, data README, launcher and icon, and a `game_data.pak` whose sha256 differs from the release asset's `digest` is downloaded, so a release that changes only the pack updates too. Each download is checked against its asset's `digest` under a temporary name and only then moved over the old file, and a new binary must report the new tag before it replaces the old one. When `data/game_data.pak` is missing, the launcher fetches the pack of the binary's own release, even with updates off. Offline, it gives up after a 4 second connect timeout and starts the game as it is. A `dev` build, as `scripts/install.sh` installs, is never updated, and `data/no-updates` turns updates off. The launcher logs what it did at the top of `log.txt`. `LAST_HARVEST_API`, `LAST_HARVEST_DOWNLOADS` and `LAST_HARVEST_PLATFORM` point it at another release source and platform, for testing.

The workflow reads the private assets with a read-only deploy key on that repository, stored as the `ASSETS_DEPLOY_KEY` secret. The same release publishes GitHub Pages, built by `scripts/pages.sh`: a landing page and `manual.html` at the root, the game in `play/`, the card editor in `editor/`, and `game_data.pak` at the root, which both fetch. It checks the site holds exactly those files and that neither page's wasm builds the tileset in; the repository's Pages source must be set to GitHub Actions. It also attaches the game as `last-harvest-<tag>-web.zip` and, once set up, pushes that zip to itch.io: see The game in a browser.

The tarball, the web zip and the site each carry `OFL.txt`, the font's licence, and `THIRD_PARTY_LICENSES.txt`, every shipped crate's licence as `tools/third_party.py` writes it. Each page's Content-Security-Policy allows its inline script by hash: after editing one, run `tools/csp.py`.

Every pull request runs `.github/workflows/ci.yml`: clippy at zero warnings, the tests, both wasm builds and their no-tileset check, `tools/manual_items.py --check`, `tools/csp.py --check` and shellcheck. A fork's PR has no `ASSETS_DEPLOY_KEY`, so it skips the Rust steps.

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
| Pause a run, or back out of a menu | Start, or B | Esc |
| Quit | Hold Home for one second | Cmd+Q, or close a window |

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
scripts/debug.sh state                      # log the player's position and the solid cells and props around them
scripts/debug.sh panic                      # crash on purpose to test crash.log
scripts/debug.sh logs                       # follow the game log, with a perf line each second: FPS, frame times, and CPU ms in update, frame building and rendering
```

The scene names are `title`, `harvest`, `farm` (the game without zombies, for looking around; X drops one a few steps ahead), `bench` (a frozen horde of 300 with every effect showing, turning a full circle every 600 frames, for comparing frame rates between builds; `bench-160x120` and the like build it on a farm that many tiles across and log the map's flow field time), `art` and `hardware`. On macOS, set `LAST_HARVEST_DEBUG=1` and write commands to the pipe directly.

### Recording and streaming

`scripts/capture.sh` records the device's screens and sound to an MP4 on the SD card, or plays them live on the Mac. It works with any running game, in debug mode or not, and needs nothing deployed. The device's hardware H.264 encoder does the work, so the game holds 60 fps. On the `bench` scene, recording both screens at 60 fps with sound left the frame rate at 60.2 and the worst frame at 21 ms. Capture takes about a third of one CPU core.

```sh
scripts/capture.sh record          # both screens and sound, to /mnt/sdcard/captures
scripts/capture.sh stop            # finish the MP4
scripts/capture.sh pull            # copy the newest here, top screen stacked above the bottom
scripts/capture.sh stream          # play the top screen live in ffplay; close the window to stop
scripts/capture.sh stream both     # both screens, stacked
AUDIO=0 scripts/capture.sh stream  # no sound, less delay
SAVE=1 scripts/capture.sh stream both  # also record the stream to the SD card
```

`record` and `stream` take `top`, `bottom` or `both`, and read which output is the top screen from `run.log`. `SAVE=1` writes the encoded stream to the SD card as it sends it, and closing the player saves it as an MP4, as `stop` does. A recording of both screens keeps each screen as its own video track. `pull` stacks them into one video, and a player that shows only the first track shows the top screen. A recording takes about 50 to 80 MB a minute, and `record` refuses to start with less than 1 GB free. Set `CAPTURE_DIR` to record somewhere else.

The stream reaches the Mac about 240 ms after the device draws it, or about 155 ms with `AUDIO=0`, before ffplay's own buffering. The Mac needs `ffmpeg`, which includes `ffplay`.

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
| `LAST_HARVEST_MAPS` | Sets the folder of maps. The default is `data/maps` beside the binary. |
| `LAST_HARVEST_PROPS` | Sets the folder of players' props. The default is `data/props` beside the binary. |
| `LAST_HARVEST_CATALOG` | Sets the sprite catalog the native sprite editor writes stands to. The default is `crates/game/art.txt`. |
| `LAST_HARVEST_PAK` | Sets the data pack path. The default is `data/game_data.pak` beside the binary. |
| `LAST_HARVEST_SOUNDS` | Sets the folder of recorded sounds, over the pack's. Without a pack, the default is `sounds` beside the binary, then `assets/private/sounds`. |
| `LAST_HARVEST_TOP` | Names the output for the top screen, such as `DSI-2`. |
| `LAST_HARVEST_UNLOCK_ALL` | Offers every powerup, whatever the save has unlocked. |
| `LAST_HARVEST_KIT` | Starts every run carrying these powerups, as comma-separated `id` or `id:level`, such as `scythe:3,crows,dog`. |
| `LAST_HARVEST_CLOCK` | Starts every night this many seconds in, such as `590` to try the run home at dawn. |
| `LAST_HARVEST_VERSION` | At build time, the release the binary and `game_data.pak` report. The default is `dev`, plus the commit for the binary. |
| `LAST_HARVEST_API` | The launcher's GitHub releases API, for testing updates. The default is `https://api.github.com/repos/KyleJamesWalker/last-harvest/releases`. |
| `LAST_HARVEST_DOWNLOADS` | The launcher's base URL for release assets. The default is `https://github.com/KyleJamesWalker/last-harvest/releases/download`. |
| `LAST_HARVEST_PLATFORM` | The launcher's platform, naming the tarball `last-harvest-<tag>-<platform>.tar.gz`. The default comes from `uname`: `aarch64-linux`. |
| `RGDS_HOST` | Sets the SSH target for the scripts. The default is `root@anbernic.local`. |

### Sprite editor

```sh
cargo run -p sprite-editor
```

A pixel editor for the game's sprites, on macOS. The left pane lists every sprite in the catalog, with the pixel canvas, tools, color and frames around it. It starts with nothing open: pick a sprite from the list, or press N for a new one. `SPRITE_EDITOR_OPEN=NAME` opens one at launch, as `?open=NAME` does on the web. The right panes show the sprite as the game would: in the 2.5D view (drag to walk around it, scroll to move closer) and on the map (scroll to zoom). The 2.5D view counts the quads the sprite is drawn with, four vertices each, to compare stands and drawings by cost. Choose how it stands in 3D: a prop, upright with a depth to extrude; a slab, lying flat as a roof at a height with its edges drawn down, for sprites drawn from above like cars; a figure that turns to face the camera; flat on the ground; or carved. A carved sprite gets two more drawings, beside its frames: a south view (the sprite seen from the south, west on the left) and an east view (seen from the east, south on the left). The solid is every voxel all three views agree on, and each face takes its color from the view that looks at it, so a car can have a raised cabin, a lower hood and wheels. Choosing Carve starts both views as a plain block in the sprite's colors, to erase into shape; the height buttons set how tall they are. When the sprite's own outline isn't its outline from above, as with a car drawn at an angle, add a footprint: a copy of the sprite to erase into the true outline. Only whether its pixels are clear counts; the sprite still colors the top. Top, North and West are optional drawings that color the top and the north and west sides in place of the sprite and the south and east views; they never change the shape, and a clear pixel leaves the color they replace. Delete on a tile removes it.

| Action | Input |
|---|---|
| Draw, erase, fill | B, E, G, or the tool buttons |
| Swap the pencil and the eraser | B again |
| Swap back to the color before | X |
| Draw a line, box or oval | The Line, Box and Oval buttons, then drag from one end or corner to the other; hold Shift as the drag starts to fill a box or oval, or keep a line straight |
| Pick a color | Hold I or Option and click, then let go to return to the tool; right-click always picks; the Pick button returns to the tool it took over from once it takes a color |
| Draw a straight line | Shift-click with the pencil or eraser, from the last pixel drawn |
| Search the sprite list | /, then type; Enter opens the first match and keeps the filter, Esc clears it |
| Mirror strokes left to right | M |
| See the previous frame faintly under this one | O, or Onion |
| Compare the sprite with its family, such as `KID_FRONT`, `KID_SIDE` and `KID_HURT`, side by side | C, or Compare; tap another to open it |
| Draw a figure's flash, hurt or dead frames from its walk, such as `SKINNY_DEAD` from `SKINNY_FRONT`, `_SIDE` and `_BACK` | Generate, on a `_FLASH`, `_HURT` or `_DEAD` sprite; a `<KIND>_DEAD` sprite of three frames gives that kind its own corpse |
| Select, then move by dragging inside | S, then drag; arrows nudge 1 pixel, 8 with Shift |
| Copy, cut and paste | Cmd+C, Cmd+X, Cmd+V; a paste floats until Enter, Esc or a click outside, and its clear pixels leave what's under them |
| Delete the selected pixels | Delete or Backspace |
| Undo and redo | Cmd+Z, Cmd+Shift+Z |
| Next and previous sprite | J and K |
| Previous and next frame | H and L, or the Left and Right arrows when nothing is selected |
| Play the animation | Space |
| New sprite, or a copy of this one | New, Copy |
| Resize every frame | W and H - and +, 8 at a time with Shift; the bottom center stays put |
| Browse earlier versions | R, or History: the commits that touched the sprite in the assets repository, newest first; for a card's part or a prop, the commits that touched its PNG. J and K step through them, shown read-only on the canvas and in the previews; Enter or Restore brings one back as unsaved changes, and Esc or Back returns to what was open. The editor only reads from git. |
| Save | Cmd+S; leaving a sprite with unsaved changes asks to save (S or Enter), discard (D) or stay (Esc) |
| Screenshot the window | F12, into `target/` |

Save writes each frame to `assets/private/sprites/NAME/0.png`, `1.png` and so on, and a changed stand to the sprite's line in `crates/game/art.txt` as `@prop 12`, `@fold 20`, `@slab 10`, `@figure` or `@ground`, or to `stand.txt` in its folder for a new sprite. A carved sprite keeps `south.png`, `east.png`, optional `footprint.png`, `roof.png`, `north.png` and `west.png`, and a `stand.txt` saying `carve` in its folder; a build without them, such as a fresh clone, stands it as a slab. A sprite with coats keeps them in `paints.txt`. Its `paint` line lists the colors that are paint, light to dark. Each `coat` line gives one color, whose darker shades follow the drawing's, or a color for each paint color. The build packs every frame once in each coat, the first coat's frames first. It repaints the views, footprint and top strip to match. The random farm and a map's `frame N` pick among the coats' frames. The game's own art keeps to `crates/game/palette.txt`: the tileset's 22 colors plus a few for what it lacks. A test fails on any drawing or coat color it doesn't list, so a new color goes there first, with what it's for. The editors draw from that palette by default. Lospec loads a palette from lospec.com by name, through the native editor or the sprite server, since lospec.com allows no fetches from a web page. Palette file loads a downloaded `.hex` file instead. The next build of the game packs a saved sprite in place of its cut from the tileset, and a folder with a name not in `crates/game/art.txt` becomes a new sprite, `art::NAME`, for the code to use. Saved sprites go in the private assets submodule, because edits of the tileset's sprites fall under its license: commit and push them there. Delete a sprite's folder to go back to the tileset's version. Set `LAST_HARVEST_SPRITES` to save somewhere else.

### The game in a browser

```sh
scripts/web-game.sh --serve   # build into target/web-game and serve it on http://127.0.0.1:8095
scripts/web-game.sh --zip     # also pack it as target/last-harvest-web.zip, for itch.io
scripts/pages.sh --serve      # the whole Pages site in target/pages, on http://127.0.0.1:8093
```

A teaser for players without the handheld: the same game built for WebAssembly by `crates/app/src/web.rs`, with both screens drawn on one canvas, top above bottom, scaled to fit the window. It is built without `embed` and fetches `game_data.pak` from the path in its page's `data-pak` attribute: beside it in the zip, `../game_data.pak` on Pages. It reads no mods, cards or maps. Sound starts with the first click or key, as browsers require. The keys are the macOS ones under Controls, without Esc or Home. A click or tap on the bottom screen is a touch. A gamepad in the browser's standard layout maps by its labels, as on an Xbox pad: A, B, X and Y, the bumpers and triggers as L1, R1, L2 and R2, View as Select, Menu as Start, and the d-pad and left stick. The save lives in the browser's local storage, in `save.txt`'s format. Brightness, rumble, screenshots and the device's sensors do nothing.

To publish it on itch.io, once:

1. Create the project with Kind of project set to HTML. Under Embed options, set the viewport to 1024 x 1536 or a smaller 2:3 size such as 640 x 960, or choose Click to launch in fullscreen; the page scales to whatever it gets.
2. Create an API key at itch.io under Settings > API keys and store it as the repository secret `BUTLER_API_KEY`.
3. Store the butler target, such as `user/last-harvest:html5`, as the repository variable `ITCH_TARGET`.
4. After the first release pushes, edit the project and mark the upload as played in the browser.

Without the secret or the variable, releases skip itch.io.

### Card editor

```sh
scripts/mod-editor.sh --serve   # build into target/mod-editor and serve it on http://127.0.0.1:8094
```

The public editor for enemy and player cards: the sprite editor built without the tileset (`--no-default-features --features modding`), so the page can be published. It keeps cards in the browser's local storage and lists only cards; New starts one from the plain figure, Import opens `.enemy.png` and `.player.png` files and the cards in a `.pak`, saying which saved cards it replaced, Export downloads the open card's saved PNG, and Export all downloads every card as one mod pack, `my-cards.pak`, for the game's `data/mods`. Sound gives the open enemy card its own death sound, an Ogg or WAV of up to 512 KB kept beside it in the browser, and Drop own on its Stats page removes it again; Export all packs it as `sounds/kill-<id>.ogg`, and Import of a pack brings its cards' sounds back. The palette is PICO-8's 32 colors, and the previews show the card on a plain floor, the ground tiles in flat colors. On Pages, the editor fetches the site's own `../game_data.pak` and previews on the farm; served on its own, without one, it falls back to the plain floor. A player can also press Game data and pick their own `game_data.pak`, which takes precedence: the page keeps it in IndexedDB, never sends it anywhere, and from then on previews the card on the farm with the tileset's palette. A pack built for a different version of the game is refused with a note. Once opened, the page keeps an offline copy, so it also works with no connection, from an iPad's home screen for one; it still loads a new release first whenever it can. On iOS, a copy added to the home screen keeps its own storage, apart from Safari's: cards made in one don't show in the other. The site shares its origin, `<owner>.github.io`, with the account's other Pages projects, so any of their pages could read or clear the cards and the game's save; publish nothing untrusted there. On a phone held upright, the editing pane spans the screen with the previews under it.

### Sprite editor on an iPad

The same editor also runs in a browser, served from the Mac to an iPad on the same network. Saves land in the Mac's `assets/private/sprites`, by the same rules as above, and History reads the same git log.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # the version in Cargo.lock
scripts/web-editor.sh                              # build, then serve on port 8080
```

The server prints the address to open on the iPad, such as `http://192.168.1.20:8080/`; add `?open=CAR_H` to start on a sprite. `SPRITE_SERVER_PORT` sets the port, and `scripts/web-editor.sh --build-only` only builds. The server answers only loopback, private-LAN and link-local addresses, so Tailscale clients (100.64.0.0/10) are refused. Safari's Add to Home Screen gives the editor the whole screen.

| Action | Touch |
|---|---|
| Draw | Apple Pencil, or a finger on the canvas |
| Zoom and pan the canvas | Pinch with two fingers; Fit sets it back |
| Turn the 2.5D view, move closer | Drag it; pinch it |
| Zoom the map | Pinch it |
| Scroll the sprite list | Drag it; tap a name to open it |
| Keyboard shortcuts | The toolbar under the panes: Undo, Redo, Copy, Cut, Paste, Delete, Enter, Esc, Shift (stays on until tapped again), the arrows, sprites, frames, Search, History, New, Play, Mirror, Fit, Save |
| Shift for the next pencil tap | Tap the Shift pad in either bottom corner; it lights until the pencil uses it, and a second tap turns it off |
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
| `crates/app` | Windows, input, audio, saves, crash log, debug mode, and the device-only sysfs and evdev code; the web page's shell in `src/web.rs` and `web/`. |
| `crates/editor` | The sprite editor, built on the game's renderer and views: a macOS window, or a web page in `crates/editor/web`. |
| `crates/pack` | Reads and writes data packs; its binary builds `game_data.pak` from the built atlas and the files in `assets`. |
| `crates/sprite-server` | Serves the web editor on the local network, with the sprites folder and its git history. |
| `scripts` | Cross-compile, sysroot, deploy, install, debug and capture scripts, the on-device run script, and the web and Pages builds. |
| `assets` | The font and its license, the Ports launcher with its cover art, the README a release puts in `data/`, and the Pages landing page in `site/`. |
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
