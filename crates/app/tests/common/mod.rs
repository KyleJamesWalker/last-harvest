//! Packs built the way the packer builds them, for tests that each install the game's data once, in a process of
//! their own.
#![allow(dead_code)]

use std::io::Cursor;
use std::path::PathBuf;

use last_harvest_game::art;
use last_harvest_pack::{self as pack, Pak, card};

pub type Files = Vec<(String, Vec<u8>)>;

/// A fresh, empty folder under the system temp folder, named for the test.
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lh-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Points every folder the app reads beside the binary at an empty one, so nothing on this machine leaks in.
pub fn isolate(name: &str) -> PathBuf {
    let dir = temp_dir(name);
    for (var, sub) in [("LAST_HARVEST_ENEMIES", "enemies"), ("LAST_HARVEST_PLAYERS", "players"), ("LAST_HARVEST_MODS", "mods"), ("LAST_HARVEST_MAPS", "maps"), ("LAST_HARVEST_PROPS", "props")] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
        // SAFETY: each test file is its own process, and this runs before anything else reads the environment.
        unsafe { std::env::set_var(var, dir.join(sub)) };
    }
    dir
}

pub fn bytes(files: &Files) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    pack::write(&mut buf, files).unwrap();
    buf.into_inner()
}

pub fn read(files: &Files) -> Pak {
    Pak::read(Cursor::new(bytes(files))).unwrap()
}

pub fn atlas() -> Vec<u8> {
    vec![0; (art::ATLAS_W * art::ATLAS_H * 4) as usize]
}

/// A base pack's files: its manifest, a right-sized atlas and the game's own seven tables.
pub fn base() -> Files {
    let mut files: Files = vec![(pack::MANIFEST.into(), pack::manifest().into_bytes()), (pack::ATLAS.into(), atlas())];
    for table in last_harvest_game::TABLES {
        files.push((table.into(), last_harvest_game::built_in_table(table).unwrap().as_bytes().to_vec()));
    }
    files
}

/// An enemy card of 16-pixel frames, opaque all over.
pub fn enemy(id: &str, sound: &str) -> Vec<u8> {
    card::write(&format!("[{id}]\nname {id}\nsound {sound}\n"), 48, 80, &[255; 48 * 80 * 4]).unwrap()
}

/// A prop of one 8-pixel frame.
pub fn prop(id: &str) -> Vec<u8> {
    pack::prop::write(&format!("[{id}]"), 8, 8, &[255; 8 * 8 * 4]).unwrap()
}

/// A 16 by 12 map of dirt.
pub fn map(name: &str) -> Vec<u8> {
    let rows: String = (0..12).map(|_| format!("{}\n", ".".repeat(16))).collect();
    format!("name {name}\nsize 16 12\nspawn 8 6\nhome 8 5\nground\n{rows}props\n").into_bytes()
}
