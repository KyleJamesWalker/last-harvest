//! Lays the mods folder's packs over a base pack the way the app does at start-up, leaving out one it refuses.

mod common;

use last_harvest::pak;
use last_harvest_game::data;
use last_harvest_pack::{self as pack};

#[test]
fn mods_lay_over_the_base_in_name_order_and_a_refused_one_is_noted() {
    let dir = common::isolate("read-mods");
    let manifest = |id: &str| (pack::MANIFEST.to_string(), pack::mod_manifest(id).into_bytes());
    let first = vec![manifest("a"), ("sounds/shot.ogg".into(), b"a".to_vec()), ("maps/ash.map.txt".into(), common::map("Ash"))];
    let second = vec![manifest("b"), ("sounds/shot.ogg".into(), b"b".to_vec())];
    let wrong_atlas = vec![manifest("c"), (pack::ATLAS.into(), vec![0; 16])];
    let bad_table = vec![manifest("d"), (pack::POWERUPS.into(), b"[nope]\nnonsense 1\n".to_vec())];
    let effects = vec![manifest("e"), (pack::EFFECTS.into(), last_harvest_game::built_in_table("effects.txt").unwrap().as_bytes().to_vec())];
    for (name, files) in [("a.pak", first), ("b.pak", second), ("c.pak", wrong_atlas), ("d.pak", bad_table), ("e.pak", effects)] {
        std::fs::write(dir.join("mods").join(name), common::bytes(&files)).unwrap();
    }
    std::fs::write(dir.join("mods").join("notes.txt"), "not a pack").unwrap();
    let mut base = common::base();
    base.push(("sounds/shot.ogg".into(), b"base".to_vec()));
    base.push(("sounds/boom.ogg".into(), b"base".to_vec()));
    let sounds = pak::install(Some(common::read(&base)), None).expect("the mods install");
    let report = data::load_report();
    assert_eq!(report.mods, ["a.pak", "b.pak", "e.pak"], "a mod with effects.txt installs too");
    assert_eq!(report.problems.len(), 2, "{:?}", report.problems);
    assert!(report.problems[0].starts_with("c.pak: its atlas"), "{:?}", report.problems);
    assert!(report.problems[1].starts_with("d.pak: powerups.txt:"), "{:?}", report.problems);
    assert_eq!(sounds, [("boom.ogg".into(), b"base".to_vec()), ("shot.ogg".into(), b"b".to_vec())], "the later mod's sound wins");
    assert_eq!(data::maps().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["ash"], "a mod's map loads");
}
