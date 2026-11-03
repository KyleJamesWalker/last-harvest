//! Photo mode: the paused run held still, horde and all, while the camera turns, zooms and rises round the player.

use super::{Flow, Harvest, PAUSE_PHOTO, SCALE, view3d};
use crate::icon::Orbit;
use crate::scene3d::CardScene;
use crate::{Align, Buttons, Canvas, Color, Frame, Input, SCREEN_H, SCREEN_W, Sfx};

const MIN_ZOOM: f32 = 1.0;
const GUIDE: Color = [1.0, 1.0, 0.6, 0.6];
const HELP: Color = [0.85, 0.85, 0.9, 1.0];
/// How far through the night, 0 to 1, as the sky counts toward dawn.
const TIMES: [(&str, f32); 5] = [("Midnight", 0.0), ("Small hours", 0.3), ("First light", 0.55), ("Daybreak", 0.8), ("Dawn", 1.0)];
/// Where the fog starts and where it is whole, as fractions of the view's reach; None leaves the run's own.
const FOGS: [(&str, Option<[f32; 2]>); 5] = [("Clear", Some([0.8, 1.0])), ("Thin", Some([0.55, 1.0])), ("Usual", None), ("Thick", Some([0.2, 0.7])), ("Pea soup", Some([0.03, 0.4]))];
const USUAL_FOG: usize = 2;
/// The lantern's reach, times the run's own.
const LANTERNS: [(&str, f32); 5] = [("Out", 0.0), ("Dim", 0.5), ("Usual", 1.0), ("Bright", 1.6), ("Blazing", 2.5)];
const USUAL_LANTERN: usize = 2;

/// The photo mode setting R2 changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Setting {
    Time,
    Fog,
    Lantern,
}

impl Setting {
    const ALL: [Self; 3] = [Self::Time, Self::Fog, Self::Lantern];

    fn next(self) -> Self {
        Self::ALL[(Self::ALL.iter().position(|&s| s == self).unwrap_or(0) + 1) % Self::ALL.len()]
    }
}

pub(super) struct Photo {
    /// The player's heading, which the camera turns back to on leaving.
    heading: f32,
    guides: bool,
    setting: Setting,
    /// An index into TIMES, or None for the run's own time.
    time: Option<usize>,
    fog: usize,
    lantern: usize,
}

impl Photo {
    fn new(heading: f32) -> Self {
        Self { heading, guides: true, setting: Setting::Time, time: None, fog: USUAL_FOG, lantern: USUAL_LANTERN }
    }

    /// Steps the chosen setting to its next value, after the last back round to the first.
    fn change(&mut self) {
        match self.setting {
            Setting::Time => {
                self.time = match self.time {
                    None => Some(0),
                    Some(i) if i + 1 < TIMES.len() => Some(i + 1),
                    Some(_) => None,
                }
            }
            Setting::Fog => self.fog = (self.fog + 1) % FOGS.len(),
            Setting::Lantern => self.lantern = (self.lantern + 1) % LANTERNS.len(),
        }
    }

    /// The time of night photo mode sets, if any.
    pub(super) fn night(&self) -> Option<f32> {
        self.time.map(|i| TIMES[i].1)
    }

    /// Thins or thickens the fog and turns the lantern up or down.
    fn light(&self, scene: &mut CardScene) {
        if let Some([near, far]) = FOGS[self.fog].1 {
            let reach = scene.fog_range[1];
            scene.fog_range = [near * reach, far * reach];
        }
        scene.light_range = (scene.light_range * LANTERNS[self.lantern].1).max(0.01);
    }

    /// Each setting and its value, the chosen one in brackets.
    fn settings_line(&self) -> String {
        let time = self.time.map_or("Now", |i| TIMES[i].0);
        let rows = [(Setting::Time, "Time", time), (Setting::Fog, "Fog", FOGS[self.fog].0), (Setting::Lantern, "Lantern", LANTERNS[self.lantern].0)];
        let rows: Vec<String> = rows.iter().map(|&(s, name, value)| if s == self.setting { format!("[{name}: {value}]") } else { format!("{name}: {value}") }).collect();
        rows.join("  ")
    }
}

