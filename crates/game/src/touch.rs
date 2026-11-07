//! On-screen controls for a phone, in window pixels: a floating stick on the left with Map above it, and on the right
//! Grenade and Turn around over a turn strip, a sideways-only floating stick that turns as a right stick does, with
//! Pause above. They go beside the picture when the window is wide enough and under it when upright. In photo mode
//! the keys become photo keys, each named by a word.

use crate::art::{self, Art};
use crate::input::{Buttons, MAX_TOUCHES};
use crate::{Align, Canvas, Color, SCREEN_H, SCREEN_W};

/// x, y, width and height.
pub type Rect = [f32; 4];

/// What an on-screen key shows: a sprite, or a word.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Face {
    Icon(Art),
    Word(&'static str),
}

/// Where the screens and the controls go in a window `size` pixels across and down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub size: [f32; 2],
    pub top: Option<Rect>,
    pub bottom: Option<Rect>,
    /// Where a thumb puts the stick down; empty outside a run.
    stick: Rect,
    stick_r: f32,
    /// Where a thumb puts the turn strip down; empty outside a run.
    turn: Rect,
    keys: [(Buttons, Face, Rect); 5],
}

impl Layout {
    /// Where the on-screen key for `b` is, if this layout has one.
    pub fn key(&self, b: Buttons) -> Option<Rect> {
        self.keys().find(|k| k.0 == b).map(|k| k.2)
    }

    fn keys(&self) -> impl Iterator<Item = &(Buttons, Face, Rect)> {
        self.keys.iter().filter(|k| k.0 != Buttons::NONE)
    }

    /// Where the stick sits with no thumb on it.
    pub fn stick_at(&self) -> [f32; 2] {
        rest(self.stick, self.stick_r)
    }
}

/// The screens to draw and the controls over them, for a platform to put on one canvas.
#[derive(Clone, Debug)]
pub struct Phone {
    pub size: [f32; 2],
    pub top: Option<Rect>,
    pub bottom: Option<Rect>,
    /// Drawn last, over the whole window; its clear color is see-through.
    pub pad: Canvas,
}

fn inside([x, y, w, h]: Rect, [px, py]: [f32; 2]) -> bool {
    (x..x + w).contains(&px) && (y..y + h).contains(&py)
}

/// The largest 4:3 rect centered in `area`.
fn fit([x, y, w, h]: Rect) -> Rect {
    let s = (w / SCREEN_W).min(h / SCREEN_H);
    let (fw, fh) = (SCREEN_W * s, SCREEN_H * s);
    [x + (w - fw) / 2.0, y + (h - fh) / 2.0, fw, fh]
}

