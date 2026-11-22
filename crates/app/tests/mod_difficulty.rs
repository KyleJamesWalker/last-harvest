//! A mod pack's tuning.txt adds a difficulty after the game's own and changes one of them.

mod common;

use last_harvest::pak;
use last_harvest_game::{Difficulty, Save, Tier, data};
use last_harvest_pack::{self as pack};

#[test]
fn a_mod_adds_a_difficulty_and_changes_one_line_of_another() {
    let dir = common::isolate("mod-difficulty");
    let tuning = "[nightmare]\nname Nightmare\nblurb Only for the brave\nkit hard\nspawn.rate 3\n\n[brutal]\nleader.speed 60\n";
    let files = vec![(pack::MANIFEST.to_string(), pack::mod_manifest("night").into_bytes()), (pack::TUNING.into(), tuning.as_bytes().to_vec())];
    std::fs::write(dir.join("mods").join("night.pak"), common::bytes(&files)).unwrap();
    pak::install(Some(common::read(&common::base())), None).expect("the mod installs");
    assert_eq!(data::load_report().mods, ["night.pak"], "{:?}", data::load_report().problems);
    let ids: Vec<&str> = Difficulty::all().map(Difficulty::key).collect();
    assert_eq!(ids, ["stroll", "easy", "normal", "hard", "brutal", "dead", "nightmare"]);
    let nightmare = Difficulty::from_key("nightmare").unwrap();
    assert_eq!((nightmare.name(), nightmare.blurb(), nightmare.tier(), nightmare.damage()), ("Nightmare", "Only for the brave", Tier::Hard, Difficulty::HARD.damage()));
    nightmare.play();
    assert_eq!(last_harvest_game::tuning::tuning().spawn_rate, 3.0);
    Difficulty::BRUTAL.play();
    assert_eq!((last_harvest_game::tuning::tuning().leader_speed, last_harvest_game::tuning::tuning().spawn_rate), (60.0, 1.0), "the rest of Brutal stays");
    let save = Save::parse("difficulty=nightmare\nrecord.nightmare=kills:4 nights:1\n");
    assert_eq!((save.difficulty, save.records(nightmare).kills), (nightmare, 4));
}
