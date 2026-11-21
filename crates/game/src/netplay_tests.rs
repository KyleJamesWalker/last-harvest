use crate::fmath as fm;
use crate::netplay::Request;
use crate::{Buttons, Game, Input};

const DT: f32 = 1.0 / 60.0;
const IDS: [u128; 2] = [11, 22];

/// A host and a client game joined by a perfect wire.
struct Pair {
    games: [Game; 2],
    /// Drops everything a game sends on the unreliable channel, or on both when `all`.
    cut: [bool; 2],
    all: bool,
}

impl Pair {
    fn new() -> Self {
        let mut games = [Game::new(1), Game::new(2)];
        for g in &mut games {
            g.set_online(true);
            g.load(crate::Save { how_to_seen: true, ..g.save() });
        }
        Pair { games, cut: [false; 2], all: false }
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
                if self.cut[i] && (self.all || p.channel == crate::netplay::UNRELIABLE) {
                    continue;
                }
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
                    drift.push((fm::powi(at[0] - h[0], 2) + fm::powi(at[1] - h[1], 2)).sqrt());
                }
            }
        }
    }
    let after = pair.games[0].player_spots()[1];
    assert!((fm::powi(after[0] - before[0], 2) + fm::powi(after[1] - before[1], 2)).sqrt() > 40.0, "the client's input walks its player on the host");
    let seen = pair.games[1].player_spots()[0];
    assert!((fm::powi(seen[0] - after[0], 2) + fm::powi(seen[1] - after[1], 2)).sqrt() < 24.0, "the client predicts its own player near where the host has it: {seen:?} {after:?}");
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

#[test]
fn a_client_who_turns_leads_the_dead_from_its_own_end_and_picks_its_rise() {
    let mut pair = Pair::started();
    pair.idle(1.0);
    let crate::State::Harvest(h) = &mut pair.games[0].state else { panic!() };
    h.fall_for_test(1);
    pair.idle(crate::harvest::TURNED_FOR_TEST + 0.5);
    let crate::State::Harvest(h) = &pair.games[0].state else { panic!() };
    let (leading, from) = h.lead_for_test(1).expect("the client's player leads on the host");
    assert!(leading);
    let crate::State::Harvest(c) = &mut pair.games[1].state else { panic!() };
    assert_eq!(c.lead_for_test(0).map(|l| l.0), Some(true), "and the client knows it");
    let mut f = crate::Frame::default();
    c.draw(&mut f, false);
    assert!(f.top.shapes.iter().any(|x| matches!(x, crate::Shape::Text { text, .. } if text.contains("YOU LEAD THE DEAD"))));
    for _ in 0..(2.0 / DT) as u32 {
        pair.step(Input::default(), Input { held: Buttons::UP, ..Input::default() });
    }
    let crate::State::Harvest(h) = &pair.games[0].state else { panic!() };
    let at = h.lead_for_test(1).unwrap().1;
    assert!((fm::powi(at[0] - from[0], 2) + fm::powi(at[1] - from[1], 2)).sqrt() > 25.0, "the client's pad walks its leader on the host");
    let crate::State::Harvest(c) = &pair.games[1].state else { panic!() };
    let seen = c.lead_for_test(0).unwrap().1;
    assert!((fm::powi(seen[0] - at[0], 2) + fm::powi(seen[1] - at[1], 2)).sqrt() < 16.0, "the client predicts its leader near the host's: {seen:?} {at:?}");
    let crate::State::Harvest(h) = &mut pair.games[0].state else { panic!() };
    h.fell_leader_for_test(1);
    pair.idle(0.3);
    let crate::State::Harvest(c) = &pair.games[1].state else { panic!() };
    assert_eq!(c.lead_for_test(0).map(|l| l.0), Some(false), "the client picks its rise");
    let crate::State::Harvest(h) = &pair.games[0].state else { panic!() };
    let cross = h.lead_for_test(1).unwrap().1;
    for _ in 0..(0.5 / DT) as u32 {
        pair.step(Input::default(), Input { held: Buttons::RIGHT, ..Input::default() });
    }
    let crate::State::Harvest(h) = &pair.games[0].state else { panic!() };
    let moved = h.lead_for_test(1).unwrap().1;
    assert!(moved[0] > cross[0] + 20.0 || moved[0] >= h.map_width_for_test() - 16.0, "the client's pad moves the cross: {cross:?} {moved:?}");
}

