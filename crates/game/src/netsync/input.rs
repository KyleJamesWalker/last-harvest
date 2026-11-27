//! A client's input message: acks, and each local player's last few ticks of input, repeated against loss.

use std::collections::BTreeMap;

use super::wire::{Reader, Writer};
use crate::{Buttons, Input};

/// Ticks of input each message repeats.
pub const HISTORY: usize = 8;
/// Client ticks between input messages.
pub const SEND_EVERY: u32 = 2;
/// Ticks of input the host lets queue up before skipping ahead.
const MOST_QUEUED: usize = 6;
/// Ticks without new input after which a remote player's held buttons and stick let go.
pub const STALE_TICKS: u32 = super::snapshot::TICK_HZ / 2;
/// Ticks without new input after which the host stops waiting on a remote player, such as for its level-up pick.
pub const AWAY_TICKS: u32 = 5 * super::snapshot::TICK_HZ;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputFrame {
    pub held: u32,
    pub pressed: u32,
    /// Stick and look in 1/127.
    pub stick: [i8; 2],
    pub look: i8,
    /// Mouse-look and its vertical movement in 1/YAW_STEPS of a radian.
    pub yaw: i16,
    pub pitch: i16,
}

const YAW_STEPS: f32 = 2048.0;

fn steps(v: f32) -> i16 {
    (v * YAW_STEPS).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

fn q(v: f32) -> i8 {
    (v.clamp(-1.0, 1.0) * 127.0).round() as i8
}

impl InputFrame {
    pub fn of(i: &Input) -> Self {
        Self { held: i.held.0, pressed: i.pressed.0, stick: i.stick.map(q), look: q(i.look), yaw: steps(i.yaw), pitch: steps(i.pitch) }
    }

    pub fn input(&self) -> Input {
        Input { held: Buttons(self.held), pressed: Buttons(self.pressed), stick: self.stick.map(|v| v as f32 / 127.0), look: self.look as f32 / 127.0, yaw: self.yaw as f32 / YAW_STEPS, pitch: self.pitch as f32 / YAW_STEPS, ..Input::default() }
    }

    fn write(&self, w: &mut Writer) {
        w.var(self.held);
        w.var(self.pressed);
        w.i8(self.stick[0]);
        w.i8(self.stick[1]);
        w.i8(self.look);
        w.i16(self.yaw);
        w.i16(self.pitch);
    }

    fn read(r: &mut Reader) -> Option<Self> {
        Some(Self { held: r.var()?, pressed: r.var()?, stick: [r.i8()?, r.i8()?], look: r.i8()?, yaw: r.i16()?, pitch: r.i16()? })
    }
}

/// One local player's latest inputs, the newest at `last` and each before it a tick earlier.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeatInputs {
    pub seat: u8,
    pub last: u32,
    pub frames: Vec<InputFrame>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputMsg {
    pub ack_snapshot: Option<u32>,
    pub ack_event: u32,
    pub seats: Vec<SeatInputs>,
}

impl InputMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(super::snapshot::VERSION);
        w.var(self.ack_snapshot.and_then(|t| t.checked_add(1)).unwrap_or(0));
        w.var(self.ack_event);
        w.u8(self.seats.len() as u8);
        for s in &self.seats {
            w.u8(s.seat);
            w.var(s.last);
            w.u8(s.frames.len() as u8);
            s.frames.iter().for_each(|f| f.write(&mut w));
        }
        w.buf
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut r = Reader::new(bytes);
        if r.u8()? != super::snapshot::VERSION {
            return None;
        }
        let ack_snapshot = r.var()?.checked_sub(1);
        let ack_event = r.var()?;
        let mut seats = Vec::new();
        for _ in 0..r.u8()? {
            let (seat, last) = (r.u8()?, r.var()?);
            let frames = (0..r.u8()?).map(|_| InputFrame::read(&mut r)).collect::<Option<Vec<_>>>()?;
            seats.push(SeatInputs { seat, last, frames });
        }
        Some(Self { ack_snapshot, ack_event, seats })
    }
}

/// The client's record of one local player's input, to send.
#[derive(Default)]
pub struct InputLog {
    frames: Vec<(u32, InputFrame)>,
}

impl InputLog {
    pub fn push(&mut self, tick: u32, frame: InputFrame) {
        self.frames.push((tick, frame));
        if self.frames.len() > HISTORY {
            self.frames.remove(0);
        }
    }

    pub fn seat(&self, seat: u8) -> SeatInputs {
        SeatInputs { seat, last: self.frames.last().map_or(0, |f| f.0), frames: self.frames.iter().map(|f| f.1).collect() }
    }
}

