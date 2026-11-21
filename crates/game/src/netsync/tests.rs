use super::events::Event;
use super::input::{InputFrame, InputLog, InputMsg, InputQueue};
use super::link::{ClientLink, HostLink};
use super::mirror::{CATCH_UP, Mirror, RUN, SNAP};
use super::pipe::Pipe;
use super::snapshot::*;
use crate::rng::Rng;
use crate::sfx::Sfx;

/// A night in miniature: `n` zombies walking at the player, some drops and barrels.
struct Sim {
    rng: Rng,
    tick: u32,
    next_id: u32,
    zombies: Vec<([f32; 2], f32, u8, u32)>,
    drops: Vec<[f32; 2]>,
}

impl Sim {
    fn new(seed: u64, n: usize) -> Self {
        let mut s = Sim { rng: Rng::new(seed), tick: 0, next_id: 1, zombies: Vec::new(), drops: Vec::new() };
        for _ in 0..n {
            s.add();
        }
        for _ in 0..60 {
            let p = [s.rng.range(0.0, 2400.0), s.rng.range(0.0, 1800.0)];
            s.drops.push(p);
        }
        s
    }

    fn add(&mut self) {
        let pos = [self.rng.range(0.0, 2400.0), self.rng.range(0.0, 1800.0)];
        let speed = self.rng.range(18.0, 40.0);
        let kind = self.rng.below(8) as u8;
        self.zombies.push((pos, speed, kind, self.next_id));
        self.next_id += 1 + (self.rng.below(10) == 0) as u32;
    }

    /// One host tick: everyone walks toward the middle, a few die and rise.
    fn step(&mut self) {
        self.tick += 1;
        let dt = 1.0 / TICK_HZ as f32;
        for z in &mut self.zombies {
            let d = [1200.0 - z.0[0], 900.0 - z.0[1]];
            let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1.0);
            z.0 = [z.0[0] + d[0] / l * z.1 * dt, z.0[1] + d[1] / l * z.1 * dt];
        }
        if self.tick.is_multiple_of(20) && !self.zombies.is_empty() {
            let i = self.rng.below(self.zombies.len());
            self.zombies.remove(i);
            self.add();
        }
        if self.tick.is_multiple_of(45) {
            self.drops.pop();
            let p = [self.rng.range(0.0, 2400.0), self.rng.range(0.0, 1800.0)];
            self.drops.push(p);
        }
    }

    fn world(&self) -> World {
        let players = vec![PlayerState { pos: q_pos([1200.0, 900.0]), yaw: q_angle(1.0), hp: 400, max_hp: 400, speed: 60 * 16, kit: vec![(1, 3), (7, 2)], ..Default::default() }];
        let zombies = self.zombies.iter().map(|z| ZombieState { id: z.3, kind: z.2, pos: q_pos(z.0), hp: 255, flags: if z.3 % 50 == 0 { Z_LEADER } else { 0 } }).collect();
        let mut things: Vec<Thing> = self.drops.iter().map(|&p| Thing { kind: T_MONEY, pos: q_pos(p), data: [5, 0, 0], ..Default::default() }).collect();
        things.extend((0..8).map(|i| Thing { kind: T_BARREL, pos: q_pos([100.0 + i as f32 * 64.0, 300.0]), data: [255, 0, 0], ..Default::default() }));
        World::new(self.tick, players, zombies, things)
    }
}

fn decode_all(packets: &[Vec<u8>], base: Option<&World>) -> World {
    let parts: Vec<Part> = packets.iter().map(|p| decode(p, base).expect("decodes")).collect();
    World::assemble(&parts)
}

#[test]
fn a_whole_snapshot_reads_back() {
    let sim = Sim::new(1, 120);
    let w = sim.world();
    let packets = encode(&w, None, 1, &[], &[], BUDGET);
    assert_eq!(decode_all(&packets, None), w);
}

#[test]
fn a_delta_reads_back_over_its_baseline_and_is_smaller() {
    let mut sim = Sim::new(2, 300);
    let base = sim.world();
    for _ in 0..(SEND_EVERY * 4) {
        sim.step();
    }
    let w = sim.world();
    let full: usize = encode(&w, None, 1, &[], &[], BUDGET).iter().map(Vec::len).sum();
    let packets = encode(&w, Some(&base), 1, &[], &[], BUDGET);
    let delta: usize = packets.iter().map(Vec::len).sum();
    assert_eq!(decode_all(&packets, Some(&base)), w);
    assert!(delta < full, "delta {delta} vs full {full}");
    assert_eq!(decode(&packets[0], None), Err(DecodeError::NoBase));
}

