//! Starts with a `game_data.pak` built for another build of the game, as a half-finished update leaves one.

mod common;

use last_harvest::pak;
use last_harvest_game::data;
use last_harvest_pack::{self as pack};

#[test]
fn a_pack_built_for_another_build_is_noted_and_the_game_still_starts() {
    let dir = common::isolate("mismatched-pak");
    let mut files = common::base();
    files[0].1 = pack::manifest().replace("hash = \"", "hash = \"f").into_bytes();
    let path = dir.join("game_data.pak");
    std::fs::write(&path, common::bytes(&files)).unwrap();
    // SAFETY: this test is its own process, and nothing else reads the environment yet.
    unsafe { std::env::set_var("LAST_HARVEST_PAK", &path) };
    pak::load().expect("a pack that won't open doesn't stop the game");
    let report = data::load_report();
    assert!(report.problems.iter().any(|p| p.contains("game_data.pak: its atlas was built for a different build")), "{:?}", report.problems);
    assert!(report.pack_version.is_none());
    assert_eq!(report.missing_pack.is_some(), !cfg!(feature = "embed"), "without built-in data the title says which pack to copy in");
}
