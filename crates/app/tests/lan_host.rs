//! Hosting on this network: one game starts its own LAN relay and another joins through it as a nearby host, with
//! no ICE servers and no internet.
#![cfg(not(target_arch = "wasm32"))]

use std::time::{Duration, Instant};

use last_harvest::online::Online;
use last_harvest_game::{Game, Save};

const STEP: f32 = 1.0 / 60.0;

fn side(seed: u64) -> (Game, Online) {
    let mut game = Game::new(seed);
    game.load(Save {
        how_to_seen: true,
        ..Save::default()
    });
    game.set_online(true);
    game.set_relay("relay.invalid");
    (game, Online::default())
}

#[test]
fn a_game_hosts_on_its_own_relay_and_another_joins_it_as_a_nearby_host() {
    let port = std::net::TcpListener::bind("0.0.0.0:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut sides = [side(1), side(2)];
    sides[0].0.set_lan(Some(port));
    sides[0].0.host_lan(Some("HJKMN"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !sides[0].0.lobby_ready() {
        assert!(
            Instant::now() < deadline,
            "the host never reached its own relay"
        );
        let (game, online) = &mut sides[0];
        online.pump(game, STEP);
        game.update(&Default::default(), STEP);
        online.send(game);
        std::thread::sleep(Duration::from_secs_f32(STEP));
    }

    sides[1].0.set_nearby(vec![format!("127.0.0.1:{port}")]);
    sides[1].0.join_online("hjkmn");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !(sides[1].0.lobby_ready() && sides[0].0.lobby_size() == 2) {
        assert!(
            Instant::now() < deadline,
            "the client never sat down in the LAN room"
        );
        for (game, online) in &mut sides {
            online.pump(game, STEP);
            game.update(&Default::default(), STEP);
            online.send(game);
        }
        std::thread::sleep(Duration::from_secs_f32(STEP));
    }
    assert_eq!(
        sides[1].0.save().relay.as_str(),
        "",
        "the saved relay is untouched"
    );
}