fn host_harvest(pair: &mut Pair) -> &mut crate::harvest::Harvest {
    let crate::State::Harvest(h) = &mut pair.games[0].state else { panic!("the host is in the night") };
    h
}

#[test]
fn a_snapshot_naming_a_powerup_past_the_book_is_drawn_without_it() {
    use crate::netsync::snapshot::*;
    let mut pair = Pair::started();
    pair.idle(0.5);
    let players = vec![PlayerState { kit: vec![(250, 3), (1, 1)], hp: 40, max_hp: 40, ..Default::default() }; 2];
    let w = World::new(100_000, players, vec![], vec![]);
    for bytes in encode(&w, None, 1, &[], &[], BUDGET) {
        pair.games[1].net_receive(crate::netplay::UNRELIABLE, IDS[0], bytes);
    }
    pair.games[1].update(&Input::default(), DT);
    assert!(pair.games[1].in_run());
}

#[test]
fn a_start_with_no_seat_for_this_game_is_refused() {
    let mut pair = Pair::new();
    pair.games[0].host_online(Some("ACDEF"));
    pair.wire();
    pair.games[1].join_online("ACDEF");
    pair.idle(0.5);
    let start = last_harvest_lobby::Start { seed: 1, map: String::new(), difficulty: 0, players: vec![], unlocked: vec![], options: vec![] };
    pair.games[1].net_receive(crate::netplay::RELIABLE, IDS[0], last_harvest_lobby::encode(&last_harvest_lobby::Message::Start(start)));
    pair.games[1].update(&Input::default(), DT);
    let _ = pair.games[1].draw_at(0.5);
    assert!(!pair.games[1].in_run());
    assert!(pair.games[1].online.message.contains("seat"), "{}", pair.games[1].online.message);
}

#[test]
fn the_host_takes_a_clients_pause_and_can_close_the_room_from_it() {
    let mut pair = Pair::started();
    pair.idle(1.0);
    pair.press(1, Buttons::START);
    assert!(host_harvest(&mut pair).paused_for_test());
    pair.press(0, Buttons::START);
    for _ in 0..4 {
        pair.press(0, Buttons::DOWN);
    }
    pair.press(0, Buttons::A);
    pair.idle(1.0);
    assert!(!pair.games[0].in_run() && !pair.games[1].in_run(), "the host quit and the room closed");
}

#[test]
fn back_on_the_host_closes_the_room_and_back_on_a_client_leaves_it() {
    let mut pair = Pair::started();
    pair.idle(1.0);
    pair.press(0, Buttons::BACK);
    pair.idle(1.0);
    assert!(!pair.games[0].in_run() && !pair.games[1].in_run());
    assert_eq!(pair.games[1].online.message, "The host ended the night.");
    let mut pair = Pair::started();
    pair.idle(1.0);
    pair.press(1, Buttons::BACK);
    pair.idle(1.0);
    assert!(!pair.games[1].in_run() && pair.games[0].in_run());
    assert!(!host_harvest(&mut pair).alive_for_test(1), "the host unseats the client that left");
}

#[test]
fn a_client_picks_its_level_up_card_with_a_tap() {
    let mut pair = Pair::started();
    pair.idle(1.0);
    host_harvest(&mut pair).level_up_for_test();
    pair.idle(3.0);
    assert_eq!(host_harvest(&mut pair).seat_cards_for_test(1), Some((0, false)), "the client's cards are up, the first selected");
    let third = Input { touch: Some([512.0, 535.0]), touch_started: true, ..Input::default() };
    pair.step(Input::default(), third);
    pair.idle(0.3);
    assert_eq!(host_harvest(&mut pair).seat_cards_for_test(1), Some((2, true)), "a tap on the third card takes it");
}

#[test]
fn a_client_gone_quiet_lets_go_and_the_cards_stop_waiting_for_it() {
    let mut pair = Pair::started();
    pair.idle(1.0);
    for _ in 0..30 {
        pair.step(Input::default(), Input { held: Buttons::UP, ..Input::default() });
    }
    pair.cut[1] = true;
    pair.idle(1.0);
    let at = pair.games[0].player_spots()[1];
    pair.idle(1.0);
    assert_eq!(pair.games[0].player_spots()[1], at, "a client gone quiet stops walking");
    host_harvest(&mut pair).level_up_for_test();
    for t in 0..(8.0 / DT) as u32 {
        let x = if t % 30 == 0 { Buttons::X } else { Buttons::NONE };
        pair.step(Input { held: x, pressed: x, ..Input::default() }, Input::default());
    }
    assert_eq!(host_harvest(&mut pair).phase_for_test(), 0, "the night went on without the quiet client's pick");
    assert!(host_harvest(&mut pair).alive_for_test(1), "and it keeps its seat for a while");
}

