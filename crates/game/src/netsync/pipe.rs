//! An in-memory network link for tests and the sandbox: it loses, delays and reorders what it carries.

use crate::rng::Rng;

pub struct Pipe<T> {
    rng: Rng,
    /// Share of sends lost, 0 to 1.
    pub loss: f32,
    /// Ticks every send takes, plus up to `jitter` more, which reorders sends.
    pub latency: u32,
    pub jitter: u32,
    flying: Vec<(u32, u64, T)>,
    sent: u64,
    pub lost: u64,
}

impl<T> Pipe<T> {
    pub fn new(seed: u64, loss: f32, latency: u32, jitter: u32) -> Self {
        Self {
            rng: Rng::new(seed),
            loss,
            latency,
            jitter,
            flying: Vec::new(),
            sent: 0,
            lost: 0,
        }
    }

    pub fn send(&mut self, now: u32, item: T) {
        self.sent += 1;
        if self.rng.range(0.0, 1.0) < self.loss {
            self.lost += 1;
            return;
        }
        let late = if self.jitter > 0 {
            self.rng.below(self.jitter as usize + 1) as u32
        } else {
            0
        };
        self.flying
            .push((now + self.latency + late, self.sent, item));
    }

    /// Everything due by `now`, in the order it lands.
    pub fn receive(&mut self, now: u32) -> Vec<T> {
        let mut due = Vec::new();
        let mut i = 0;
        while i < self.flying.len() {
            if self.flying[i].0 <= now {
                due.push(self.flying.swap_remove(i));
            } else {
                i += 1;
            }
        }
        due.sort_by_key(|d| (d.0, d.1));
        due.into_iter().map(|d| d.2).collect()
    }
}
