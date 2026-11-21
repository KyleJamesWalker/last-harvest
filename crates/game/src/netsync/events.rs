//! One-off happenings sent over the unreliable channel until acked, and delivered in order once each.

use std::collections::{BTreeMap, VecDeque};

use super::wire::{Reader, Writer};
use crate::sfx::Sfx;

/// Ticks a sound cue is sent for.
pub const SOUND_TTL: u32 = 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Sound(Sfx),
    /// A zombie killed, where it fell, by which player.
    Kill { id: u32, kind: u8, pos: [u16; 2], by: u8 },
    /// The level-up cards appear for this level, and close again.
    LevelUp { level: u8 },
    LevelDone,
    RunEnd { won: bool },
}

impl Event {
    /// Never expires.
    pub fn critical(&self) -> bool {
        !matches!(self, Event::Sound(_))
    }

    pub fn write(&self, w: &mut Writer) {
        match *self {
            Event::Sound(s) => {
                w.u8(0);
                write_sfx(s, w);
            }
            Event::Kill { id, kind, pos, by } => {
                w.u8(1);
                w.var(id);
                w.u8(kind);
                w.u16(pos[0]);
                w.u16(pos[1]);
                w.u8(by);
            }
            Event::LevelUp { level } => {
                w.u8(2);
                w.u8(level);
            }
            Event::LevelDone => w.u8(3),
            Event::RunEnd { won } => {
                w.u8(4);
                w.u8(won as u8);
            }
        }
    }

    pub fn read(r: &mut Reader) -> Option<Self> {
        Some(match r.u8()? {
            0 => Event::Sound(read_sfx(r)?),
            1 => Event::Kill { id: r.var()?, kind: r.u8()?, pos: [r.u16()?, r.u16()?], by: r.u8()? },
            2 => Event::LevelUp { level: r.u8()? },
            3 => Event::LevelDone,
            4 => Event::RunEnd { won: r.u8()? != 0 },
            _ => return None,
        })
    }
}

fn write_sfx(s: Sfx, w: &mut Writer) {
    let placed = |w: &mut Writer, tag: u8, pan: i8, gain: u8| {
        w.u8(tag);
        w.i8(pan);
        w.u8(gain);
    };
    match s {
        Sfx::Tick(v) => {
            w.u8(0);
            w.u8(v);
        }
        Sfx::Left => w.u8(1),
        Sfx::Right => w.u8(2),
        Sfx::Start => w.u8(3),
        Sfx::Miss => w.u8(4),
        Sfx::GameOver => w.u8(5),
        Sfx::Turn => w.u8(6),
        Sfx::Shot => w.u8(7),
        Sfx::Blast => w.u8(8),
        Sfx::Slash => w.u8(9),
        Sfx::Thud(c) => {
            w.u8(10);
            w.u8(c as u8);
        }
        Sfx::Kill(k) => {
            w.u8(11);
            w.u8(k);
        }
        Sfx::Boom { pan, gain } => placed(w, 12, pan, gain),
        Sfx::Throw => w.u8(13),
        Sfx::Pickup(v) => {
            w.u8(14);
            w.u8(v);
        }
        Sfx::Heal => w.u8(15),
        Sfx::Hurt => w.u8(16),
        Sfx::Heartbeat => w.u8(17),
        Sfx::LevelUp => w.u8(18),
        Sfx::Pick(r) => {
            w.u8(19);
            w.u8(r as u8);
        }
        Sfx::Rise { pan, gain } => placed(w, 20, pan, gain),
        Sfx::Groan { pan, gain } => placed(w, 21, pan, gain),
        Sfx::Caw { pan, gain } => placed(w, 22, pan, gain),
        Sfx::Spit { pan, gain } => placed(w, 23, pan, gain),
        Sfx::Splat { pan, gain } => placed(w, 24, pan, gain),
        Sfx::Zap => w.u8(25),
        Sfx::Bark => w.u8(26),
        Sfx::Bell => w.u8(27),
        Sfx::Horde => w.u8(28),
        Sfx::Dawn => w.u8(29),
        Sfx::Group { group, or } => {
            w.u8(30);
            w.u8(group);
            w.u8(or);
        }
    }
}

