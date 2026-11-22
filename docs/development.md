# Development

This page covers setup, the build, the tests and the checks a pull request must pass. It also lists the workspace layout and every environment variable. Platform builds have their own pages: [device](device.md), [web, Android and muOS](platforms.md), and [releasing](releasing.md).

## Requirements

- Rust 1.94 or later.
- The tileset in `assets/private/`. See [Setup](#setup).
- For the device build: the `aarch64-unknown-linux-gnu` target, [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) and [Zig](https://ziglang.org).
- For the web builds: the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` at the version in `Cargo.lock`. The web scripts stop with the install command when the version differs.

## Setup

Every build reads the tileset from `assets/private/`. That folder is a submodule of a private repository, because the tileset's license forbids redistribution. It also holds the edited sprites, recorded sounds, and the cards, props, maps and mods that the deploy scripts copy to the device.

With access to the private repository:

```sh
git submodule update --init
```

Without access, buy the [Zombie Apocalypse Tileset](https://ittaimanero.itch.io/zombie-apocalypse-tileset) and extract it. The game then builds with the tileset's own sprites.

```sh
scripts/assets.sh <path to the tileset zip>   # extracts to assets/private/zombie-apocalypse/
```

For the device build, also run:

```sh
rustup target add aarch64-unknown-linux-gnu
scripts/sysroot.sh   # copies the device's libasound into .sysroot/ for linking
```

## Build and run

```sh
cargo run -p last-harvest       # the game, in one desktop window
cargo run -p last-harvest-pack  # builds target/game_data.pak
cargo run -p sprite-editor      # the sprite editor; see sprite-editor.md
```

The default `embed` feature builds the game data into the binary. The binary still reads a pack when one is present. A build with `--no-default-features` reads its data only from `data/game_data.pak` beside the binary. If that pack is missing or built for a different atlas, the game starts with no sprites. The title then names the release whose pack to copy, and the log says why.

`game_data.pak` is a ZIP of the sprite atlas, sounds, font and data tables. Its `manifest.toml` names its atlas and its release: `LAST_HARVEST_VERSION` at the packer's build time, or `dev`. The binary reports its own release, or `dev-<commit>`, with `--version`, at the top of `log.txt`, and on the Stats page.

On a desktop, `LAST_HARVEST_WINDOWS=2` opens one window per screen, as on the device. The mouse is then a finger on the bottom window.

### Split-screen co-op

At the title, Start on a second pad joins it as P2, and again leaves; the title names who joined. A run then starts with one player and one view per seat, up to four. Split-screen runs on desktop, web and muOS only, never on a touch screen, a phone or the two-screen handhelds.

Two views sit side by side in a wide or square window and stack in a tall one; three or four share a 2x2 grid, with the fourth cell black for three. The 3D view fills each view at its own aspect, and the HUD keeps to the view's edges. Each player has their own pad, view, corner map, L1 map, kit and cards. XP is shared: a level-up pauses everyone, deals each player three cards from their own kit and resumes once all have picked; banishes, rerolls and favors are each player's own. Start pauses everyone, and only the player who paused works the menu, its Settings, whose changes apply to everyone, and photo mode, which shows in their view only. Each pad rumbles for its own player: through gilrs on Windows (macOS pads have no force feedback in gilrs), the gamepad's vibration actuator in the browser, and SDL rumble for a joined pad on muOS, where player 1 keeps the device's motor. Hit-stop never freezes the shared night, and grenade aiming never slows it. A player who turns while others stand leads the dead: a gold-lit leader zombie rises from their body with a swarm around it, their pad steers it (X calls the dead near it into the swarm), and its bites are its only harm. Its swarm drops nothing and counts no kills. Once the leader falls, the player has 15 s to pick, on their map, where the next one rises, at least 10 tiles from the living, or it rises where a horde's leader would (`[leader]` in `tuning.txt`). The end screen adds how long each led and how many bites their swarm landed. The save records the run as one night, with player 1's build and damage.

Without a second pad, `LAST_HARVEST_SPLIT=2` (up to `4`) shows at least that many views, and `LAST_HARVEST_BOTS` players fill the views past the seated ones. The web game takes `?split=2` instead.

```sh
LAST_HARVEST_SPLIT=2 LAST_HARVEST_BOTS=1 LAST_HARVEST_WINDOW=1280x720 LAST_HARVEST_START=harvest cargo run -p last-harvest
```

## Test and check

Run these before every commit. CI runs the same checks with `-D warnings`.

```sh
cargo clippy --workspace --all-targets   # zero warnings
cargo test --workspace
```

The code is not rustfmt-formatted. Do not run `cargo fmt`.

| Check | When to run it |
|---|---|
| `cargo deny check advisories` | Before a dependency change. Allow an advisory only in `deny.toml`, with its reason. |
| `python3 tools/csp.py` | After editing an inline `<script>` in a web page. Each page's Content-Security-Policy allows its inline script by hash. |
| `python3 tools/manual_items.py` | After changing `powerups.txt` or `enemies.txt`. It fills the manual's weapon, passive and zombie tables. |
| `shellcheck --severity=error scripts/*.sh assets/LastHarvest.sh` | After editing a shell script. |

Some tests guard the data and the art:

- `the_games_own_data_must_be_whole_and_name_its_sprites` fails on any bad entry in a `crates/game/*.txt` file and names the line.
- A palette test fails on any drawing or coat color that `crates/game/palette.txt` does not list.
- A test that calls `data::install` belongs in `crates/game/tests/` or `crates/app/tests/`, because each process installs the data once.

## DPS sandbox

`last-harvest-sandbox` plays whole nights headless, with no renderer or audio, to put numbers behind a balance change in `powerups.txt`. Each night runs the normal spawn schedule, enemy mix and zombie cap on a generated farm, from a fixed seed. By default a scripted player walks a circle or a figure eight, turns round on a schedule, lobs a grenade straight ahead on a timer and takes no level-up card. It never aims. The build is forced from the command line, and the player is undying unless `--mortal` is given.

Options make the player closer to a real one. The defaults replay older CSVs exactly.

| Option | The player |
|---|---|
| `--walk corner` | Walks to the farm's nearest corner, turns round to face out and stands there, the way a corner camper plays. It stands wherever it got to after 60 s. |
| `--walk avoid` | Never stops walking, and turns toward the open way that leads least toward the zombies the map shows: each where the view ahead last saw it, fading as it's forgotten. It uses only the move and turn buttons and what the map shows, would rather veer than turn back, and never turns round on a schedule. Fair to weapons that hit behind or around the player. |
| `--pick random` | Takes one of the three level-up cards at random, from the game's own draw. |
| `--pick held` | Takes a card for a powerup it holds, rare first. With none on offer it rerolls, then banishes, while the run allows, then takes a new one. It favors a held weapon from `favor_level`. |
| `--pick first:a+b+c` | Takes the first powerup on the list that's on offer, rerolling once for one, else picks as `held`. |
| `--locked` | With a pick, offers only what a new player has unlocked; otherwise every powerup can come up. |
| `--no-throw` | Lobs no grenades, the same as `--throw-every 0`. The 12 s default flatters blast passives. |
| `--secs S` | Past 600 the night goes on after dawn, the Harvester awake and the dead growing stronger. A walker that happens home ends its night there. |
| `--net-sim SPEC` | Plays each seed online instead: a second, scripted player is a client behind a simulated link (`loss=0.1,lat=80,jitter=20`, in ms, any left out), its input crossing to the host and snapshots crossing back. Prints bytes a second each way and how far the client's picture strays from the host's, in pixels. |
| `--replay FILE` | Plays a recorded night again instead. See [Replays](#replays). |
| `--players N` | 1 to 4 players, each walking the same script from its own spot by the spawn and carrying the build. A mortal night ends once all have fallen; a fallen player leads the dead at the nearest living one until then. The table adds a line per player (kills, useful damage, hearts, falls) and the first fall of any; the weapon lines are the first player's. |

With a pick, the forced build is the starting kit and cards add to it; `--json` records each night's final build and the cards taken, with the time of each. It also records the zombie cap and when the field first reached it, each player's falls, kills and time leading the dead, and per minute the zombies on the field (mean and most at once), the most shots in flight and effects at once, and the players still standing: the counts that weigh on the frame rate. The CSV's first line is a `#` comment with the settings it ran under; `--diff` and `report.py` skip it.

```sh
cargo build --release -p last-harvest --bin last-harvest-sandbox
target/release/last-harvest-sandbox                              # each weapon alone at levels 1 to top and its breaker, 3 seeds each
target/release/last-harvest-sandbox --weapons pistol,knife --seeds 5 --csv before.csv
target/release/last-harvest-sandbox pistol:top,hollow_points:3 scarecrow:break --mortal --difficulty hard
target/release/last-harvest-sandbox --diff before.csv after.csv  # what a data change did, by build and weapon
target/release/last-harvest-sandbox --mods my_mods/              # with mod packs laid over the data; --pak for another base pack
```

A night takes about a second and a half on one core, and nights run on every core. `--help` lists every option.

The table and the CSV give, per build and per weapon in it, mean over seeds:

| Column | Meaning |
|---|---|
| `kills`, `k/min` | Kills, with their spread over seeds. Per weapon, its killing blows. The headline number. |
| `useful`, `raw`, `over` | Damage capped at each zombie's remaining health, damage as dealt, and the share lost to overkill. |
| `dps`, `raw dps` | Useful and raw damage a second the weapon was held. |
| `hearts`, `lvl5`, `lvl` | Hearts picked up, and the level at minute 5 and at the end. The CSV has kills, hearts and level by minute. |
| `fell` | Nights where the player fell, and when on average. An undying player stands back up and plays on. |
| `survival`, `censored` | CSV only. The mean time to the first fall, a night that never fell counting as its whole length, and how many never fell. Treat those nights as censored at the night's length, not as a time: a build that never falls could have lasted longer. `fell_at` averages only the nights that fell. |
| `cap`, `ceil` | The share of the night at the zombie cap, and whether the build hit the kill-rate ceiling: the field never filled and nearly every zombie that rose died, so the spawn rate, not the build, held its kills back. |

`grenades` covers grenades and barrels. `--json` writes every night played.

### Replays

The game records every night it plays or hosts: the night's setup, each tick's input that changed and a checksum of where every player and zombie stood. It writes the replay, gzipped, when the night ends, is left, or the game closes, into `replays/` beside `save.txt`, and keeps the last 10 as `night-<unix seconds>-<n>.lhr`. A 12-minute night is about 30 to 80 KB. An online client records nothing; the host's replay holds every seat's input as the host played it. The web game keeps its last 5 in the browser's local storage instead, and Android doesn't record yet. Stats > Replays plays one back in the game; see the [manual](guides/GAME_MANUAL.md#menus-and-settings).

The sandbox plays a replay again headless and checks it tick by tick. It prints the header, then `match`, or the first tick that drifted and the first checkpoint that differs, and exits 1. `--watch dogs` also counts what each player's dogs spent the night doing. `--record` writes a sandbox night as a replay.

```sh
target/release/last-harvest-sandbox --replay night-1795237918-00.lhr --watch dogs
target/release/last-harvest-sandbox pistol:3,dog:3 --seed 7 --pick held --walk avoid --record dog.lhr
```

A replay plays exactly only with the same build and data: its header names the game's version, a fingerprint of the data, the pack, the mods and the device, and the sandbox flags any that differ here. The device may differ: a night recorded on the Mac replays on the RG DS, muOS, Windows or the web build. That holds only while the game crate's float functions come from `crates/game/src/fmath.rs` (the pure-Rust `libm`) rather than std's `sin`, `cos`, `atan2` or `powi`, which call each platform's own libm and round differently; `crates/game/clippy.toml` makes CI refuse them outside the drawing-only modules. `sqrt`, `rem_euclid`, `to_radians` and arithmetic are exact everywhere and stay on std. A change that moves the simulation's results bumps `replay::FORMAT`, so older replays say so instead of drifting. Pull replays from a handheld over SSH:

```sh
scp 'root@anbernic.local:/userdata/last-harvest/replays/*.lhr' .          # RG DS, deploy.sh
scp 'root@anbernic.local:/mnt/sdcard/PORTS/last-harvest/replays/*.lhr' .  # RG DS, install.sh
scp 'muos:/mnt/sdcard/MUOS/application/LastHarvest/replays/*.lhr' .       # muOS, card 2; /mnt/mmc for card 1
```

### Balance matrix

`tools/matrix/run.py` runs the sandbox over every weapon alone, every pair, every weapon with every passive, every set of three, and a few thousand random full builds, about 22,000 nights, roughly an hour on a 14-core Mac. `tools/matrix/report.py` turns the CSVs into one page with level curves, pair and passive heatmaps, and a fitted effect for each powerup and each pair.

```sh
tools/matrix/run.py                     # every tier into target/matrix; a finished tier is kept
tools/matrix/run.py t1 t2 --random 1000 # some tiers, and a smaller random tier
tools/matrix/run.py t1 --player avoid,held,nothrow   # the player's options for every tier, into target/matrix/avoid-held-nothrow
uv run --no-project --with numpy tools/matrix/report.py --findings notes.html   # target/matrix/report.html
```

`--player` takes a walk (`circle`, `eight`, `avoid`, `corner`), a pick (`none`, `random`, `held`, `first:a+b`), `nothrow` and `locked`; the report lists each tier's settings under "How it was run".

Under the normal rules most builds kill nearly every zombie that rises by level 8, so kills tie. The pressure tiers lay `tools/matrix/pressure/tuning.txt` over the data (spawns up to 25/s, a cap of 600) to lift that ceiling; they need the `[spawn]` section of `tuning.txt` and are skipped without it. Survival, the time of the undying player's first fall, separates builds that still tie on kills.

## Continuous integration

| Workflow | Runs on | What it does |
|---|---|---|
| `ci.yml` | Every pull request and push to `main` | Clippy, the tests, the Windows, muOS and wasm builds, the no-tileset check on both wasm builds, `manual_items.py --check`, `csp.py --check` and shellcheck |
| `audit.yml` | Every pull request, push to `main`, and each Monday | `cargo deny check advisories` against `Cargo.lock` |
| `build.yml` | Called by `release.yml`, and on pull requests that touch the build | The Android, muOS and desktop packages |
| `release.yml` | A published GitHub release | Every package, GitHub Pages and itch.io. See [releasing](releasing.md). |

CI reads the private submodule with the `ASSETS_DEPLOY_KEY` secret. A pull request from a fork has no access to the secret, so it skips the Rust steps.

## Layout

| Path | Contents |
|---|---|
| `crates/game` | Scenes, rules, input model, save format and data tables. No platform code. The loop is `App::tick` (app), then `Game::update`, then `Harvest::update`. |
| `crates/game/*.txt` | The data files: the game's rules. See [modding](modding.md#data-files). |
| `crates/game/art.txt` | The sprite catalog. `build.rs` cuts each sprite from the tileset and packs one texture. |
| `crates/game/placeholder` | Original placeholder art, such as the upgrade icons. A sprite saved in the editor replaces its placeholder. |
| `crates/render` | The OpenGL renderer: GL 3.3 on macOS, GLES 3.0 on the device. |
| `crates/app` | The device, desktop, Android, muOS and web binaries: windows, input, audio, saves, crash log and debug mode. |
| `crates/editor` | The sprite editor and the card editor (`web/`). |
| `crates/sprite-server` | Serves the sprite editor to an iPad on the local network. |
| `crates/pack` | Reads and writes data packs. Its binary builds `game_data.pak` and mod packs. |
| `crates/net` | WebRTC peers in a matchbox signaling room, each with a reliable and an unreliable channel. Native and browser builds. Nothing uses it yet. |
| `crates/lobby` | The online co-op lobby: room codes, the join handshake, ready-up, start and drop-in. No transport of its own. See [online](online.md). |
| `crates/relay` | `last-harvest-relay`: signaling and TURN credentials for online co-op. Native only. See [online](online.md). |
| `deploy` | The relay's Dockerfile and a compose file with Caddy and coturn. Not deployed. |
| `scripts` | Build, deploy, install, debug, capture, web and Pages scripts. |
| `tools` | Python checks and generators for the manual, the CSP hashes and the third-party licenses. |
| `assets` | The font, the launchers, the data folder README and the Pages site. |
| `assets/private` | The private submodule: the tileset, edited sprites, sounds, and test cards, props, maps and mods. |

## Environment variables

### Game

| Variable | Effect |
|---|---|
| `LAST_HARVEST_START` | Opens a [debug scene](device.md#debug-scenes) at launch. |
| `LAST_HARVEST_RANGE` | The weapon the `range` scene holds: `id` or `id:level`, such as `rifle:3`. It can also name a file with a `level N` line and a `powerups.txt` section. An editor's Try it writes that file. |
| `LAST_HARVEST_DEBUG` | Turns on the debug command pipe. |
| `LAST_HARVEST_PIPE` | The debug command pipe. Default: `/tmp/last-harvest.ctl`. |
| `LAST_HARVEST_LOG_INPUT` | Logs every gamepad event. |
| `LAST_HARVEST_WINDOW` | The desktop window's size, such as `720x720`, to try a handheld's screen. |
| `LAST_HARVEST_WINDOWS` | `2`: one window per screen, top above bottom. |
| `LAST_HARVEST_SPLIT` | `2` to `4`: every run shows at least that many split-screen views, `LAST_HARVEST_BOTS` players in those past the seated ones. See [Split-screen co-op](#split-screen-co-op). |
| `LAST_HARVEST_TOUCH` | `off`: the desktop window plays without touch, as on muOS. |
| `LAST_HARVEST_TOP` | The output for the top screen, such as `DSI-2`. |
| `LAST_HARVEST_SAVE` | The save file. |
| `LAST_HARVEST_PAK` | The data pack. Default: `data/game_data.pak` beside the binary. |
| `LAST_HARVEST_SOUNDS` | A folder of recorded sounds, over the pack's. Without a pack, the default is `sounds` beside the binary, then `assets/private/sounds`. |
| `LAST_HARVEST_ENEMIES` | Enemy cards. Default: `data/enemies` beside the binary. |
| `LAST_HARVEST_PLAYERS` | Player cards. Default: `data/players`. |
| `LAST_HARVEST_PROPS` | Props. Default: `data/props`. |
| `LAST_HARVEST_MAPS` | Maps. Default: `data/maps`. |
| `LAST_HARVEST_FARMS` | Farms a player card picks: `<id>.txt` and `<id>.ground.txt`. Default: `data/farms`. |
| `LAST_HARVEST_MODS` | Mod packs. Default: `data/mods`. |
| `LAST_HARVEST_UNLOCK_ALL` | Offers every powerup, whatever the save has unlocked. |
| `LAST_HARVEST_KIT` | Starts every run with these powerups, as comma-separated `id` or `id:level`, such as `scythe:3,crows,dog`. |
| `LAST_HARVEST_CLOCK` | Starts every night this many seconds in, such as `590` to try dawn. |
| `LAST_HARVEST_GRENADES` | `endless`: grenades are never spent. |
| `LAST_HARVEST_BOTS` | 1 to 3: scripted players start every run beside you, each in another player card's look when there is one. They walk the sandbox's `avoid` script, fight with their own kit, throw a grenade every 12 s and fall on their own. Turned, they steer their leader at the nearest living player and rise again as soon as they may. |

### Build, editor and scripts

| Variable | Effect |
|---|---|
| `LAST_HARVEST_VERSION` | At build time, the release that the binary and `game_data.pak` report. |
| `LAST_HARVEST_NO_PAK` | `1`: `scripts/android.sh` leaves `game_data.pak` out of the APK, which then fetches the release's pack on first run. Release builds set it. |
| `LAST_HARVEST_SPRITES` | Where the sprite editor saves. Default: `assets/private/sprites`. |
| `LAST_HARVEST_CATALOG` | The sprite catalog the native sprite editor writes stands to. Default: `crates/game/art.txt`. |
| `SPRITE_EDITOR_OPEN` | The sprite the editor opens at launch. |
| `SPRITE_SERVER_PORT` | The sprite server's port. Default: `8080`. |
| `RGDS_HOST` | The device's SSH target. Default: `root@anbernic.local`. |
| `MUOS_HOST` | The muOS device's SSH target. Default: the `muos` SSH alias. |
| `CAPTURE`, `POWERLOG`, `DEBUG`, `TIMEOUT` | Options for `scripts/deploy.sh`. See [device](device.md#deploy). |
| `LAST_HARVEST_API`, `LAST_HARVEST_DOWNLOADS`, `LAST_HARVEST_PLATFORM` | Point the launcher at another release source or platform, for testing. See [releasing](releasing.md#the-launcher). |

### Network tests

Native `last-harvest-net` peers read these.

| Variable | Effect |
|---|---|
| `LAST_HARVEST_NET_LAG_MS` | Delays each unreliable packet this long. |
| `LAST_HARVEST_NET_LOSS` | Drops this percentage of unreliable packets. |
| `LAST_HARVEST_NET_TEST_RESTART_MS` | The offering peer restarts ICE this long after it connects, once. |
| `LAST_HARVEST_NET_TEST_DROP_SIGNAL_MS` | Drops the signaling connection without a close this long after it opens, once. |
| `LAST_HARVEST_NET_TURN_URL`, `_USERNAME`, `_CREDENTIAL` | The `echo_peer` example connects through this TURN relay only. |

The relay's own settings (`LAST_HARVEST_RELAY_*`) are in [online](online.md#relay-settings).
