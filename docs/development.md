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
