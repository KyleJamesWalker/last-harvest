//! Watching a recorded night: the saved replays to choose from, and one played back on the run's own screens with
//! pause, speed and whose view to follow. Nothing here touches the save.

use crate::draw::{Align, Canvas, Color};
use crate::harvest::replay::{Device, Kind, Replay, Viewer};
use crate::input::{Buttons, Input};
use crate::{DIM, HOT, SCREEN_W, Sfx, WHITE, pad};

pub(crate) const SPEEDS: [f32; 6] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0];
const NORMAL: usize = 2;
/// Faster than this, a replay plays without its sounds.
const LOUDEST: f32 = 2.0;
const RED: Color = [1.0, 0.45, 0.4, 1.0];
/// The strip of controls: its height, then each button's width and the gap between them.
pub(crate) const STRIP_H: f32 = 128.0;
const BUTTON_W: f32 = 152.0;
const GAP: f32 = 12.0;

/// A replay the platform kept, as read for the list: its file name, its bytes, and a line saying what night it was or
/// why it can't be watched.
pub(crate) struct Saved {
    pub name: String,
    raw: Vec<u8>,
    pub line: Result<String, String>,
}

impl Saved {
    pub fn new(name: String, raw: Vec<u8>) -> Self {
        let line = Replay::read(&raw).map(|r| describe(&r, &name));
        Self { name, raw, line }
    }

    /// The replay, ready to watch, and a word on anything that may keep it from playing exactly here.
    pub fn open(&self) -> Result<(Replay, Option<String>), String> {
        let r = Replay::read(&self.raw)?;
        let here = crate::data::fingerprint().map_or_else(|| "built-in".to_string(), |d| format!("{d:016x}"));
        let warn = r.info("data").filter(|d| *d != here).map(|_| "Recorded with other game data or mods: it may drift".to_string());
        Ok((r, warn))
    }
}

/// One line for the list: when, the difficulty, how long, the map and who played.
fn describe(r: &Replay, name: &str) -> String {
    let s = &r.setup;
    let secs = r.ticks / 60;
    let map = crate::data::maps().iter().find(|m| m.id == s.map).map_or("Random farm", |m| m.name.as_str());
    let who = match &s.kind {
        Kind::Host { .. } => format!("online, {}", plural(s.players, "player")),
        _ if s.players + s.bots > 1 => plural(s.players + s.bots, "player"),
        _ => crate::player_looks().iter().find(|l| l.id == s.look).map_or_else(|| "Farmer".to_string(), |l| l.name.clone()),
    };
    let when = when(name).unwrap_or_else(|| name.to_string());
    format!("{when}   {}   {}:{:02}   {map}   {who}", s.difficulty.name(), secs / 60, secs % 60)
}

