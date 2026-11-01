//! On-screen controls for a phone, in window pixels: a floating stick on the left, and on the right Grenade and Turn
//! around over a turn strip, a sideways-only floating stick that turns as a right stick does, with Pause above. They
//! go beside the picture when the window is wide enough and under it when upright.

use crate::art::{self, Art};
use crate::input::{Buttons, MAX_TOUCHES};
use crate::{Canvas, Color, SCREEN_H, SCREEN_W};

/// x, y, width and height.
pub type Rect = [f32; 4];

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
    keys: [(Buttons, Art, Rect); 3],
}

impl Layout {
    /// Where the on-screen key for `b` is, if this layout has one.
    pub fn key(&self, b: Buttons) -> Option<Rect> {
        self.keys.iter().find(|k| k.0 == b && b != Buttons::NONE).map(|k| k.2)
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

/// A run shows one screen with the controls around it; menus show both screens, side by side or stacked.
pub fn layout(size: [f32; 2], run: bool) -> Layout {
    let [w, h] = size;
    let wide = w >= h;
    let none = (Buttons::NONE, art::WHITE, [0.0; 4]);
    if !run {
        let (top, bottom) = if wide { (fit([0.0, 0.0, w / 2.0, h]), fit([w / 2.0, 0.0, w / 2.0, h])) } else { (fit([0.0, 0.0, w, h / 2.0]), fit([0.0, h / 2.0, w, h / 2.0])) };
        return Layout { size, top: Some(top), bottom: Some(bottom), stick: [0.0; 4], stick_r: 0.0, turn: [0.0; 4], keys: [none; 3] };
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
    let cell = (right[2] / 2.0).min(right[3] / 4.0).min(0.2 * u);
    let cx = right[0] + right[2] / 2.0;
    let square = |x: f32, y: f32, s: f32| [x - s / 2.0, y - s / 2.0, s, s];
    let keys = [
        (Buttons::L2, art::GRENADE, square(cx, right[1] + 1.2 * cell, cell)),
        (Buttons::B, art::TOUCH_AROUND, square(cx, right[1] + 2.3 * cell, 0.85 * cell)),
        (Buttons::START, art::TOUCH_PAUSE, square(right[0] + right[2] - 0.4 * cell, right[1] + 0.4 * cell, 0.6 * cell)),
    ];
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
    hidden: bool,
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
        let rest = |[x, y, w, h]: Rect| [x + w / 2.0, y + h - r * 1.4];
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
        for &(b, icon, [x, y, w, _]) in &l.keys {
            let down = self.held.contains(b);
            let mid = [x + w / 2.0, y + w / 2.0];
            sprite(&mut c, if down { art::TOUCH_PAD_DOWN } else { art::TOUCH_PAD }, mid, w, if down { used } else { idle });
            sprite(&mut c, icon, mid, w * 0.6, if down { used } else { idle });
        }
        c
    }
}

fn claim(l: &Layout, p: [f32; 2]) -> Claim {
    if let Some(&(b, ..)) = l.keys.iter().find(|&&(b, _, r)| b != Buttons::NONE && inside(r, p)) {
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
        let l = layout([2400.0, 1080.0], true);
        let pic = l.top.unwrap();
        assert!(l.bottom.is_none());
        assert!(l.stick[0] + l.stick[2] <= pic[0] + 0.5);
        for &(b, _, r) in &l.keys {
            assert!(r[0] >= pic[0] + pic[2] - 0.5, "{b:?} sits right of the picture");
        }
    }

    #[test]
    fn menus_show_both_screens_and_no_controls() {
        let l = layout([2400.0, 1080.0], false);
        let (t, b) = (l.top.unwrap(), l.bottom.unwrap());
        assert!(t[0] + t[2] <= b[0] + 0.5);
        assert_eq!(Touchpad::default().draw(&l).shapes.len(), 0);
    }

    #[test]
    fn a_thumb_on_the_left_drags_the_stick_and_keys_press_once() {
        let l = layout([2400.0, 1080.0], true);
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
        let l = layout([2400.0, 1080.0], true);
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
        let l = layout([2400.0, 1080.0], true);
        let pic = l.top.unwrap();
        let mut pad = Touchpad::default();
        let out = pad.update(&fingers(&[[pic[0] + pic[2] / 2.0, pic[1] + pic[3] / 4.0]]), &l, false);
        let t = out.touch.unwrap();
        assert!(out.touch_started && (t[0] - SCREEN_W / 2.0).abs() < 1.0 && (t[1] - SCREEN_H / 4.0).abs() < 1.0);
        assert!(!pad.update(&fingers(&[[pic[0] + 1.0, pic[1] + 1.0]]), &l, false).touch_started);
    }

    #[test]
    fn on_menus_only_the_bottom_screen_takes_touches() {
        let l = layout([2400.0, 1080.0], false);
        let (t, b) = (l.top.unwrap(), l.bottom.unwrap());
        let mut pad = Touchpad::default();
        assert!(pad.update(&fingers(&[[t[0] + 10.0, t[1] + 10.0]]), &l, false).touch.is_none());
        pad.update(&fingers(&[]), &l, false);
        assert!(pad.update(&fingers(&[[b[0] + 10.0, b[1] + 10.0]]), &l, false).touch.is_some());
    }

    #[test]
    fn a_gamepad_hides_the_controls_until_the_next_touch() {
        let l = layout([2400.0, 1080.0], true);
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
    fn upright_puts_the_controls_under_the_picture() {
        let l = layout([1080.0, 2400.0], true);
        let pic = l.top.unwrap();
        assert!(l.stick[1] >= pic[1] + pic[3] - 0.5);
        assert!(l.keys.iter().all(|&(_, _, r)| r[1] >= pic[1] + pic[3] - 0.5));
    }
}
