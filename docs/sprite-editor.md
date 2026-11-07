# Sprite editor

The sprite editor is the pixel editor for the game's own sprites, cards, props, maps and data files. It runs on macOS, or in a browser on an iPad served from the Mac. It builds the licensed tileset in, so it is never published. The public card editor is the same program without the tileset. The [game manual](guides/GAME_MANUAL.md#the-card-editor) covers the buttons the two editors share.

```sh
cargo run -p sprite-editor
SPRITE_EDITOR_OPEN=CAR_H cargo run -p sprite-editor   # open a sprite at launch
```

The left pane lists every sprite in the catalog. The right panes show the open sprite as the game draws it: in the 2.5D view, and on the map. The 2.5D view counts the quads the sprite draws with, to compare stands and drawings by cost.

## Saving

Save writes each frame to `assets/private/sprites/NAME/0.png`, `1.png` and so on. `LAST_HARVEST_SPRITES` sets another folder.

- A changed stand goes to the sprite's line in `crates/game/art.txt`, such as `@prop 12`, `@fold 20`, `@slab 10`, `@figure` or `@ground`. A new sprite keeps its stand in `stand.txt` in its folder.
- A carved sprite keeps `south.png`, `east.png`, and the optional `footprint.png`, `roof.png`, `north.png` and `west.png`, with `carve` in `stand.txt`. A build without them, such as a fresh clone, stands it as a slab.
- Coats go in `paints.txt`.
- A see-through sprite keeps its [alpha](modding.md#alpha) in `alpha.txt`.
- The next build packs a saved sprite in place of its cut from the tileset. A folder whose name is not in `art.txt` becomes a new sprite, `art::NAME`, for the code to use.
- Delete a sprite's folder to go back to the tileset's version.

Edits of the tileset's sprites fall under its license. Commit them in the `assets/private` submodule and push them there.

## Keys

| Action | Input |
|---|---|
| Draw, erase, fill | B, E, G, or the tool buttons. B again swaps the pencil and the eraser. |
| Swap back to the previous color | X |
| Draw a line, box or oval | The Line, Box and Oval buttons, then drag. Hold Shift as the drag starts to fill a shape or keep a line straight. |
| Draw a straight line | Shift-click with the pencil or eraser, from the last pixel drawn |
| Pick a color | Hold I or Option and click. Right-click always picks. |
| Search the sprite list | `/`, then type. Enter opens the first match, and Esc clears the filter. |
| Mirror strokes left to right | M |
| Show the previous frame faintly | O, or Onion |
| Draw at finer pixels, same size in the game | Detail - and +. Going coarser keeps each block's most common color. |
| Draw a sprite or prop see-through | Alpha - and + step the frame on show by a tenth, from 0.1 to 1. Shift steps every frame. |
| Place a player's ready grenade | W, or Grenade, on a player's walking part, then click. The first click sets every frame, a later click sets the frame on show, and Shift-click sets every frame. |
| Compare a sprite with its family | C, or Compare |
| Draw flash, hurt or dead frames from the walk | Generate, on a `_FLASH`, `_HURT` or `_DEAD` sprite. A three-frame `<KIND>_DEAD` sprite gives that kind its own corpse. |
| Select, then move | S, then drag inside. The arrows nudge 1 pixel, or 8 with Shift. |
| Copy, cut, paste | Cmd+C, Cmd+X, Cmd+V. A paste floats until Enter, Esc or a click outside. |
| Delete the selection | Delete or Backspace |
| Undo, redo | Cmd+Z, Cmd+Shift+Z |
| Next and previous sprite | J and K |
| Previous and next frame | H and L, or Left and Right with nothing selected |
| Play the animation | Space |
| New sprite, or a copy | New, Copy |
| Resize every frame | W and H - and +, 8 at a time with Shift. The bottom center stays put. |
| Browse earlier versions | R, or History |
| Save | Cmd+S |
| Screenshot the window | F12, into `target/` |

In the previews, drag the 2.5D view to walk around and scroll to move closer. Scroll the map preview to zoom.

## Stands

A stand sets how a sprite stands in the 3D view. Pick one under "Stands in 3D as".

| Stand | Shape | Suits |
|---|---|---|
| Prop | Upright and fixed to the map, extruded to a set depth. A fold makes the rows above the fold line lie flat as a roof. | Walls, and buildings drawn with the roof seen from above |
| Slab | Flat at a height, its edges drawn down to the ground. Float lifts it off the ground as a plate. | Sprites drawn from above, such as cars |
| Figure | Turns to face the camera. A Top drawing can color each column's top. | Characters |
| Ground | Flat on the ground | Tiles and stains |
| Carve | A solid where three views agree. See [Carving](#carving). | Shapes with depth, such as a car with a cabin |
| Turned | Upright like a prop, but running north to south. Draw it seen from the east, south on the left. | Pieces drawn from above, such as a fence's sides |

### Carving

A carved sprite adds a south view (west on the left) and an east view (south on the left) beside its frames. Carve starts both as a plain block to erase into shape, and the height buttons set their height. The solid is every voxel all three views agree on. Each face takes its color from the view that looks at it.

Optional drawings refine it. Add each with its + tile, and remove it with Delete.

- **Footprint:** a copy of the sprite to erase into its true outline from above, when the sprite's outline differs. Only its clear pixels count.
- **Top:** colors the top in place of the sprite.
- **North** and **West:** color those sides in place of the south and east views.

Top, North and West change colors only, never the shape. A clear pixel keeps the color the side would have without the drawing.

### No 3D

**No 3D** leaves the current color out of the 3D view, while the map keeps it. Use it for a backdrop color that should not extrude.

### Coats

Coats repaint a sprite, so one drawing comes in several colors.

1. Pick a shade of the part that changes, such as a car's body, and press **Paint**. Repeat for each shade.
2. Press **Coats**, then **Add coat**. The coat takes the current color, and its darker shades follow the drawing's. Click a shade to give it the current color.
3. Click a coat's picture to preview it, or As drawn for the drawing's own colors.

The build packs every frame once in each coat, the first coat first, and repaints the views, footprint and top to match. A map's `frame N` and the random farm pick among them.

### Palette

The palette is `crates/game/palette.txt`: the tileset's 22 colors plus a few the game adds. A color outside it shows its code in yellow. A test fails on any color in the game's drawings or coats that the file does not list. Add a new color there first, with what it is for.

**Lospec** loads a palette from [Lospec](https://lospec.com/palette-list) by name, such as `endesga-32`. A blank name goes back to the game's palette. The sprite editor fetches it through the native app or the sprite server, because Lospec blocks fetches from a web page. **Palette file** loads a downloaded `.hex` file instead. A mod's art can use any palette.

## History

R, or History, lists the commits that touched the open sprite in the assets repository, newest first. For a card's part or a prop, it lists the commits that touched its PNG. The editor only reads from git.

- J and K step through versions, read-only on the canvas and in the previews.
- Enter or Restore brings one back as unsaved changes.
- Esc or Back returns to what was open.

## Cards, props and maps

The editor lists the cards, props and maps in `assets/private/`.

- **Enemy cards** list as six sprites: `CARD_<ID>_FRONT`, `_SIDE` and `_BACK` walking, then `_FLASH`, `_HURT` and `_DEAD`. **Player cards** list as `PLAYER_CARD_<ID>_FRONT` and so on.
    - Each part saves into its row of the card's PNG. Resizing one part resizes the whole card.
    - A walking part takes 1 to 8 frames, and the other walking parts follow its count. Player card parts keep three.
    - A new card leaves the flash, hurt and dead parts clear for the game to draw. Generate draws them in, and Clear empties them again.
    - New with a name starting `CARD_` starts a card. Copy on a part copies the whole card under a new id.
    - The Stats page (T) works as in the card editor.
- **Props** list as `PROP_<ID>`. The stand, fps, Cover, Broken frame and HP buttons set the prop's text. New with a name starting `PROP_` starts one.
- **Maps** list as `MAP_<ID>`. New with a name starting `MAP_` starts a dirt map in `assets/private/maps/`. The map tools match the card editor's.

The game's own sprites follow the card rules where the game needs them. A zombie's walk takes 1 to 8 frames, its flash, hurt and dead parts take 3, and the player's sprites take 3.

## Data pages

The Data pages work as in the card editor. Save writes the file into `crates/game/` only when the game would read it. Try it starts the `last-harvest` binary built beside the editor.

## On an iPad

The same editor runs in a browser on an iPad on the Mac's network. Saves land in the Mac's `assets/private/sprites`, and History reads the same git log.

```sh
cargo install wasm-bindgen-cli --version <the version in Cargo.lock>
scripts/web-editor.sh                # build, then serve on port 8080
scripts/web-editor.sh --build-only   # build only
```

- The server prints the address to open, such as `http://192.168.1.20:8080/`. Add `?open=CAR_H` to start on a sprite.
- `SPRITE_SERVER_PORT` sets the port.
- The server also serves `target/web-game` under `/play/` for Try it. Build it with `scripts/web-game.sh`.
- Safari's Add to Home Screen gives the editor the whole screen. A hardware keyboard keeps the macOS shortcuts.

The build embeds the tileset. The server answers only loopback, private-LAN and link-local addresses, so it refuses Tailscale clients (100.64.0.0/10). Never publish `target/web-editor` or expose the server to the internet. Anyone on the local network can open the editor and save sprites while it runs.

| Action | Touch |
|---|---|
| Draw | Apple Pencil, or a finger on the canvas |
| Zoom and pan the canvas | Pinch with two fingers. Fit resets it. |
| Turn the 2.5D view, move closer | Drag it, pinch it |
| Zoom the map | Pinch it |
| Scroll the sprite list | Drag it. Tap a name to open it. |
| Keyboard shortcuts | The toolbar under the panes. Its Shift stays on until tapped again. |
| Shift for the next pencil press | Tap a Shift pad in a bottom corner. It lights until the pencil uses it. A second tap turns it off. |
| Name a sprite or search | Type in the field that appears |
