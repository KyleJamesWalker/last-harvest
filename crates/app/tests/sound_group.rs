//! A weapon whose `sound` names a group of its own plays that group's clips, found by name in the sounds.

mod common;

use last_harvest::{pak, samples};
use last_harvest_game::{Sfx, sound_group, sound_groups};
use last_harvest_pack as pack;

fn wav(samples: &[i16]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut b = Vec::new();
    b.extend(b"RIFF");
    b.extend((36 + data).to_le_bytes());
    b.extend(b"WAVEfmt ");
    for v in [16u32.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), 48_000u32.to_le_bytes().to_vec(), 96_000u32.to_le_bytes().to_vec(), 2u16.to_le_bytes().to_vec(), 16u16.to_le_bytes().to_vec()] {
        b.extend(v);
    }
    b.extend(b"data");
    b.extend(data.to_le_bytes());
    samples.iter().for_each(|s| b.extend(s.to_le_bytes()));
    b
}

#[test]
fn a_weapons_own_sound_group_loads_its_clips_from_the_pack() {
    common::isolate("sound-group");
    let mut files = common::base();
    for (path, bytes) in &mut files {
        if path == pack::POWERUPS {
            let text = String::from_utf8(bytes.clone()).unwrap().replace("\r\n", "\n");
            *bytes = text.replacen("[pistol]\n", "[pistol]\nsound rattle\n", 1).replacen("[shotgun]\n", "[shotgun]\nsound blast\n", 1).into_bytes();
        }
    }
    for (name, level) in [("rattle.wav", 1000), ("rattle-2.wav", 2000), ("clank.wav", 3000), ("shot.wav", 4000)] {
        files.push((format!("{}{name}", pack::SOUNDS), wav(&[level; 480])));
    }
    let sounds = pak::install(Some(common::read(&files)), None).expect("the pack installs");
    assert!(last_harvest_game::data::load_report().problems.is_empty(), "{:?}", last_harvest_game::data::load_report().problems);
    assert_eq!(sound_groups().collect::<Vec<_>>(), ["rattle", "rifle", "bolt", "varmint", "casing", "varmint_hit", "rocket", "rocket_pop", "spray", "detonator", "stump_blast", "wire_land", "nail", "shovel", "stake", "arc", "drone"], "a name with a voice of its own is no group");
    assert_eq!(sound_group(0), "rattle");
    assert_eq!(samples::cue(Sfx::group(0, Sfx::Shot)).0, "rattle");
    assert_eq!(Sfx::group(0, Sfx::Shot).voice(), Some(Sfx::Shot), "without clips it plays the pistol's own voice");
    let bank = samples::Bank::load(&sounds, 48_000.0);
    let (first, second) = (bank.get("rattle", 0.0).unwrap(), bank.get("rattle", 0.99).unwrap());
    assert_ne!(first.frames[0], second.frames[0], "both clips of the group load as its variations");
    assert!(bank.get("shot", 0.0).is_some(), "the shot keeps its clip");
    assert!(bank.get("clank", 0.0).is_none(), "a file no weapon names stays on disk");
}
