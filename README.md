# Last Harvest

Last Harvest is a zombie survivors-like for the ANBERNIC RG DS Plus, a dual-screen Linux handheld. It also runs in a browser, on Android, on muOS handhelds, and on macOS, Windows and Linux. You hold a farm through one night while your weapons fire on their own, then run home at dawn. Players add their own zombies, looks, props, maps and mod packs without the source.

## Play

- **Download:** every [release](https://github.com/KyleJamesWalker/last-harvest/releases/latest) has a package for each platform and one shared `game_data.pak`. The RG DS, muOS and Android builds fetch the pack on first run. On a desktop, copy it into the `data` folder.
- **Browser:** [play the game](https://kylejameswalker.github.io/last-harvest/play/) or [open the card editor](https://kylejameswalker.github.io/last-harvest/editor/) to make cards, props and maps.
- **Learn:** the [game manual](docs/guides/GAME_MANUAL.md) covers controls, the night, powerups, photo mode and the card editor.

## Develop

The game is Rust, built on the [twinbit-ds](https://github.com/KyleJamesWalker/twinbit-ds) template. Every build needs the licensed tileset. See [development](docs/development.md) to set it up.

| Page | Covers |
|---|---|
| [Development](docs/development.md) | Setup, build, tests, CI checks, layout and environment variables |
| [Device](docs/device.md) | Deploy, debug mode, capture and hardware notes for the RG DS |
| [Platforms](docs/platforms.md) | The web, Android, muOS and desktop builds |
| [Modding](docs/modding.md) | Card, prop, map, mod pack, sound and data file formats |
| [Sprite editor](docs/sprite-editor.md) | The developer pixel editor, on a Mac or an iPad |
| [Releasing](docs/releasing.md) | Releases, GitHub Pages, itch.io and the launcher |

## License

The code is under the [MIT License](LICENSE).

The art is the [Zombie Apocalypse Tileset](https://ittaimanero.itch.io/zombie-apocalypse-tileset) by Ittai Manero, under its own license. That license allows use and edits in games but forbids redistribution. The tileset and the sprites edited from it live in the private `assets/private` submodule. Published binaries and web pages never build the tileset in, and read the art from `game_data.pak` at run time. Without access to the submodule, buy the tileset and run `scripts/assets.sh`.

The font is Atkinson Hyperlegible, under the SIL Open Font License in `assets/fonts/OFL.txt`.
