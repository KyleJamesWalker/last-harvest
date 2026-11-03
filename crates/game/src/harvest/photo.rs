//! Photo mode: the paused run held still, horde and all, while the camera turns, zooms and rises round the player.

use super::{Flow, Harvest, PAUSE_PHOTO, SCALE, view3d};
use crate::icon::Orbit;
use crate::{Align, Buttons, Canvas, Color, Frame, Input, SCREEN_H, SCREEN_W, Sfx};

const MIN_ZOOM: f32 = 1.0;
const GUIDE: Color = [1.0, 1.0, 0.6, 0.6];
const HELP: Color = [0.85, 0.85, 0.9, 1.0];

pub(super) struct Photo {
    /// The player's heading, which the camera turns back to on leaving.
    heading: f32,
    guides: bool,
}

impl Harvest {
    pub(super) fn open_photo(&mut self) {
        self.menu = None;
        self.photo = Some(Photo { heading: self.view_yaw, guides: true });
    }

    pub fn in_photo(&self) -> bool {
        self.photo.is_some()
    }

    /// Hides or shows the guides, where the platform can't save a capture and the player takes their own.
    pub fn toggle_guides(&mut self) {
        if let Some(p) = &mut self.photo {
            p.guides = !p.guides;
        }
    }

    /// Whether R1 or a tap takes a capture the app saves; else it hides the guides.
    pub fn set_saves_photos(&mut self, on: bool) {
        self.saves_photos = on;
    }

    /// Left and Right turn round the player, Up and Down zoom, and with L1 held raise or lower the camera; R1 or a
    /// tap asks for a capture, and B or Start goes back to the pause menu. Nothing else moves.
    pub(super) fn photo(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        let Some(p) = &self.photo else { return };
        if input.pressed.any(Buttons::B | Buttons::START) {
            (self.view_yaw, self.zoom, self.lift) = (p.heading, 1.0, 0.0);
            self.photo = None;
            self.menu = Some(PAUSE_PHOTO);
            sounds.push(Sfx::Tick(0));
            return;
        }
        let mut cam = Orbit { yaw: self.view_yaw, zoom: self.zoom, lift: self.lift };
        cam.steer(input, dt, input.held.contains(Buttons::L1), MIN_ZOOM);
        (self.view_yaw, self.zoom, self.lift) = (cam.yaw, cam.zoom, cam.lift);
        flow.capture = input.pressed.contains(Buttons::R1) || input.touch_started;
    }

    /// The frozen 3D view with no HUD on top, and the map below; the guides show unless `clean`.
    pub(super) fn photo_frame(&self, f: &mut Frame, clean: bool) {
        let scene = view3d::scene(self);
        f.top.clear = [scene.fog[0], scene.fog[1], scene.fog[2], 1.0];
        f.top.cards = Some(scene);
        let c = &mut f.bottom;
        c.clear = [0.0, 0.0, 0.0, 1.0];
        let cam = self.camera();
        self.world(c, cam, SCALE, self.map_tint());
        self.view_cone(c, cam);
        if clean || !self.photo.as_ref().is_some_and(|p| p.guides) {
            return;
        }
        // The middle square, as the capture's square crop takes it.
        let x = (SCREEN_W - SCREEN_H) / 2.0;
        outline(&mut f.top, x, 0.0, SCREEN_H, GUIDE);
        let shoot = if self.saves_photos { "R1 or tap: take a photo" } else { "R1 or tap: hide these guides" };
        let lines = ["PHOTO MODE".to_string(), "Left / Right: turn   Up / Down: zoom   L1 + Up / Down: height".into(), format!("{shoot}   B or Start: back")];
        // On one screen the help goes over the picture, where the guides already are.
        let (c, y) = if self.single { (&mut f.top, SCREEN_H - 120.0) } else { (&mut f.bottom, 40.0) };
        c.rect(0.0, y - 10.0, SCREEN_W, 112.0, [0.0, 0.0, 0.0, 0.55]);
        for (i, line) in lines.iter().enumerate() {
            c.label(line.as_str(), SCREEN_W / 2.0, y + i as f32 * 32.0, 26.0, if i == 0 { GUIDE } else { HELP }, Align::Center);
        }
    }
}