/// A run shows one screen with the controls around it; menus show both screens, side by side or stacked. Without
/// `touch` there are no controls: a run's screen fills the window, and menus show the bottom screen alone. A run in
/// `photo` mode has the photo keys.
pub fn layout(size: [f32; 2], run: bool, touch: bool, photo: bool) -> Layout {
    let [w, h] = size;
    let wide = w >= h;
    let none = (Buttons::NONE, Face::Icon(art::WHITE), [0.0; 4]);
    if !touch {
        let (top, bottom) = if run { (Some(fit([0.0, 0.0, w, h])), None) } else { (None, Some(fit([0.0, 0.0, w, h]))) };
        return Layout { size, top, bottom, stick: [0.0; 4], stick_r: 0.0, turn: [0.0; 4], keys: [none; 5] };
    }
    if !run {
        let side_by_side = w * SCREEN_H >= h * SCREEN_W;
        let (top, bottom) = if side_by_side { (fit([0.0, 0.0, w / 2.0, h]), fit([w / 2.0, 0.0, w / 2.0, h])) } else { (fit([0.0, 0.0, w, h / 2.0]), fit([0.0, h / 2.0, w, h / 2.0])) };
        return Layout { size, top: Some(top), bottom: Some(bottom), stick: [0.0; 4], stick_r: 0.0, turn: [0.0; 4], keys: [none; 5] };
    }
    let u = w.min(h);
    let (pic, left, right) = if wide {
        let pic = fit([0.0, 0.0, w, h]);
        let side = pic[0];
        if side >= 0.2 * u {
            (pic, [0.0, 0.0, side, h], [pic[0] + pic[2], 0.0, side, h])
        } else {
            let (zw, zh) = (pic[2] * 0.3, pic[3] * 0.6);
            (pic, [pic[0], pic[1] + pic[3] - zh, zw, zh], [pic[0] + pic[2] - zw, pic[1] + pic[3] - zh, zw, zh])
        }
    } else {
        let pic = fit([0.0, 0.0, w, h * 0.6]);
        let band = pic[1] + pic[3];
        (pic, [0.0, band, w / 2.0, h - band], [w / 2.0, band, w / 2.0, h - band])
    };
    let cell = (right[2] / 2.0).min(right[3] / 4.4).min(0.2 * u);
    let cx = right[0] + right[2] / 2.0;
    let square = |x: f32, y: f32, s: f32| [x - s / 2.0, y - s / 2.0, s, s];
    let corner = square(right[0] + right[2] - 0.4 * cell, right[1] + 0.4 * cell, 0.6 * cell);
    let keys = if photo {
        let (l, r, s) = (cx - 0.5 * cell, cx + 0.5 * cell, 0.85 * cell);
        [
            (Buttons::L2, Face::Word("Setting"), square(l, right[1] + 1.2 * cell, s)),
            (Buttons::R2, Face::Word("Change"), square(r, right[1] + 1.2 * cell, s)),
            (Buttons::L1, Face::Word("Height"), square(l, right[1] + 2.3 * cell, s)),
            (Buttons::X, Face::Word("Reset"), square(r, right[1] + 2.3 * cell, s)),
            (Buttons::B, Face::Word("Back"), corner),
        ]
    } else {
        [
            (Buttons::L2, Face::Icon(art::GRENADE), square(cx, right[1] + 1.2 * cell, cell)),
            (Buttons::B, Face::Icon(art::TOUCH_AROUND), square(cx, right[1] + 2.3 * cell, 0.85 * cell)),
            (Buttons::START, Face::Icon(art::TOUCH_PAUSE), corner),
            // Held, the whole map paused, as R2 shows it on a pad.
            (Buttons::R2, Face::Icon(art::TOUCH_MAP), square(left[0] + 0.4 * cell, left[1] + 0.4 * cell, 0.6 * cell)),
            none,
        ]
    };
    let strip = right[1] + 2.85 * cell;
    let turn = [right[0], strip, right[2], (right[1] + right[3] - strip).max(cell)];
    Layout { size, top: Some(pic), bottom: None, stick: left, stick_r: cell * 0.8, turn, keys }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Claim {
    Stick,
    Turn,
    Key(Buttons),
    /// A finger on a screen: true for the one that takes touches (the bottom, or a run's one picture).
    Screen(bool),
    Nothing,
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    claim: Claim,
    /// The stick's base, dragged along by a thumb past its edge.
    origin: [f32; 2],
    at: [f32; 2],
    new: bool,
}

/// What the fingers on the controls amount to this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pad {
    pub held: Buttons,
    pub pressed: Buttons,
    pub stick: [f32; 2],
    /// The turn strip's sideways push, -1..=1.
    pub look: f32,
    /// A finger on the screen that takes touches, in its pixels, and whether it just went down.
    pub touch: Option<[f32; 2]>,
    pub touch_started: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Touchpad {
    fingers: [Option<Finger>; MAX_TOUCHES],
    held: Buttons,
    /// Gamepad or keyboard input hides the controls until a finger comes down off the screens.
    pub(crate) hidden: bool,
}

impl Touchpad {
    /// Reads the fingers, in window pixels by slot; `other` is any button or stick input from elsewhere.
    pub fn update(&mut self, touches: &[Option<[f32; 2]>; MAX_TOUCHES], l: &Layout, other: bool) -> Pad {
        for (slot, &t) in self.fingers.iter_mut().zip(touches) {
            match (slot.as_mut(), t) {
                (None, Some(p)) => {
                    let claim = claim(l, p);
                    self.hidden &= matches!(claim, Claim::Screen(_));
                    *slot = Some(Finger { claim, origin: p, at: p, new: true });
                }
                (Some(f), Some(p)) => {
                    f.at = p;
                    f.new = false;
                    let d = [p[0] - f.origin[0], if f.claim == Claim::Turn { 0.0 } else { p[1] - f.origin[1] }];
                    let len = d[0].hypot(d[1]);
                    if matches!(f.claim, Claim::Stick | Claim::Turn) && len > l.stick_r {
                        let k = (len - l.stick_r) / len;
                        f.origin = [f.origin[0] + d[0] * k, f.origin[1] + d[1] * k];
                    }
                }
                (_, None) => *slot = None,
            }
        }
        if other && self.fingers.iter().all(Option::is_none) {
            self.hidden = true;
        }
        let mut pad = Pad::default();
        for f in self.fingers.iter().flatten() {
            match f.claim {
                Claim::Key(b) => pad.held |= b,
                Claim::Stick => {
                    let d = [(f.at[0] - f.origin[0]) / l.stick_r, (f.at[1] - f.origin[1]) / l.stick_r];
                    if d[0].hypot(d[1]) > 0.15 {
                        pad.stick = d.map(|v| v.clamp(-1.0, 1.0));
                    }
                }
                Claim::Turn => {
                    let d = (f.at[0] - f.origin[0]) / l.stick_r;
                    if d.abs() > 0.12 {
                        pad.look = d.clamp(-1.0, 1.0);
                    }
                }
                Claim::Screen(true) if pad.touch.is_none() => {
                    let [x, y, w, h] = l.bottom.or(l.top).unwrap_or([0.0, 0.0, 1.0, 1.0]);
                    pad.touch = Some([(f.at[0] - x) / w * SCREEN_W, (f.at[1] - y) / h * SCREEN_H]);
                    pad.touch_started = f.new;
                }
                _ => {}
            }
        }
        pad.pressed = pad.held & !self.held;
        self.held = pad.held;
        pad
    }

    /// The controls over the window, see-through around them.
    pub fn draw(&self, l: &Layout) -> Canvas {
        let mut c = Canvas { clear: [0.0; 4], size: Some(l.size), ..Default::default() };
        if self.hidden || l.stick_r == 0.0 {
            return c;
        }
        let idle: Color = [1.0, 1.0, 1.0, 0.6];
        let used: Color = [1.0, 1.0, 1.0, 0.95];
        let shade: Color = [0.0, 0.0, 0.0, 0.25];
        let r = l.stick_r;
        // A sprite `size` pixels across at its widest, centered on (x, y).
        let sprite = |c: &mut Canvas, a: Art, [x, y]: [f32; 2], size: f32, color: Color| {
            let k = size / a.w().max(a.h());
            c.art(a, x - a.w() * k / 2.0, y - a.h() * k / 2.0, k, false, color);
        };
        let rest = |area: Rect| rest(area, r);
        let stick = self.fingers.iter().flatten().find(|f| f.claim == Claim::Stick);
        let (base, knob) = stick.map_or((rest(l.stick), rest(l.stick)), |f| (f.origin, f.at));
        let tint = if stick.is_some() { used } else { idle };
        c.circle(base[0], base[1], r, shade);
        sprite(&mut c, art::TOUCH_RING, base, 2.0 * r, tint);
        let d = [knob[0] - base[0], knob[1] - base[1]];
        let k = (r / d[0].hypot(d[1]).max(r)).min(1.0);
        sprite(&mut c, art::TOUCH_KNOB, [base[0] + d[0] * k, base[1] + d[1] * k], r * 0.9, tint);
        let turn = self.fingers.iter().flatten().find(|f| f.claim == Claim::Turn);
        let (base, knob) = turn.map_or((rest(l.turn), rest(l.turn)[0]), |f| (f.origin, f.at[0]));
        let tint = if turn.is_some() { used } else { idle };
        sprite(&mut c, art::TOUCH_TRACK, base, 2.0 * r, tint);
        sprite(&mut c, art::TOUCH_KNOB, [knob.clamp(base[0] - r, base[0] + r), base[1]], r * 0.9, tint);
        for &(b, face, [x, y, w, _]) in l.keys() {
            let down = self.held.contains(b);
            let (mid, tint) = ([x + w / 2.0, y + w / 2.0], if down { used } else { idle });
            sprite(&mut c, if down { art::TOUCH_PAD_DOWN } else { art::TOUCH_PAD }, mid, w, tint);
            match face {
                Face::Icon(icon) => sprite(&mut c, icon, mid, w * 0.6, tint),
                Face::Word(word) => c.label(word, mid[0], mid[1] - w * 0.11, w * 0.2, tint, Align::Center),
            }
        }
        c
    }
}

/// Where the stick or the turn strip sits in `area` with no thumb on it.
fn rest([x, y, w, h]: Rect, r: f32) -> [f32; 2] {
    [x + w / 2.0, y + h - r * 1.4]
}

fn claim(l: &Layout, p: [f32; 2]) -> Claim {
    if let Some(&(b, ..)) = l.keys().find(|&&(_, _, r)| inside(r, p)) {
        return Claim::Key(b);
    }
    if l.stick_r > 0.0 && inside(l.stick, p) {
        return Claim::Stick;
    }
    if l.stick_r > 0.0 && inside(l.turn, p) {
        return Claim::Turn;
    }
    match (l.top, l.bottom) {
        (_, Some(r)) if inside(r, p) => Claim::Screen(true),
        (Some(r), None) if inside(r, p) => Claim::Screen(true),
        (Some(r), _) if inside(r, p) => Claim::Screen(false),
        _ => Claim::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingers(at: &[[f32; 2]]) -> [Option<[f32; 2]>; MAX_TOUCHES] {
        let mut out = [None; MAX_TOUCHES];
        for (slot, &p) in out.iter_mut().zip(at) {
            *slot = Some(p);
        }
        out
    }

    #[test]
    fn a_landscape_run_puts_the_stick_left_and_the_keys_right_of_the_picture() {
        let l = layout([2400.0, 1080.0], true, true, false);
        let pic = l.top.unwrap();
        assert!(l.bottom.is_none());
        assert!(l.stick[0] + l.stick[2] <= pic[0] + 0.5);
        for &(b, _, r) in l.keys() {
            if b == Buttons::R2 {
                assert!(r[0] + r[2] <= pic[0] + 0.5, "the map key sits left of the picture, over the stick");
            } else {
                assert!(r[0] >= pic[0] + pic[2] - 0.5, "{b:?} sits right of the picture");
            }
        }
    }

    #[test]
    fn menus_show_both_screens_and_no_controls() {
        let l = layout([2400.0, 1080.0], false, true, false);
        let (t, b) = (l.top.unwrap(), l.bottom.unwrap());
        assert!(t[0] + t[2] <= b[0] + 0.5);
        assert_eq!(Touchpad::default().draw(&l).shapes.len(), 0);
    }

    #[test]
    fn a_thumb_on_the_left_drags_the_stick_and_keys_press_once() {
        let l = layout([2400.0, 1080.0], true, true, false);
        let mut pad = Touchpad::default();
        let down = [l.stick[0] + l.stick[2] / 2.0, l.stick[1] + l.stick[3] / 2.0];
        let grenade = l.keys[0].2;
        let key = [grenade[0] + 5.0, grenade[1] + 5.0];
        pad.update(&fingers(&[down]), &l, false);
        let out = pad.update(&fingers(&[[down[0], down[1] - l.stick_r], key]), &l, false);
        assert!(out.stick[1] < -0.9 && out.stick[0].abs() < 0.01);
        assert!(out.held.contains(Buttons::L2) && out.pressed.contains(Buttons::L2));
        let again = pad.update(&fingers(&[[down[0], down[1] - l.stick_r], key]), &l, false);
        assert!(again.held.contains(Buttons::L2) && !again.pressed.contains(Buttons::L2));
        assert_eq!(pad.update(&fingers(&[]), &l, false), Pad::default());
    }

    #[test]
    fn a_thumb_on_the_turn_strip_turns_by_how_far_it_slides_sideways() {
        let l = layout([2400.0, 1080.0], true, true, false);
        let mut pad = Touchpad::default();
        let down = [l.turn[0] + l.turn[2] / 2.0, l.turn[1] + l.turn[3] / 2.0];
        assert_eq!(pad.update(&fingers(&[down]), &l, false).look, 0.0);
        let half = pad.update(&fingers(&[[down[0] + l.stick_r / 2.0, down[1] + l.stick_r]]), &l, false);
        assert!((half.look - 0.5).abs() < 0.01 && half.stick == [0.0; 2], "sideways only, and not the walking stick");
        let left = pad.update(&fingers(&[[down[0] - 3.0 * l.stick_r, down[1]]]), &l, false);
        assert!(left.look < -0.999);
        let back = pad.update(&fingers(&[[down[0] - 2.0 * l.stick_r, down[1]]]), &l, false);
        assert!(back.look.abs() < 0.01, "the strip followed the thumb, so a little back is straight ahead");
    }

    #[test]
    fn a_tap_on_the_picture_is_a_touch_in_screen_pixels() {
        let l = layout([2400.0, 1080.0], true, true, false);
        let pic = l.top.unwrap();
        let mut pad = Touchpad::default();
        let out = pad.update(&fingers(&[[pic[0] + pic[2] / 2.0, pic[1] + pic[3] / 4.0]]), &l, false);
        let t = out.touch.unwrap();
        assert!(out.touch_started && (t[0] - SCREEN_W / 2.0).abs() < 1.0 && (t[1] - SCREEN_H / 4.0).abs() < 1.0);
        assert!(!pad.update(&fingers(&[[pic[0] + 1.0, pic[1] + 1.0]]), &l, false).touch_started);
    }

    #[test]
    fn on_menus_only_the_bottom_screen_takes_touches() {
        let l = layout([2400.0, 1080.0], false, true, false);
        let (t, b) = (l.top.unwrap(), l.bottom.unwrap());
        let mut pad = Touchpad::default();
        assert!(pad.update(&fingers(&[[t[0] + 10.0, t[1] + 10.0]]), &l, false).touch.is_none());
        pad.update(&fingers(&[]), &l, false);
        assert!(pad.update(&fingers(&[[b[0] + 10.0, b[1] + 10.0]]), &l, false).touch.is_some());
    }

    #[test]
    fn a_gamepad_hides_the_controls_until_the_next_touch() {
        let l = layout([2400.0, 1080.0], true, true, false);
        let mut pad = Touchpad::default();
        pad.update(&fingers(&[]), &l, true);
        assert!(pad.draw(&l).shapes.is_empty());
        let pic = l.top.unwrap();
        pad.update(&fingers(&[[pic[0] + pic[2] / 2.0, pic[1] + pic[3] / 2.0]]), &l, false);
        assert!(pad.draw(&l).shapes.is_empty(), "a tap on the picture, as to throw, leaves them hidden");
        pad.update(&fingers(&[]), &l, false);
        pad.update(&fingers(&[[10.0, 10.0]]), &l, false);
        assert!(!pad.draw(&l).shapes.is_empty());
    }

    #[test]
    fn without_touch_a_run_fills_the_window_and_menus_show_the_bottom_screen() {
        let run = layout([720.0, 480.0], true, false, false);
        assert_eq!((run.top, run.bottom), (Some([40.0, 0.0, 640.0, 480.0]), None));
        let menus = layout([720.0, 480.0], false, false, false);
        assert!(menus.top.is_none() && menus.bottom.is_some());
        assert!(Touchpad::default().draw(&run).shapes.is_empty());
    }

    #[test]
    fn at_every_shape_the_menus_take_the_bigger_of_side_by_side_and_stacked() {
        for size in (4..=25).map(|tenths| [1000.0 * tenths as f32 / 10.0, 1000.0]).chain([[640.0, 480.0], [720.0, 720.0], [720.0, 480.0]]) {
            let [w, h] = size;
            let l = layout(size, false, true, false);
            let (t, b) = (l.top.unwrap(), l.bottom.unwrap());
            let best = (w / 2.0 / SCREEN_W).min(h / SCREEN_H).max((w / SCREEN_W).min(h / 2.0 / SCREEN_H));
            assert!((t[2] - best * SCREEN_W).abs() < 0.01 && t[2] == b[2], "{size:?}: {t:?} {b:?} could be {}", best * SCREEN_W);
            for [x, y, rw, rh] in [t, b] {
                assert!(x >= 0.0 && y >= 0.0 && x + rw <= w + 0.01 && y + rh <= h + 0.01, "{size:?}: clipped");
            }
            let apart = t[0] + t[2] <= b[0] + 0.01 || t[1] + t[3] <= b[1] + 0.01;
            assert!(apart, "{size:?}: the screens overlap");
        }
    }

    #[test]
    fn upright_puts_the_controls_under_the_picture() {
        let l = layout([1080.0, 2400.0], true, true, false);
        let pic = l.top.unwrap();
        assert!(l.stick[1] >= pic[1] + pic[3] - 0.5);
        assert!(l.keys().all(|&(_, _, r)| r[1] >= pic[1] + pic[3] - 0.5));
    }

    #[test]
    fn the_map_key_holds_r2_for_the_whole_map_and_clears_the_resting_stick() {
        for tenths in 4..=25 {
            let size = [1000.0 * tenths as f32 / 10.0, 1000.0];
            let l = layout(size, true, true, false);
            let map = l.key(Buttons::R2).unwrap();
            let [x, y] = rest(l.stick, l.stick_r);
            let ring = [x - l.stick_r, y - l.stick_r, 2.0 * l.stick_r, 2.0 * l.stick_r];
            let apart = |a: Rect, b: Rect| a[0] + a[2] <= b[0] || b[0] + b[2] <= a[0] || a[1] + a[3] <= b[1] || b[1] + b[3] <= a[1];
            assert!(apart(map, ring), "{size:?}: the map key {map:?} sits on the stick {ring:?}");
            assert!(map[0] >= 0.0 && map[1] >= 0.0 && map[0] + map[2] <= size[0] && map[1] + map[3] <= size[1], "{size:?}");
        }
        let l = layout([1080.0, 2400.0], true, true, false);
        let map = l.key(Buttons::R2).unwrap();
        let mut pad = Touchpad::default();
        let out = pad.update(&fingers(&[[map[0] + map[2] / 2.0, map[1] + map[3] / 2.0]]), &l, false);
        assert!(out.held.contains(Buttons::R2) && out.stick == [0.0; 2], "held, and not the stick");
    }

    #[test]
    fn at_every_shape_the_resting_turn_knob_clears_the_keys() {
        for tenths in 4..=25 {
            let size = [1000.0 * tenths as f32 / 10.0, 1000.0];
            let l = layout(size, true, true, false);
            let [x, y] = rest(l.turn, l.stick_r);
            let half = l.stick_r * 0.45;
            let knob = [x - half, y - half, 2.0 * half, 2.0 * half];
            for &(b, _, k) in l.keys() {
                let apart = k[0] + k[2] <= knob[0] || knob[0] + knob[2] <= k[0] || k[1] + k[3] <= knob[1] || knob[1] + knob[3] <= k[1];
                assert!(apart, "{size:?}: {b:?} {k:?} overlaps the turn knob {knob:?}");
            }
            assert!(knob[1] + knob[3] <= size[1], "{size:?}: the turn knob runs off the window");
        }
    }
    #[test]
    fn photo_mode_has_its_own_named_keys_clear_of_each_other_and_the_thumbs() {
        let apart = |a: Rect, b: Rect| a[0] + a[2] <= b[0] || b[0] + b[2] <= a[0] || a[1] + a[3] <= b[1] || b[1] + b[3] <= a[1];
        for size in (4..=25).map(|tenths| [1000.0 * tenths as f32 / 10.0, 1000.0]).chain([[1080.0, 2400.0]]) {
            let l = layout(size, true, true, true);
            let keys: Vec<(Buttons, Face)> = l.keys().map(|k| (k.0, k.1)).collect();
            let named = ["Setting", "Change", "Height", "Reset", "Back"].map(Face::Word);
            assert_eq!(keys, [Buttons::L2, Buttons::R2, Buttons::L1, Buttons::X, Buttons::B].into_iter().zip(named).collect::<Vec<_>>());
            let [x, y] = rest(l.stick, l.stick_r);
            let ring = [x - l.stick_r, y - l.stick_r, 2.0 * l.stick_r, 2.0 * l.stick_r];
            let [x, y] = rest(l.turn, l.stick_r);
            let half = l.stick_r * 0.45;
            let knob = [x - half, y - half, 2.0 * half, 2.0 * half];
            for (i, &(b, _, k)) in l.keys().enumerate() {
                assert!(k[0] >= 0.0 && k[1] >= 0.0 && k[0] + k[2] <= size[0] && k[1] + k[3] <= size[1], "{size:?}: {b:?} off the window");
                assert!(apart(k, ring) && apart(k, knob), "{size:?}: {b:?} {k:?} sits under a thumb");
                for &(o, _, other) in l.keys().skip(i + 1) {
                    assert!(apart(k, other), "{size:?}: {b:?} overlaps {o:?}");
                }
            }
        }
    }

    #[test]
    fn photo_keys_draw_their_names_and_a_run_draws_none() {
        let words = |l: &Layout| Touchpad::default().draw(l).shapes.iter().filter_map(|s| if let crate::Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect::<Vec<_>>();
        assert_eq!(words(&layout([2400.0, 1080.0], true, true, true)), ["Setting", "Change", "Height", "Reset", "Back"]);
        assert!(words(&layout([2400.0, 1080.0], true, true, false)).is_empty());
    }

    #[test]
    fn in_photo_mode_one_thumb_holds_height_while_the_other_works_the_stick() {
        let l = layout([2400.0, 1080.0], true, true, true);
        let height = l.key(Buttons::L1).unwrap();
        let key = [height[0] + height[2] / 2.0, height[1] + height[3] / 2.0];
        let down = [l.stick[0] + l.stick[2] / 2.0, l.stick[1] + l.stick[3] / 2.0];
        let mut pad = Touchpad::default();
        pad.update(&fingers(&[down, key]), &l, false);
        let out = pad.update(&fingers(&[[down[0], down[1] - l.stick_r], key]), &l, false);
        assert!(out.held == Buttons::L1 && out.stick[1] < -0.9, "{out:?}");
        assert_eq!(l.key(Buttons::START), None, "Back, not Pause, leaves photo mode");
    }
}
