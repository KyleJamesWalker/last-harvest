//! Play stats reach the metrics hook through the shells' `serve` only once the player shares them.

use last_harvest::platform::{self, Field, Hooks, Recorder};
use last_harvest_game::sandbox::{self, Plan};
use last_harvest_game::{Ask, Consent, Game, Save};

#[test]
fn a_night_sends_its_end_and_one_pick_per_powerup_it_ended_with() {
    let night = sandbox::night(&Plan {
        until: Some(20.0),
        ..Plan::new(sandbox::parse_build("pistol:3,knife:2").unwrap())
    });
    let seen = Recorder::new();
    let mut backend = seen.clone();
    let mut t = last_harvest::metrics::Telemetry::new(None);
    t.frame(Consent::Yes, &[Ask::RunEnded(night.run)], &mut backend, 0.0);
    t.frame(
        Consent::Yes,
        &[],
        &mut backend,
        last_harvest::metrics::FLUSH_EVERY,
    );
    let got = seen.delivered();
    let end = got.iter().find(|e| e.name == "run_end").unwrap();
    assert_eq!(
        end.field("kills"),
        Some(&Field::Number(night.run.kills.into()))
    );
    assert_eq!(
        end.field("build"),
        Some(&Field::Text("pistol:3 knife:2".into()))
    );
    let picks: Vec<_> = got
        .iter()
        .filter(|e| e.name == "build_pick")
        .map(|e| (e.field("powerup").cloned(), e.field("level").cloned()))
        .collect();
    assert_eq!(
        picks,
        [
            (Some(Field::Text("pistol".into())), Some(Field::Number(3.0))),
            (Some(Field::Text("knife".into())), Some(Field::Number(2.0))),
        ]
    );
}

fn played(metrics: Consent) -> Game {
    let mut game = Game::new(1);
    let mut save = Save {
        metrics,
        ..Save::default()
    };
    save.records_mut(last_harvest_game::Difficulty::NORMAL)
        .unwrap()
        .nights = 1;
    game.load(save);
    game
}

#[test]
fn serve_sends_nothing_until_the_player_shares_and_then_their_install() {
    let seen = Recorder::new();
    let mut hooks = Hooks::new(Box::new(seen.clone()), None);
    let mut game = played(Consent::Unasked);
    for _ in 0..3 {
        platform::serve(&mut game, &mut hooks, 60.0);
    }
    assert!(
        seen.delivered().is_empty() && seen.calls().len() == 1,
        "the achievement alone: {:?}",
        seen.calls()
    );

    let mut game = played(Consent::Yes);
    for _ in 0..3 {
        platform::serve(&mut game, &mut hooks, 60.0);
    }
    let names: Vec<String> = seen.delivered().into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["install"]);
}
