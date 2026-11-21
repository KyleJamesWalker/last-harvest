//! Several players in one run.

use super::map::{MapSource, TILE};
use super::*;

fn open_pair(gap: f32) -> Harvest {
    let mut h = Harvest::calm_on(3, &MapSource::new("Open", [40, 40])).unwrap().with_players(2);
    h.birds.clear();
    h.players[0].pos = [20.5 * TILE - gap / 2.0, 20.5 * TILE];
    h.players[1].pos = [20.5 * TILE + gap / 2.0, 20.5 * TILE];
    for p in &mut h.players {
        p.kit.weapons.clear();
    }
    h
}

fn step(h: &mut Harvest, secs: f32) {
    let mut sounds = Vec::new();
    for _ in 0..(secs * 60.0) as u32 {
        h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
    }
}

#[test]
fn a_zombie_between_two_players_goes_for_the_nearer() {
    for (off, toward) in [(-30.0, 0), (30.0, 1)] {
        let mut h = open_pair(200.0);
        let mut z = Zombie::new(Kind::common(), [20.5 * TILE + off, 20.5 * TILE], 1000.0, 1.0, 0.0);
        z.id = 1;
        h.zombies.push(z);
        let start = len(sub(h.zombies[0].pos, h.players[toward].pos));
        step(&mut h, 1.0);
        let now = len(sub(h.zombies[0].pos, h.players[toward].pos));
        assert!(now < start - 10.0, "player {toward}: {start} -> {now}");
    }
}

#[test]
fn a_players_blast_hurts_them_but_never_another_player() {
    let mut h = open_pair(0.0);
    h.players[1].pos[0] += 2.0;
    let at = h.players[ME].pos;
    let hp = h.players[ME].hp;
    h.thrown.push(Grenade { from: at, to: at, t: crate::tuning::tuning().grenade_flight, kind: Thrown::Grenade, by: None, owner: 1 });
    step(&mut h, 0.1);
    assert_eq!(h.players[ME].hp, hp, "the local player stands in another's blast unhurt");
    assert!(h.players[1].hp < h.players[1].kit.max_hp(), "the thrower beside it is hurt by their own");
}

#[test]
fn a_grenade_bursts_into_its_throwers_bomblets() {
    for (owner, bomblets) in [(ME, 0), (1, 3)] {
        let mut h = open_pair(200.0);
        h.players[1].kit.mods.cluster = 3.0;
        let to = [20.5 * TILE, 10.5 * TILE];
        h.thrown.push(Grenade { from: to, to, t: crate::tuning::tuning().grenade_flight, kind: Thrown::Grenade, by: None, owner });
        step(&mut h, 1.0 / 60.0);
        let thrown: Vec<&Grenade> = h.thrown.iter().filter(|g| g.kind == Thrown::Bomblet).collect();
        assert_eq!(thrown.len(), bomblets, "player {owner}'s grenade");
        assert!(thrown.iter().all(|g| g.owner == owner));
    }
}

#[test]
fn the_run_goes_on_while_anyone_stands_and_ends_once_all_have_turned() {
    let mut h = open_pair(200.0);
    h.players[1].hp = 0.0;
    step(&mut h, 1.0 / 60.0);
    assert!(!h.players[1].alive() && h.players[0].alive());
    assert!(matches!(h.phase, Phase::Playing), "one player left standing plays on");
    step(&mut h, TURNED + 2.0);
    assert!(h.players[1].lead.is_some(), "the turned player leads the dead");
    h.players[0].hp = 0.0;
    step(&mut h, 1.0 / 60.0);
    assert!(matches!(h.phase, Phase::Over { won: false, .. }), "the last to fall ends it");
}

#[test]
fn drops_go_to_whoever_picks_them_up_and_xp_is_shared() {
    let mut h = open_pair(200.0);
    let at = h.players[1].pos;
    h.drops.push(Drop { kind: DropKind::Money(3), pos: at, z: 0.0, vz: 0.0, vel: [0.0; 2], age: 0.0, pulled: None, to: ME });
    let xp = h.xp;
    step(&mut h, 0.5);
    assert!(h.drops.is_empty());
    assert_eq!((h.juice.tally.book(1).hearts, h.juice.tally.book(ME).hearts), (1, 0));
    assert!(h.xp > xp, "the heart's XP goes to the shared pool");
}

#[test]
fn two_scripted_players_play_a_night_beside_the_local_one_without_trouble() {
    let mut h = Harvest::new(9).with_bots(2);
    let mut sounds = Vec::new();
    let mut levels = 0;
    for _ in 0..(150 * 60) {
        h.update(&Input { held: Buttons::UP, look: 0.2, ..Input::default() }, 1.0 / 60.0, &mut sounds);
        if let Phase::LevelUp { picked: None, t, .. } = h.phase && t > 2.0 {
            h.update(&Input { pressed: Buttons::X, ..Input::default() }, 1.0 / 60.0, &mut sounds);
            levels += 1;
        }
        sounds.clear();
    }
    assert_eq!(h.players.len(), 3);
    assert!(h.players[1].pos != h.players[2].pos, "each walks their own way");
    assert!(h.kills > 0 && (1..3).any(|p| h.juice.tally.sum(p).0 > 0), "the scripted players kill too");
    assert!(levels > 0, "level-ups came and went");
}
