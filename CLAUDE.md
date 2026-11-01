# Last Harvest

A survivors-like for a dual-screen Linux handheld, with a web build and sprite/card editors. README.md is the full reference.

## Workspace
- `crates/game`: scenes, rules and data tables; no platform code. The loop is `App::tick` (app) → `Game::update` → `Harvest::update`.
- `crates/render`: OpenGL renderer. `crates/app`: the device/macOS binary and the wasm game. `crates/editor`: sprite editor (native, iPad via `crates/sprite-server`) and the public card editor (wasm). `crates/pack`: builds `game_data.pak` and mod packs.

## Checks
- `cargo clippy --workspace --all-targets` at zero warnings and `cargo test --workspace` before every commit; CI (`.github/workflows/ci.yml`) runs both with `-D warnings`. The code is not rustfmt-formatted: never run `cargo fmt`.
- Device: `scripts/deploy.sh`, `scripts/install.sh`, `scripts/debug.sh`. Web: `scripts/web-game.sh`, `scripts/mod-editor.sh`, `scripts/pages.sh`, `scripts/web-editor.sh`.
- After editing an inline `<script>` in a web page, run `tools/csp.py`; after changing `powerups.txt` or `enemies.txt`, `tools/manual_items.py`.

## Rules
- The tileset is licensed: the `embed` (game) and `tileset` (editor) features must never reach a published wasm build. Published builds use `--no-default-features` (`--features modding` for the card editor); CI's `cargo tree` check enforces it.
- Licensed art and edited sprites live only in the `assets/private` submodule; commit them there.
- Art colors come only from `crates/game/palette.txt`; a test fails on any other.
- The `crates/game/*.txt` data files are the rules; `the_games_own_data_must_be_whole_and_name_its_sprites` fails on any bad entry.
- Every game option must be settable in both the sprite editor and the card (mod) editor.
- A test that calls `data::install` goes in `crates/game/tests/` or `crates/app/tests/` (one install per process); nothing reachable from `install` may call `book()`, `roster()` or a `data::*()` getter.
- Releases: `gh release create`, never `git tag` + push. One shared `game_data.pak` per release, uploaded beside the tarballs, never inside one.
- Since v0.0.1 every change goes through a PR; never push to `main`.
