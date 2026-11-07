//! Installs props the way the app does at start-up, in a process of its own: installing reads the game's data once
//! and for good.

use last_harvest_game::art::{self, Art};
use last_harvest_game::data::{self, Card, Data};

/// A prop of `frames` 8-pixel frames side by side, opaque all over.
fn prop(name: &str, text: &str, frames: u32) -> Card {
    Card { detail: 1, worn: String::new(), name: name.into(), text: text.into(), frame: [8, 8], sheet: vec![255; (8 * frames * 8 * 4) as usize] }
}

#[test]
fn a_props_alpha_line_draws_its_frames_see_through_and_an_old_prop_draws_opaque() {
    let props = vec![prop("ghost_lamp.prop.png", "[ghost_lamp]\nframes 3\nalpha 1 0.6\n", 3), prop("old_post.prop.png", "[old_post]\n", 1)];
    let notes = data::install(Data { props, ..Data::default() }).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let frames = |name: &str| last_harvest_game::named_props().into_iter().find(|n| n.name == name).unwrap().frames;
    let lamp: Vec<f32> = frames("PROP_GHOST_LAMP").iter().map(|a| a.alpha()).collect();
    assert_eq!(lamp, [1.0, 0.6, 0.6], "the last listed carries on");
    assert_eq!(frames("PROP_OLD_POST")[0].alpha(), 1.0);
    assert_eq!(Art(0).alpha(), 1.0, "the game's own sprites are opaque unless their alpha.txt says");
    assert_eq!(Art((art::count() + 5) as u16).alpha(), 1.0, "a live sprite the editor hasn't given an alpha");
}

#[test]
fn a_prop_whose_alpha_is_out_of_bounds_does_not_read() {
    assert!(last_harvest_game::prop_fields("[ghost]\nalpha 0").unwrap_err().contains("alpha"));
    assert!(last_harvest_game::prop_fields("[ghost]\nalpha 1.5").unwrap_err().contains("alpha"));
}
