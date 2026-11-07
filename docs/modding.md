# Modding reference

This page specifies every file a player or modder can add: cards, props, maps, mod packs, sounds and the data files. The [game manual](guides/GAME_MANUAL.md#make-your-own) shows where the files go and how to make them in the card editor. Distances are in pixels, a tile is 16 pixels, and times are in seconds.

The game reads these files at start and skips any file that cannot load. The title screen shows the first reason, and the log lists them all.

## The packer

`last-harvest-pack` makes and checks cards, props and mod packs from the command line.

```sh
cargo run -p last-harvest-pack -- card sheet.png ghoul.txt ghoul.enemy.png     # an enemy card
cargo run -p last-harvest-pack -- player sheet.png jo.txt jo.player.png        # a player card
cargo run -p last-harvest-pack -- prop sheet.png tank.txt rusty_tank.prop.png  # a prop
cargo run -p last-harvest-pack -- mod <folder> ghoul.pak                       # a mod pack from a folder
cargo run -p last-harvest-pack -- show ghoul.enemy.png                         # a card's frames, text and spawn mix
cargo run -p last-harvest-pack -- show ghoul.pak                               # a pack's files, and whether the game takes it
cargo run -p last-harvest-pack -- show old_farm.map.txt                        # whether a map reads, or its first mistake
```

`card`, `player` and `prop` take an optional last argument, `2` or `4`, for the [detail](#detail). The packer refuses a mod pack whose tables the game would refuse.

## Enemy cards

An enemy card, `<id>.enemy.png`, adds a kind of zombie or replaces the kind with the same id. A new kind goes just before the last kind in `enemies.txt`.

The image holds the frames in rows. Every frame is the same size, up to 64 pixels, and the sheet is as wide as its longest row.

| Row | Frames |
|---|---|
| 1 to 3 | Walking front, side and back. As many frames as the `walk` line says, 1 to 8, or 3 without one. |
| 4 | A flash frame for each facing |
| 5 | A hurt frame for each facing |
| 6, optional | A dead frame for each facing. An uncompressed `last-harvest-rows` text chunk of `6` marks it. |

The game draws a fully clear flash or hurt frame from that facing's first walking frame. The flash is the figure's shape in one pale color. The hurt frame is the figure in reds, with its outline kept. A corpse tips its dead frame over, or the front hurt frame without one.

PNG chunks hold the rest:

- `last-harvest-enemy`, a compressed `iTXt` chunk, holds the card's [`enemies.txt`](#enemiestxt) section, without `look`. Its optional `about` line holds one sentence, up to 80 printable ASCII characters.
- `lhSd`, an optional private chunk, holds the card's own death sound: a whole Ogg or WAV file of up to 512 KB. It plays as `kill-<id>` when the `sound` line names that cue. A sound file for the same cue in a pack or the sounds folder wins over it. A card whose sound is too big or damaged loads without it.
- `last-harvest-detail`, optional and uncompressed. See [Detail](#detail).

Some apps strip a PNG's text chunks when they save it again. Share the file itself.

## Player cards

A player card, `<id>.player.png`, changes the player's look and can change the starting kit. It uses the same five-row sheet, with three walking frames a facing. Its `last-harvest-player` text chunk holds:

- `[id]` and a `name` line.
- An optional `about` line, up to 80 printable ASCII characters.
- A `start DIFFICULTY WEAPON...` line for each difficulty whose starting weapons it sets, such as `start normal crows knife`. A difficulty without a line keeps the game's own.
- Optional `hp`, `speed` and `magnet` lines for the start of a run. The rest come from `tuning.txt`.

An optional uncompressed `last-harvest-worn` chunk says where the walking frames wear a ready grenade. Each `front`, `side` or `back` line gives `X Y` per frame, split by `;`, and the last pair carries on. `hide` leaves the grenade off. A facing without a line wears it on the right hip. The game's own player keeps the same in `grenade.txt` beside its frames.

## Detail

An uncompressed `last-harvest-detail` chunk of `2` or `4` draws a card or prop at finer pixels. That many sheet pixels make one game pixel across and down, so the figure keeps its size in the game.

- A card's frames can then reach 128 sheet pixels: 64 game pixels at detail 2, or 32 at detail 4.
- A prop at detail 2 can reach 128 game pixels a side.
- 3D sides, carving and blocked tiles follow the outline at game-pixel size.
- A prop's `stand` numbers stay in game pixels, and `views` counts sheet pixels.
- The game's own sprites take the same from a `detail.txt` beside their frames.
- A game from before the chunk draws the card that many times bigger.

## Alpha

A sprite can draw see-through. Its alpha is one number from 0.1 to 1, or one number a frame, such as `1 0.8 0.5`. The last number carries on to later frames. The 3D view and the map both draw by it.

- A prop sets it with an `alpha` line. See [Props](#props).
- The game's own sprites take it from an `alpha.txt` beside their frames.
- A sprite without one draws opaque, as does a prop from before the line.

## Props

A prop, `<id>.prop.png`, is a sprite for maps. Its frames sit side by side in one row, each up to 256 pixels a side. A map places it as `PROP_<ID>`, in upper case. A weapon's `icon`, an effect's `art`, a shot style's sprites and the random farm's scatter can also name it. The game draws props into the sprite texture after the cards.

Its compressed `last-harvest-prop` text chunk holds `[id]` and any of these lines that differ from the default:

| Line | Sets | Default |
|---|---|---|
| `frames N` | Frames across the sheet, 1 to 16 | 1 |
| `fps N` | Animation speed | Still |
| `open N` | Top rows a figure walks through | 0 |
| `stand prop N`, `fold N`, `slab N`, `slab N float N`, `figure`, `ground`, `carve` or `turned [N]` | How it stands in 3D. See [stands](sprite-editor.md#stands). | Prop |
| `views N` | Height of the south and east views, for a prop that carves or turns | |
| `footprint yes` | A carving's outline from above comes from a footprint drawing | |
| `roof yes`, `north yes`, `west yes` | A carving's top, north or west side takes its colors from its own drawing | |
| `clear 5A6E3C ...` | Colors the 3D view leaves out. The 2D view keeps them. | |
| `paint ...` and `coat ...` | Its [coats](sprite-editor.md#coats) | |
| `cover no` | It lets shots through | It stops shots |
| `hp N` | Hit points before it breaks, 0 to 1000 | It never breaks |
| `broken yes` | Its last frame shows once it breaks, and the rest animate | It vanishes |
| `alpha N ...` | How opaque each frame draws. See [Alpha](#alpha). | Opaque |

A prop that carves or turns keeps its south and east views under its frames, `views` pixels tall, then any north and west views. A carving's footprint and then its top follow the last frame.

## Maps

A map, `<id>.map.txt`, is a hand-made farm. The id is 1 to 24 lowercase letters, digits and `_`. It reads as a header, its ground, then its props:

```text
name Old Farm
size 40 30          # 16x12 to 160x120 tiles
spawn 12 17         # the player's tile
home 9 16           # the tile the player runs back to at dawn
scatter             # optional: trees, bushes and wrecks over the open ground, new each run
ground              # `size` rows of one code per tile
..........vvv...........,,,......ccccc..
props
HOUSE 6 9 open 1    # a sprite and its top-left tile, then optional `open`, `frame` and `fps`
SIGN 14 3 hp 60     # optional `hp` (0 to 1000, 0 for never breaks) and `cover yes` or `no`
fence rail 25 4 10 8 gate 4 cover yes
```

- **Ground:** one [`ground.txt`](#groundtxt) code per tile. The game's own codes are `.` dirt, `,` dirt with tufts, `g` grass, `s` soil, `p` sprouts and `c` corn. Roads are `v` (north-south) and `h` (east-west), and `=` is a path. Roads and paths join their neighbors, and corn grows a top and bottom at a field's edges.
- **Props:** a sprite name from [`props.txt`](#propstxt), or `PROP_<ID>`, and its top-left tile.
- **Fences:** `rail` or `picket` around a rectangle, with an optional two-tile gate that many tiles from its left.
- **Hit points and cover:** `hp` and `cover` on a line override the sprite's own. Zombies chew through what breaks, and shots and blasts break what is cover. A broken prop stops blocking.
- **Rules:** a ring of trees and bushes always closes the edge. The spawn and home must be open, off the edge, and joined by open ground without breaking anything.

## Mod packs

A mod pack is a `.pak` file: a ZIP in the same layout as `game_data.pak`. The game lays each pack over `game_data.pak` in file-name order. A pack's file replaces the base file at the same path:

- a sound in `sounds/`
- a card in `enemies/` or `players/`
- a prop in `props/` or a map in `maps/`
- a [data file](#data-files)

A card in `data/enemies/` or `data/players/` replaces a pack's card with the same file name. A pack with an atlas works only with the game version it came from. The game leaves out a pack that would break it, and Stats counts the cards and packs that loaded.

For a card with its own death sound as a separate file, put `enemies/ghoul.enemy.png` and `sounds/kill-ghoul.ogg` in a folder and run the packer's `mod` command.

## Sounds

The game synthesizes every sound at runtime. A recorded Ogg Vorbis or WAV file replaces a cue when a pack's `sounds/` folder holds a file named after it. The developer build reads them from `assets/private/sounds/`.

- Add `-2`, `-3` and so on for variations, such as `shot.wav` and `shot-2.wav`. Each play picks one at random.
- WAV files can be 8, 16, 24 or 32-bit PCM or 32-bit float. Either format can be mono or stereo, at any sample rate.
- Ogg Vorbis is about a tenth the size of WAV and decodes once at start.
- Every file plays a few percent sharp or flat at random. Explosions, rising, groans, crows and spit pan and fade with distance.
- `music.ogg` replaces the synth drone and swells with the danger. `ambience.ogg`, `ambience-2.ogg` and `ambience-3.ogg` play quietly underneath and fade out during a pause. These stream and loop.

| Cue | Plays when |
|---|---|
| `shot`, `blast`, `slash` | The pistol, shotgun or knife fires |
| `thud`, `crit` | A hit lands, or a critical hit |
| `kill-skinny`, `kill-kid`, `kill-big` | A zombie dies |
| `throw`, `boom` | A grenade leaves the player's hand, or anything explodes |
| `pickup` | The player collects money. Quick pickups play it higher, up a scale. |
| `heal`, `hurt`, `heartbeat` | The player picks up health, takes a hit, or runs low on health |
| `levelup`, `pick`, `pick-rare` | The bar fills, or an upgrade is chosen |
| `rise`, `groan`, `caw` | A zombie breaks out of the ground or groans, or a crow takes off |
| `spit`, `splat` | A ranged zombie spits, or its spit lands |
| `bell`, `horde`, `dawn`, `gameover` | Each minute, each horde, surviving, or dying |
| `start`, `miss` | A run starts, or a throw with no grenade ready |

The recorded sounds in the private submodule carry their own licenses. Check each before sharing it.

## Data files

The game reads its rules from seven text files in `crates/game/`. The packer puts them in `game_data.pak`, and a mod pack can replace any of them. Each file's header comments list every key. A pack's `tuning.txt`, and the `[colors]` section of its `effects.txt`, need only the lines they change. The other files replace the whole file.

| File | Sets |
|---|---|
| [`tuning.txt`](#tuningtxt) | Numbers no card sets |
| [`props.txt`](#propstxt) | The game's own props a map can place |
| [`ground.txt`](#groundtxt) | Each ground code |
| [`farm.txt`](#farmtxt) | How the random farm lays itself out |
| [`effects.txt`](#effectstxt) | Effects, bullets, shot styles and particle colors |
| [`powerups.txt`](#powerupstxt) | Every weapon and passive |
| [`enemies.txt`](#enemiestxt) | Every kind of zombie |

### tuning.txt

`key number` lines in these sections:

| Section | Sets |
|---|---|
| `[player]` | Health, speed, money magnet, critical-hit chance, pickup reach and footprint before upgrades, and turn speed |
| `[night]` | The night's length, where zombies rise at dusk and dawn, how near home counts, the rare-card chance and the level-up push |
| `[easy]`, `[normal]`, `[hard]` | Bite damage, how long a hit keeps the player safe, and how often kills drop health and how much |
| `[zombies]` | The most alive at once, rising time, stagger, growth each minute, a leader's health, dawn's fury, footprint, walk speed, bite reach, `charge_reach`, spit speed and rate, `burrow_reach`, and chewing speed |
| `[harvester]` | Dawn's boss: health, speed, bite and how far away it rises |
| `[grenade]` | Recharge, flight time, radius, damage, bomblets, one-screen aiming (`aim_`), and R1's quick throw (`quick_near`, `quick_far`, `quick_grow`) |
| `[barrel]` | Health, blast radius and damage |
| `[weapons]` | The view ahead and behind, the pistol's point-blank reach, crow and dog reach, and how long fire burns |
| `[blast]` | How much any blast hurts the player, and how near counts |
| `[map]` | The map's scale, how far the view sees zombies for the map, how long one out of sight takes to fade, and the shade outside the view |

### props.txt

Each `[NAME]` section is a sprite from the game's art, with the prop lines `open N`, `cover no` and `hp N`. `[fence]` sets these for every fence piece, and `[other]` for any other sprite a map places.

### ground.txt

Each `[c]` section is one single-character ground code. The editors' paint buttons follow the file's order, and the first code is a new map's ground.

| Line | Sets |
|---|---|
| `name Text` | The paint button's label |
| `art NAME` | The tile. A tile with several frames picks one by the cell's position. |
| `joins across`, `joins down` | A run of cells takes the first, middle and last frames |
| `cell open\|road\|path\|field` | Open ground, a road, a path that joins neighboring paths and roads, or a field the farm grows on |
| `crop TOP MIDDLE BOTTOM` | A crop on the cell, with TOP at a run's north end |

### farm.txt

`farm.txt` lays out the random farm around its road. A count `N` means one for each N cells of the map.

- `[farm]` sets `tufts`, `grass`, `corpses`, `fields` and `more_fields`. Its `ground`, `tufted` and `patch` lines pick the ground codes.
- `[scatter]` lists props strewn on free ground, as a sprite and a count. A sprite can be `PROP_<ID>`.
- Every other section is a building. The first building that stands is home. Each takes these lines:
    - `sprites NAME ...`: the sprites it picks from at random.
    - `within N`: the most tiles from the middle it may stand.
    - `margin N`: tiles kept clear around it.
    - `door N`: tiles in from its left to its door.
    - `near NAME N [door]`: a sprite within N tiles of the building's middle, or of its door step.
    - `near windmill` or `near graveyard`: the windmill with its fan, or tombstones in a picket fence.

### effects.txt

Each section names what an effect, swing, bullet, thrown item, pickup or animal plays. A section with a new name is a new effect a weapon can use.

| Line | Sets |
|---|---|
| `art NAME` | The sprite, which can be `PROP_<ID>`. Its frames play in order. |
| `fps N` | Frames a second |
| `life N` | A swing's seconds |
| `reach N` | How far out a swing sweeps, as a fraction of the weapon's range or area |
| `alpha N` | How opaque it draws, 0.1 to 1. Default: 1. |
| `fade_in N` | The fraction of its play, 0 to 1, it takes to come in from clear |
| `fade_out N` | The fraction of its play, 0 to 1, it takes to fade to clear at the end |

Alpha and fades apply to an effect that plays once where it happens, such as a blast or a puff, and to a swing. The game's blasts, smoke and the eruption around a rising zombie fade out. The zombie stays solid.

A slab sprite lies flat and turns with the swing. Any other sprite faces the camera. A mod's file may leave out `[spit]`, `[mound]` and `[mound_breaks]` and keep the game's.

A section can instead set a shot style, which a weapon fires with `bullet NAME`. A shot style draws a tracer, a muzzle flare, sparks and smoke, and sparks where the bullet hits.

| Line | Sets | Limit |
|---|---|---|
| `color RRGGBB` | The tracer and sparks, as `palette.txt` writes a color | |
| `length N` | The tracer's length | 64 |
| `size N` | The tracer's width at its head, and the sparks' size | 8 |
| `sparks N` | Sparks from the muzzle per shot, 0 for none | 32 |
| `impact N` | Sparks where each bullet hits, 0 for none | 32 |
| `core N` | The muzzle flare's reach | 16 |
| `tracer NAME` | The sprite stretched along the path, head to the right | |
| `spark NAME`, `smoke NAME` | Each spark's and smoke puff's sprite, its frames played over its life | |
| `flare NAME` | The muzzle flare's sprite, pointing right | |

A shot-style section needs no `art`. It takes what it leaves out from `[bullet]`, the style of a gun without a `bullet` line. The pistol fires `[pistol_shot]`, the shotgun and back holster `[shotgun_pellet]`, and the rifle `[rifle_round]`. A missing or unknown sprite draws as a plain colored shape. The game's own shot sprites are the `SHOT_` placeholders.

`[colors]` sets the particle colors `blood`, `dark_blood`, `dirt`, `spark`, `ember`, `gold` and `firefly`, each as red, green and blue from 0 to 1.

### powerups.txt

The file opens with lines for the whole run:

| Line | Sets |
|---|---|
| `slots weapons N`, `slots passives N` | How many of each a run carries |
| `start DIFFICULTY ID ...` | The starting weapons at each difficulty |
| `grenades N` | Grenades every run starts with |
| `finds N` | The most powerups one run can find |
| `first_level N` | Money for the first level-up |

Each `[id]` section is one powerup:

| Line | Sets |
|---|---|
| `name`, `blurb`, `icon` | Its card. `icon` can be `PROP_<ID>`. |
| `kind` | `weapon`, `passive` or `filler`. Fillers appear only when nothing else remains. |
| `zone` | `ahead`, `around`, `behind` or `map` |
| `unlock` | `start`, `kills N`, `night SECONDS`, `level N`, `nights N` or `dawns N` |
| `find`, `after`, `chest` | How often it shows as a find, the earliest minute, and chest-only |
| `max` | Its highest level |

Each stat line lists its value at each level, and a short list repeats its last value. Weapon stats are `cooldown`, `damage`, `count`, `range`, `area`, `pierce`, `speed`, `duration`, `spread`, `knockback`, `slow` and `gap`. Passive stats add together. The header lists every passive stat and which ones take a percentage.

#### Weapons from data

A new weapon needs no code. Copy a weapon's section under a new id and change its lines. `does` picks the behavior and defaults to the id, so `does scythe` under a new id is a second scythe. Three behaviors take their shape from data:

- `does gun` fires `count` shots at what its `aim` finds: `nearest` (the default), `ahead`, `behind` or `around`. `pattern` spreads them:
    - `fan`: evenly across `spread` degrees, the default.
    - `ring`: evenly all the way round.
    - `spray`: at random across `spread`.
    - `burst`: one after another, `gap` seconds apart, each aimed as it fires. `does pistol` also fires bursts.
- With `targets each`, each later shot of a burst goes to the nearest unshot zombie in range and in clear line of fire. `targets same`, the default, keeps every shot on one target.
- `does blade` cuts everything within `area` across `spread` degrees, toward what its `aim` finds. A `spread` of 360 cuts all round.
- `does lob` throws `count` at a crowd within `range`. `lands` sets what happens there, with its `damage`, `area`, `duration` and `slow`: `blast` (the default), `fire`, `flare` or `tacks`.

Any weapon can also take:

| Line | Sets |
|---|---|
| `effect NAME` | An `effects.txt` effect in place of its behavior's own. A lob plays it where it lands. |
| `bullet NAME` | An `effects.txt` section its bullets fly as. A lob without one flies as the grenade. |
| `sound NAME` | `shot`, `blast`, `slash`, `zap`, `bark` or `throw` |
| `kick N`, `punch N`, `shake N` | The camera's kick back, punch in and shake on each attack |
| `flash R G B REACH SECONDS` | A flash of light on each attack. A color above 1 is brighter than white. REACH is in tiles. |

### enemies.txt

Each `[id]` section is one kind of zombie:

| Key | Sets |
|---|---|
| `name` | Its name on the Enemies page |
| `about` | One sentence on what to expect, up to 80 printable ASCII characters |
| `look` | Its sprites from the game's art. A card leaves this out. |
| `walk` | On a card, walking frames per facing, 1 to 8. Default 3. |
| `hp` | Health at dusk. It grows through the night. |
| `speed` | Pixels a second |
| `damage` | A bite |
| `xp` | The money it drops |
| `mass` | Divides knockback |
| `gibs` | Blood specks when it dies |
| `sound` | Its death sound. Default `kill-<id>`. |
| `after` | The earliest minute it arrives |
| `chance` | Its share of arrivals. Default 1. |
| `per_minute` | Added to `chance` each minute |
| `charge` | Its speed multiple within `tuning.txt`'s `charge_reach`. Default 1. |
| `ranged` | How near it stops to spit, up to 160. Default 0, a biter. |
| `burrow` | Seconds it stays up before it tunnels again, up to 60. Default 0, never. |
| `blood` | Its blood color as `RRGGBB`. Default: `effects.txt`'s `blood`. |
| `heavy` | A crit or hard hit roots it, and its death shakes the screen |
| `leader` | A horde's leader, which carries the chest, is one of these |
| `harvester` | Dawn's Harvester wears its sprites |

Each arriving zombie rolls a number from 0 to 1 and becomes the first kind, top to bottom, whose chance is above the roll. Put the rarest kinds first, and end on one with no chance. A fallen player rises as the last kind.

- A `ranged` zombie walks in until it is that near with a clear shot, then stands and spits. Each spit does its `damage`, flies slower than the player walks, and splashes on cover. `effects.txt`'s `[spit]` sets its look.
- A `burrow` zombie tunnels as a moving dirt mound that nothing can hit. Within `burrow_reach` of the player, it breaks out and is safe until it has climbed out, which takes `rise` seconds. `[mound]` and `[mound_breaks]` in `effects.txt` set its look.