#[test]
fn removed_zombies_and_things_are_gone() {
    let sim = Sim::new(3, 50);
    let base = sim.world();
    let mut w = base.clone();
    w.tick += 3;
    let gone: Vec<u32> = w.zombies.iter().step_by(7).map(|z| z.id).collect();
    w.zombies.retain(|z| !gone.contains(&z.id));
    let lost_thing = w.things.remove(4).0;
    let got = decode_all(&encode(&w, Some(&base), 1, &[], &[], BUDGET), Some(&base));
    assert_eq!(got, w);
    assert!(gone.iter().all(|&id| got.zombie(id).is_none()));
    assert!(got.things.iter().all(|t| t.0 != lost_thing));
}

#[test]
fn a_big_snapshot_splits_into_parts_that_each_decode_alone() {
    let mut sim = Sim::new(4, 600);
    let base = sim.world();
    for _ in 0..SEND_EVERY {
        sim.step();
    }
    let w = sim.world();
    for b in [None, Some(&base)] {
        let packets = encode(&w, b, 1, &[(1, Event::LevelDone)], &[], BUDGET);
        assert!(packets.len() > 1);
        assert!(packets.iter().all(|p| p.len() <= BUDGET), "{:?}", packets.iter().map(Vec::len).collect::<Vec<_>>());
        for p in &packets {
            let part = decode(p, b).expect("each part decodes alone");
            assert_eq!(part.players, w.players);
            assert!(part.zombies.iter().all(|z| w.zombie(z.id) == Some(z)));
        }
        assert_eq!(decode_all(&packets, b), w);
    }
}

#[test]
fn snapshot_sizes_at_300_and_600_zombies() {
    for n in [300, 600] {
        let mut sim = Sim::new(5, n);
        let mut history = Vec::new();
        for _ in 0..8 {
            history.push(sim.world());
            for _ in 0..SEND_EVERY {
                sim.step();
            }
        }
        let w = sim.world();
        let size = |b: Option<&World>| {
            let p = encode(&w, b, 1, &[], &[], BUDGET);
            (p.iter().map(Vec::len).sum::<usize>(), p.len())
        };
        let (full, full_n) = size(None);
        let (d1, d1_n) = size(history.last());
        let (d4, d4_n) = size(Some(&history[history.len() - 4]));
        println!("{n} zombies: full {full} B in {full_n} packets; delta vs 1 back {d1} B in {d1_n}; vs 4 back (~200 ms RTT) {d4} B in {d4_n}; at 20 Hz {} KB/s", d4 * 20 / 1024);
        assert!(d4 < full);
    }
}

