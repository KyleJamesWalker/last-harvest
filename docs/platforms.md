# Web, Android, muOS and desktop

This page covers the builds beyond the RG DS: the web game, the card editor, Android, muOS and the desktops. Each section lists its script, what it needs, and how the platform differs. The [game manual](guides/GAME_MANUAL.md) covers how each one plays.

## Web

```sh
scripts/web-game.sh --serve   # the game, in target/web-game, on http://127.0.0.1:8095
scripts/web-game.sh --zip     # also packs target/last-harvest-web.zip, for itch.io
scripts/mod-editor.sh --serve # the card editor, in target/mod-editor, on http://127.0.0.1:8094
scripts/mod-editor.sh --remote # the same, with the remote door on 8792; see docs/sprite-editor.md
scripts/pages.sh --serve      # the whole Pages site, in target/pages, on http://127.0.0.1:8093
```

Both pages build without the tileset: the game with `--no-default-features`, and the card editor with `--no-default-features --features modding`. The `embed` (game) and `tileset` (editor) features must never reach a published build. CI fails either page if it builds the game crate with `embed`.

The web game is `crates/app/src/web.rs`. It fetches `game_data.pak` from the path in its page's `data-pak` attribute: beside it in the zip, or `../game_data.pak` on Pages.

- **Save:** the browser's local storage, in the `save.txt` format.
- **Mod packs:** the start overlay's Add mod keeps them in IndexedDB. On the same site, the game also lays the card editor's saved work over them as a mod named `card editor`. It reads that work from local storage by the keys in `crates/pack/src/browser.rs`.
- **Screens:** one by default, or two with `?screens=2`.
- **Not supported:** brightness, rumble, screenshots and the device sensors.

The card editor is the sprite editor's public build. It keeps cards in local storage and the player's own `game_data.pak` in IndexedDB. Its sound conversion uses libvorbis, vendored in `crates/editor/web/vendor/` and fetched on the first conversion only. Its offline copy loads a new release first whenever it can.

The Pages site shares its origin, `<owner>.github.io`, with the account's other Pages projects. Any of their pages can read or clear the cards and the game's save, so publish nothing untrusted there.

The touch layout lives in `crates/game/src/touch.rs`. A new platform passes the window's size and its fingers in pixels to `Game::set_phone`, then draws `Frame::phone`. The touch controls draw from the `TOUCH_` sprites in `art.txt`.

## Android

`scripts/android.sh` builds `target/android/last-harvest.apk` for arm64 devices on Android 8.0 or newer. `--install` installs it with `adb`.

It needs:

- The Android SDK and NDK: `ANDROID_HOME` and `ANDROID_NDK_HOME`, or the newest under `~/Library/Android/sdk`.
- JDK 17: `JAVA_HOME`, or Homebrew's `openjdk@17`.
- `cargo install cargo-ndk` and `rustup target add aarch64-linux-android`.

The game is a NativeActivity library (`crates/app/src/android.rs`) with `game_data.pak` as an APK asset. A small `MainActivity` (`crates/app/android/java/`) adds the vibrator, a second screen and the game folder. The script builds without Gradle: `javac` and `d8` compile the activity, and `aapt2`, `zipalign` and `apksigner` package the APK. It signs with the local debug key, `~/.android/debug.keystore`, and makes one if missing. A release build signs with the release key instead. See [releasing](releasing.md).

- **Second screen:** when Android reports a presentation display, `MainActivity` shows the bottom screen there and passes its touches on. To try it, turn on Developer options > Simulate secondary displays, or run `adb shell settings put global overlay_display_devices 1024x768/240`. `adb shell input -d <display> tap X Y` touches it.
- **Game folder:** Settings > Game folder opens `ACTION_OPEN_DOCUMENT_TREE`, and `MainActivity` keeps the grant, so the game needs no storage permission. At start it copies the folder's `mods/`, `enemies/`, `players/`, `props/` and `maps/` into internal storage. Without a folder, the game reads `Android/data/<app>/files/data`.
- **Logs:** `adb logcat -s last-harvest`. The save sits in internal storage.

## muOS

`scripts/muxapp.sh` builds `target/muos/LastHarvest.muxapp` for [muOS](https://muos.dev) on the 64-bit H700 Anbernics, such as the RG34XX-SP, RG35XX Plus and RG40XX. Run `scripts/sysroot.sh` once first.

- **Install:** copy the file to `ARCHIVE/` on either card and install it with Applications > Archive Manager. `--install` unpacks it over SSH instead, to `MUOS_HOST` (default: the `muos` SSH alias).
- **Libraries:** the binary (`crates/app/src/muos.rs`, the `muos` feature) uses the device's own SDL2 for a GLES 3 window, ALSA for sound, and muOS's SDL controller mapping. The script copies `libSDL2` from the device once. `MUOS_SDL` names a local copy instead.
- **Play:** one screen, no touch. L2 aims grenades. Rumble drives the motor that muOS's device config names (`board/rumble`).
- **Launcher:** `mux_launch.sh` sets up SDL, runs the CPU at full speed during play, and writes `log.txt` beside the game.
- **Performance:** on an RG34XX-SP (720x480), a run holds 60 FPS with 50 zombies on screen.

## Desktop

`cargo run -p last-harvest` builds the desktop game for macOS, Windows or Linux. It opens one resizable window and plays as on a phone. `LAST_HARVEST_WINDOWS=2` opens a window per screen instead, as on the device. `LAST_HARVEST_WINDOW` and `LAST_HARVEST_TOUCH` emulate a handheld's screen. See [environment variables](development.md#environment-variables).
