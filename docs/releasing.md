# Releasing

A published GitHub release builds every package, attaches one shared `game_data.pak`, and publishes the web build. Every change reaches `main` through a pull request first.

## Publish a release

```sh
gh release create v0.2.0 --generate-notes
```

Never create a release with `git tag` and `git push`. Publishing runs `.github/workflows/release.yml`, which calls `build.yml`. Both build with the tag as `LAST_HARVEST_VERSION`. To rebuild an existing release, run `release.yml` by hand with its tag.

A pre-release, or a tag not shaped `vX.Y.Z`, gets every package but skips GitHub Pages and itch.io. The RG DS launcher follows the latest release, so it never updates to a pre-release.

## Assets

Each release carries one `game_data.pak` beside the packages. No package carries the pack.

| Asset | For | Where its pack comes from |
|---|---|---|
| `last-harvest-<tag>-aarch64-linux.tar.gz` | The RG DS. Extract it into the SD card's `Ports` folder. | The launcher fetches it into `Ports/last-harvest/data/`. |
| `last-harvest-<tag>-android.apk` | Android 8.0 or newer on arm64 | The app fetches it on first run. |
| `last-harvest-<tag>-muos.muxapp` | muOS on the H700 Anbernics | `mux_launch.sh` fetches it into `data/`. |
| `last-harvest-<tag>-macos-arm64.zip`, `-windows-x86_64.zip`, `-windows-arm64.zip`, `-linux-x86_64.tar.gz`, `-linux-arm64.tar.gz` | Desktops | The player downloads it into `LastHarvest/data/`. |
| `last-harvest-<tag>-web.zip` | The browser game, also on Pages and itch.io | Inside the zip |

- The macOS package is `Last Harvest.app`, ad-hoc signed. It keeps its data, save and screenshots in the folder beside it.
- The Windows ARM64 build is cross-compiled and untested.
- The tarball, the web zip and the site carry `OFL.txt`, the font's license, and `THIRD_PARTY_LICENSES.txt` from `tools/third_party.py`.

## Secrets and settings

| Name | Kind | Use |
|---|---|---|
| `ASSETS_DEPLOY_KEY` | Secret | A read-only deploy key for the private assets repository |
| `ANDROID_KEYSTORE_B64`, `ANDROID_KEYSTORE_PASSWORD` | Secrets | The APK signing key, alias `last-harvest`, so each release installs over the last. Without them, the workflow signs with a throwaway key and warns. |
| `BUTLER_API_KEY` | Secret | An itch.io API key, from itch.io Settings > API keys |
| `ITCH_TARGET` | Variable | The butler target, such as `user/last-harvest:html5` |

Without `BUTLER_API_KEY` or `ITCH_TARGET`, releases skip itch.io. The repository's Pages source must be GitHub Actions.

## GitHub Pages

`scripts/pages.sh` builds the site into `target/pages`:

- `index.html`, the landing page from `assets/site/`.
- `manual.html`, rendered from [the game manual](guides/GAME_MANUAL.md) by `tools/render_manual.py`.
- The game in `play/` and the card editor in `editor/`.
- `game_data.pak` at the root, which both pages fetch.

The script refuses stale manual tables. The workflow checks that the site holds exactly the expected files and that neither wasm build includes the tileset.

## itch.io

Set up the project once:

1. Create the project with Kind of project set to HTML.
2. Under Embed options, set the viewport to a 2:3 size, such as 1024 x 1536 or 640 x 960. Click to launch in fullscreen also works.
3. Store the API key and the butler target as above.
4. After the first release pushes, edit the project and mark the upload as played in the browser.

## The launcher

`assets/LastHarvest.sh` keeps an RG DS install current before each start, never during a run.

- It asks the GitHub API for the latest release. A newer tag downloads this platform's tarball and swaps in its binary, data README, launcher and icon.
- It downloads `game_data.pak` when its sha256 differs from the release asset's `digest`, so a pack-only release also updates.
- It checks each download against its `digest` under a temporary name before it moves the file into place. A new binary must report the new tag.
- A new binary installs only with its release's pack. If the pack fails, the launcher keeps the old binary and pack.
- If `data/game_data.pak` is missing, it fetches the pack of the binary's own release, even with updates off.
- Offline, it starts the game as it is. curl stops after a 4-second connect timeout. On the device, which fetches with python, `timeout` stops the call after 8 seconds.
- It never updates a `dev` build. `data/no-updates` turns updates off.
- It logs what it did at the top of `log.txt`.

`LAST_HARVEST_API`, `LAST_HARVEST_DOWNLOADS` and `LAST_HARVEST_PLATFORM` point it at another release source and platform, for testing. The defaults are this repository's releases API, its release downloads, and `aarch64-linux` from `uname`.
