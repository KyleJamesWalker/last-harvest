//! Each end of one host-client connection: what has been sent and acked, and what has arrived.

use std::collections::{BTreeMap, VecDeque};

use super::events::{Event, Inbox, Outbox};
use super::snapshot::{self, BUDGET, Part, Spawned, World};

/// Snapshots the host keeps to code against; an ack older than these gets a whole snapshot.
pub const HISTORY: usize = 32;
/// Complete snapshots the client keeps as baselines, more than the host will name.
const KEPT: usize = HISTORY + 8;
/// Ticks a spawn is resent for while the client hasn't acked a snapshot after it.
pub const SPAWN_TTL: u32 = 30;

/// The host's side of one client.
#[derive(Default)]
pub struct HostLink {
    sent: VecDeque<World>,
    acked: Option<u32>,
    pub outbox: Outbox,
    spawns: VecDeque<Spawned>,
    pub bytes: u64,
    pub packets: u64,
}

impl HostLink {
    pub fn push_event(&mut self, tick: u32, ev: Event) {
        self.outbox.push(tick, ev);
    }

    pub fn push_spawn(&mut self, s: Spawned) {
        self.spawns.push_back(s);
    }

    /// What the client says it has: the newest whole snapshot and the last event in order.
    pub fn ack(&mut self, snapshot: Option<u32>, event: u32) {
        if let Some(t) = snapshot {
            self.acked = Some(self.acked.map_or(t, |a| a.max(t)));
        }
        self.outbox.ack(event);
    }

    /// The packets for `world`, coded against the newest snapshot the client acked that's still kept.
    pub fn snapshot(&mut self, world: World) -> Vec<Vec<u8>> {
        let now = world.tick;
        self.outbox.expire(now);
        let base = self.acked.and_then(|t| self.sent.iter().find(|w| w.tick == t));
        let since = base.map_or(0, |b| b.tick);
        while self.spawns.front().is_some_and(|s| s.tick <= since || now.saturating_sub(s.tick) > SPAWN_TTL) {
            self.spawns.pop_front();
        }
        let spawns: Vec<Spawned> = self.spawns.iter().cloned().collect();
        let events: Vec<(u32, Event)> = self.outbox.pending(now).collect();
        let packets = snapshot::encode(&world, base, self.outbox.first(), &events, &spawns, BUDGET);
        self.bytes += packets.iter().map(|p| p.len() as u64).sum::<u64>();
        self.packets += packets.len() as u64;
        self.sent.push_back(world);
        while self.sent.len() > HISTORY {
            self.sent.pop_front();
        }
        packets
    }
}

/// The client's side: baselines, snapshots coming together part by part, and events in order.
#[derive(Default)]
pub struct ClientLink {
    complete: VecDeque<World>,
    building: BTreeMap<u32, Vec<Part>>,
    pub inbox: Inbox,
    seen: VecDeque<u32>,
    pub undecodable: u64,
}

impl ClientLink {
    /// Decodes a packet into the part it carries, or None when it can't be read or its baseline is gone.
    pub fn receive(&mut self, bytes: &[u8]) -> Option<Part> {
        let (tick, base) = snapshot::peek(bytes)?;
        let base_world = base.and_then(|t| self.complete.iter().find(|w| w.tick == t));
        let Ok(part) = snapshot::decode(bytes, base_world) else {
            self.undecodable += 1;
            return None;
        };
        if let Some((first, events)) = &part.events {
            self.inbox.receive(*first, events);
        }
        let newest = self.complete.back().map(|w| w.tick);
        if newest.is_none_or(|n| tick > n) {
            let parts = self.building.entry(tick).or_default();
            if !parts.iter().any(|p| p.index == part.index) {
                parts.push(part.clone());
            }
            if parts.len() == part.count as usize {
                let parts = self.building.remove(&tick).unwrap_or_default();
                self.complete.push_back(World::assemble(&parts));
                while self.complete.len() > KEPT {
                    self.complete.pop_front();
                }
                self.building.retain(|&t, _| t > tick);
            }
        }
        Some(part)
    }

    /// Spawns not seen before.
    pub fn fresh(&mut self, spawns: &[Spawned]) -> Vec<Spawned> {
        let mut out = Vec::new();
        for s in spawns {
            if !self.seen.contains(&s.id) {
                self.seen.push_back(s.id);
                out.push(s.clone());
            }
        }
        while self.seen.len() > 1024 {
            self.seen.pop_front();
        }
        out
    }

    /// The newest whole snapshot and the last event delivered in order.
    pub fn ack(&self) -> (Option<u32>, u32) {
        (self.complete.back().map(|w| w.tick), self.inbox.acked())
    }

    pub fn latest(&self) -> Option<&World> {
        self.complete.back()
    }

    pub fn events(&mut self) -> Vec<Event> {
        self.inbox.drain()
    }
}