/// The host's queue of one remote player's input: each tick played once, in order.
#[derive(Default)]
pub struct InputQueue {
    queued: BTreeMap<u32, InputFrame>,
    played: Option<u32>,
    last: InputFrame,
    /// Presses and mouse-look from ticks skipped to catch up, played with the next tick.
    skipped: u32,
    skipped_yaw: f32,
    skipped_pitch: f32,
    quiet: u32,
}

impl InputQueue {
    pub fn receive(&mut self, s: &SeatInputs) {
        let n = s.frames.len() as u64;
        for (i, f) in s.frames.iter().enumerate() {
            let Some(tick) = (s.last as u64 + 1 + i as u64).checked_sub(n).and_then(|t| u32::try_from(t).ok()) else { continue };
            if self.played.is_none_or(|p| tick > p) && self.queued.insert(tick, *f).is_none() {
                self.quiet = 0;
            }
        }
        while self.queued.len() > MOST_QUEUED {
            let Some((t, f)) = self.queued.pop_first() else { break };
            self.played = Some(t);
            self.skipped |= f.pressed;
            self.skipped_yaw += f.input().yaw;
            self.skipped_pitch += f.input().pitch;
            self.last = f;
        }
    }

    /// The input for the host's next tick, and the last client tick played. With nothing new the last input is
    /// held, without its presses or mouse-look.
    pub fn play(&mut self) -> (Input, Option<u32>) {
        match self.queued.pop_first() {
            Some((t, f)) => {
                self.quiet = 0;
                self.played = Some(t);
                self.last = f;
                let mut i = f.input();
                i.pressed = Buttons(f.pressed | std::mem::take(&mut self.skipped));
                i.yaw += std::mem::take(&mut self.skipped_yaw);
                i.pitch += std::mem::take(&mut self.skipped_pitch);
                (i, self.played)
            }
            None => {
                self.quiet = self.quiet.saturating_add(1);
                let mut i = if self.quiet > STALE_TICKS { Input::default() } else { self.last.input() };
                i.pressed = Buttons(std::mem::take(&mut self.skipped));
                i.yaw = std::mem::take(&mut self.skipped_yaw);
                i.pitch = std::mem::take(&mut self.skipped_pitch);
                (i, self.played)
            }
        }
    }

    pub fn quiet(&self) -> u32 {
        self.quiet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_reads_back() {
        let mut log = InputLog::default();
        for t in 1..=10 {
            log.push(t, InputFrame { held: t, pressed: (t == 9) as u32 * Buttons::B.0, stick: [-127, 64], look: 5, yaw: -300, pitch: 77 });
        }
        let msg = InputMsg { ack_snapshot: Some(0), ack_event: 4, seats: vec![log.seat(1)] };
        assert_eq!(msg.seats[0].frames.len(), HISTORY);
        assert_eq!(InputMsg::decode(&msg.encode()), Some(msg));
    }

    #[test]
    fn repeated_ticks_play_once_in_order_and_a_lost_message_is_covered() {
        let mut log = InputLog::default();
        let mut host = InputQueue::default();
        let mut played = Vec::new();
        for t in 1..=40u32 {
            log.push(t, InputFrame { held: t, ..Default::default() });
            if t % 2 == 0 && t % 6 != 0 {
                host.receive(&log.seat(0));
            }
            if t > 3 {
                let (i, at) = host.play();
                if at.is_some_and(|a| played.last() != Some(&a)) {
                    played.push(at.unwrap());
                    assert_eq!(i.held.0, at.unwrap());
                }
            }
        }
        let want: Vec<u32> = (1..=played.len() as u32).collect();
        assert_eq!(played, want);
    }

    #[test]
    fn mouse_look_plays_once_and_never_repeats_while_quiet() {
        let mut log = InputLog::default();
        let mut host = InputQueue::default();
        log.push(1, InputFrame { yaw: 512, ..Default::default() });
        host.receive(&log.seat(0));
        assert_eq!(host.play().0.yaw, 0.25);
        assert_eq!(host.play().0.yaw, 0.0, "a quiet tick holds buttons, not the turn");
        for t in 2..=HISTORY as u32 + 1 {
            log.push(t, InputFrame { yaw: 64, ..Default::default() });
        }
        host.receive(&log.seat(0));
        let turned: f32 = (0..HISTORY + 2).map(|_| host.play().0.yaw).sum();
        assert_eq!(turned, HISTORY as f32 * 64.0 / YAW_STEPS, "ticks skipped to catch up still turn");
    }
}
