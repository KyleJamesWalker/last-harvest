LAST HARVEST: the data folder

game_data.pak    The game's own data: sprites, sounds and tables. The game won't start without it,
                 and it has to come from the same release as the game.

enemies/         Enemy cards, named like ghoul.enemy.png. Each adds a kind of zombie to the night, or
                 replaces one of the game's own. Settings > Enemies lists them and turns them on or off.

players/         Player cards, named like jo.player.png. Each is a look for you, and may set the weapons
                 you start with. Settings > Player picks one.

mods/            Mod packs, named like loud.pak. Each is laid over game_data.pak in name order: a file in
                 a mod replaces the game's file at the same path, such as a sound or a card.

Make cards in your browser with the card editor:
https://kylejameswalker.github.io/last-harvest/

Cards and mods load when the game starts. A card or mod that can't load is left out, and log.txt
beside the game says why.
