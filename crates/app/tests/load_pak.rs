//! Installs a base pack the way the app does at start-up, end to end.

mod common;

use last_harvest::pak;
use last_harvest_game::{data, kill_sounds};

#[test]
fn a_base_packs_card_map_sound_and_props_reach_the_game() {
    common::isolate("load-pak");
    let mut files = common::base();
    files.push(("enemies/ghoul.enemy.png".into(), common::enemy("ghoul", "kill-ghoul")));
    files.push(("maps/ash.map.txt".into(), common::map("Ash")));
    files.push(("sounds/kill-ghoul.ogg".into(), b"ogg".to_vec()));
    files.push(("props/a.prop.png".into(), common::prop("crate")));
    files.push(("props/b.prop.png".into(), common::prop("crate")));
    let sounds = pak::install(Some(common::read(&files)), None).expect("the pack installs");
    let report = data::load_report();
    assert_eq!(report.problems.iter().map(String::as_str).collect::<Vec<_>>(), ["b.prop.png: another prop is [crate] too"]);
    assert_eq!(report.cards, 1, "a prop left out doesn't count against the cards");
    assert_eq!(report.pack_version.as_deref(), Some(last_harvest_pack::VERSION));
    assert!(report.missing_pack.is_none());
    assert_eq!(data::maps().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["ash"]);
    assert!(kill_sounds().any(|s| s == "kill-ghoul"), "the card joins the roster");
    assert_eq!(sounds.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["kill-ghoul.ogg"]);
}