fn read_sfx(r: &mut Reader) -> Option<Sfx> {
    Some(match r.u8()? {
        0 => Sfx::Tick(r.u8()?),
        1 => Sfx::Left,
        2 => Sfx::Right,
        3 => Sfx::Start,
        4 => Sfx::Miss,
        5 => Sfx::GameOver,
        6 => Sfx::Turn,
        7 => Sfx::Shot,
        8 => Sfx::Blast,
        9 => Sfx::Slash,
        10 => Sfx::Thud(r.u8()? != 0),
        11 => Sfx::Kill(r.u8()?),
        12 => Sfx::Boom { pan: r.i8()?, gain: r.u8()? },
        13 => Sfx::Throw,
        14 => Sfx::Pickup(r.u8()?),
        15 => Sfx::Heal,
        16 => Sfx::Hurt,
        17 => Sfx::Heartbeat,
        18 => Sfx::LevelUp,
        19 => Sfx::Pick(r.u8()? != 0),
        20 => Sfx::Rise { pan: r.i8()?, gain: r.u8()? },
        21 => Sfx::Groan { pan: r.i8()?, gain: r.u8()? },
        22 => Sfx::Caw { pan: r.i8()?, gain: r.u8()? },
        23 => Sfx::Spit { pan: r.i8()?, gain: r.u8()? },
        24 => Sfx::Splat { pan: r.i8()?, gain: r.u8()? },
        25 => Sfx::Zap,
        26 => Sfx::Bark,
        27 => Sfx::Bell,
        28 => Sfx::Horde,
        29 => Sfx::Dawn,
        30 => Sfx::Group { group: r.u8()?, or: r.u8()? },
        _ => return None,
    })
}

/// The host's events for one client, numbered from 1, until the client acks them.
#[derive(Default)]
pub struct Outbox {
    next: u32,
    queue: VecDeque<(u32, u32, Event)>,
}

impl Outbox {
    pub fn push(&mut self, tick: u32, ev: Event) {
        self.next += 1;
        self.queue.push_back((self.next, tick, ev));
    }

    /// The client has every event up to `seq`.
    pub fn ack(&mut self, seq: u32) {
        while self.queue.front().is_some_and(|e| e.0 <= seq) {
            self.queue.pop_front();
        }
    }

    /// Drops stale sound cues from the front; a client still missing one skips past it once `first` moves on.
    pub fn expire(&mut self, now: u32) {
        while self.queue.front().is_some_and(|&(_, t, e)| !e.critical() && now.saturating_sub(t) > SOUND_TTL) {
            self.queue.pop_front();
        }
    }

    /// The oldest event still owed.
    pub fn first(&self) -> u32 {
        self.queue.front().map_or(self.next + 1, |e| e.0)
    }

    pub fn pending(&self, now: u32) -> impl Iterator<Item = (u32, Event)> + '_ {
        self.queue.iter().filter(move |&&(_, t, e)| e.critical() || now.saturating_sub(t) <= SOUND_TTL).map(|&(s, _, e)| (s, e))
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

/// The client's side: each event comes out once, in the order the host made them.
pub struct Inbox {
    next: u32,
    held: BTreeMap<u32, Event>,
}

impl Default for Inbox {
    fn default() -> Self {
        Self { next: 1, held: BTreeMap::new() }
    }
}

impl Inbox {
    /// Takes a packet's events; `first` is the host's oldest event still owed.
    pub fn receive(&mut self, first: u32, events: &[(u32, Event)]) {
        if first > self.next {
            self.held = self.held.split_off(&first);
            self.next = first;
        }
        for &(seq, ev) in events {
            if seq >= self.next {
                self.held.insert(seq, ev);
            }
        }
    }

    /// Events ready to act on, in order.
    pub fn drain(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Some(ev) = self.held.remove(&self.next) {
            out.push(ev);
            self.next += 1;
        }
        out
    }

    /// The last event delivered in order, which the client acks.
    pub fn acked(&self) -> u32 {
        self.next - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_reads_back() {
        let all = [
            Event::Sound(Sfx::Tick(3)),
            Event::Sound(Sfx::Boom { pan: -40, gain: 200 }),
            Event::Sound(Sfx::Group { group: 2, or: 255 }),
            Event::Sound(Sfx::Thud(true)),
            Event::Kill { id: 70_000, kind: 4, pos: [123, 4567], by: 1 },
            Event::LevelUp { level: 9 },
            Event::LevelDone,
            Event::RunEnd { won: true },
        ];
        let mut w = Writer::new();
        all.iter().for_each(|e| e.write(&mut w));
        let mut r = Reader::new(&w.buf);
        for e in all {
            assert_eq!(Event::read(&mut r), Some(e));
        }
        assert!(r.done());
    }

    #[test]
    fn expired_sounds_are_skipped_and_the_rest_come_once_in_order() {
        let mut out = Outbox::default();
        out.push(0, Event::Sound(Sfx::Shot));
        out.push(0, Event::LevelUp { level: 2 });
        out.push(30, Event::Sound(Sfx::Hurt));
        let mut inbox = Inbox::default();
        let late: Vec<_> = out.pending(30).collect();
        assert_eq!(late.iter().map(|e| e.0).collect::<Vec<_>>(), vec![2, 3]);
        inbox.receive(out.first(), &late[1..]);
        assert!(inbox.drain().is_empty());
        out.expire(30);
        inbox.receive(out.first(), &late);
        assert_eq!(inbox.drain(), vec![Event::LevelUp { level: 2 }, Event::Sound(Sfx::Hurt)]);
        inbox.receive(out.first(), &late);
        assert!(inbox.drain().is_empty());
        out.ack(inbox.acked());
        assert!(out.is_empty());
    }
}
