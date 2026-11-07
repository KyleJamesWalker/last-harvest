//! Installs a pack with a weapon's section tried over it, as the range scene does for an editor's Try it.

mod common;

use last_harvest::pak;
use last_harvest_game::{Game, RangeSetup, Scene, data, weapon_levels};

#[test]
fn a_tried_section_stands_in_for_the_packs_own_and_the_range_holds_it() {
    common::isolate("range-section");
    let setup = RangeSetup::read("level 2\n[pistol]\nkind weapon\ndoes shotgun\nicon ICON_PISTOL\nmax 3\ndamage 40 50 60\n").expect("the setup reads");
    pak::try_section(setup.section.as_deref().unwrap());
    pak::install(Some(common::read(&common::base())), None).expect("the pack installs");
    assert!(data::load_report().problems.is_empty(), "{:?}", data::load_report().problems);
    let pistol = weapon_levels(data::powerups(), "pistol").unwrap();
    assert_eq!(pistol.max, 3);
    assert!(data::powerups().contains("[shotgun]"), "the rest of the pack's powerups stay");
    let mut game = Game::new(1);
    game.set_range(setup);
    game.open(Scene::Range);
    assert!(game.summary().starts_with("range pistol 2"), "{}", game.summary());
}
