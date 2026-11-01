LAST HARVEST: the data folder

game_data.pak    The game's own data: sprites, sounds and tables. It has to come from the same
                 release as the game. When it's missing, the game downloads it as it starts; with
                 no connection, the title says which release's game_data.pak to copy in here.

no-updates       Make an empty file with this name to stop updates. Otherwise, each time you start
                 the game, it looks for a newer release online and installs it first, along with
                 a changed game_data.pak. With no connection it starts as it is, and log.txt beside
                 the game says what the update did.

enemies/         Enemy cards, named like ghoul.enemy.png. Each adds a kind of zombie to the night, or
                 replaces one of the game's own. Settings > Enemies lists them, with each one's about line, and turns them on or off.

players/         Player cards, named like jo.player.png. Each is a look for you, and may set the weapons
                 you start with. Settings > Player picks one.

props/           Props, named like barn.prop.png: a player's own sprites for maps.

maps/            Maps, named like orchard.map.txt: hand-made farms. The title screen's Map row picks
                 one, or a random farm.

mods/            Mod packs, named like loud.pak. Each is laid over game_data.pak in name order: a file in
                 a mod replaces the game's file at the same path, such as a sound or a card.

Make cards in your browser with the card editor:
https://kylejameswalker.github.io/last-harvest/

Cards and mods load when the game starts. A card, mod or sound that can't load is left out: the
title screen says why, and log.txt beside the game lists every one. Stats counts the cards and
mods that loaded.