/// A square outline two pixels thick.
fn outline(c: &mut Canvas, x: f32, y: f32, size: f32, color: Color) {
    c.rect(x, y, size, 2.0, color);
    c.rect(x, y + size - 2.0, size, 2.0, color);
    c.rect(x, y, 2.0, size, color);
    c.rect(x + size - 2.0, y, 2.0, size, color);
}

#[cfg(test)]
mod tests {
    use super::super::Harvest;
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn press(h: &mut Harvest, pressed: Buttons) -> Flow {
        h.update(&Input { pressed, ..Default::default() }, DT, &mut Vec::new())
    }

    fn photo_mode(seed: u64) -> Harvest {
        let mut h = Harvest::new(seed);
        for _ in 0..120 {
            h.update(&Input::default(), DT, &mut Vec::new());
        }
        press(&mut h, Buttons::START);
        for _ in 0..PAUSE_PHOTO {
            press(&mut h, Buttons::DOWN);
        }
        press(&mut h, Buttons::A);
        h
    }

    #[test]
    fn the_pause_menu_opens_photo_mode_and_b_or_start_goes_back_to_it() {
        let mut h = photo_mode(3);
        assert!(h.in_photo() && h.menu.is_none());
        let heading = h.photo.as_ref().unwrap().heading;
        for _ in 0..30 {
            h.update(&Input { held: Buttons::RIGHT | Buttons::UP, ..Default::default() }, DT, &mut Vec::new());
        }
        assert!(h.view_yaw != heading && h.zoom > 1.0, "the camera turns and zooms");
        for _ in 0..30 {
            h.update(&Input { held: Buttons::L1 | Buttons::UP, ..Default::default() }, DT, &mut Vec::new());
        }
        assert!(h.lift > 0.0 && !h.showing_whole_map(), "L1 raises the camera instead of showing the map");
        press(&mut h, Buttons::B);
        assert!(!h.in_photo() && h.menu == Some(PAUSE_PHOTO), "B goes back to the pause menu");
        assert_eq!((h.view_yaw, h.zoom, h.lift), (heading, 1.0, 0.0), "with the player's own view");
        press(&mut h, Buttons::A);
        press(&mut h, Buttons::START);
        assert!(h.menu == Some(PAUSE_PHOTO), "and so does Start");
    }

    #[test]
    fn the_run_holds_still_in_photo_mode() {
        let mut h = photo_mode(4);
        let (t, pos, hp) = (h.t, h.pos, h.hp);
        let zombies: Vec<[f32; 2]> = h.zombies.iter().map(|z| z.pos).collect();
        assert!(!zombies.is_empty());
        for _ in 0..300 {
            h.update(&Input { held: Buttons::LEFT | Buttons::A | Buttons::X, ..Default::default() }, DT, &mut Vec::new());
        }
        assert_eq!((h.t, h.pos, h.hp), (t, pos, hp));
        assert_eq!(h.zombies.iter().map(|z| z.pos).collect::<Vec<_>>(), zombies, "the horde stands where it was");
    }

    #[test]
    fn r1_or_a_tap_asks_for_a_capture() {
        let mut h = photo_mode(5);
        assert!(!press(&mut h, Buttons::NONE).capture);
        assert!(press(&mut h, Buttons::R1).capture);
        let tap = Input { touch: Some([500.0, 300.0]), touch_started: true, ..Default::default() };
        assert!(h.update(&tap, DT, &mut Vec::new()).capture);
    }

    #[test]
    fn a_clean_frame_has_the_scene_and_nothing_over_it() {
        let h = photo_mode(6);
        let mut f = Frame::default();
        h.photo_frame(&mut f, false);
        assert!(f.top.cards.is_some() && !f.top.shapes.is_empty(), "the guides show while setting up");
        let mut f = Frame::default();
        h.photo_frame(&mut f, true);
        assert!(f.top.cards.is_some() && f.top.shapes.is_empty(), "a capture has no HUD and no guides");
    }
}
