//! Play's page with player cards to pick from, in a process of its own: installing reads the game's data once.

use last_harvest_game::data::{self, Card, Data};
use last_harvest_game::replay::{self, Replay};
use last_harvest_game::{Buttons, Frame, Game, Input, Save, Shape};

fn card(name: &str, text: &str) -> Card {
    Card { detail: 1, worn: String::new(), name: name.into(), text: text.into(), frame: [16, 16], sheet: vec![255; 16 * 3 * 16 * 5 * 4] }
}

fn texts(f: &Frame) -> Vec<String> {
    [&f.top, &f.bottom].into_iter().flat_map(|c| &c.shapes).filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect()
}

fn press(game: &mut Game, one: Buttons, two: Buttons) {
    game.update_all(&[Input { pressed: one, held: one, ..Input::default() }, Input { pressed: two, held: two, ..Input::default() }], 1.0 / 60.0);
}

#[test]
fn each_seated_player_picks_their_card_and_the_replay_keeps_it() {
    let players = vec![card("jo.player.png", "[jo]\nname Jo\nabout Up with the sun.\nstart easy crows\n"), card("kd.player.png", "[kd]\nname Kuldeep\nabout Never late.\nstart easy knife\n")];
    data::install(Data { players, ..Data::default() }).unwrap();
    let mut game = Game::new(5);
    game.load(Save { how_to_seen: true, ..Save::default() });
    game.set_players(2);
    game.record_replays(Vec::new());
    press(&mut game, Buttons::A, Buttons::NONE);
    assert!(game.at_title(), "Play's page lets pads join and leave as the title does");
    let shown = texts(&game.draw());
    assert!(shown.iter().any(|t| t == "P2") && shown.iter().any(|t| t == "Jo"), "player 2 starts in the card a run gave them: {shown:?}");
    press(&mut game, Buttons::NONE, Buttons::RIGHT);
    let shown = texts(&game.draw());
    assert!(shown.iter().any(|t| t == "Kuldeep") && !shown.iter().any(|t| t == "Jo"), "their Right is theirs: {shown:?}");
    assert_eq!(game.save().player.as_str(), "", "player 1 keeps theirs");
    press(&mut game, Buttons::A, Buttons::NONE);
    assert!(game.in_run(), "A on Start plays");
    for _ in 0..120 {
        press(&mut game, Buttons::NONE, Buttons::NONE);
    }
    let raw = game.end_replays().pop().expect("a replay");
    let r = Replay::read(&raw).unwrap();
    assert_eq!(r.setup.seats, ["kd"], "{}", r.setup.to_text());
    assert_eq!(replay::play(&r, &mut replay::Nothing).drift, None, "it plays back the same night");
}
