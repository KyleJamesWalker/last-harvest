//! Installs a base pack whose tables this build refuses, as a pack from another release would have.

mod common;

use last_harvest::pak;
use last_harvest_game::data;
use last_harvest_pack::{self as pack};

#[test]
fn a_base_table_the_game_would_refuse_falls_back_to_the_built_in_one() {
    common::isolate("base-pak-tables");
    let mut files = common::base();
    for (path, bytes) in &mut files {
        match path.as_str() {
            pack::TUNING => bytes.extend(b"\n[player]\nwingspan 3\n"),
            pack::POWERUPS => *bytes = vec![0xff, 0xfe],
            pack::EFFECTS => bytes.extend(b"\nwobble 3\n"),
            _ => {}
        }
    }
    pak::install(Some(common::read(&files)), None).expect("a bad table doesn't stop the game");
    let problems = &data::load_report().problems;
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems.iter().any(|p| p.starts_with("game_data.pak: tuning.txt:")), "{problems:?}");
    assert!(problems.iter().any(|p| p.starts_with("game_data.pak: powerups.txt is not UTF-8")), "{problems:?}");
    assert!(problems.iter().any(|p| p.starts_with("game_data.pak: effects.txt:")), "{problems:?}");
    assert_eq!(last_harvest_game::tuning::tuning().player_width, last_harvest_game::tuning::built_in().player_width, "the built-in numbers stand in");
    assert!(data::powerups().contains("[pistol]"), "the built-in powerups stand in");
}
