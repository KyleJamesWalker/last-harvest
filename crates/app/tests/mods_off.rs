//! A mod pack the save turns off adds nothing at the next start, but still lists for the menu to turn back on.

mod common;

use last_harvest::pak;
use last_harvest_game::data;
use last_harvest_pack::{self as pack};

#[test]
fn an_off_pack_adds_nothing_and_the_rest_layer_in_order() {
    let dir = common::isolate("mods-off");
    let manifest = |id: &str, name: &str| (pack::MANIFEST.to_string(), pack::named_mod_manifest(id, name).into_bytes());
    let a = vec![manifest("autumn", "Autumn"), ("sounds/shot.ogg".into(), b"a".to_vec()), ("maps/ash.map.txt".into(), common::map("Ash"))];
    let b = vec![manifest("thanksgiving", "Thanksgiving"), ("sounds/shot.ogg".into(), b"b".to_vec()), ("maps/oak.map.txt".into(), common::map("Oak")), (pack::TUNING.into(), b"[player]\nspeed 61\n".to_vec())];
    let c = vec![(pack::MANIFEST.to_string(), pack::mod_manifest("my-cards").into_bytes()), ("sounds/boom.ogg".into(), b"c".to_vec())];
    for (name, files) in [("a.pak", a), ("b.pak", b), ("c.pak", c)] {
        std::fs::write(dir.join("mods").join(name), common::bytes(&files)).unwrap();
    }
    let mut base = common::base();
    base.push(("sounds/shot.ogg".into(), b"base".to_vec()));
    let sounds = pak::install_without(Some(common::read(&base)), None, &["thanksgiving".into(), "gone".into()]).expect("the mods install");
    let report = data::load_report();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(report.mods, ["a.pak", "c.pak"]);
    assert_eq!(report.mod_packs.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(), ["a.pak", "c.pak"], "online compares only the packs on");
    let listed: Vec<(&str, &str, &str, bool)> = report.packs.iter().map(|p| (p.file.as_str(), p.key.as_str(), p.name.as_str(), p.on)).collect();
    assert_eq!(listed, [("a.pak", "autumn", "Autumn", true), ("b.pak", "thanksgiving", "Thanksgiving", false), ("c.pak", "c", "my-cards", true)]);
    assert_eq!(sounds, [("boom.ogg".into(), b"c".to_vec()), ("shot.ogg".into(), b"a".to_vec())], "the off pack's sound stays out");
    assert_eq!(data::maps().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["ash"], "and its map");
    let identity = last_harvest_game::Game::new(1).identity_for_test();
    assert_eq!(identity.mods.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["a.pak", "c.pak"]);
}
