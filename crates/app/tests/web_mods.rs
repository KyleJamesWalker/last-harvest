//! Lays a web page's mods, the packs a player added and the card editor's work held in memory, over the base pack.

mod common;

use last_harvest::pak;
use last_harvest_game::data;
use last_harvest_pack::{self as pack, Pak};

#[test]
fn a_pages_mods_lay_over_the_base_in_order() {
    common::isolate("web-mods");
    let manifest = (pack::MANIFEST.to_string(), pack::mod_manifest("added").into_bytes());
    let added = common::read(&vec![manifest, ("sounds/shot.ogg".into(), b"added".to_vec()), ("maps/ash.map.txt".into(), common::map("Ash"))]);
    let editor = Pak::from_files(vec![("sounds/shot.ogg".into(), b"editor".to_vec())]);
    let mods = vec![("added.pak".to_string(), pak::read_mod(Ok(added)).unwrap()), ("card editor".to_string(), pak::read_mod(Ok(editor)).unwrap())];
    let mut base = common::base();
    base.push(("sounds/shot.ogg".into(), b"base".to_vec()));
    let sounds = pak::install_with(common::read(&base), mods, vec!["broken.pak: no manifest.toml".into()]).expect("the mods install");
    let report = data::load_report();
    assert_eq!(report.mods, ["added.pak", "card editor"]);
    assert_eq!(report.problems, ["broken.pak: no manifest.toml"]);
    assert_eq!(sounds, [("shot.ogg".into(), b"editor".to_vec())], "the card editor's work goes on top");
    assert_eq!(data::maps().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["ash"]);
}
