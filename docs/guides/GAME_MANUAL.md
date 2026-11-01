# Last Harvest manual

Last Harvest is a zombie survivors-like for the ANBERNIC RG DS Plus, and it also plays in a browser. You walk a farm through one night while your weapons fire on their own, then run home at dawn. This manual covers how to play first. It then shows how to make your own zombies, looks, props, maps and mod packs with the two editors. Build setup, device notes and developer tools live in the [README](../../README.md).

## Playing

### Starting a run

Choose Play on the title's bottom screen to start a run. Each run takes place on a newly generated farm, or on the map the title's Map row picks. A generated farm has a road with a gas station, and a farmhouse and barn joined to the road by dirt paths. Fenced crop and corn fields, a windmill and a graveyard lie around them. Trees, wrecks and barrels lie scattered across the farm.

### The two screens

The top screen is the main view, in 2.5D. Every sprite's silhouette stands extruded into depth, lit by your lantern and fading into fog. You see the farm from just behind yourself.

The bottom screen shows the same farm from above at twice the art's size. Dots mark what the main view faces. Tap it to throw a grenade.

In a browser, pick One screen or Two screens as the game starts. One screen, the default, fills the window: menus show both screens side by side, and a run plays on one screen with on-screen controls (see [On a phone](#on-a-phone)). Two screens share the window, the top screen above the bottom, as on the handheld.

### Controls

Your weapons fire on their own. You move, turn, throw grenades and pick upgrades.

| Action | Device | macOS and browser |
|---|---|---|
| Walk forward and back | Up and Down, or the stick | Up and Down arrows |
| Strafe | Left and Right, or the stick | Left and Right arrows |
| Turn | Y and A | A and X |
| Turn around | B | Z |
| Aim at barrels | Hold X | Hold S |
| Throw a grenade | Tap the map | Click the bottom screen |
| Pick an upgrade | Tap it, or Up, Down and X | Click it, or the arrows and S |
| Show the whole map | Hold L1 | Hold 1 |
| Pause | Start | Enter, or Esc |
| Choose a difficulty | Left and Right on Play | Left and Right arrows on Play |
| Screenshot | R1 | W |
| Volume | V+ and V- | `=` and `-` |
| Back to the menu | Back, or B after a run | Tab, or Z after a run |
| Back out of a menu | B | Z, or Esc |
| Quit | Hold Home for one second | Cmd+Q, or close a window |

#### One screen

Settings > Screens set to One plays a run on the top screen alone; the bottom screen goes dark. The map is still there, but looking at it means losing sight of the 3D view.

| Action | Device | macOS and browser |
|---|---|---|
| Show the map, live | Hold L1 | Hold 1 |
| Aim a grenade, in slowed time | Hold L2, then the d-pad or stick | Hold Q, then the arrows |
| Throw it | Let go of L2 | Let go of Q |
| Cancel the throw | B | Z |
| Show the whole map, paused | Hold R2 | Hold 2 |

Grenades show only on the map. With none ready, L2 shows the map at full speed and clicks. While you aim, you keep walking the way you were going, and after 3 seconds the grenade throws itself. The cursor starts where your last throw went, relative to your view, if that was under 5 seconds ago, else straight ahead.

#### On a phone

A run plays on one screen with controls beside it, or under it when the phone is upright.

| Action | Touch |
|---|---|
| Walk and strafe | Put a thumb down left of the picture and drag; the stick follows it |
| Turn | Slide a thumb sideways on the strip under Around; further turns faster |
| Turn around | Around |
| Show the map | Tap Grenade; tap it again to put the 3D view back. You keep moving |
| Throw a grenade | While the map is up, tap where it should land |
| Pick an upgrade, or a pause choice | Tap it |
| Pause | Pause |

Putting the map away never throws, and nothing slows down. That is the Touch style; Settings > Grenades set to Pad makes Grenade (and L2 on a gamepad) aim with a cursor in slowed time instead, held and let go, as on one screen, which suits a handheld with buttons. The controls hide when you use a gamepad or keyboard, and come back at your next touch.

In a browser, Esc and Home do nothing, and neither do screenshots, brightness or rumble. Sound starts with your first click or key. A gamepad in the browser's standard layout maps by its labels, as on an Xbox pad:

- A, B, X and Y are the same buttons.
- The bumpers and triggers are L1, R1, L2 and R2.
- View is Select, and Menu is Start.
- The d-pad and left stick move you.

The browser keeps your save in its local storage. It reads no cards, mod packs or maps. For the full key map, see [Controls](../../README.md#controls) in the README.

### A night

A night lasts ten minutes, from dusk to dawn. A bell rings each minute.

- **Zombies rise.** Zombies claw their way out of the ground. Early on they rise at the edge of the fog. By the end of the night they rise only a few tiles away. They find their way around buildings, or chew through a fence when that is quicker. They grow tougher each minute of the night.
- **Shots and cover.** Shots fly past bushes, fences and signs. Buildings, vehicles and trees stop them.
- **Money and level-ups.** Kills drop money, and money levels you up. Each level-up shows three cards on either screen, and you pick one. A level-up also pushes nearby zombies back. Some cards are rare and count twice. See [Powerups](#powerups).
- **Health.** Some kills drop health instead of money. How often depends on the difficulty.
- **Grenades.** Tap the map to lob a grenade there. You start with one, two on Easy, and each recharges in 12 seconds.
- **Barrels.** Shooting a red barrel sets it off. Hold X, and your guns go for the barrels in view. A blast close to you hurts you too.
- **Hordes.** Hordes arrive during the night, each led by a gold-lit leader. The leader drops a chest when it dies.
- **Dawn and the Harvester.** At dawn, the Harvester wakes behind you. From then on the dead grow 1% stronger every second.
- **Getting home.** Run back to the farmhouse's front step to survive the night.
- **Falling.** If you fall, you sink into the dirt and climb back out as one of the dead. You lead them off to the next farm.

Start pauses a run. The pause menu offers Resume, Settings, New farm and Quit to title.

### Difficulties

Pick a difficulty under Play with Left and Right, or tap its arrows. A new save starts on Easy, and the game remembers your choice.

| Difficulty | Bites | Health drops | You start with |
|---|---|---|---|
| Easy | Gentle | Often | A pistol, a knife and two grenades |
| Normal | They hurt | Scarce | A pistol, a knife and a grenade |
| Hard | Every bite counts | Rarer still | A pistol, a knife and a grenade |

A player card can change what each difficulty starts you with. See [Player cards](#player-cards).

### Powerups

Powerups are the weapons and passives that level-up cards offer. Each level-up offers three cards, drawn from 17 weapons and 17 passives. A run carries at most six weapons and six passives. Once both are full, the cards only offer levels for what you already carry. When nothing is left to level, the cards offer a snack, an ammo crate or pocket change.

The first level-up takes just 2 money. Each one after costs more.

#### Zones

Each weapon covers a zone, shown on its card:

- **Ahead** weapons fire into the view the top screen shows. The pistol also shoots anything within 70 pixels, whichever way you face.
- **Around** weapons hit a ring centered on you.
- **Behind** weapons fire or drop things behind you.
- **Map** weapons land on crowds anywhere on the farm.

#### Finds and unlocks

Eight powerups are open from the first night. Each of the rest has a condition: a number of kills, a long enough night, a level reached, nights played or dawns survived. Kills and nights add up over every run. Night length and level count your best single run.

Meeting a condition only makes the powerup findable. From then on, a level-up may offer it on a green FOUND card. Taking that card unlocks it for good.

- A run can find two powerups at most.
- A find appears only when you have a free slot of its kind. Keep a weapon slot open to find new weapons.
- Some finds appear only after a set minute of the night.

The title screen shows how many powerups are unlocked, how many wait to be found, and the condition you are closest to. The game-over screen names what the run found.

#### Chests

The biggest finds appear only in the chest a horde's leader drops. Picking up a chest gives you a free level-up. That level-up holds a find whenever one is waiting.

### The title menu

The title's bottom screen lists Play, Map, Settings, Stats and Credits. Up and Down choose a row, A or a tap opens it, and B goes back. The top screen shows the game's version in its top-left corner.

- **Map** picks a hand-made map, or a random farm, for every run after. The game saves the choice.
- **Settings** sets each screen's brightness, the volume, rumble, the frame-rate overlay, your look, which zombies come, and Reset progress.
    - **Player** picks your look from the player cards, showing each walking on the top screen.
    - **Enemies** shows each kind of zombie walking on the top screen with its numbers. A turns a kind on or off. Nights skip the kinds turned off, and at least one stays on.
    - **Reset progress** clears kills, records and found powerups after a second press. Settings stay.
    - From the pause menu, Settings changes everything but your look, the zombies and the reset.
- **Stats** lists the records the save keeps, and counts the cards and mod packs that loaded.
- **Credits** names who made the game, its art, its template and its font.

### Saving screenshots

R1 saves both screens, the top above the bottom, as `screenshots/shot-NNNN.png` beside the save.

## Weapons, passives and zombies

These tables list what the game ships with. A mod pack can change any of them, and a card can add zombies.

### Weapons

Weapons attack on their own, each in its zone. Each weapon levels up to its highest level, and every level changes its numbers.

<!-- items: weapons -->

| Weapon | Zone | Unlocks | Levels | On its card |
|---|---|---|---|---|
| Pistol | Ahead | From the start | 8 | Shoots whatever's in front of you, else the nearest |
| Shotgun | Ahead | From the start | 8 | A spread of pellets at close range |
| Hunting rifle | Ahead | 500 kills over every run | 8 | A slow, heavy round straight down your heading |
| Lantern beam | Ahead | Survive 2:00 of one night | 8 | Your light burns and slows what it falls on |
| Weed burner | Ahead | 1500 kills over every run | 8 | A short cone of flame that sets them burning |
| Pitchfork | Ahead | 3000 kills over every run | 8 | Skewers everything in a line in front of you |
| Knife | Around | From the start | 8 | Slashes everything close by |
| Scythe | Around | 100 kills over every run | 8 | A full sweep all the way around you |
| Crows | Around | Survive 4:00 of one night | 8 | Birds circle you and peck at what they touch |
| Bug zapper | Around | 2000 kills over every run, then found from minute 2 | 8 | Lightning that jumps from one zombie to the next |
| Farm dog | Around | 1 dawn survived | 8 | A good dog that runs down whatever comes close |
| Tacks | Behind | 250 kills over every run | 8 | Scattered behind you; they hurt and slow |
| Gas trail | Behind | Survive 7:00 of one night | 8 | Leaves a line of burning fuel where you walk |
| Back holster | Behind | Survive 5:00 of one night | 8 | A sawn-off that fires behind you |
| Scarecrow decoy | Behind | 4000 kills over every run, then found only in a leader's chest | 8 | Dropped behind you; draws them in, then bursts |
| Molotov | Map | 1000 kills over every run | 8 | Lobbed into a crowd, leaving a pool of fire |
| Flare gun | Map | Survive 9:00 of one night, then found from minute 4 | 8 | Lights up a far crowd, and burns it |

<!-- /items -->

### Passives

Passives change you or every weapon at once. Their effects add together, and each level raises the total. A passive with a percentage scales the matching stat on every weapon.

<!-- items: passives -->

| Passive | Unlocks | Levels | On its card |
|---|---|---|---|
| Hollow points | From the start | 5 | More damage for every weapon |
| Quick hands | From the start | 5 | Every weapon attacks faster |
| Running shoes | From the start | 5 | Move faster |
| Canned peaches | From the start | 5 | More max health, and a bite to eat now |
| Deep pockets | From the start | 5 | Pick up money from further away |
| Work jacket | Level 5 in one run | 5 | Takes the edge off every bite |
| Garden tea | 2 nights played | 5 | Slowly heals you |
| Sharp eye | Level 8 in one run | 5 | A better chance of a critical hit |
| Bigger blast | Level 12 in one run | 5 | Wider sweeps, pools and explosions |
| Long barrel | Level 10 in one run | 5 | Weapons reach further |
| Lucky penny | 5 nights played | 5 | Rare cards turn up more often |
| Pawn shop | Level 14 in one run | 5 | Money is worth more |
| Heavy rounds | 2 dawns survived, then found from minute 3 | 5 | Hits shove harder, and bullets go through |
| Brighter lantern | 5000 kills over every run | 5 | See wider and further ahead |
| Vaccine | 5 dawns survived, then found only in a leader's chest | 2 | When you start to turn, you come back as yourself |
| Grenade belt | 3 nights played | 5 | More grenades, sooner, and they scatter bomblets |
| Fuel drums | 3 dawns survived, then found only in a leader's chest | 5 | Blown barrels come back, and blow bigger |

<!-- /items -->

### Zombies

Zombies rise through the night in a mix that shifts minute by minute. Some kinds arrive only after a set minute. Big leads the hordes and dresses dawn's Harvester, and a hard hit roots it for a moment.

<!-- items: enemies -->

| Zombie | Health | Speed | Bite | Money | Comes from minute | What to expect |
|---|---|---|---|---|---|---|
| Big | 90 | 12 | 32 | 5 | 2 | Slow and huge; it bites hard, shrugs off knockback and leads the hordes. |
| Kid | 10 | 32 | 10 | 1 | 1 | Small and quick; it falls to a slap, but it reaches you first. |
| Skinny | 20 | 20 | 15 | 1 | 0 | The common dead: slow, frail, and never alone for long. |

<!-- /items -->

## Making things

You can add to the game without its source. Cards change zombies and your look. Props and maps add your own sprites and farms. Mod packs change anything in the game's data.

### Where files go

The game reads your files from its `data/` folder, beside `game_data.pak`, when it starts:

| Folder | Holds |
|---|---|
| `data/enemies/` | Enemy cards, `<id>.enemy.png` |
| `data/players/` | Player cards, `<id>.player.png` |
| `data/props/` | Props, `<id>.prop.png` |
| `data/maps/` | Maps, `<id>.map.txt` |
| `data/mods/` | Mod packs, `<name>.pak` |

A file that cannot load is left out. The title screen shows the first reason. The browser game reads none of these folders.

### Enemy cards

An enemy card adds a kind of zombie, or replaces one. It is a single PNG named `<id>.enemy.png`. It previews anywhere as an image, and the game and the editors know it as a card by its name.

The image holds the enemy's frames in five rows:

1. Walking front.
2. Walking side.
3. Walking back.
4. A flash frame for each facing.
5. A hurt frame for each facing.

Each walking row holds as many frames as the card's `walk` line says, 1 to 8. Without a `walk` line, it holds 3. Every frame is the same size, up to 64 pixels. The sheet is as wide as its longest row.

A flash or hurt frame left fully clear is drawn by the game from that facing's first walking frame. The flash becomes the figure's shape in one pale color. The hurt frame becomes the figure in reds with its outline kept.

An optional sixth row holds dead frames, one per facing. A corpse tips its dead frame over. A card without dead frames tips its front hurt frame instead.

The card's numbers live in the PNG as its `enemies.txt` section, without `look`. See [`enemies.txt`](#enemiestxt) for every key. The `about` line is one sentence on what to expect, up to 80 printable ASCII characters. Settings > Enemies shows it above the kind.

A card with the id of an existing kind replaces that kind. A new kind joins just before the last one. Its death sound is `kill-<id>` unless its `sound` line names another cue. A mod pack can carry that sound, as the card editor's Sound button does.

Some apps strip a PNG's text when they save it again, and the card's numbers go with it. Share the file itself.

### Player cards

A player card changes how you look, and can change what you start with. It is a PNG named `<id>.player.png`. It uses the same five-row sheet, with three walking frames a facing.

Its text holds these lines:

- `[id]` and a `name` line.
- An optional `about` line, one sentence of up to 80 printable ASCII characters. Settings > Player shows it under the name.
- A `start DIFFICULTY WEAPON...` line for each difficulty whose starting weapons it sets, such as `start normal crows knife`. Any weapon counts, up to the run's weapon slots. A difficulty without a line keeps the game's own.
- Optional `hp`, `speed` and `magnet` lines for what you start a run with, before upgrades. The rest keep `tuning.txt`'s values.

Settings > Player picks a card, and the save keeps the choice by id.

### Props

A prop is your own sprite for maps, named `<id>.prop.png`. Its frames sit side by side in one row, each up to 256 pixels a side. A map places it as `PROP_<ID>`, in upper case. A weapon's `icon`, an effect's `art` and the random farm's scatter can also name it.

The prop's text holds its `[id]` and any of these lines that differ from the defaults:

| Line | What it sets |
|---|---|
| `frames N` | Frames across the sheet, 1 to 16. One when left out. |
| `fps N` | How fast the frames animate. Still when left out. |
| `open N` | Top rows a figure walks through. |
| `stand prop N`, `fold N`, `slab N`, `slab N float N`, `figure`, `ground`, `carve` or `turned [N]` | How it stands in 3D. See [Stands](#stands). |
| `views N` | How tall its south and east views are, for a prop that carves or turns. |
| `footprint yes` | A carving's outline from above comes from a footprint drawing. |
| `roof yes` | A carving's top takes its colors from a top drawing. |
| `north yes`, `west yes` | A carving's north or west side takes its colors from a drawing of its own. |
| `clear 5A6E3C ...` | Colors the 3D view leaves out, which the 2D view keeps. |
| `paint 917662 726650 ...` | Its colors that are paint, light to dark. See [Coats](#coats). |
| `coat 4A6FA5` | One coat each frame comes in: one color, or a color for each paint color. |
| `cover no` | It lets shots through. It stops them when left out. |
| `hp N` | Hit points before zombies or shots break it, 0 to 1000. It never breaks when left out. |
| `broken yes` | Its last frame shows once it breaks, and the rest animate. It vanishes when left out. |

A prop that carves or turns keeps its south and east views under its frames, `views` pixels tall. Any north and west views follow them, in that order. A carving's footprint and then its top sit after its last frame.

### Maps

A map is a hand-made farm for a run, named `<id>.map.txt`. The id is 1 to 24 lowercase letters, digits and `_`. The title's Map row picks one.

A map reads as a header, then its ground, then its props:

```text
name Old Farm
size 40 30          # 16x12 to 160x120 tiles
spawn 12 17         # the player's tile
home 9 16           # the tile they run back to at dawn
scatter             # optional: trees, bushes and wrecks over the rest of the open ground, new each run
ground              # `size` rows of one code per tile
..........vvv...........,,,......ccccc..
props
HOUSE 6 9 open 1    # a sprite and its top-left tile, then optional `open`, `frame` and `fps`
SIGN 14 3 hp 60     # optional `hp` (0 to 1000, 0 for never breaks) and `cover yes` or `no`
fence rail 25 4 10 8 gate 4 cover yes
```

- **Ground.** Each tile is one code from `ground.txt`. The game's own are `.` dirt, `,` dirt with tufts, `g` grass, `s` soil, `p` sprouts and `c` corn. `v` and `h` are roads running north-south and east-west, and `=` is a path. Roads and paths join up with their neighbors. Corn grows a top and bottom at a field's edges.
- **Props.** A prop line names a sprite and its top-left tile. `props.txt` lists the game's own sprites, and your props place as `PROP_<ID>`.
- **Fences.** A fence is `rail` or `picket` around a rectangle. An optional two-tile gate sits that many tiles from its left.
- **Hit points and cover.** Each sprite has its own hit points and cover, which `hp` and `cover` on its line override. Zombies chew through what breaks. Shots and blasts break what is cover. A broken prop stops blocking.
- **Edges and paths.** A ring of trees and bushes always closes the map's edge. The spawn and home must be open, off the edge, and joined by open ground without breaking anything.

A map that does not read is left out, and the title screen shows its first mistake.

### Mod packs

A mod pack is a `.pak` file in `data/mods/`. It changes any file in the game's data: a sound in `sounds/`, a card in `enemies/`, or one of the [data files](#data-files).

The game lays each pack over `game_data.pak` in file-name order. A pack's file replaces the base file at the same path. A card in `data/enemies/` or `data/players/` replaces a pack's card of the same file name.

A pack lets a card bring its own death sound. Put `enemies/ghoul.enemy.png` and `sounds/kill-ghoul.ogg` in one pack. The card editor's Export all builds packs like this for you.

The game checks a pack's files as it reads them, and leaves out a pack that would break the game. A pack with an atlas must be built for this version of the game. For the command-line packer, see [Cards and mods](../../README.md#cards-and-mods) in the README.

### Data files

The game reads its rules from seven text files. A mod pack can replace any of them, and both editors edit them. Each file's header comments list every key.

| File | What it sets |
|---|---|
| `tuning.txt` | Numbers no card sets: your base health and speed, each difficulty, zombies, the Harvester, the night, grenades, barrels, blasts and weapon reach |
| `props.txt` | The game's own props a map can place, and the defaults for fences and other sprites |
| `ground.txt` | Each ground code a map paints |
| `farm.txt` | How the random farm lays itself out |
| `effects.txt` | The sprite and speed of each effect, swing, bullet, thrown item, pickup and animal, and the particle colors |
| `powerups.txt` | Every weapon and passive |
| `enemies.txt` | Every kind of zombie |

A pack's `tuning.txt`, and the `[colors]` of its `effects.txt`, need only the lines they change. The other files replace the game's whole file. Distances are in pixels, and a tile is 16 pixels. Times are in seconds.

#### tuning.txt

`tuning.txt` holds `key number` lines in these sections:

- `[player]`: your health, speed, money magnet, critical-hit chance, pickup reach and footprint, before upgrades, and how fast the view turns.
- `[night]`: the night's length, where zombies rise at dusk and dawn, and how near home counts. It also sets the rare-card chance and the level-up push.
- `[easy]`, `[normal]` and `[hard]`: how hard bites hit and how long a hit keeps you safe. They also set how often kills drop health, and how much.
- `[zombies]`: the most alive at once, rising time, stagger, growth each minute, a leader's health, dawn's fury, footprint, walk speed, bite reach, `charge_reach` and how fast zombies chew through a prop.
- `[harvester]`: dawn's boss, with its health, speed, bite and how far away it rises.
- `[grenade]`: recharge time, flight time, radius, damage and bomblets.
- `[barrel]`: health, blast radius and damage.
- `[weapons]`: the view ahead and behind, the pistol's point-blank reach, crow and dog reach, and how long fire keeps burning.
- `[blast]`: how much any blast hurts you, and how near counts.

#### props.txt

Each `[NAME]` section is a sprite from the game's art. It takes the same lines as one of your props:

- `open N`: top rows a figure walks through, 0 when left out.
- `cover no`: it lets shots through, cover when left out.
- `hp N`: hit points before it breaks, up to 1000. It does not break when left out.

`[fence]` sets these for every fence piece, and `[other]` for any other sprite a map places.

#### ground.txt

Each `[c]` section is one ground code, a single character. The editors' paint buttons follow the file's order, and the first code is a new map's ground.

- `name Text`: the paint button's label.
- `art NAME`: the tile. A tile with several frames picks between them by where the cell is.
- `joins across` or `joins down`: a run of cells takes the first, middle and last frames, left to right or top to bottom.
- `cell open|road|path|field`: open ground, a road, a path that joins the paths and roads beside it, or a field the farm grows on.
- `crop TOP MIDDLE BOTTOM`: a crop standing on the cell, with TOP at a run's north end and BOTTOM at its south end.

#### farm.txt

`farm.txt` lays out the random farm around its road, gas station, house, barn, windmill and graveyard. A count `N` means one for each N cells of the map.

- `[farm]` sets `tufts`, `grass` patches, `corpses`, `fields` and `more_fields`. Its `ground`, `tufted` and `patch` lines pick the ground codes the farm uses.
- `[scatter]` lists props strewn anywhere free, in order, as a sprite and a count. A sprite can be one of your props, `PROP_<ID>`.
- Every other section is a building, by a name of your own. The first building that stands is home. Each takes these lines:
    - `sprites NAME ...`: the sprites it picks one of at random.
    - `within N`: how far from the middle it may stand, in tiles.
    - `margin N`: tiles kept clear around it.
    - `door N`: tiles in from its left to its door.
    - `near NAME N [door]`: a sprite placed within N tiles of the building's middle, or of its door step.
    - `near windmill` or `near graveyard`: the windmill with its fan, or tombstones in a picket fence.

#### effects.txt

Each section names what an effect, swing or animal plays:

- `art NAME`: the sprite, which can be one of your props. Its frames play in order.
- `fps N`: frames a second.
- `life N`: a swing's seconds.
- `reach N`: how far out a swing sweeps, as a fraction of the weapon's range or area.

A sprite that stands as a slab lies flat and turns with the swing. Anything else faces the camera. A section with a name of your own is a new effect a weapon can use.

The `[colors]` section sets the particle colors: `blood`, `dark_blood`, `dirt`, `spark`, `ember`, `gold` and `firefly`. Each is red, green and blue from 0 to 1.

#### powerups.txt

The file opens with lines for the whole run:

- `slots weapons N` and `slots passives N`: how many of each a run carries.
- `start DIFFICULTY ID ...`: the weapons a run starts with at each difficulty.
- `grenades N`: grenades every run starts with.
- `finds N`: the most powerups one run can find.
- `first_level N`: money for the first level-up.

Each `[id]` section is one powerup:

- `name`, `blurb` and `icon` set its card. An `icon` can be one of your props, `PROP_<ID>`.
- `kind` is `weapon`, `passive` or `filler`. Fillers appear only when nothing else is left.
- `zone` is `ahead`, `around`, `behind` or `map`.
- `unlock` is `start`, `kills N`, `night SECONDS`, `level N`, `nights N` or `dawns N`.
- `find` sets how often it shows as a find, `after` the earliest minute, and `chest` makes it a chest-only find.
- `max` is its highest level. Each stat line lists its value at each level, and a short list repeats its last value.

Weapon stats are `cooldown`, `damage`, `count`, `range`, `area`, `pierce`, `speed`, `duration`, `spread`, `knockback` and `slow`. Passive stats are totals that add together. The header lists every passive stat and which ones take a percentage.

##### Making a weapon from data

A new weapon needs no code. Copy a weapon's section under a new id and change its lines. `does` picks the behavior it fires with. It defaults to the id, so `does scythe` under a new id is a second scythe with its own numbers.

Three behaviors take their shape from data:

- **`does gun`** fires `count` shots at what its `aim` finds. `aim` is `nearest`, `ahead`, `behind` or `around`, and `nearest` when left out. `pattern` spreads the shots:
    - `fan`: evenly across `spread` degrees, the default.
    - `ring`: evenly all the way round.
    - `spray`: at random across `spread`.
- **`does blade`** cuts everything within `area` across `spread` degrees, toward what its `aim` finds. A `spread` of 360 cuts all round.
- **`does lob`** throws `count` at a crowd within `range`. `lands` says what happens where it lands, using its `damage`, `area`, `duration` and `slow`:
    - `blast`: an explosion, the default.
    - `fire`: a burning pool.
    - `flare`: a flare.
    - `tacks`: a patch of tacks.

Any weapon can also take these lines:

- `effect NAME`: an `effects.txt` effect it plays in place of its behavior's own. A lob plays it where it lands.
- `bullet NAME`: an `effects.txt` effect its bullets fly as. A lob flies as it, or as the grenade when left out.
- `sound NAME`: `shot`, `blast`, `slash`, `zap`, `bark` or `throw`, in place of its behavior's own.
- `kick N`, `punch N` and `shake N`: the camera's kick back, punch in and screen shake on each attack.
- `flash R G B REACH SECONDS`: a flash of light on each attack. The color can go above 1 for brighter than white. REACH is in tiles.

#### enemies.txt

Each `[id]` section is one kind of zombie:

| Key | What it sets |
|---|---|
| `name` | What the Enemies page calls it |
| `about` | One sentence on what to expect, up to 80 printable ASCII characters |
| `look` | Its sprites from the game's art. A card leaves this out. |
| `walk` | On a card, its walking frames a facing, 1 to 8. Three when left out. |
| `hp` | Health at dusk. It grows through the night. |
| `speed` | Pixels a second |
| `damage` | A bite |
| `xp` | The money it drops |
| `mass` | Divides knockback |
| `gibs` | Blood specks when it dies |
| `sound` | Its death sound, `kill-<id>` when left out |
| `after` | The earliest minute it arrives in a night |
| `chance` | The share of arrivals it takes, 1 when left out |
| `per_minute` | Added to `chance` for each minute of the night |
| `charge` | Times its speed once it gets near you. 1 when left out. |
| `heavy` | A crit or hard hit roots it for a moment, and its death shakes the screen |
| `leader` | A horde's leader, carrying the chest, is one of these |
| `harvester` | Dawn's Harvester wears its sprites |

Each arriving zombie rolls a number from 0 to 1. It becomes the first kind, top to bottom, whose chance is above the roll. Put the rarest kinds first, and end on one with no chance. A lost night's player rises as the last kind.

A `charge` speeds a zombie up once it comes within `tuning.txt`'s `charge_reach` of you. `charge 2` doubles its speed there.

## The card editor

The card editor makes enemy cards, player cards, props and maps in a browser, and edits the data files. It is public, and it runs from the game's GitHub Pages site. It keeps your work in the browser until you delete it. It has Undo, but no History.

The palette is PICO-8's 32 colors. The previews show your card on a plain floor, or on the farm when game data is loaded.

### Buttons

- **New** starts a card from a plain figure. Type its id with letters, digits and `_`. Start the name with `PLAYER_` for a player card, `PROP_` for a prop, or `MAP_` for a map.
- **Import** opens `.enemy.png`, `.player.png`, `.prop.png` and `.map.txt` files, and every card, map and prop in a `.pak`. It says which saved cards it replaced. Importing a pack also brings back its cards' death sounds.
- **Export** downloads the open card's saved PNG, `<id>.enemy.png` or `<id>.player.png`. On a prop, it downloads the prop's PNG. On a map, it downloads `<id>.pak`, holding the map and every prop it places.
- **Export all** downloads every card, map, prop, death sound and changed data file as one mod pack, `my-cards.pak`. Put it in the game's `data/mods`.
- **Sound** gives the open enemy card its own death sound. Pick an Ogg or WAV file of up to 512 KB. Export all packs it as `sounds/kill-<id>.ogg`.
- **Delete card** removes the open card for good. Press it twice to confirm.
- **Game data** picks your own `game_data.pak`. See [Game data](#game-data).
- **Help** shows a short guide to making a card.

### Making a card

1. Press **New** and type the card's id.
2. Draw its parts from the list: walking front, side and back, then the flash, hurt and dead frames. An enemy's walk can have 1 to 8 frames. **Compare** shows the parts side by side.
3. Open the **Stats** page with `T` to set the card's name and numbers.
4. Press **Save**, then **Export**.
5. Copy the PNG into the game's `data/enemies` or `data/players` folder, and start the game. Settings > Enemies or Settings > Player shows it.

Hurt and Kill play a hit or the game's death on the figure in the previews.

### The Stats page

The Stats page sets what the game reads from the card. Rename and About type the name and the about line.

For an enemy card, it steps each number with - and +, ten times as far with Shift:

- Health, Speed, Bite, Money, Weight and Gibs.
- Arrives after (minutes), Chance and Chance per minute.
- Charge, its speed near you as a multiple.

Three toggles set Heavy, Leads hordes and Harvester. The page also shows the card's death sound. **Drop own**, beside it, removes the card's own sound.

Under the numbers, a chart shows how a night's spawns split between the game's kinds and your cards, minute by minute from dusk to dawn. Its legend gives each kind's share, and flags any kind that never comes.

For a player card, the Stats page sets the name, the about line, and the weapons each difficulty starts with. Default keeps the game's own.

### The Data pages

Each data file lists under Data, in a folder of its own. Each section of the file is one page, such as `tuning.txt`'s `[player]` under Data > tuning. The random farm's `farm.txt` lists as Data > random farm.

A page shows one line a row, with its comment under it:

- **-** and **+** step a number.
- **Edit** types any value.
- **x** removes a line, and **Add line** types a new `key value` line.
- **Up** and **Down** page through a long section.
- **Revert** goes back to the last save.
- **Save** keeps the file only when the game would read it.

The card editor keeps a changed file in the browser, and Export all carries it into the mod pack. A sprite that `props.txt` or `effects.txt` names shows a Data button that opens its section.

### Props and maps

Props list under Props as `PROP_<ID>`, and maps under Maps as `MAP_<ID>`. They edit as in the sprite editor. See [Props](#props-1) and [Maps](#maps-1) there.

### Game data

On the Pages site, the editor fetches the site's own `game_data.pak` and previews your card on the farm. Served without one, it falls back to the plain floor.

Press **Game data** to pick your own `game_data.pak`. It takes precedence over the site's. The page keeps it in the browser and never sends it anywhere. From then on, the previews show the farm with the tileset's palette. The editor refuses a pack built for a different version of the game.

### Offline use

Once opened, the page keeps an offline copy, so it works with no connection. It still loads a new release first whenever it can.

### On an iPad or phone

Add the page to an iPad's home screen to use it offline with the whole screen. Draw with the Apple Pencil or a finger. The toolbar under the panes stands in for the keyboard.

- Pinch with two fingers to zoom the canvas, and press Fit to set it back.
- Drag the 2.5D view to turn it, and pinch it to move closer.
- The toolbar's Shift stays on until you tap it again.
- The Shift pads in the bottom corners latch. Tap a pad, and it lights until your next pencil press uses it. A second tap turns it off.

On a phone held upright, the editing pane spans the screen with the previews under it.

## The sprite editor

The sprite editor is the developer's pixel editor for the game's own sprites. It runs on macOS, or in a browser on an iPad served from the Mac. It needs the private tileset, so it is never published. To set it up, see [Sprite editor](../../README.md#sprite-editor) in the README.

The left pane lists every sprite. The pixel canvas, tools, color and frames sit around it. The right panes show the sprite as the game draws it, in the 2.5D view and on the map. The 2.5D view counts the quads the sprite is drawn with in 3D, four vertices each, so you can compare stands and drawings by cost. About half of them face the camera at any moment. It starts with nothing open: pick a sprite from the list, or press N for a new one.

### Tools

| Action | Input |
|---|---|
| Draw, erase, fill | B, E, G, or the tool buttons |
| Swap the pencil and the eraser | B again |
| Swap back to the color before | X |
| Draw a line, box or oval | The Line, Box and Oval buttons, then drag. Hold Shift as the drag starts to fill a box or oval, or keep a line straight. |
| Draw a straight line | Shift-click with the pencil or eraser, from the last pixel drawn |
| Pick a color | Hold I or Option and click. Right-click always picks. |
| Search the sprite list | `/`, then type. Enter opens the first match, and Esc clears the filter. |
| Mirror strokes left to right | M |
| See the previous frame faintly | O, or Onion |
| Compare a sprite with its family side by side | C, or Compare |
| Draw flash, hurt or dead frames from the walk | Generate, on a `_FLASH`, `_HURT` or `_DEAD` sprite |
| Select, then move by dragging inside | S, then drag. The arrows nudge 1 pixel, or 8 with Shift. |
| Copy, cut and paste | Cmd+C, Cmd+X, Cmd+V |
| Delete the selected pixels | Delete or Backspace |
| Undo and redo | Cmd+Z, Cmd+Shift+Z |
| Next and previous sprite | J and K |
| Previous and next frame | H and L |
| Play the animation | Space |
| New sprite, or a copy of this one | New, Copy |
| Resize every frame | W and H - and +, 8 at a time with Shift |
| Browse earlier versions | R, or History |
| Save | Cmd+S |

### Stands

A stand sets how a sprite stands in the 3D view. Pick one under "Stands in 3D as":

- **Prop**: upright, fixed to the map like a wall, extruded to a depth you set. Set a fold for a building drawn with its roof seen from above: the rows above the fold line lie flat as the roof, and the rows below it stand as the front. The building is as deep as the roof's rows. Fold - and + move the line, shown in orange on the canvas, and 0 is no fold.
- **Slab**: lying flat as a roof at a height, its edges drawn down to the ground. It suits sprites drawn from above, like cars. Float lifts it off the ground as a plate of that height.
- **Figure**: turning to face the camera like a character. A figure can add a Top drawing that colors each column's top in 3D.
- **Ground**: flat on the ground, like a tile or a stain.
- **Carve**: a solid carved where three views agree. See [Carving](#carving).
- **Turned**: upright like a prop, but running north to south. Draw it seen from the east, with south on the left. It stands where the sprite's outline from above puts it, at a depth you set. It suits pieces drawn from above, like a fence's sides.

#### Carving

A carved sprite gets two more drawings beside its frames. The south view shows it from the south, west on the left. The east view shows it from the east, south on the left. Choosing Carve starts both as a plain block in the sprite's colors, for you to erase into shape. The height buttons set how tall they are.

The solid is every voxel all three views agree on. Each face takes its color from the view that looks at it.

When the sprite's own outline is not its outline from above, add a footprint. A footprint is a copy of the sprite to erase into the true outline. Only whether its pixels are clear counts, and the sprite still colors the top. Delete on the footprint tile removes it.

Three more drawings are optional, each added with its + tile and removed with Delete. They change colors only, never the shape.

- **Top** colors the top in place of the sprite, for when the 2D drawing looks wrong from above. It starts as a copy of the sprite.
- **North** colors the north side, seen from the north with east on the left. Without it, the south view colors that side too.
- **West** colors the west side, seen from the west with north on the left. Without it, the east view colors that side too.

North and West start as the south and east views seen from behind. A clear pixel in Top, North or West leaves the color the side would have without that drawing.

#### No 3D colors

**No 3D** leaves the current color out of the 3D view, while the 2D map keeps it. Press it again to bring the color back. Use it for a backdrop color that should not extrude.

#### Coats

Coats repaint a sprite, so one drawing comes in several colors, like the wrecked cars. **Paint**, beside the color, marks the current color as paint. Mark each shade of the part that changes, such as a car's body, and leave the rest, such as tires and lights, unmarked.

**Coats** opens the Coats page. **Add coat** adds a coat of the current color, its darker shades following the drawing's. Click a shade to give it the current color instead. Click a coat's picture to show it in the previews, or As drawn for the drawing's own colors.

Each frame comes once in each coat: every frame in the first coat, then every frame in the next. A map's `frame N` picks one, and the random farm picks among them. The carving views, footprint and top strip are repainted to match.

#### Palette

The palette under the color is the game's own: the tileset's 22 colors and the few added for what it lacks, listed in `crates/game/palette.txt`. A color the palette lacks shows its code in yellow. The game's own art keeps to this palette. A test fails on any color in its drawings or coats that the file doesn't list, so add a color there, with what it's for, before drawing with it. A new coat's darker shades come from the palette too.

**Lospec** draws from a palette on [Lospec](https://lospec.com/palette-list) instead. Type its name from its address, such as `endesga-32`, or leave it blank to go back to the game's. The sprite editor fetches it from Lospec. In the card editor online, download the palette's `.hex` file from Lospec and pick it with **Palette file**. A mod's art can use any palette.

### The previews

The 2.5D preview shows the sprite as the top screen draws it. Drag it to walk around, and scroll to move closer. The map preview shows it from above, and scroll zooms it. For a map, the previews show the map around the last tile touched.

### History

R, or History, browses earlier versions of the open sprite, newest first. It lists the commits that touched the sprite in the assets repository. For a card's part or a prop, it lists the commits that touched its PNG.

- J and K step through versions, shown read-only on the canvas and in the previews.
- Enter or Restore brings one back as unsaved changes.
- Esc or Back returns to what was open.

The editor only reads from git.

### Cards

Enemy cards list under Enemy cards as six sprites: `CARD_<ID>_FRONT`, `_SIDE` and `_BACK` walking, then `_FLASH`, `_HURT` and `_DEAD`. Player cards list under Player cards as `PLAYER_CARD_<ID>_FRONT` and so on.

- Each part edits like any sprite and saves into its row of the card's PNG. Resizing one part resizes the whole card.
- A walking part takes 1 to 8 frames, and the other walking parts follow its count. Player card parts keep three frames.
- A new card leaves the flash, hurt and dead parts clear for the game to draw. Generate draws them in to edit from, and Clear empties them again.
- An open dead part shows its corpse lying as the game tips it over.
- New with a name starting `CARD_` writes a card from a plain figure. Copy from a part copies the whole card under a new id.
- The Stats page (T) works as in the card editor. See [The Stats page](#the-stats-page).

### Props

A prop lists under Props as `PROP_<ID>` and edits like any sprite. Its stand, fps, Cover, Broken frame and HP buttons set what its text says. New with a name starting `PROP_` starts one.

### Maps

Maps list under Maps as `MAP_<ID>`. New with a name starting `MAP_` starts a map of dirt. The canvas shows the map from above as the game draws it.

- **Ground** paints a ground code. A road paints three lanes wide. Shift-click paints a straight line from the last tile.
- **Prop** places a sprite from the picker: the game's own, then your props.
- **Move** drags a prop or fence and picks it. It shows the piece's hit points and cover to set for this map. Defaults restores its sprite's own.
- **Erase** takes a piece off.
- **Fence** drags a fence, with its gate mid-bottom.
- **Spawn** and **Home** set those tiles.
- **Width** and **Height** grow or shrink the map 4 tiles at a time, or 16 with Shift.
- **Randomize** lays out a farm as the random generator does, every piece a prop to edit. Undo takes it back.

A line under the canvas says whether the game would load the map, or why not. Scroll or pinch to zoom, and right-drag or use the arrows to move.

### The Data pages

Each data file is a folder under one Data heading, as in the card editor. See [The Data pages](#the-data-pages). Save writes the file into the game's source only when the game would read it.

### On an iPad

The sprite editor also runs in a browser on an iPad on the same network as the Mac. Saves land on the Mac, and History reads the same git log. A hardware keyboard keeps the macOS shortcuts. Safari's Add to Home Screen gives the editor the whole screen.

| Action | Touch |
|---|---|
| Draw | Apple Pencil, or a finger on the canvas |
| Zoom and pan the canvas | Pinch with two fingers. Fit sets it back. |
| Turn the 2.5D view, move closer | Drag it, pinch it |
| Zoom the map | Pinch it |
| Scroll the sprite list | Drag it, and tap a name to open it |
| Keyboard shortcuts | The toolbar under the panes |
| Shift for the next pencil press | Tap a Shift pad in a bottom corner. It lights until the pencil uses it, and a second tap turns it off. |
| Name a sprite or search | Type in the field that appears |

To serve it, see [Sprite editor on an iPad](../../README.md#sprite-editor-on-an-ipad) in the README.
