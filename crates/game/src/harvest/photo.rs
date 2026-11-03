//! Photo mode: the paused run held still, horde and all, while the camera turns, zooms and rises round the player.

use super::map::TILE;
use super::{Facing, Flow, Harvest, PAUSE_PHOTO, SCALE, Zombie, len, norm, sub, view3d};
use crate::art::{self, Art, Stand};
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

/// How far ahead of the player the camera looks, in map pixels, and the zombies R2 picks from round there.
const FOCUS: f32 = 4.0 * TILE;
const PICKS: usize = 8;
const PICK_REACH: f32 = 16.0 * TILE;
/// Radians a second Left or Right turns a posed zombie, and Up or Down a dropped prop.
const POSE_TURN: f32 = 3.0;
/// The props R2 drops, by name and sprite; the choice after the last takes back the one dropped last.
const PROPS: [(&str, &str); 21] = [
    ("Barrier", "BARRIER"),
    ("Barrel", "BARREL"),
    ("Tank", "TANK"),
    ("Hay bale", "HAY"),
    ("Haystack", "HAY_STACK"),
    ("Scarecrow", "SCARECROW"),
    ("Tombstone", "TOMBSTONE"),
    ("Sign", "SIGN"),
    ("Mailbox", "MAILBOX"),
    ("Skull fence", "SKULL_FENCE"),
    ("Dead tree", "DEAD_TREE"),
    ("Tall tree", "TALL_TREE"),
    ("Stump", "STUMP"),
    ("Log", "LOG"),
    ("Bush", "BUSH"),
    ("Corpse", "CORPSE"),
    ("Gas pump", "GAS_PUMP"),
    ("Tractor", "TRACTOR"),
    ("Truck", "TRUCK"),
    ("Car", "CAR_H"),
    ("Wreck", "WRECK_H"),
];
const TAKE_BACK: usize = PROPS.len();

/// The photo mode setting R2 changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Setting {
    Time,
    Fog,
    Lantern,
    Pose,
    Props,
}

impl Setting {
    const ALL: [Self; 5] = [Self::Time, Self::Fog, Self::Lantern, Self::Pose, Self::Props];

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
    /// The zombie being posed, by its place in the frozen horde.
    picked: Option<usize>,
    /// Each posed zombie as the run left it, put back on leaving.
    posed: Vec<(usize, Zombie)>,
    /// Up (-1) or Down (1) held last frame, so a held push steps one frame.
    pushed: i8,
    /// The prop R2 drops, an index into PROPS, or TAKE_BACK.
    prop: usize,
    /// Left (-1) or Right (1) held last frame, so a held push chooses once.
    swung: i8,
    /// Props dropped into the picture, last dropped last; never in the run's map.
    props: Vec<Dropped>,
}

/// A prop photo mode dropped: its place in PROPS, its sprite and how that stands, the middle of its foot in map
/// pixels, and how far it is turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Dropped {
    kind: usize,
    art: Art,
    stand: Stand,
    at: [f32; 2],
    angle: f32,
}

/// The first frame of a sprite by name, or a blank where a mod's art lacks it.
fn sprite(name: &str) -> Art {
    art::ALL.iter().find(|(n, _)| *n == name).and_then(|(_, f)| f.first().copied()).unwrap_or(art::WHITE)
}

/// Which way a push of `v` points: -1, 0 or 1.
fn push_of(v: f32) -> i8 {
    if v < -0.5 { -1 } else { (v > 0.5) as i8 }
}

