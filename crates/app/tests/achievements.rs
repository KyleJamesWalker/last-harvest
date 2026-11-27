//! Each of the game's achievements unlocks through the platform hook after a sandbox night that earns it.

use last_harvest::platform::{self, Recorder};
use last_harvest_game::sandbox::{self, Night, Plan};
use last_harvest_game::{Progress, Unlock, achievements};

fn night(build: &str, secs: f32) -> Night {
    sandbox::night(&Plan {
        until: Some(secs),
        ..Plan::new(sandbox::parse_build(build).unwrap())
    })
}

/// Records one short of `when`, for the conditions that add up over nights; none for a best single run's.
fn just_short(when: Unlock) -> Progress {
    let p = Progress::default();
    match when {
        Unlock::Kills(n) => Progress { kills: n - 1, ..p },
        Unlock::Nights(n) => Progress { nights: n - 1, ..p },
        Unlock::Dawns(n) => Progress { dawns: n - 1, ..p },
        Unlock::Breakers(n) => Progress {
            breakers: n - 1,
            ..p
        },
        Unlock::BreakerDawns(n) => Progress {
            breaker_dawns: n - 1,
            ..p
        },
        Unlock::Start | Unlock::Night(_) | Unlock::Level(_) => p,
    }
}

#[test]
fn every_achievement_unlocks_through_the_hook_once_a_sandbox_night_earns_it() {
    let short = night("pistol:top", 30.0);
    let long = night("pistol:top,shotgun:top,knife:top", 320.0);
    assert!(short.run.kills > 0, "{:?}", short.run);
    for a in achievements::all() {
        let before = just_short(a.when);
        assert!(
            !achievements::reached(&before).any(|id| id == a.id),
            "[{}] is not yet earned before the night",
            a.id
        );
        let n = match a.when {
            Unlock::Night(_) | Unlock::Level(_) => &long,
            _ => &short,
        };
        let seen = Recorder::new();
        let mut backend = seen.clone();
        platform::hand(n.asks(before), &mut backend);
        assert!(
            seen.unlocked().contains(&a.id),
            "[{}] {}: {:?} after {:?}",
            a.id,
            a.about(),
            seen.unlocked(),
            n.run
        );
    }
}

#[test]
fn the_shells_serve_the_games_asks_once_a_frame() {
    let mut game = last_harvest_game::Game::new(1);
    let mut played = last_harvest_game::Save::default();
    played
        .records_mut(last_harvest_game::Difficulty::NORMAL)
        .unwrap()
        .nights = 1;
    game.load(played);
    let seen = Recorder::new();
    let mut hooks = platform::Hooks::new(Box::new(seen.clone()), None);
    platform::serve(&mut game, &mut hooks, 0.016);
    platform::serve(&mut game, &mut hooks, 0.016);
    assert_eq!(seen.unlocked(), ["first_night"]);
    assert_eq!(
        (seen.calls().len(), seen.ticks()),
        (1, 2),
        "asked once, ticked each frame"
    );
}
