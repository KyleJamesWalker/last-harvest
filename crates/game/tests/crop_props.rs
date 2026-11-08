//! Installs a mod's props and its own ground.txt the way the app does at start-up, in a process of its own:
//! installing reads the game's data once and for good.

use last_harvest_game::data::{self, Card, Data};
use last_harvest_game::{MapSource, Shape, Showcase};

/// A prop of `frames` 16-pixel frames side by side, opaque all over.
fn prop(id: &str, frames: u32) -> Card {
    Card { detail: 1, worn: String::new(), name: format!("{id}.prop.png"), text: format!("[{id}]\nframes {frames}\n"), frame: [16, 16], sheet: vec![255; (16 * frames * 16 * 4) as usize] }
}

#[test]
fn a_mods_crop_of_its_own_props_stands_on_its_map() {
    let props = vec![prop("wheat_top", 2), prop("wheat", 1), prop("wheat_bottom", 1)];
    let ground = format!("{}\n[w]\nname Wheat\nart SOIL\ncell field\ncrop PROP_WHEAT_TOP PROP_WHEAT PROP_WHEAT_BOTTOM\n", last_harvest_game::built_in_table("ground.txt").unwrap());
    let rows: String = (0..12).map(|y| format!("{}\n", if (2..6).contains(&y) { format!("{}{}", "w".repeat(3), ".".repeat(13)) } else { ".".repeat(16) })).collect();
    let map = format!("name Wheat\nsize 16 12\nspawn 8 6\nhome 8 5\nground\n{rows}props\n");
    let maps = vec![data::MapFile { file: "wheat.map.txt".into(), text: map.clone() }];
    let notes = data::install(Data { props, maps, ground: Some(ground), ..Data::default() }).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert!(data::problems().is_none(), "{:?}", data::problems());
    assert_eq!(data::maps().len(), 1, "the map reads with the mod's ground");
    let frames = |name: &str| last_harvest_game::named_props().into_iter().find(|n| n.name == name).unwrap().frames;
    let mut s = Showcase::new(7);
    s.show_map(Some(&MapSource::parse(&map).unwrap())).unwrap();
    let mut c = last_harvest_game::Canvas::default();
    s.plan(&mut c, [0.0, 0.0], 1.0);
    let drawn = |name: &str| c.shapes.iter().any(|sh| matches!(sh, Shape::Art { art, .. } if frames(name).contains(art)));
    assert!(["PROP_WHEAT_TOP", "PROP_WHEAT", "PROP_WHEAT_BOTTOM"].iter().all(|n| drawn(n)), "every piece stands in the field");
}
