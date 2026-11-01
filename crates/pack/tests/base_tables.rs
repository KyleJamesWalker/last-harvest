//! Its own test binary: the game's data can be installed only once per process.

use std::io::Cursor;

use last_harvest_pack::{EFFECTS, FARM, MANIFEST, Pak, TUNING, manifest, write};

#[test]
fn a_base_paks_tables_are_checked_one_by_one_before_the_data_installs() {
    let files: Vec<(String, Vec<u8>)> = [
        (MANIFEST, manifest().into_bytes()),
        (EFFECTS, include_bytes!("../../game/effects.txt").to_vec()),
        (TUNING, include_bytes!("../../game/tuning.txt").to_vec()),
        (FARM, include_str!("../../game/farm.txt").replace("\nfields 4", "\nfields 4000000000").into_bytes()),
    ]
    .into_iter()
    .map(|(p, b)| (p.to_string(), b))
    .collect();
    let mut buf = Cursor::new(Vec::new());
    write(&mut buf, &files).unwrap();
    let pak = Pak::read(Cursor::new(buf.into_inner())).unwrap();
    let problems = pak.table_problems();
    assert_eq!(problems.iter().map(|(f, _)| *f).collect::<Vec<_>>(), [FARM], "{problems:?}");
    assert!(problems[0].1.starts_with("farm.txt:") && problems[0].1.contains("fields is at most"), "{problems:?}");
    assert!(pak.check_mod().unwrap_err().starts_with("farm.txt:"));
    last_harvest_game::data::install(last_harvest_game::data::Data::default()).expect("checking read none of the game's data");
}