fn plural(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

/// A replay file's time, from its `night-<unix seconds>-<n>` name, in UTC.
pub(crate) fn when(name: &str) -> Option<String> {
    let unix: i64 = name.strip_prefix("night-")?.split('-').next()?.parse().ok()?;
    let (days, secs) = (unix.div_euclid(86_400), unix.rem_euclid(86_400));
    let z = days + 719_468;
    let (era, doe) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + (month <= 2) as i64;
    Some(format!("{year}-{month:02}-{day:02} {:02}:{:02}", secs / 3600, secs % 3600 / 60))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Leave,
    Slower,
    Play,
    Faster,
    Player,
    Photo,
}

const CONTROLS: [Control; 6] = [Control::Leave, Control::Slower, Control::Play, Control::Faster, Control::Player, Control::Photo];

fn button(i: usize, top: f32) -> [f32; 4] {
    let x0 = (SCREEN_W - (CONTROLS.len() as f32 * (BUTTON_W + GAP) - GAP)) / 2.0;
    [x0 + i as f32 * (BUTTON_W + GAP), top + 62.0, BUTTON_W, 56.0]
}

fn inside([x, y]: [f32; 2], [rx, ry, w, h]: [f32; 4]) -> bool {
    (rx..rx + w).contains(&x) && (ry..ry + h).contains(&y)
}

/// A replay on show: how fast it plays, whether it's paused and whose view it follows.
pub(crate) struct Watching {
    viewer: Viewer,
    speed: usize,
    paused: bool,
    /// The player whose view is shown, by place in the night's players.
    follow: usize,
    /// Ticks owed at the speed chosen, played once a whole one is due.
    owed: f32,
    stepped: bool,
    /// The replay stopped itself where it drifted, once.
    stopped_at_drift: bool,
    warn: Option<String>,
    /// The screen photo mode's guides are laid out for.
    device: Device,
    /// Photo mode asked for a capture this frame.
    capture: bool,
}

impl Watching {
    pub fn new(r: Replay, warn: Option<String>) -> Self {
        Self { viewer: Viewer::new(r), speed: NORMAL, paused: false, follow: 0, owed: 0.0, stepped: false, stopped_at_drift: false, warn, device: Device::default(), capture: false }
    }

    /// Lays photo mode's guides out for `device`.
    pub fn set_device(&mut self, device: Device) {
        self.device = device;
    }

    pub fn in_photo(&self) -> bool {
        self.viewer.in_photo()
    }

    /// Photo mode asked for a capture last frame.
    pub fn take_capture(&mut self) -> bool {
        std::mem::take(&mut self.capture)
    }

    /// Hides or shows photo mode's guides.
    pub fn toggle_guides(&mut self) {
        self.viewer.toggle_guides();
    }

    /// A frame: photo mode while it's open, else the viewer's buttons, or a tap on the strip at `top`, then the ticks
    /// due; false once left. X or Photo pauses and opens photo mode where the night stands.
    pub fn update(&mut self, input: &Input, tap: Option<[f32; 2]>, top: f32, dt: f32, sounds: &mut Vec<Sfx>) -> bool {
        self.stepped = false;
        if self.viewer.in_photo() {
            self.capture = self.viewer.photo(input, dt, sounds).capture;
            self.viewer.hold();
            return true;
        }
        let tapped = tap.and_then(|p| (0..CONTROLS.len()).find(|&i| inside(p, button(i, top)))).map(|i| CONTROLS[i]);
        let pressed = |b: Buttons| input.pressed.contains(b);
        if pressed(Buttons::B) || tapped == Some(Control::Leave) {
            sounds.push(Sfx::Tick(0));
            return false;
        }
        if pressed(Buttons::X) || tapped == Some(Control::Photo) {
            (self.paused, self.owed) = (true, 0.0);
            self.viewer.open_photo(self.follow, self.device);
            self.viewer.hold();
            sounds.push(Sfx::Tick(6));
            return true;
        }
        if pressed(Buttons::A) || pressed(Buttons::START) || tapped == Some(Control::Play) {
            if self.viewer.ended() {
                self.viewer.rewind();
                (self.paused, self.stopped_at_drift) = (false, false);
            } else {
                self.paused = !self.paused;
            }
            sounds.push(Sfx::Tick(if self.paused { 0 } else { 6 }));
        }
        let by = (pressed(Buttons::RIGHT) || tapped == Some(Control::Faster)) as i32 - (pressed(Buttons::LEFT) || tapped == Some(Control::Slower)) as i32;
        let speed = (self.speed as i32 + by).clamp(0, SPEEDS.len() as i32 - 1) as usize;
        if speed != self.speed {
            self.speed = speed;
            sounds.push(Sfx::Tick(speed as u8 * 2));
        }
        let n = self.viewer.players();
        let turn = (pressed(Buttons::R1) || tapped == Some(Control::Player)) as i32 - pressed(Buttons::L1) as i32;
        if turn != 0 && n > 1 {
            self.follow = (self.follow as i32 + turn).rem_euclid(n as i32) as usize;
            sounds.push(Sfx::Tick(self.follow as u8 * 3));
        }
        self.stepped = false;
        let once = self.paused && pressed(Buttons::Y);
        if (self.paused && !once) || self.viewer.ended() {
            self.owed = 0.0;
            self.viewer.hold();
            return true;
        }
        self.owed += if once { 1.0 } else { SPEEDS[self.speed] };
        let loud = SPEEDS[self.speed] <= LOUDEST;
        while self.owed >= 1.0 {
            self.owed -= 1.0;
            let mut heard = Vec::new();
            if !self.viewer.step(&mut heard) {
                break;
            }
            self.stepped = true;
            if loud {
                sounds.extend(heard);
            }
            if self.viewer.drift().is_some() && !self.stopped_at_drift {
                (self.stopped_at_drift, self.paused, self.owed) = (true, true, 0.0);
                sounds.push(Sfx::Miss);
            }
        }
        if !self.stepped {
            self.viewer.hold();
        }
        true
    }

    /// The night as the followed player saw it; `alpha` of the way through the last tick when one played this frame.
    /// In photo mode, a `clean` frame is the capture.
    pub fn draw(&mut self, alpha: f32, f: &mut crate::Frame, clean: bool) {
        let alpha = if self.stepped && self.speed == NORMAL { alpha } else { 1.0 };
        self.viewer.draw(self.follow, alpha, f, clean);
    }

    #[cfg(test)]
    pub fn paused(&self) -> bool {
        self.paused
    }

    #[cfg(test)]
    pub fn at(&self) -> (u64, u64, Option<usize>) {
        (self.viewer.played().0, self.viewer.checksum(), self.viewer.menu())
    }

    #[cfg(test)]
    pub fn ended(&self) -> bool {
        self.viewer.ended()
    }

    #[cfg(test)]
    pub fn drift(&self) -> Option<&crate::harvest::replay::Drift> {
        self.viewer.drift()
    }

    pub fn dread(&self) -> f32 {
        if self.paused { 0.0 } else { self.viewer.dread() }
    }

    /// Whose view is on show, and where the night drifted, across the top screen.
    pub fn draw_banner(&self, c: &mut Canvas) {
        c.anchor([0.5, 0.0]);
        let mut y = 200.0;
        let shown = self.viewer.shown(self.follow);
        if self.viewer.players() > 1 {
            let who = if shown == self.follow { format!("WATCHING P{}", self.follow + 1) } else { format!("P{} TURNED  -  WATCHING P{}", self.follow + 1, shown + 1) };
            c.label(who, SCREEN_W / 2.0, y, 30.0, HOT, Align::Center);
            y += 44.0;
        }
        if let Some(d) = self.viewer.drift() {
            c.rect(0.0, y - 6.0, SCREEN_W, 96.0, [0.0, 0.0, 0.0, 0.6]);
            let at = clock(d.tick as f32 / 60.0);
            c.label(format!("Drifted from the recording at {at} (tick {})", d.tick), SCREEN_W / 2.0, y, 32.0, RED, Align::Center);
            let more = match &d.check {
                Some((tick, want, got)) => {
                    let off = want.players.iter().zip(&got.players).map(|(a, b)| {
                        let d = [a[0] - b[0], a[1] - b[1]];
                        (d[0] * d[0] + d[1] * d[1]).sqrt()
                    }).fold(0.0f32, f32::max);
                    format!("Checkpoint at tick {tick}: {} zombies recorded, {} here; a player {off:.0} px off", want.zombies, got.zombies)
                }
                None => "From here it no longer shows the night as it was played".to_string(),
            };
            c.label(more, SCREEN_W / 2.0, y + 44.0, 24.0, WHITE, Align::Center);
        } else if let Some(w) = &self.warn {
            c.label(w.clone(), SCREEN_W / 2.0, y, 24.0, DIM, Align::Center);
        }
    }

    /// The controls and how far through the night it is, in a strip from `top`; `hints` names the pad's buttons.
    pub fn draw_strip(&self, c: &mut Canvas, top: f32, hints: bool) {
        c.anchor([0.5, if top > 0.0 { 1.0 } else { 0.0 }]);
        c.rect(0.0, top, SCREEN_W, STRIP_H, [0.0, 0.0, 0.0, 0.7]);
        let r = self.viewer.replay();
        let (ticks, secs) = self.viewer.played();
        let total = r.ticks.max(1);
        let [bx, bw] = [24.0, SCREEN_W - 48.0];
        c.rect(bx, top + 10.0, bw, 8.0, [1.0, 1.0, 1.0, 0.2]);
        c.rect(bx, top + 10.0, bw * ticks as f32 / total as f32, 8.0, HOT);
        if let Some(d) = self.viewer.drift() {
            c.rect(bx + bw * d.tick as f32 / total as f32 - 2.0, top + 4.0, 4.0, 20.0, RED);
        }
        let state = if self.viewer.ended() { "Ended" } else if self.paused { "Paused" } else { "Replay" };
        c.label(format!("{state}   {} / {}", clock(secs), clock(total as f32 / 60.0)), bx, top + 24.0, 28.0, WHITE, Align::Left);
        let speed = SPEEDS[self.speed];
        let speed = if speed < 1.0 { format!("{speed}x") } else { format!("{speed:.0}x") };
        let n = self.viewer.players();
        let who = if n > 1 { format!("   P{} of {n}", self.follow + 1) } else { String::new() };
        c.label(format!("{speed}{who}"), bx + bw, top + 24.0, 28.0, WHITE, Align::Right);
        let play = if self.viewer.ended() { "Again" } else if self.paused { "Play" } else { "Pause" };
        for (i, control) in CONTROLS.into_iter().enumerate() {
            let [x, y, w, h] = button(i, top);
            let off = control == Control::Player && n < 2;
            c.rect(x, y, w, h, [1.0, 1.0, 1.0, if off { 0.04 } else { 0.12 }]);
            let text = match (control, hints) {
                (Control::Leave, true) => pad::hint("{B}: Leave"),
                (Control::Leave, false) => "Leave".into(),
                (Control::Slower, true) => "Left: Slower".into(),
                (Control::Slower, false) => "Slower".into(),
                (Control::Play, true) => pad::hint(&format!("{{A}}: {play}")),
                (Control::Play, false) => play.into(),
                (Control::Faster, true) => "Right: Faster".into(),
                (Control::Faster, false) => "Faster".into(),
                (Control::Player, true) => pad::hint("{L1} {R1}: Player"),
                (Control::Player, false) => "Player".into(),
                (Control::Photo, true) => pad::hint("{X}: Photo"),
                (Control::Photo, false) => "Photo".into(),
            };
            crate::menu::fit_label(c, &text, x + w / 2.0, y + h / 2.0 - 13.0, 26.0, w - 12.0, if off { DIM } else { WHITE }, Align::Center);
        }
        if hints && self.paused && !self.viewer.ended() {
            c.label(pad::hint("{Y}: one step"), bx + bw, top + STRIP_H - 4.0, 20.0, DIM, Align::Right);
        }
    }
}

/// Seconds as m:ss.
fn clock(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replay_name_reads_as_its_time() {
        assert_eq!(when("night-1795237918-00.lhr").as_deref(), Some("2026-11-21 05:11"));
        assert_eq!(when("night-0000000000-01.lhr").as_deref(), Some("1970-01-01 00:00"));
        assert_eq!(when("dog.lhr"), None);
    }
}
