# Last Harvest

A survivors-like for a dual-screen Linux handheld, with a web build and sprite/card editors. Developer docs live in `docs/` (start at `docs/development.md`); the player manual is `docs/guides/GAME_MANUAL.md`.

## Workspace
- `crates/game`: scenes, rules and data tables; no platform code. The loop is `App::tick` (app) → `Game::update` → `Harvest::update`.
- `crates/render`: OpenGL renderer. `crates/app`: the device/macOS binary and the wasm game. `crates/editor`: sprite editor (native, iPad via `crates/sprite-server`) and the public card editor (wasm). `crates/pack`: builds `game_data.pak` and mod packs.
- `crates/platform`: store and console hooks (achievements, cloud saves, metrics, store overlay, input glyphs) with no-op and recording backends; no vendor SDK code. The game queues `Ask`s; `crates/app/src/platform.rs` picks the backend and hands them to it.

## Checks
- `cargo fmt --all`, `cargo clippy --workspace --all-targets` at zero warnings and `cargo test --workspace` before every commit; CI (`.github/workflows/ci.yml`) runs `cargo fmt --all --check` and clippy with `-D warnings`.
- `cargo deny check advisories` passes before a dependency change (`.github/workflows/audit.yml` runs it); allow an advisory only in `deny.toml`, with its reason.
- Device: `scripts/deploy.sh`, `scripts/install.sh`, `scripts/debug.sh`. Web: `scripts/web-game.sh`, `scripts/mod-editor.sh`, `scripts/pages.sh`, `scripts/web-editor.sh`.
- After editing an inline `<script>` in a web page, run `tools/csp.py`; after changing `powerups.txt` or `enemies.txt`, `tools/manual_items.py`.

## Worktrees
- Make worktrees under `.claude/worktrees/` and run `scripts/share-target.sh` once in the main checkout: every worktree there then compiles into the main checkout's `target/`, so dependencies build once instead of 4-8 GB per worktree. Each checkout's own outputs (`game_data.pak`, `target/muos`, `target/web-game`, `target/android`) still go in its own `target/`.
- Binaries are shared: find them with `cargo metadata` (`scripts/lib/target.sh` sets `$bin`), never a literal `target/release/...` in a script. `target/release/last-harvest` and the sandbox are whichever worktree built last, so build and use them in one go; parallel builds wait on cargo's lock.
- Remove a worktree once its PR merges.
- Run `scripts/prune-target.sh` when `target/` grows: debug builds on macOS leave a few hundred MB of `.o` files per rebuild that cargo never deletes.

## Rules
- The tileset is licensed: the `embed` (game) and `tileset` (editor) features must never reach a published wasm build. Published builds use `--no-default-features` (`--features modding` for the card editor); CI's `cargo tree` check enforces it.
- Licensed art and edited sprites live only in the `assets/private` submodule; commit them there.
- Art colors come only from `crates/game/palette.txt`; a test fails on any other.
- The `crates/game/*.txt` data files are the rules; `the_games_own_data_must_be_whole_and_name_its_sprites` fails on any bad entry.
- Every game option must be settable in both the sprite editor and the card (mod) editor.
- A test that calls `data::install` goes in `crates/game/tests/` or `crates/app/tests/` (one install per process); nothing reachable from `install` may call `book()`, `roster()` or a `data::*()` getter.
- Releases: `gh release create`, never `git tag` + push. One shared `game_data.pak` per release, uploaded beside the tarballs, never inside one.
- Since v0.0.1 every change goes through a PR; never push to `main`.