impl Photo {
    fn new(heading: f32) -> Self {
        Self { heading, guides: true, setting: Setting::Time, time: None, fog: USUAL_FOG, lantern: USUAL_LANTERN, picked: None, posed: Vec::new(), pushed: 0, prop: 0, swung: 0, props: Vec::new() }
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
            Setting::Pose | Setting::Props => {}
        }
    }

    /// Drops the chosen prop at `at`, its front toward a camera looking along `yaw`, or takes back the last.
    fn drop_prop(&mut self, at: [f32; 2], yaw: f32) {
        if self.prop == TAKE_BACK {
            self.props.pop();
        } else {
            let art = sprite(PROPS[self.prop].1);
            self.props.push(Dropped { kind: self.prop, art, stand: view3d::drop_stand(art), at, angle: yaw + std::f32::consts::FRAC_PI_2 });
        }
    }

    /// Chooses the next prop on a fresh push of `x` left or right, and turns the last dropped by `turn` radians.
    fn handle_prop(&mut self, x: f32, turn: f32) {
        let push = push_of(x);
        if push != 0 && push != self.swung {
            let n = PROPS.len() + 1;
            self.prop = (self.prop + if push < 0 { n - 1 } else { 1 }) % n;
        }
        self.swung = push;
        if let Some(d) = self.props.last_mut() {
            d.angle += turn;
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

    /// Each setting and its value in two lines, the chosen one in brackets; `posing` names the picked zombie.
    fn settings_lines(&self, posing: &str) -> [String; 2] {
        let time = self.time.map_or("Now", |i| TIMES[i].0);
        let prop = PROPS.get(self.prop).map_or("Take back", |p| p.0);
        let props = format!("{prop} ({} in)", self.props.len());
        let rows = [(Setting::Time, "Time", time), (Setting::Fog, "Fog", FOGS[self.fog].0), (Setting::Lantern, "Lantern", LANTERNS[self.lantern].0), (Setting::Pose, "Pose", posing), (Setting::Props, "Props", props.as_str())];
        let rows: Vec<String> = rows.iter().map(|&(s, name, value)| if s == self.setting { format!("[{name}: {value}]") } else { format!("{name}: {value}") }).collect();
        [rows[..3].join("  "), rows[3..].join("  ")]
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

    /// Whether the screen takes taps, so the guides offer them.
    pub fn set_taps(&mut self, on: bool) {
        self.taps = on;
    }

    /// Left and Right turn round the player, Up and Down zoom, and with L1 held raise or lower the camera; L2 chooses
    /// a setting and R2 changes it; R1 or a tap asks for a capture, and B or Start goes back to the pause menu.
    /// Posing, R2 picks a zombie, and Left and Right turn it and Up and Down step its frame. With props, R2 drops one
    /// where the camera looks, Left and Right choose which, Up and Down turn the last and Y takes it back. Nothing else
    /// moves.
    pub(super) fn photo(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        let (focus, yaw) = (self.focus(), self.view_yaw);
        let Some(p) = &mut self.photo else { return };
        if input.pressed.any(Buttons::B | Buttons::START) {
            (self.view_yaw, self.zoom, self.lift) = (p.heading, 1.0, 0.0);
            for (i, z) in std::mem::take(&mut p.posed) {
                self.zombies[i] = z;
            }
            self.photo = None;
            self.menu = Some(PAUSE_PHOTO);
            sounds.push(Sfx::Tick(0));
            return;
        }
        if input.pressed.contains(Buttons::L2) {
            p.setting = p.setting.next();
            sounds.push(Sfx::Tick(3));
        }
        let posing = p.setting == Setting::Pose;
        let propping = p.setting == Setting::Props;
        if propping && input.pressed.contains(Buttons::Y) {
            p.props.pop();
            sounds.push(Sfx::Tick(0));
        }
        if input.pressed.contains(Buttons::R2) {
            if propping {
                p.drop_prop(focus, yaw);
            } else if posing {
                let next = self.next_zombie();
                if let Some(p) = &mut self.photo {
                    p.picked = next;
                }
            } else {
                p.change();
            }
            sounds.push(Sfx::Tick(6));
        }
        let lifting = input.held.contains(Buttons::L1);
        let mut steer = *input;
        if let Some(i) = self.photo.as_ref().and_then(|p| p.picked).filter(|_| posing && !lifting) {
            let [x, y] = input.dir();
            self.pose(i, x * POSE_TURN * dt, y);
            // The turn strip still turns the camera.
            (steer.held, steer.stick) = (Buttons::NONE, [0.0; 2]);
        } else if let Some(p) = self.photo.as_mut().filter(|_| propping && !lifting) {
            let [x, y] = input.dir();
            p.handle_prop(x, -y * POSE_TURN * dt);
            (steer.held, steer.stick) = (Buttons::NONE, [0.0; 2]);
        }
        let mut cam = Orbit { yaw: self.view_yaw, zoom: self.zoom, lift: self.lift };
        cam.steer(&steer, dt, lifting, MIN_ZOOM);
        (self.view_yaw, self.zoom, self.lift) = (cam.yaw, cam.zoom, cam.lift);
        flow.capture = input.pressed.contains(Buttons::R1) || input.touch_started;
    }

    /// Where the camera looks, on the ground ahead of the player.
    fn focus(&self) -> [f32; 2] {
        [self.pos[0] + self.view_yaw.cos() * FOCUS, self.pos[1] + self.view_yaw.sin() * FOCUS]
    }

    /// The few zombies in the picture nearest where the camera looks, nearest first.
    fn pickable(&self) -> Vec<usize> {
        let focus = self.focus();
        let view_proj = view3d::scene(self).view_proj;
        let shown = |z: &Zombie| view3d::to_screen(view_proj, z.pos, 12.0).is_some_and(|([x, y], _)| (0.0..SCREEN_W).contains(&x) && (0.0..SCREEN_H).contains(&y));
        let mut near: Vec<usize> = (0..self.zombies.len()).filter(|&i| len(sub(self.zombies[i].pos, self.pos)) < PICK_REACH && shown(&self.zombies[i])).collect();
        near.sort_by(|&a, &b| len(sub(self.zombies[a].pos, focus)).total_cmp(&len(sub(self.zombies[b].pos, focus))));
        near.truncate(PICKS);
        near
    }

    /// The zombie after the picked one among those pickable, round to the first.
    fn next_zombie(&self) -> Option<usize> {
        let near = self.pickable();
        let picked = self.photo.as_ref()?.picked;
        let at = picked.and_then(|i| near.iter().position(|&j| j == i));
        near.get(at.map_or(0, |k| (k + 1) % near.len())).copied()
    }

    /// Turns zombie `i` by `turn` radians and steps it a frame on a fresh push of `y` up or down, through its walk
    /// and then its hurt and flash frames, first keeping it as the run left it, in its walk.
    fn pose(&mut self, i: usize, turn: f32, y: f32) {
        let (Some(p), Some(z)) = (self.photo.as_mut(), self.zombies.get_mut(i)) else { return };
        if !p.posed.iter().any(|q| q.0 == i) {
            p.posed.push((i, z.clone()));
            (z.flash, z.hurt, z.stagger) = (0.0, 0.0, 0.0);
        }
        let d = if len(z.dir) > 1e-4 { norm(z.dir) } else { [0.0, 1.0] };
        let (s, c) = turn.sin_cos();
        z.dir = [d[0] * c - d[1] * s, d[0] * s + d[1] * c];
        let push = push_of(y);
        if push != 0 && push != p.pushed {
            let walk = z.kind.stats().look.walk[Facing::Front.index()].len().max(1);
            let now = if z.flash > 0.0 { walk + 1 } else if z.hurt > 0.0 { walk } else { z.step() % walk };
            let frames = walk + 2;
            let next = (now + if push < 0 { 1 } else { frames - 1 }) % frames;
            (z.flash, z.hurt) = (if next == walk + 1 { 1.0 } else { 0.0 }, if next == walk { 1.0 } else { 0.0 });
            if next < walk {
                let rate = (crate::tuning::tuning().walk_fps * z.kind.stats().speed / 20.0).max(1e-3);
                z.anim = (next as f32 + 0.5) / rate;
            }
        }
        p.pushed = push;
    }

    /// The picked zombie's name, or "none".
    fn posing(&self) -> &str {
        let picked = self.photo.as_ref().and_then(|p| p.picked);
        picked.and_then(|i| self.zombies.get(i)).map_or("none", |z| z.kind.stats().name.as_str())
    }

    /// The frozen 3D view with no HUD on top, and the map below; the guides show unless `clean`.
    pub(super) fn photo_frame(&self, f: &mut Frame, clean: bool) {
        let mut scene = view3d::scene(self);
        let photo = self.photo.as_ref();
        if let Some(p) = photo {
            p.light(&mut scene);
            for d in &p.props {
                view3d::dropped(&mut scene.cards, scene.eye, d.art, d.stand, d.at, d.angle);
            }
        }
        f.top.clear = [scene.fog[0], scene.fog[1], scene.fog[2], 1.0];
        let view_proj = scene.view_proj;
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
        let picked = p.picked.filter(|_| p.setting == Setting::Pose).and_then(|i| self.zombies.get(i));
        let marked = match (p.setting, picked, p.props.last()) {
            (Setting::Pose, Some(z), _) => Some((z.pos, z.art(Facing::Front).h() * if z.boss { 2.0 } else { 1.0 })),
            (Setting::Props, _, Some(d)) => Some((d.at, d.art.h())),
            _ => None,
        };
        if let Some(([sx, sy], _)) = marked.and_then(|(at, tall)| view3d::to_screen(view_proj, at, tall + 6.0)) {
            // A marker pointing down at the posed zombie's head, or the prop dropped last.
            for (i, w) in [18.0, 12.0, 6.0].into_iter().enumerate() {
                f.top.rect(sx - w / 2.0, sy - 18.0 + i as f32 * 6.0, w, 6.0, GUIDE);
            }
        }
        let shoot = match (self.saves_photos, self.taps) {
            (true, true) => "R1 or tap: take a photo",
            (true, false) => "R1: take a photo",
            (false, true) => "R1 or tap: hide these guides",
            (false, false) => "R1: hide these guides",
        };
        let keys = match (p.setting, picked) {
            (Setting::Pose, Some(_)) => "Left / Right: turn it   Up / Down: step its frame   R2: next zombie",
            (Setting::Pose, None) => "R2: pick the zombie nearest the middle   Left / Right: turn   Up / Down: zoom",
            (Setting::Props, _) if p.prop == TAKE_BACK => "R2 or Y: take back the last prop   Left / Right: choose a prop",
            (Setting::Props, _) => "R2: drop it ahead   Left / Right: choose   Up / Down: turn it   Y: take back",
            _ => "Left / Right: turn   Up / Down: zoom   L1 + Up / Down: height",
        };
        let [settings, more] = p.settings_lines(self.posing());
        let lines = ["PHOTO MODE".to_string(), keys.into(), settings, more, format!("L2: choose  R2: change   {shoot}   B or Start: back")];
        // On one screen the help goes over the picture, where the guides already are.
        let (c, y) = if self.single { (&mut f.top, SCREEN_H - 184.0) } else { (&mut f.bottom, 40.0) };
        c.rect(0.0, y - 10.0, SCREEN_W, 176.0, [0.0, 0.0, 0.0, 0.55]);
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
        assert_eq!(p(&h).0, Setting::Pose);
        press(&mut h, Buttons::L2);
        assert_eq!(p(&h).0, Setting::Props);
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
        let text = help(&h);
        assert!(text.contains("L2: choose") && text.contains("[Fog: Usual]") && text.contains("Time: Now") && text.contains("Pose: none"), "{text}");
    }

    fn help(h: &Harvest) -> String {
        let mut f = Frame::default();
        h.photo_frame(&mut f, false);
        let text = f.bottom.shapes.iter().chain(&f.top.shapes).filter_map(|s| if let crate::Shape::Text { text, .. } = s { Some(text.clone()) } else { None });
        text.collect::<Vec<_>>().join("\n")
    }

    fn hold(h: &mut Harvest, held: Buttons, frames: usize) {
        for _ in 0..frames {
            h.update(&Input { held, ..Default::default() }, DT, &mut Vec::new());
        }
    }

    /// Photo mode on the Pose setting with the zombie nearest the camera's focus picked.
    fn posing(seed: u64) -> Harvest {
        let mut h = photo_mode(seed);
        let nearest = h.zombies.iter().map(|z| sub(z.pos, h.pos)).min_by(|a, b| len(*a).total_cmp(&len(*b))).unwrap();
        h.view_yaw = nearest[1].atan2(nearest[0]);
        for _ in 0..3 {
            press(&mut h, Buttons::L2);
        }
        assert_eq!(h.photo.as_ref().unwrap().setting, Setting::Pose);
        assert!(help(&h).contains("R2: pick"));
        press(&mut h, Buttons::R2);
        assert!(h.photo.as_ref().unwrap().picked.is_some(), "a zombie is near enough to pick");
        h
    }

    #[test]
    fn r2_picks_the_zombies_nearest_the_cameras_focus_in_turn() {
        let mut h = posing(10);
        let first = h.photo.as_ref().unwrap().picked.unwrap();
        let near = h.pickable();
        assert_eq!(near[0], first, "the first is the nearest the focus");
        let scene = view3d::scene(&h);
        let ahead = |i: usize| view3d::to_screen(scene.view_proj, h.zombies[i].pos, 12.0).is_some();
        assert!(near.iter().all(|&i| ahead(i)), "and every pick is in front of the camera");
        let mut seen = vec![first];
        for _ in 1..near.len() {
            press(&mut h, Buttons::R2);
            seen.push(h.photo.as_ref().unwrap().picked.unwrap());
        }
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), seen.len(), "each press picks another: {seen:?}");
        press(&mut h, Buttons::R2);
        assert_eq!(h.photo.as_ref().unwrap().picked, Some(first), "and then round to the first");
        assert!(help(&h).contains(&format!("[Pose: {}]", h.zombies[first].kind.stats().name)));
    }

    #[test]
    fn a_posed_zombie_turns_and_steps_its_frame_and_the_run_gets_it_back() {
        let mut h = posing(11);
        let i = h.photo.as_ref().unwrap().picked.unwrap();
        let before: Vec<([f32; 2], f32)> = h.zombies.iter().map(|z| (z.dir, z.anim)).collect();
        let yaw = h.view_yaw;
        hold(&mut h, Buttons::RIGHT, 20);
        assert_eq!(h.view_yaw, yaw, "Left and Right turn the zombie, not the camera");
        assert!(len(sub(norm(h.zombies[i].dir), norm(before[i].0))) > 0.5, "the zombie turned");
        let frames = frame_count(&h, i);
        let step = frame(&h, i);
        hold(&mut h, Buttons::UP, 10);
        assert_eq!(frame(&h, i), (step + 1) % frames, "a held Up steps one frame");
        hold(&mut h, Buttons::NONE, 1);
        hold(&mut h, Buttons::DOWN, 1);
        hold(&mut h, Buttons::NONE, 1);
        hold(&mut h, Buttons::DOWN, 1);
        assert_eq!(frame(&h, i), (step + frames - 1) % frames, "and each Down one back");
        let mut f = Frame::default();
        h.photo_frame(&mut f, true);
        assert!(f.top.shapes.is_empty(), "the capture has no marker");
        press(&mut h, Buttons::B);
        let after: Vec<([f32; 2], f32)> = h.zombies.iter().map(|z| (z.dir, z.anim)).collect();
        assert_eq!(after, before, "leaving gives the run its horde back as it was");
    }

    /// How many frames Up and Down step a posed zombie through: its walk, then hurt and flash.
    fn frame_count(h: &Harvest, i: usize) -> usize {
        h.zombies[i].kind.stats().look.walk[Facing::Front.index()].len() + 2
    }

    /// Which of those zombie `i` shows.
    fn frame(h: &Harvest, i: usize) -> usize {
        let (z, walk) = (&h.zombies[i], frame_count(h, i) - 2);
        if z.flash > 0.0 { walk + 1 } else if z.hurt > 0.0 { walk } else { z.step() % walk }
    }

    #[test]
    fn up_steps_a_posed_zombie_past_its_walk_to_its_hurt_and_flash_frames_and_round() {
        let mut h = posing(13);
        let i = h.photo.as_ref().unwrap().picked.unwrap();
        let (look, frames) = (h.zombies[i].kind.stats().look.clone(), frame_count(&h, i));
        let mut shown = Vec::new();
        for _ in 0..frames {
            hold(&mut h, Buttons::UP, 1);
            hold(&mut h, Buttons::NONE, 1);
            shown.push(h.zombies[i].art(Facing::Front));
        }
        assert!(shown.contains(&look.hurt[Facing::Front.index()]) && shown.contains(&look.flash[Facing::Front.index()]), "{shown:?}");
        assert!(look.walk[Facing::Front.index()].contains(shown.last().unwrap()), "and round to its walk");
        press(&mut h, Buttons::B);
        assert_eq!((h.zombies[i].hurt, h.zombies[i].flash), (0.0, 0.0), "the run gets it back unhurt");
    }

    /// Photo mode on the Props setting.
    fn propping(seed: u64) -> Harvest {
        let mut h = photo_mode(seed);
        for _ in 0..4 {
            press(&mut h, Buttons::L2);
        }
        assert_eq!(h.photo.as_ref().unwrap().setting, Setting::Props);
        h
    }

    fn dropped(h: &Harvest) -> Vec<Dropped> {
        h.photo.as_ref().unwrap().props.clone()
    }

    #[test]
    fn r2_drops_the_chosen_prop_where_the_camera_looks_and_it_stands_in_the_picture() {
        let mut h = propping(14);
        let (cards, map) = (top_scene(&h).cards.len(), h.map.props.len());
        assert!(help(&h).contains("[Props: Barrier (0 in)]"), "{}", help(&h));
        press(&mut h, Buttons::R2);
        let focus = h.focus();
        let barrier = super::sprite("BARRIER");
        assert_eq!(dropped(&h), vec![Dropped { kind: 0, art: barrier, stand: view3d::drop_stand(barrier), at: focus, angle: h.view_yaw + std::f32::consts::FRAC_PI_2 }]);
        assert!(top_scene(&h).cards.len() > cards, "the barrier stands in the frozen view");
        hold(&mut h, Buttons::RIGHT, 10);
        hold(&mut h, Buttons::NONE, 1);
        hold(&mut h, Buttons::RIGHT, 1);
        assert_eq!(h.photo.as_ref().unwrap().prop, 2, "a held Right chooses once");
        hold(&mut h, Buttons::LEFT, 1);
        assert_eq!(h.photo.as_ref().unwrap().prop, 1);
        press(&mut h, Buttons::R2);
        assert_eq!(dropped(&h).iter().map(|d| d.kind).collect::<Vec<_>>(), [0, 1]);
        let (yaw, angle) = (h.view_yaw, dropped(&h)[1].angle);
        hold(&mut h, Buttons::UP, 20);
        assert_eq!(h.view_yaw, yaw, "Up and Down turn the prop, not the camera");
        assert!(dropped(&h)[1].angle != angle && dropped(&h)[0].angle == yaw + std::f32::consts::FRAC_PI_2, "only the last one turns");
        assert!(help(&h).contains("Barrel (2 in)"), "{}", help(&h));
        assert_eq!(h.map.props.len(), map, "the run's map never gets them");
    }

    #[test]
    fn y_or_the_take_back_choice_removes_the_last_prop_and_leaving_removes_them_all() {
        let mut h = propping(15);
        let before: Vec<(f32, f32)> = h.map.props.iter().map(|p| (p.x, p.y)).collect();
        for _ in 0..3 {
            press(&mut h, Buttons::R2);
        }
        press(&mut h, Buttons::Y);
        assert_eq!(dropped(&h).len(), 2, "Y takes back the last");
        hold(&mut h, Buttons::LEFT, 1);
        assert_eq!(h.photo.as_ref().unwrap().prop, TAKE_BACK, "Left from the first comes round to taking back");
        assert!(help(&h).contains("take back the last prop"));
        press(&mut h, Buttons::R2);
        assert_eq!(dropped(&h).len(), 1, "and R2 on it does too, for a touch screen with no Y");
        let cards = top_scene(&h).cards.len();
        press(&mut h, Buttons::B);
        assert!(view3d::scene(&h).cards.len() < cards, "leaving takes the props out of the view");
        assert_eq!(h.map.props.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>(), before);
        press(&mut h, Buttons::A);
        assert!(dropped(&h).is_empty(), "and photo mode opens without them");
    }

    #[test]
    fn every_prop_offered_is_a_sprite_the_game_has_and_stands_in_3d() {
        for (name, sprite) in PROPS {
            let a = super::sprite(sprite);
            assert!(a != art::WHITE, "{name}: no sprite {sprite}");
            let mut cards = Vec::new();
            view3d::dropped(&mut cards, glam::Vec3::new(2.0, 1.5, 7.0), a, view3d::drop_stand(a), [2.0 * TILE, 2.0 * TILE], 0.3);
            assert!(!cards.is_empty(), "{name} draws nothing");
            let middle = cards.iter().flat_map(|c| c.corners).sum::<glam::Vec3>() / (cards.len() * 4) as f32;
            assert!((middle.x - 2.0).abs() < 1.0 && (middle.z - 2.0).abs() < 1.0, "{name} stands round where it was dropped: {middle}");
        }
    }

    #[test]
    fn a_dropped_props_front_faces_the_camera_whichever_way_it_looks() {
        let upright: Vec<Art> = PROPS.iter().map(|p| super::sprite(p.1)).filter(|&a| matches!(view3d::drop_stand(a), Stand::Prop { .. })).take(4).collect();
        assert!(!upright.is_empty());
        let at = [6.0 * TILE, 6.0 * TILE];
        let center = glam::Vec3::new(6.0, 0.0, 6.0);
        for a in upright {
            for k in 0..8 {
                let yaw = k as f32 * std::f32::consts::TAU / 8.0 - 3.0;
                let mut p = Photo::new(yaw);
                p.prop = PROPS.iter().position(|q| super::sprite(q.1) == a).unwrap();
                p.drop_prop(at, yaw);
                let d = p.props[0];
                let eye = center - glam::Vec3::new(yaw.cos(), 0.0, yaw.sin()) * 3.0 + glam::Vec3::Y;
                let mut cards = Vec::new();
                view3d::dropped(&mut cards, eye, d.art, d.stand, d.at, d.angle);
                let area = |c: &crate::scene3d::Card| (c.corners[1] - c.corners[0]).cross(c.corners[3] - c.corners[0]).length();
                let front = cards.iter().max_by(|x, y| area(x).total_cmp(&area(y))).unwrap();
                let toward = glam::Vec3::new(-yaw.cos(), 0.0, -yaw.sin());
                assert!(front.normal.normalize().dot(toward) > 0.99, "{a:?} at yaw {yaw}: its front faces {}", front.normal);
            }
        }
    }

    #[test]
    fn l1_still_raises_the_camera_while_posing() {
        let mut h = posing(12);
        hold(&mut h, Buttons::L1 | Buttons::UP, 20);
        assert!(h.lift > 0.0);
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
