//! Installs cards the way the app does at start-up, in a process of its own: installing reads the game's data
//! once and for good, so nothing on the way may read it first.

use last_harvest_game::data::{self, Card, Data};
use last_harvest_game::{Difficulty, kill_sounds, player_looks};

/// A card of 16-pixel frames, opaque all over, `walk` frames across its walking rows.
fn card(name: &str, text: &str, walk: u32) -> Card {
    let cols = walk.max(3);
    Card { name: name.into(), text: text.into(), frame: [16, 16], sheet: vec![255; (16 * cols * 16 * 5 * 4) as usize] }
}

#[test]
fn enemy_and_player_cards_install_and_reach_the_game() {
    let cards = vec![card("ghoul.enemy.png", "[ghoul]\nname Ghoul\nabout Quick and quiet.\nsound kill-ghoul\nwalk 5\n", 5)];
    let players = vec![card("jo.player.png", "[jo]\nname Jo\nabout Up with the sun.\nstart normal crows knife crows\nstart hard\n", 3)];
    // A map painted with a code only the mod's own ground.txt has, checked against that file.
    let ground = format!("{}\n[x]\nname Ash\nart DIRT_TUFTS\ncell open\n", last_harvest_game::built_in_table("ground.txt").unwrap());
    let rows: String = (0..12).map(|_| format!("{}\n", "x".repeat(16))).collect();
    let map = format!("name Ash\nsize 16 12\nspawn 8 6\nhome 8 5\nground\n{rows}props\n");
    let maps = vec![data::MapFile { file: "ash.map.txt".into(), text: map }];
    let notes = data::install(Data { cards, players, maps, ground: Some(ground), ..Data::default() }).expect("installing cards and maps leaves the data unread until it's done");
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(data::maps().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["ash"], "the map reads with the mod's ground");
    assert!(kill_sounds().any(|s| s == "kill-ghoul"), "the enemy card joins the roster");
    let jo = player_looks().iter().find(|l| l.id == "jo").expect("the player card joins the looks");
    assert_eq!(jo.about, "Up with the sun.");
    assert_eq!(jo.start_names(Difficulty::Normal), ["Crows", "Knife"]);
    assert!(jo.start_names(Difficulty::Hard).is_empty());
    assert_eq!(jo.start_names(Difficulty::Easy), ["Pistol", "Knife"], "a difficulty it doesn't set keeps the game's");
}
