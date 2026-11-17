//! A mod pack's weapons as the app installs them at start-up: one kept to the pack's player, one left to everyone.

mod common;

use last_harvest::pak;
use last_harvest_game::{Difficulty, data, player_looks, player_powerups};
use last_harvest_pack::{self as pack, card};

#[test]
fn a_packs_player_only_weapon_is_offered_only_with_its_player() {
    let dir = common::isolate("player-weapons");
    let own = last_harvest_game::built_in_table("powerups.txt").unwrap();
    let powerups = format!("{own}\n[urumi]\nname Urumi\nkind weapon\nicon WHITE\ndoes blade\nplayer kd\n\n[desi_katta]\nname Desi Katta\nkind weapon\nicon WHITE\ndoes pistol\n");
    let kd = card::write_as(card::Kind::Player, "[kd]\nname Kuldeep\nstart normal urumi pistol\n", "", 1, 48, 80, 5, &[255; 48 * 80 * 4]).unwrap();
    let files = vec![(pack::MANIFEST.to_string(), pack::mod_manifest("kd").into_bytes()), (pack::POWERUPS.into(), powerups.into_bytes()), ("players/kd.player.png".into(), kd)];
    std::fs::write(dir.join("mods/kd.pak"), common::bytes(&files)).unwrap();
    pak::install(Some(common::read(&common::base())), None).expect("the pack installs");
    assert!(data::load_report().problems.is_empty(), "{:?}", data::load_report().problems);

    let has = |id: &str, up: &str| player_powerups(id).iter().any(|p| p == up);
    assert!(has("kd", "urumi") && !has("", "urumi"), "the Urumi stays with Kuldeep");
    assert!(has("kd", "desi_katta") && has("", "desi_katta"), "a weapon without the setting is every player's");
    let kd = player_looks().iter().find(|l| l.id == "kd").unwrap();
    assert_eq!(kd.start_names(Difficulty::Normal), ["Urumi", "Pistol"]);
}
