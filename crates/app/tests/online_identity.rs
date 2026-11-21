//! The join handshake's identity names the rules as installed, and each mod pack with its fingerprint.

mod common;

use last_harvest::pak;
use last_harvest_game::data;
use last_harvest_pack::{self as pack};

#[test]
fn the_installed_rules_and_each_mod_reach_the_identity() {
    let dir = common::isolate("online-identity");
    let manifest = (pack::MANIFEST.to_string(), pack::mod_manifest("t").into_bytes());
    let tuning = (pack::TUNING.to_string(), b"[player]\nspeed 61\n".to_vec());
    std::fs::write(dir.join("mods").join("t.pak"), common::bytes(&vec![manifest, tuning])).unwrap();
    pak::install(Some(common::read(&common::base())), None).expect("the mod installs");
    let report = data::load_report();
    assert_eq!(report.mod_packs.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(), ["t.pak"]);
    let identity = last_harvest_game::Game::new(1).identity_for_test();
    assert_eq!(Some(identity.pak), data::fingerprint(), "the identity names the installed rules");
    assert_eq!(identity.mods.iter().map(|m| (m.name.clone(), m.fingerprint)).collect::<Vec<_>>(), report.mod_packs);
}
