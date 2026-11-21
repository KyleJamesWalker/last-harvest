use crate::netplay::Request;
use crate::{Buttons, Game, Input};

const DT: f32 = 1.0 / 60.0;
const IDS: [u128; 2] = [11, 22];

/// A host and a client game joined by a perfect wire.
struct Pair {
    games: [Game; 2],
}

impl Pair {
    fn new() -> Self {
        let mut games = [Game::new(1), Game::new(2)];
        for g in &mut games {
            g.set_online(true);
            g.load(crate::Save { how_to_seen: true, ..g.save() });
        }
        Pair { games }
    }

    fn wire(&mut self) {
        for i in 0..2 {
            while let Some(r) = self.games[i].net_request() {
                if let Request::Connect { .. } = r {
                    self.games[i].net_joined(IDS[i]);
                    if self.games[1 - i].net_traffic().is_some() {
                        self.games[i].net_peer(IDS[1 - i], true);
                        self.games[1 - i].net_peer(IDS[i], true);
                    }
                }
            }
        }
        for (i, &from) in IDS.iter().enumerate() {
            for p in self.games[i].net_outgoing() {
                let to = IDS.iter().position(|&id| id == p.peer).expect("a known peer");
                self.games[to].net_receive(p.channel, from, p.bytes);
            }
        }
    }

    fn step(&mut self, host: Input, client: Input) {
        self.games[0].update(&host, DT);
        self.games[1].update(&client, DT);
        self.wire();
    }

    fn idle(&mut self, secs: f32) {
        for _ in 0..(secs / DT) as u32 {
            self.step(Input::default(), Input::default());
        }
    }

    fn press(&mut self, who: usize, b: Buttons) {
        let p = Input { held: b, pressed: b, ..Input::default() };
        if who == 0 { self.step(p, Input::default()) } else { self.step(Input::default(), p) }
        self.idle(0.1);
    }

    fn started() -> Self {
        let mut pair = Pair::new();
        pair.games[0].host_online(Some("ACDEF"));
        pair.wire();
        pair.games[1].join_online("acd-ef");
        pair.wire();
        pair.idle(0.5);
        pair.press(1, Buttons::A);
        pair.press(0, Buttons::A);
        pair.press(0, Buttons::START);
        pair
    }
}

#[test]
fn a_host_and_a_client_play_one_night_and_the_client_draws_what_the_host_has() {
    let mut pair = Pair::started();
    assert!(pair.games[0].in_run() && pair.games[1].in_run(), "both are in the night");
    let walk = Input { held: Buttons::UP, ..Input::default() };
    let before = pair.games[0].player_spots()[1];
    let mut drift = Vec::new();
    for t in 0..(20.0 / DT) as u32 {
        pair.step(Input::default(), walk);
        if t % 30 == 0 && t > 120 {
            let host = pair.games[0].zombie_spots();
            for (id, at) in pair.games[1].zombie_spots() {
                if let Some((_, h)) = host.iter().find(|z| z.0 == id) {
                    drift.push(((at[0] - h[0]).powi(2) + (at[1] - h[1]).powi(2)).sqrt());
                }
            }
        }
    }
    let after = pair.games[0].player_spots()[1];
    assert!(((after[0] - before[0]).powi(2) + (after[1] - before[1]).powi(2)).sqrt() > 40.0, "the client's input walks its player on the host");
    let seen = pair.games[1].player_spots()[0];
    assert!(((seen[0] - after[0]).powi(2) + (seen[1] - after[1]).powi(2)).sqrt() < 24.0, "the client predicts its own player near where the host has it: {seen:?} {after:?}");
    assert!(!drift.is_empty(), "zombies rose and were drawn");
    let mean = drift.iter().sum::<f32>() / drift.len() as f32;    assert!(mean < 12.0, "zombies drift {mean} px on average");
    let t = pair.games[1].net_traffic().unwrap();
    assert!(t.down > 0 && t.up > 0);
}

#[test]
fn a_client_from_another_release_is_refused_with_the_reason() {
    let mut pair = Pair::new();
    pair.games[1].set_identity("v9.9.9", 0);
    pair.games[0].host_online(Some("ACDEF"));
    pair.wire();
    pair.games[1].join_online("ACDEF");
    pair.idle(0.5);
    assert!(pair.games[1].online.message.contains("v9.9.9"), "{}", pair.games[1].online.message);
    assert_eq!(pair.games[1].page, crate::menu::Page::Online);
}

#[test]
fn a_client_pause_pauses_the_host_and_its_cards_are_its_own() {
    let mut pair = Pair::started();
    pair.idle(1.0);
    pair.press(1, Buttons::START);
    pair.idle(0.3);
    let crate::State::Harvest(h) = &pair.games[0].state else { panic!("the host is in the night") };
    assert!(h.paused_for_test(), "a client's Start pauses the host");
    let crate::State::Harvest(c) = &pair.games[1].state else { panic!("the client is in the night") };
    assert!(c.paused_for_test(), "and the client draws the pause");
    pair.press(1, Buttons::B);
    pair.idle(0.3);
    let crate::State::Harvest(h) = &pair.games[0].state else { panic!() };
    assert!(!h.paused_for_test(), "the client resumes it");
}

#[test]
fn a_client_leaving_turns_its_player_and_the_host_closing_ends_the_clients_night() {
    let mut pair = Pair::started();
    pair.idle(1.0);
    pair.games[0].net_peer(IDS[1], false);
    pair.idle(0.5);
    let crate::State::Harvest(h) = &pair.games[0].state else { panic!() };
    assert!(!h.alive_for_test(1), "the seat's player turned");
    let mut pair = Pair::started();
    pair.idle(1.0);
    pair.press(0, Buttons::START);
    for _ in 0..4 {
        pair.press(0, Buttons::DOWN);
    }
    pair.press(0, Buttons::A);
    pair.idle(1.0);
    assert!(!pair.games[0].in_run(), "the host quit");
    assert!(!pair.games[1].in_run(), "the client's night ended with it");
    assert_eq!(pair.games[1].online.message, "The host ended the night.");
}