#[test]
fn twenty_percent_loss_and_reordering_still_converge_and_events_come_once_in_order() {
    let mut sim = Sim::new(6, 300);
    let mut host = HostLink::default();
    let mut client = ClientLink::default();
    let mut mirror = Mirror::default();
    let mut down: Pipe<Vec<u8>> = Pipe::new(11, 0.2, 5, 4);
    let mut up: Pipe<Vec<u8>> = Pipe::new(12, 0.2, 5, 4);
    let mut made = Vec::new();
    let mut got = Vec::new();
    for _ in 0..1800 {
        sim.step();
        let t = sim.tick;
        if t.is_multiple_of(7) {
            let ev = if t.is_multiple_of(14) { Event::Kill { id: t, kind: 1, pos: [1, 2], by: 0 } } else { Event::LevelUp { level: (t % 200) as u8 } };
            host.push_event(t, ev);
            made.push(ev);
        }
        if t.is_multiple_of(5) {
            host.push_event(t, Event::Sound(Sfx::Shot));
        }
        if t.is_multiple_of(SEND_EVERY) {
            for p in host.snapshot(sim.world()) {
                down.send(t, p);
            }
        }
        for p in down.receive(t) {
            if let Some(part) = client.receive(&p) {
                mirror.apply(&part);
            }
        }
        got.extend(client.events().into_iter().filter(Event::critical));
        mirror.advance(1.0 / TICK_HZ as f32);
        if t.is_multiple_of(2) {
            let (s, e) = client.ack();
            up.send(t, InputMsg { ack_snapshot: s, ack_event: e, seats: vec![] }.encode());
        }
        for m in up.receive(t) {
            let m = InputMsg::decode(&m).expect("input decodes");
            host.ack(m.ack_snapshot, m.ack_event);
        }
    }
    (down.loss, up.loss) = (0.0, 0.0);
    for _ in 0..60 {
        let t = sim.tick + 1;
        sim.tick = t;
        if t.is_multiple_of(SEND_EVERY) {
            for p in host.snapshot(sim.world()) {
                down.send(t, p);
            }
        }
        for p in down.receive(t) {
            if let Some(part) = client.receive(&p) {
                mirror.apply(&part);
            }
        }
        got.extend(client.events().into_iter().filter(Event::critical));
        mirror.advance(1.0 / TICK_HZ as f32);
        let (s, e) = client.ack();
        up.send(t, InputMsg { ack_snapshot: s, ack_event: e, seats: vec![] }.encode());
        for m in up.receive(t) {
            let m = InputMsg::decode(&m).expect("input decodes");
            host.ack(m.ack_snapshot, m.ack_event);
        }
    }
    assert_eq!(got, made, "every critical event once, in order");
    let latest = client.latest().expect("a whole snapshot");
    let mut want = sim.world();
    want.tick = latest.tick;
    assert_eq!(latest, &want);
    assert_eq!(mirror.zombie_count(), want.zombies.len());
    let worst = want.zombies.iter().map(|z| {
        let p = mirror.zombie_pos(z.id).expect("drawn");
        let h = unq_pos(z.pos);
        ((p[0] - h[0]).powi(2) + (p[1] - h[1]).powi(2)).sqrt()
    });
    assert!(worst.fold(0.0, f32::max) < 0.1);
}

#[test]
fn a_zombie_behind_catches_up_at_a_capped_speed_and_snaps_when_far() {
    let z = |tick, x| {
        let w = World::new(tick, vec![], vec![ZombieState { id: 9, pos: q_pos([x, 100.0]), ..Default::default() }], vec![]);
        decode(&encode(&w, None, 1, &[], &[], BUDGET)[0], None).unwrap()
    };
    let dt = 1.0 / 60.0;
    let mut m = Mirror::default();
    m.apply(&z(100, 100.0));
    m.advance(0.0);
    assert_eq!(m.zombie_pos(9), Some([100.0, 100.0]));
    m.apply(&z(110, 130.0));
    let mut before = 100.0;
    let mut lagged = false;
    for f in 0..60u32 {
        if f % 10 == 9 {
            m.apply(&z(111 + f, 130.0));
        }
        m.advance(dt);
        let x = m.zombie_pos(9).unwrap()[0];
        assert!(x - before <= (CATCH_UP + RUN) * dt + 1e-3, "moved {} in a frame", x - before);
        lagged |= x < 130.0 && x - before > 0.0;
        before = x;
    }
    assert!(lagged);
    assert_eq!(m.zombie_pos(9), Some([130.0, 100.0]));
    for f in 0..40 {
        m.apply(&z(180 + f, 130.0 + SNAP * 2.0));
        m.advance(dt);
    }
    assert_eq!(m.zombie_pos(9), Some([130.0 + SNAP * 2.0, 100.0]));
}

#[test]
fn the_host_plays_a_clients_input_once_each_tick_despite_loss() {
    let mut log = InputLog::default();
    let mut q = InputQueue::default();
    let mut pipe: Pipe<Vec<u8>> = Pipe::new(3, 0.2, 4, 3);
    let mut played = Vec::new();
    for t in 1..=600u32 {
        log.push(t, InputFrame { held: t, ..Default::default() });
        if t.is_multiple_of(super::input::SEND_EVERY) {
            pipe.send(t, InputMsg { ack_snapshot: None, ack_event: 0, seats: vec![log.seat(1)] }.encode());
        }
        for m in pipe.receive(t) {
            q.receive(&InputMsg::decode(&m).unwrap().seats[0]);
        }
        let (_, at) = q.play();
        if let Some(a) = at.filter(|a| played.last() != Some(a)) {
            played.push(a);
        }
    }
    assert!(played.windows(2).all(|w| w[1] > w[0]));
    assert!(played.len() > 560, "played {}", played.len());
}