#[test]
fn a_client_with_its_lid_shut_keeps_its_seat_and_a_silent_one_loses_it() {
    let mut pair = Pair::started();
    pair.idle(1.0);
    let shut = Input { lid_closed: true, ..Input::default() };
    for _ in 0..(12.0 / DT) as u32 {
        pair.step(Input::default(), shut);
    }
    assert!(host_harvest(&mut pair).alive_for_test(1), "acks keep a shut client seated");
    assert!(pair.games[1].in_run());
    pair.cut[1] = true;
    pair.all = true;
    pair.idle(11.0);
    assert!(!host_harvest(&mut pair).alive_for_test(1), "a client heard from for no while is unseated");
}

#[test]
fn a_client_with_other_rules_or_mods_is_refused() {
    let mut pair = Pair::new();
    pair.games[1].set_identity("", 77);
    pair.games[0].host_online(Some("ACDEF"));
    pair.wire();
    pair.games[1].join_online("ACDEF");
    pair.idle(0.5);
    assert!(pair.games[1].online.message.contains("game data differs"), "{}", pair.games[1].online.message);
}

#[test]
fn a_client_that_hears_nothing_from_the_host_leaves_the_night() {
    let mut pair = Pair::started();
    pair.idle(1.0);
    pair.cut[0] = true;
    pair.all = true;
    pair.idle(11.0);
    assert!(!pair.games[1].in_run());
    assert_eq!(pair.games[1].online.message, "Lost the connection to the host.");
}

#[test]
fn a_client_sees_the_drivers_ticket_as_the_host_has_it_and_the_cash_in_whoever_makes_it() {
    let mut pair = Pair::started();
    pair.idle(1.0);
    assert!(host_harvest(&mut pair).ticket_for_test().is_none(), "no ticket this early");
    host_harvest(&mut pair).ready_ticket_for_test();
    pair.idle(0.5);
    let driver = host_harvest(&mut pair).ticket_for_test();
    assert!(driver.is_some(), "the host's ticket is ready");
    let crate::State::Harvest(c) = &pair.games[1].state else { panic!() };
    assert_eq!(c.ticket_for_test(), driver, "the client draws the coin over the driver too");
    host_harvest(&mut pair).walk_to_driver_for_test(0);
    pair.idle(0.5);
    assert!(host_harvest(&mut pair).ticket_for_test().is_none(), "the host cashed in");
    let crate::State::Harvest(c) = &mut pair.games[1].state else { panic!() };
    assert!(c.ticket_for_test().is_none(), "and the client's coin is gone with it");
    let mut f = crate::Frame::default();
    c.draw(&mut f, false);
    let said = |s: &crate::Shape| matches!(s, crate::Shape::Text { text, .. } if text.contains("SOUL HARVEST"));
    assert!(f.top.shapes.iter().chain(&f.bottom.shapes).any(said), "the client hears of the cash-in");
}

#[test]
fn a_clients_end_screen_shows_the_hosts_tally_for_its_player() {
    let mut pair = Pair::started();
    pair.idle(15.0);
    let h = host_harvest(&mut pair);
    h.fall_for_test(0);
    h.fall_for_test(1);
    pair.idle(1.0);
    assert_eq!(host_harvest(&mut pair).phase_for_test(), 2, "the night is over");
    let want = host_harvest(&mut pair).end_numbers_for_test(1);
    assert!(want.len() > 7 && want[want.len() - 7] > 0.0, "the client's player held a weapon and dealt damage: {want:?}");
    let crate::State::Harvest(c) = &pair.games[1].state else { panic!() };
    let got = c.end_numbers_for_test(0);
    assert_eq!(got.len(), want.len(), "the same rows: {got:?} {want:?}");
    assert!(got.iter().zip(&want).all(|(g, w)| (g - w).abs() <= 0.5 + 0.01 * w.abs()), "the same numbers: {got:?} {want:?}");
}
