//! A pack's own farm, picked by a player card, in a process of its own: installing reads the game's data once.

use last_harvest_game::data::{self, Card, Data, FarmFile};
use last_harvest_game::{MapSource, farm_ids, map_ground, use_farm, use_player_farm};

fn card(name: &str, text: &str) -> Card {
    Card { detail: 1, worn: String::new(), name: name.into(), text: text.into(), frame: [16, 16], sheet: vec![255; 16 * 3 * 16 * 5 * 4] }
}

fn has_code(code: char) -> bool {
    map_ground().iter().any(|(c, _)| *c == code as u8)
}

#[test]
fn a_player_card_lays_out_and_paints_its_packs_farm() {
    let own = last_harvest_game::built_in_table("farm.txt").unwrap();
    let cane = FarmFile {
        id: "cane".into(),
        farm: Some(own.replace("plain s 1 p 1", "plain").replace("crops c 2", "crops k 1")),
        ground: Some("[k]\nname Cane\nart SOIL\ncell field\ncrop HAY_STACK HAY HAY\n".into()),
    };
    let broken = FarmFile { id: "broken".into(), farm: Some(own.replace("crops c 2", "crops q 1")), ground: None };
    let players = vec![card("kd.player.png", "[kd]\nname Kuldeep\nfarm cane\n"), card("lost.player.png", "[lost]\nfarm nowhere\n")];
    let rows: String = (0..12).map(|_| format!("{}\n", "k".repeat(16))).collect();
    let map = format!("name Cane\nsize 16 12\nspawn 8 6\nhome 8 5\nground\n{rows}props\n");
    let maps = vec![data::MapFile { file: "cane.map.txt".into(), text: map }];
    let notes = data::install(Data { players, maps, farms: vec![cane, broken], ..Data::default() }).unwrap();
    assert_eq!(farm_ids(), ["cane"]);
    assert!(notes.iter().any(|n| n.starts_with("farms/broken: ") && n.contains("crops q: not a ground code")), "{notes:?}");
    assert!(notes.iter().any(|n| n.starts_with("player lost: no farm called nowhere")), "{notes:?}");
    assert_eq!(data::maps().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["cane"], "a map may paint a farm's own code");

    let codes = |src: &MapSource| src.ground.iter().filter(|&&c| c == b'k').count();
    assert!(!has_code('k'), "the game's own farm is in use until a player picks one");
    use_player_farm("kd");
    assert!(has_code('k') && has_code('c'), "the farm's ground lies over the game's");
    assert!(codes(&MapSource::random("Kd", [72, 54], 7)) > 0, "the fields grow cane");
    use_player_farm("lost");
    assert!(!has_code('k'), "a farm no pack brings plays on the game's");
    use_player_farm("kd");
    use_farm(None);
    assert_eq!(codes(&MapSource::random("Farmer", [72, 54], 7)), 0);
}
