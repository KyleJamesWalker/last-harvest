//! A modder's own crop, end to end in the card editor, in a process of its own: a mod's `ground.txt` sets the ground
//! every map in the process paints.

use std::path::Path;

use sprite_editor::editor::{Editor, Key};
use sprite_editor::store::FsStore;
use sprite_editor::studio::Studio;

const PIECES: [&str; 3] = ["PROP_WHEAT_TOP", "PROP_WHEAT", "PROP_WHEAT_BOTTOM"];

fn store(dir: &Path) -> Box<FsStore> {
    Box::new(FsStore { dir: dir.to_path_buf(), catalog: dir.join("art.txt"), cards: dir.join("enemies") })
}

#[test]
fn a_mods_crop_of_three_props_says_where_each_stands_stacks_paints_and_exports() {
    let dir = std::env::temp_dir().join(format!("lh-mod-crop-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("maps")).unwrap();
    let ground = format!("{}\n[w]\nname Wheat\nart SOIL\ncell field\ncrop {}\n", last_harvest_game::built_in_table("ground.txt").unwrap(), PIECES.join(" "));
    std::fs::write(dir.join("ground.txt"), &ground).unwrap();
    let mut e = Editor::new(store(&dir), "");
    e.set_modding();
    for name in PIECES {
        e.key(Key::Char('n'));
        e.set_text(name);
        e.key(Key::Enter);
        e.key(Key::Save);
        assert!(dir.join(format!("props/{}.prop.png", name[5..].to_ascii_lowercase())).exists(), "{name} saved");
    }
    assert!(last_harvest_game::map_ground().contains(&(b'w', "Wheat".to_string())), "the mod's ground paints");
    let roles: Vec<Option<String>> = PIECES.iter().map(|n| Editor::new(store(&dir), n).crop_role()).collect();
    assert_eq!(roles[0].as_deref(), Some("Crop: north end of a Wheat run (ground w)"));
    assert_eq!(roles[1].as_deref(), Some("Crop: middle of a Wheat run (ground w)"));
    assert_eq!(roles[2].as_deref(), Some("Crop: south end of a Wheat run (ground w)"));

    let mut s = Studio::new(store(&dir), "PROP_WHEAT");
    s.tick(0.1);
    let (top, middle, _) = s.editor.crop_preview().expect("packed");
    assert_eq!(middle, s.editor.shown()[0], "the open middle stands live");
    assert!(!top.contains(&middle), "beside the top as saved");

    let rows: String = (0..54).map(|y| format!("{}\n", if (10..14).contains(&y) { format!("{}{}", ".".repeat(10), "w".repeat(62)) } else { ".".repeat(72) })).collect();
    std::fs::write(dir.join("maps/wheat.map.txt"), format!("name Wheat\nsize 72 54\nspawn 36 30\nhome 36 29\nground\n{rows}props\n")).unwrap();
    let mut m = Editor::new(store(&dir), "MAP_WHEAT");
    let (_, files) = m.export_map().expect("a saved map exports");
    let names: Vec<&str> = files.iter().map(|(f, _)| f.as_str()).collect();
    for f in ["maps/wheat.map.txt", "props/wheat_top.prop.png", "props/wheat.prop.png", "props/wheat_bottom.prop.png", "ground.txt"] {
        assert!(names.contains(&f), "{f} goes with the map: {names:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
