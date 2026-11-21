//! A host and a client game playing one night through a relay on this machine and `last-harvest-net`'s WebRTC
//! peers, in real time.
#![cfg(any(target_os = "macos", all(target_os = "linux", target_arch = "x86_64")))]

use std::time::{Duration, Instant};

use last_harvest::online::Online;
use last_harvest_game::{Buttons, Game, Input, Save};

const STEP: f32 = 1.0 / 60.0;

struct Side {
    game: Game,
    online: Online,
}

impl Side {
    fn new(seed: u64, relay: &str) -> Self {
        let mut game = Game::new(seed);
        game.load(Save { how_to_seen: true, ..Save::default() });
        game.set_online(true);
        game.set_relay(relay);
        Side { game, online: Online::default() }
    }

    fn tick(&mut self, input: Input) {
        self.online.pump(&mut self.game, STEP);
        self.game.update(&input, STEP);
        self.online.send(&mut self.game);
    }
}

fn run(sides: &mut [Side; 2], secs: f32, mut each: impl FnMut(&mut [Side; 2], u32) -> [Input; 2]) {
    let start = Instant::now();
    for t in 0..(secs / STEP) as u32 {
        let inputs = each(sides, t);
        for (s, i) in sides.iter_mut().zip(inputs) {
            s.tick(i);
        }
        let due = start + Duration::from_secs_f32((t + 1) as f32 * STEP);
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
    }
}

fn until(sides: &mut [Side; 2], what: &str, secs: f32, done: impl Fn(&[Side; 2]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs_f32(secs);
    while !done(sides) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        run(sides, STEP * 6.0, |_, _| [Input::default(), Input::default()]);
    }
}

fn press(b: Buttons) -> Input {
    Input { held: b, pressed: b, ..Input::default() }
}

#[test]
fn a_host_and_a_client_play_a_night_through_the_relay_and_stay_in_step() {
    let port = last_harvest_relay::start(&last_harvest_relay::Config::embedded(0)).unwrap();
    let relay = format!("127.0.0.1:{port}");
    let mut sides = [Side::new(1, &relay), Side::new(2, &relay)];
    sides[0].game.host_online(Some("HJKMN"));
    until(&mut sides, "the host in its room", 10.0, |s| s[0].game.net_traffic().is_some_and(|_| true) && s[0].game.lobby_ready());
    sides[1].game.join_online("hjkmn");
    until(&mut sides, "the client seated", 20.0, |s| s[1].game.lobby_ready() && s[0].game.lobby_size() == 2);
    run(&mut sides, 0.1, |_, t| [Input::default(), if t == 0 { press(Buttons::A) } else { Input::default() }]);
    run(&mut sides, 0.5, |_, t| [if t == 0 { press(Buttons::A) } else { Input::default() }, Input::default()]);
    until(&mut sides, "both ready", 10.0, |s| s[0].game.lobby_all_ready());
    run(&mut sides, 0.1, |_, t| [if t == 0 { press(Buttons::START) } else { Input::default() }, Input::default()]);
    until(&mut sides, "the night on both", 10.0, |s| s[0].game.in_run() && s[1].game.in_run());

    let walk = Input { held: Buttons::UP, ..Input::default() };
    let before = sides[0].game.player_spots()[1];
    let mut drift = Vec::new();
    run(&mut sides, 12.0, |s, t| {
        if t > 240 && t % 20 == 0 {
            let host = s[0].game.zombie_spots();
            for (id, at) in s[1].game.zombie_spots() {
                if let Some((_, h)) = host.iter().find(|z| z.0 == id) {
                    drift.push(((at[0] - h[0]).powi(2) + (at[1] - h[1]).powi(2)).sqrt());
                }
            }
        }
        [Input::default(), walk]
    });
    let after = sides[0].game.player_spots()[1];
    let walked = ((after[0] - before[0]).powi(2) + (after[1] - before[1]).powi(2)).sqrt();
    assert!(walked > 40.0, "the client's input walked its player {walked} px on the host");
    let own = sides[1].game.player_spots()[0];
    let gap = ((own[0] - after[0]).powi(2) + (own[1] - after[1]).powi(2)).sqrt();
    assert!(gap < 32.0, "the client's own player is {gap} px from the host's");
    assert!(drift.len() > 20, "zombies rose on both: {} samples", drift.len());
    drift.sort_by(f32::total_cmp);
    let mean = drift.iter().sum::<f32>() / drift.len() as f32;
    let p99 = drift[(drift.len() - 1) * 99 / 100];
    let t = sides[1].game.net_traffic().unwrap();
    eprintln!("drift mean {mean:.1} px, p99 {p99:.1} px over {} samples; client down {:.0} B/s, up {:.0} B/s", drift.len(), t.down as f32 / t.secs, t.up as f32 / t.secs);
    assert!(mean < 16.0 && p99 < 64.0, "zombies drift {mean} px on average, {p99} at p99");
}