impl Harvest {
    pub(super) fn open_photo(&mut self) {
        self.menu = None;
        self.photo = Some(Photo::new(self.view_yaw));
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

    /// Left and Right turn round the player, Up and Down zoom, and with L1 held raise or lower the camera; L2 chooses
    /// a setting and R2 changes it; R1 or a tap asks for a capture, and B or Start goes back to the pause menu.
    /// Nothing else moves.
    pub(super) fn photo(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        let Some(p) = &mut self.photo else { return };
        if input.pressed.any(Buttons::B | Buttons::START) {
            (self.view_yaw, self.zoom, self.lift) = (p.heading, 1.0, 0.0);
            self.photo = None;
            self.menu = Some(PAUSE_PHOTO);
            sounds.push(Sfx::Tick(0));
            return;
        }
        if input.pressed.contains(Buttons::L2) {
            p.setting = p.setting.next();
            sounds.push(Sfx::Tick(3));
        }
        if input.pressed.contains(Buttons::R2) {
            p.change();
            sounds.push(Sfx::Tick(6));
        }
        let mut cam = Orbit { yaw: self.view_yaw, zoom: self.zoom, lift: self.lift };
        cam.steer(input, dt, input.held.contains(Buttons::L1), MIN_ZOOM);
        (self.view_yaw, self.zoom, self.lift) = (cam.yaw, cam.zoom, cam.lift);
        flow.capture = input.pressed.contains(Buttons::R1) || input.touch_started;
    }

    /// The frozen 3D view with no HUD on top, and the map below; the guides show unless `clean`.
    pub(super) fn photo_frame(&self, f: &mut Frame, clean: bool) {
        let mut scene = view3d::scene(self);
        let photo = self.photo.as_ref();
        if let Some(p) = photo {
            p.light(&mut scene);
        }
        f.top.clear = [scene.fog[0], scene.fog[1], scene.fog[2], 1.0];
        f.top.cards = Some(scene);
        let c = &mut f.bottom;
        c.clear = [0.0, 0.0, 0.0, 1.0];
        let cam = self.camera();
        self.world(c, cam, SCALE, self.map_tint());
        self.view_cone(c, cam);
        let Some(p) = photo.filter(|p| p.guides && !clean) else { return };
        // The middle square, as the capture's square crop takes it.
        let x = (SCREEN_W - SCREEN_H) / 2.0;
        outline(&mut f.top, x, 0.0, SCREEN_H, GUIDE);
        let shoot = if self.saves_photos { "R1 or tap: take a photo" } else { "R1 or tap: hide these guides" };
        let lines = [
            "PHOTO MODE".to_string(),
            "Left / Right: turn   Up / Down: zoom   L1 + Up / Down: height".into(),
            format!("L2: choose  R2: change   {}", p.settings_line()),
            format!("{shoot}   B or Start: back"),
        ];
        // On one screen the help goes over the picture, where the guides already are.
        let (c, y) = if self.single { (&mut f.top, SCREEN_H - 152.0) } else { (&mut f.bottom, 40.0) };
        c.rect(0.0, y - 10.0, SCREEN_W, 144.0, [0.0, 0.0, 0.0, 0.55]);
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

    fn top_scene(h: &Harvest) -> CardScene {
        let mut f = Frame::default();
        h.photo_frame(&mut f, true);
        f.top.cards.unwrap()
    }

    #[test]
    fn l2_chooses_a_setting_and_r2_steps_it_round() {
        let mut h = photo_mode(7);
        let p = |h: &Harvest| {
            let p = h.photo.as_ref().unwrap();
            (p.setting, p.time, p.fog, p.lantern)
        };
        assert_eq!(p(&h), (Setting::Time, None, USUAL_FOG, USUAL_LANTERN));
        press(&mut h, Buttons::R2);
        assert_eq!(p(&h).1, Some(0));
        for _ in 0..TIMES.len() {
            press(&mut h, Buttons::R2);
        }
        assert_eq!(p(&h).1, None, "after Dawn comes the run's own time again");
        press(&mut h, Buttons::L2);
        press(&mut h, Buttons::R2);
        assert_eq!(p(&h), (Setting::Fog, None, USUAL_FOG + 1, USUAL_LANTERN));
        press(&mut h, Buttons::L2);
        press(&mut h, Buttons::R2);
        assert_eq!(p(&h), (Setting::Lantern, None, USUAL_FOG + 1, USUAL_LANTERN + 1));
        press(&mut h, Buttons::L2);
        assert_eq!(p(&h).0, Setting::Time, "and L2 comes back round to the time");
    }

    #[test]
    fn the_settings_light_the_frozen_view_and_leave_the_run_as_it_was() {
        let mut h = photo_mode(8);
        let (t, usual) = (h.t, top_scene(&h));
        assert_eq!(view3d::sky(&h).fog, usual.fog);
        let p = h.photo.as_mut().unwrap();
        p.time = Some(TIMES.len() - 1);
        p.fog = FOGS.iter().position(|f| f.0 == "Pea soup").unwrap();
        p.lantern = LANTERNS.iter().position(|l| l.0 == "Bright").unwrap();
        let set = top_scene(&h);
        assert!(set.fog[0] > usual.fog[0] && set.ambient[0] > usual.ambient[0], "dawn warms the sky");
        assert!(set.fog_range[1] < usual.fog_range[1], "thick fog closes in");
        assert!(set.light_range > usual.light_range, "a brighter lantern reaches further");
        h.photo.as_mut().unwrap().lantern = 0;
        assert!(top_scene(&h).light_range < 0.1, "and one put out lights nothing");
        assert_eq!(h.t, t, "the run's clock never moves");
        press(&mut h, Buttons::B);
        let back = view3d::scene(&h);
        assert_eq!((back.fog, back.fog_range, back.light_range), (usual.fog, usual.fog_range, usual.light_range), "leaving puts the run's own night back");
        press(&mut h, Buttons::A);
        let p = h.photo.as_ref().unwrap();
        assert_eq!((p.time, p.fog, p.lantern), (None, USUAL_FOG, USUAL_LANTERN), "and photo mode opens on it again");
    }

    #[test]
    fn the_help_lists_the_settings_with_the_chosen_one_marked() {
        let mut h = photo_mode(9);
        press(&mut h, Buttons::L2);
        let mut f = Frame::default();
        h.photo_frame(&mut f, false);
        let text: Vec<String> = f.bottom.shapes.iter().chain(&f.top.shapes).filter_map(|s| if let crate::Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect();
        assert!(text.iter().any(|t| t.contains("L2: choose") && t.contains("[Fog: Usual]") && t.contains("Time: Now")), "{text:?}");
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
