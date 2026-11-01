//! Its own test binary: the game's data can be installed only once per process.

#[test]
fn checking_every_table_reads_none_of_the_games_data() {
    for file in last_harvest_game::TABLES {
        let text = last_harvest_game::built_in_table(file).unwrap();
        last_harvest_game::check_table(file, text).unwrap_or_else(|e| panic!("{e}"));
    }
    let own = |file| last_harvest_game::built_in_table(file).map(String::from);
    last_harvest_game::check_tables(own("powerups.txt").as_deref(), own("enemies.txt").as_deref(), own("props.txt").as_deref()).unwrap();
    last_harvest_game::data::install(last_harvest_game::data::Data::default()).expect("a check read the game's data, so install can't");
}
