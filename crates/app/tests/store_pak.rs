//! A store build starts from the `game_data.pak` built into it, with none beside it.
#![cfg(feature = "store")]

mod common;

use last_harvest::pak;
use last_harvest_game::data;

#[test]
fn a_store_build_installs_its_built_in_pack() {
    common::isolate("store-pak");
    // SAFETY: this test is its own process, and nothing else reads the environment yet.
    unsafe { std::env::remove_var("LAST_HARVEST_PAK") };
    pak::load().expect("the built-in pack installs");
    let report = data::load_report();
    assert!(report.missing_pack.is_none(), "{:?}", report.problems);
    assert!(report.pack_version.is_some(), "{:?}", report.problems);
}
