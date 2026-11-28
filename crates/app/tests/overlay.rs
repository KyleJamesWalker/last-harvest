//! The platform's overlay pauses a run in play, which stays paused once it closes; menus carry on as before.

use last_harvest::platform::{self, Hooks, Recorder};
use last_harvest_game::{Buttons, Consent, Game, Input, Save};

/// One shell frame: the game's update, then the platform served, `dt` seconds long.
fn frame(game: &mut Game, hooks: &mut Hooks, pressed: Buttons, dt: f32) {
    game.update(
        &Input {
            pressed,
            ..Default::default()
        },
        dt,
    );
    platform::serve(game, hooks, dt);
}

fn frames(game: &mut Game, hooks: &mut Hooks, n: usize) {
    for _ in 0..n {
        frame(game, hooks, Buttons::NONE, 0.5);
    }
}

#[test]
fn the_overlay_pauses_a_run_in_play_until_the_player_resumes_and_leaves_menus_alone() {
    let seen = Recorder::new();
    let mut hooks = Hooks::new(Box::new(seen.clone()), None);
    let mut game = Game::new(1);
    game.load(Save {
        metrics: Consent::No,
        how_to_seen: true,
        ..Save::default()
    });

    seen.set_overlay(true);
    frames(&mut game, &mut hooks, 5);
    assert!(game.at_title() && !game.in_run(), "{}", game.summary());
    seen.set_overlay(false);
    frames(&mut game, &mut hooks, 1);
    assert!(game.at_title(), "{}", game.summary());

    frame(&mut game, &mut hooks, Buttons::A, 0.016);
    frame(&mut game, &mut hooks, Buttons::A, 0.016);
    assert!(game.in_run(), "{}", game.summary());
    assert_eq!(game.escape(), Buttons::START, "in play");
    let before = game.summary();
    frames(&mut game, &mut hooks, 4);
    assert_ne!(game.summary(), before, "the night runs");

    seen.set_overlay(true);
    frames(&mut game, &mut hooks, 2);
    let paused = game.summary();
    frames(&mut game, &mut hooks, 5);
    assert_eq!(game.escape(), Buttons::B, "the pause menu is open");
    assert!(!game.mouse_look());
    assert_eq!(game.summary(), paused, "the night waits under the overlay");

    seen.set_overlay(false);
    frames(&mut game, &mut hooks, 5);
    assert_eq!(game.escape(), Buttons::B, "still paused once it closes");
    assert_eq!(game.summary(), paused);

    frame(&mut game, &mut hooks, Buttons::START, 0.016);
    assert_eq!(game.escape(), Buttons::START, "Start resumes");
    frames(&mut game, &mut hooks, 4);
    assert_ne!(game.summary(), paused, "and the night runs on");
}
