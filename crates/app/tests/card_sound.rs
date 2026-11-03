//! A card that carries its own death sound in its PNG brings it to the game, where no sound file has its cue.

mod common;

use last_harvest::pak;
use last_harvest_game::{data, kill_sounds};
use last_harvest_pack::card;

#[test]
fn a_cards_own_death_sound_reaches_the_game() {
    let dir = common::isolate("card-sound");
    let ogg = |n: usize| [b"OggS".as_slice(), &vec![1; n]].concat();
    let with = |id: &str, sound: &[u8]| card::with_sound(&common::enemy(id, &format!("kill-{id}")), Some(sound)).unwrap();
    let mut files = common::base();
    files.push(("enemies/ghoul.enemy.png".into(), with("ghoul", &ogg(64))));
    files.push(("enemies/bat.enemy.png".into(), with("bat", &ogg(64))));
    files.push(("sounds/kill-bat.ogg".into(), b"pack".to_vec()));
    std::fs::write(dir.join("enemies/crow.enemy.png"), with("crow", b"RIFF\0\0\0\0WAVE")).unwrap();
    let mut big = Vec::new();
    let mut encoder = png::Encoder::new(&mut big, 48, 80);
    encoder.set_color(png::ColorType::Rgba);
    encoder.add_itxt_chunk(card::KEYWORD.into(), "[hulk]\nname hulk\nsound kill-hulk\n".into()).unwrap();
    let mut writer = encoder.write_header().unwrap();
    writer.write_chunk(png::chunk::ChunkType(*b"lhSd"), &ogg(card::MAX_SOUND)).unwrap();
    writer.write_image_data(&[255; 48 * 80 * 4]).unwrap();
    writer.finish().unwrap();
    std::fs::write(dir.join("enemies/hulk.enemy.png"), &big).unwrap();
    let sounds = pak::install(Some(common::read(&files)), None).expect("the pack installs");
    let names: Vec<&str> = sounds.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["kill-bat.ogg", "kill-crow.wav", "kill-ghoul.ogg"]);
    assert_eq!(sounds[0].1, b"pack", "a sound file of the same cue wins over the card's own");
    assert_eq!(sounds[2].1, ogg(64));
    let problems = &data::load_report().problems;
    assert!(problems.iter().any(|p| p.starts_with("hulk.enemy.png: its death sound is")), "{problems:?}");
    assert!(kill_sounds().any(|s| s == "kill-crow"), "a loose card brings its sound too");
}
