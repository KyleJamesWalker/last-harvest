//! A mod pack's farm and a loose one, picked by player cards, as the app installs them at start-up.

mod common;

use last_harvest::pak;
use last_harvest_game::{MapSource, data, farm_ids, use_player_farm};
use last_harvest_pack::{self as pack, card};

fn player(id: &str, farm: &str) -> Vec<u8> {
    card::write_as(card::Kind::Player, &format!("[{id}]\nname {id}\nfarm {farm}\n"), "", 1, 48, 80, 5, &[255; 48 * 80 * 4]).unwrap()
}

#[test]
fn a_packs_farm_and_a_loose_one_reach_the_players_that_pick_them() {
    let dir = common::isolate("player-farms");
    let own = last_harvest_game::built_in_table("farm.txt").unwrap();
    let cane = own.replace("plain s 1 p 1", "plain").replace("crops c 2", "crops k 1");
    let kd = vec![
        (pack::MANIFEST.to_string(), pack::mod_manifest("kd").into_bytes()),
        ("farms/kd.txt".into(), cane.into_bytes()),
        ("farms/kd.ground.txt".into(), b"[k]\nname Cane\nart SOIL\ncell field\ncrop HAY_STACK HAY HAY\n".to_vec()),
        ("players/kd.player.png".into(), player("kd", "kd")),
    ];
    std::fs::write(dir.join("mods/kd.pak"), common::bytes(&kd)).unwrap();
    std::fs::write(dir.join("farms/ash.ground.txt"), "[.]\nname Ash\nart DIRT_TUFTS\ncell open\n").unwrap();
    std::fs::write(dir.join("farms/Bad.txt"), "").unwrap();
    std::fs::write(dir.join("players/ash.player.png"), player("ash", "ash")).unwrap();
    pak::install(Some(common::read(&common::base())), None).expect("the farms install");
    let problems = &data::load_report().problems;
    assert!(problems.len() == 1 && problems[0].starts_with("farms/Bad.txt: "), "{problems:?}");
    assert_eq!(farm_ids(), ["kd", "ash"], "the pack's farm, then the folder's");

    let cane = |seed| MapSource::random("Farm", [72, 54], seed).ground.iter().filter(|&&c| c == b'k').count();
    use_player_farm("kd");
    assert!(cane(3) > 0, "Kuldeep's fields grow cane");
    use_player_farm("ash");
    assert_eq!(cane(3), 0);
    assert_eq!(last_harvest_game::map_ground().iter().find(|(c, _)| *c == b'.').map(|(_, n)| n.as_str()), Some("Ash"), "a loose farm repaints the game's dirt");
    use_player_farm("");
    assert_eq!(last_harvest_game::map_ground().iter().find(|(c, _)| *c == b'.').map(|(_, n)| n.as_str()), Some("Dirt"));
}
