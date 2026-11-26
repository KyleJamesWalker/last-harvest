//! Each split-screen seat plays as the card it picked, in a process of its own: installing reads the game's data once.

use last_harvest_game::Difficulty;
use last_harvest_game::data::{self, Card, Data};
use last_harvest_game::replay::Setup;

fn card(name: &str, text: &str) -> Card {
    Card { detail: 1, worn: String::new(), name: name.into(), text: text.into(), frame: [16, 16], sheet: vec![255; 16 * 3 * 16 * 5 * 4] }
}

#[test]
fn each_seat_starts_with_its_own_cards_look_stats_and_weapons() {
    let players = vec![
        card("jo.player.png", "[jo]\nname Jo\nhp 150\nspeed 30\nmagnet 90\nstart easy crows\n"),
        card("kd.player.png", "[kd]\nname Kuldeep\nhp 70\nspeed 55\nmagnet 40\nstart easy knife pitchfork\n"),
    ];
    data::install(Data { players, ..Data::default() }).unwrap();
    let setup = Setup { difficulty: Difficulty::EASY, look: "jo".into(), seats: vec!["kd".into(), String::new()], players: 3, views: 3, ..Setup::night(9) };
    let h = setup.build();
    assert_eq!(h.seat_summary(0), "jo hp=150/150 speed=30 magnet=90 weapons=crows");
    assert_eq!(h.seat_summary(1), "kd hp=70/70 speed=55 magnet=40 weapons=knife,pitchfork");
    let farmer = h.seat_summary(2);
    assert!(farmer.starts_with(" hp=") && farmer.ends_with("weapons=pistol,knife"), "an empty seat is the farmer: {farmer}");
    let unpicked = Setup { seats: Vec::new(), ..setup }.build();
    let auto = [unpicked.seat_summary(1), unpicked.seat_summary(2)];
    assert!(auto[0].starts_with(" hp=") && auto[1].starts_with("kd "), "without picks, seats take the other cards in turn: {auto:?}");
}
