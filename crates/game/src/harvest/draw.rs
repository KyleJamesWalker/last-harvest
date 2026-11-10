//! Both screens' 2D drawing: the map below, and the HUD, numbers and overlays over the 3D view above.

use std::f32::consts::{PI, TAU};

use super::juice::{self, Tone};
use super::map::TILE;
use super::sight::Mark;
use crate::art::Stand;
use super::powerups::{Kind, Zone, book};
use super::weapons::{BOLT_LIFE, Hazard};
use super::{CHOICE_ROW, CHOICE_TOP, Choice, DropKind, Facing, GOLD, LEVEL_KEYS, LevelKey, level_key_shows, PAUSE_CHOICES, PAUSE_ROW, Harvest, PICK_TIME, PROP_FLASH, Phase, RED, map_scale, TELL_TIME, TURNED, Thrown, WHITE, clock, player_pose, player_worn, view3d, worn_corner};
use crate::art::{self, Art};
use crate::{Align, Buttons, Canvas, Color, Frame, SCREEN_H, SCREEN_W, pad};

impl Harvest {
    /// The run's screens; in photo mode, a `clean` frame is the capture, with no guides.
    pub fn draw(&self, f: &mut Frame, clean: bool) {
        if self.photo.is_some() {
            self.photo_frame(f, clean);
        } else {
            self.draw_screens(f);
            self.draw_range(&mut f.top, &mut f.bottom);
        }
        if self.single {
            self.single_frame(f);
        }
    }

    /// The top screen is the 3D view; the bottom is the map, where a tap throws a grenade.
    fn draw_screens(&self, f: &mut Frame) {
        let scene = view3d::scene(self);
        let view_proj = scene.view_proj;
        f.top.clear = [scene.fog[0], scene.fog[1], scene.fog[2], 1.0];
        self.popups(&mut f.top, &scene);
        self.motes(&mut f.top, &scene);
        f.top.cards = Some(scene);
        self.vignette(&mut f.top);
        self.hud(&mut f.top);
        self.way_home(&mut f.top, view_proj);
        self.barrel_mark(&mut f.top, view_proj);
        self.ticket_mark(&mut f.top, view_proj);
        self.banner(&mut f.top);
        self.overlays(&mut f.top);
        self.paused_top(&mut f.top);

        let c = &mut f.bottom;
        c.clear = [0.0, 0.0, 0.0, 1.0];
        let tint = self.map_tint();
        // Sight stops counting once the run ends, so the map shows the night as it is.
        let fog = !matches!(self.phase, Phase::Over { .. });
        let cam = self.camera();
        self.world(c, cam, map_scale(), tint, fog);
        self.view_cone(c, cam);
        self.home_on_map(c, cam);
        self.ticket_on_map(c, cam);
        self.grenade_tray(c);
        self.kit_tray(c);
        self.aim_mark(c, cam);
        self.paused_bottom(c);
        if let Phase::LevelUp { choices, selected, t, picked } = &self.phase {
            let fade = picked.map_or(1.0, |p| 1.0 - p / PICK_TIME);
            c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.6 * (t / 0.2).min(1.0) * fade]);
            let how = match (self.banishing, self.favoring, self.taps) {
                (true, ..) => "Tap the card to banish, or Banish to keep them all",
                (_, true, _) => "Tap the card to take and favor, or Favor to cancel",
                (.., true) => &pad::hint("Tap an upgrade, or Up / Down and {X}"),
                (.., false) => &pad::hint("Up / Down and {X}: choose an upgrade"),
            };
            c.label(how, SCREEN_W / 2.0, 60.0, 32.0, [1.0, 1.0, 1.0, fade], Align::Center);
            self.choices(c, choices, *selected, *t, *picked, CHOICE_TOP);
            self.level_keys(c, fade);
        }
    }

    /// Banish, Reroll and Favor under the cards, each with its pad button and how many the run has left. A key that
    /// `tuning.txt` sets to 0 isn't drawn, nor Favor before `favor_level`.
    fn level_keys(&self, c: &mut Canvas, fade: f32) {
        for (key, [x, y, w, h]) in LEVEL_KEYS {
            let (name, pad, left) = match key {
                LevelKey::Banish => ("Banish", pad::label(Buttons::L1), self.banishes),
                LevelKey::Reroll => ("Reroll", pad::label(Buttons::R1), self.rerolls),
                LevelKey::Favor => ("Favor", pad::label(Buttons::Y), self.favors),
            };
            if !level_key_shows(key, self.level) {
                continue;
            }
            let a = if left > 0 { fade } else { 0.4 * fade };
            let fill = match key {
                LevelKey::Banish if self.banishing => [RED[0], RED[1], RED[2], 0.6 * a],
                LevelKey::Favor if self.favoring => [GOLD[0], GOLD[1], GOLD[2], 0.6 * a],
                _ => [1.0, 1.0, 1.0, 0.1 * a],
            };
            c.rect(x, y, w, h, fill);
            c.label(pad, x + 16.0, y + 10.0, 26.0, [1.0, 1.0, 1.0, 0.6 * a], Align::Left);
            c.label(format!("{name}  {left}"), x + w / 2.0, y + h / 2.0 - 20.0, 40.0, [WHITE[0], WHITE[1], WHITE[2], a], Align::Center);
        }
    }

    /// The sky's wash multiplied into the map's colors: the same look as a translucent layer, without its fill cost.
    pub(super) fn map_tint(&self) -> Color {
        let wash = view3d::sky(self).wash;
        [0, 1, 2, 3].map(|i| if i == 3 { 1.0 } else { 1.0 - wash[3] + wash[3] * wash[i] })
    }

    /// Damage numbers over the 3D view, popping up and drifting away.
    fn popups(&self, c: &mut Canvas, scene: &crate::scene3d::CardScene) {
        for p in &self.juice.popups {
            let (rise, pop) = p.rise();
            let Some(([x, y], dist)) = view3d::to_screen(scene.view_proj, p.pos, rise) else { continue };
            let size = 30.0 * pop * (3.5 / dist).clamp(0.45, 1.6);
            let a = p.fade();
            let (color, text) = match p.tone {
                Tone::Hit => ([1.0, 1.0, 1.0, a], p.value.to_string()),
                Tone::Crit => ([1.0, 0.55, 0.15, a], format!("{}!", p.value)),
                Tone::Hurt => ([1.0, 0.25, 0.2, a], format!("-{}", p.value)),
                Tone::Heal => ([0.45, 1.0, 0.5, a], format!("+{}", p.value)),
            };
            c.label(text.clone(), x + 2.0, y + 2.0, size, [0.0, 0.0, 0.0, a * 0.8], Align::Center);
            c.label(text, x, y, size, color, Align::Center);
        }
    }

    /// Collected money arcing up into the XP bar.
    fn motes(&self, c: &mut Canvas, scene: &crate::scene3d::CardScene) {
        let end = [SCREEN_W * (self.xp_shown / self.next_xp as f32).clamp(0.0, 1.0), 7.0];
        for m in &self.juice.motes {
            let Some((start, _)) = view3d::to_screen(scene.view_proj, m.pos, 8.0) else { continue };
            let k = m.age / juice::MOTE_TIME;
            let e = k * k;
            let lift = (k * PI).sin() * 60.0;
            let (x, y) = (start[0] + (end[0] - start[0]) * e, start[1] + (end[1] - start[1]) * e - lift);
            c.circle(x, y, 7.0 - 3.0 * k, [1.0, 0.9, 0.4, 1.0]);
            c.circle(x, y, 3.0, [1.0, 1.0, 0.9, 1.0]);
        }
    }

    /// Red creeping in from the edges: a flash when hit, and a pulse in time with the heart when nearly dead.
    fn vignette(&self, c: &mut Canvas) {
        let health = self.hp / self.kit.max_hp();
        let low = ((0.35 - health) / 0.35).clamp(0.0, 1.0);
        let beat = (1.0 - (self.heartbeat.max(0.0) / (0.5 + health * 2.0)) * 4.0).max(0.0);
        let hit = self.juice.hurt / 0.35;
        let a = (low * (0.3 + 0.4 * beat) + hit * 0.5).min(0.9);
        if hit > 0.0 {
            c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.8, 0.05, 0.05, hit * 0.18]);
        }
        if a <= 0.01 {
            return;
        }
        let red = [0.6, 0.0, 0.0, a / 6.0];
        for i in 1..=6 {
            let t = i as f32 * 26.0;
            c.rect(0.0, 0.0, SCREEN_W, t, red);
            c.rect(0.0, SCREEN_H - t, SCREEN_W, t, red);
            c.rect(0.0, t, t, SCREEN_H - 2.0 * t, red);
            c.rect(SCREEN_W - t, t, t, SCREEN_H - 2.0 * t, red);
        }
    }

    fn banner(&self, c: &mut Canvas) {
        let Some(b) = &self.juice.banner else { return };
        let k = (b.age / 0.18).min(1.0);
        let fade = ((b.life - b.age) / 0.4).clamp(0.0, 1.0);
        let color = [b.color[0], b.color[1], b.color[2], b.color[3] * fade];
        if b.big {
            let scale = 16.0 * back_out(k);
            let y = 300.0 - scale * 2.5;
            c.text_centered(y + scale * 0.4, scale, [0.0, 0.0, 0.0, 0.6 * fade], &b.text);
            c.text_centered(y, scale, color, &b.text);
        } else {
            let x = SCREEN_W / 2.0 - (1.0 - back_out(k)) * 500.0;
            c.rect(0.0, 112.0, SCREEN_W, 64.0, [0.0, 0.0, 0.0, 0.45 * fade * k]);
            c.label(b.text.clone(), x, 120.0, 44.0, color, Align::Center);
        }
    }

    /// Dots along the edges of what the 3D view can see.
    pub(super) fn view_cone(&self, c: &mut Canvas, cam: [f32; 2]) {
        let half = self.ahead_half();
        let lit = if self.beam > 0.0 { [1.0, 1.0, 0.8, 1.0] } else { [1.0, 0.9, 0.6, 0.8] };
        let sight = crate::tuning::tuning().map_sight;
        for edge in [-half, half] {
            let a = self.view_yaw + edge;
            for k in 1..9 {
                let r = k as f32 / 8.0 * sight;
                let (x, y) = ((self.pos[0] + a.cos() * r - cam[0]) * map_scale(), (self.pos[1] - 6.0 + a.sin() * r - cam[1]) * map_scale());
                c.circle(x, y, 5.0, [lit[0], lit[1], lit[2], lit[3] - k as f32 * 0.08]);
            }
        }
    }

    /// Grenades ready, bottom right, with the next one filling in.
    fn grenade_tray(&self, c: &mut Canvas) {
        let a = art::GRENADE;
        let slot = a.w() * 5.0 + 12.0;
        let most = self.kit.grenades();
        let panel = most * slot + 24.0;
        c.rect(SCREEN_W - 40.0 - panel + 12.0, SCREEN_H - 40.0 - a.h() * 5.0, panel, a.h() * 5.0 + 34.0, [0.0, 0.0, 0.0, 0.6]);
        for i in 0..most as usize {
            let x = SCREEN_W - 40.0 - (i as f32 + 1.0) * slot;
            let y = SCREEN_H - 24.0 - a.h() * 5.0;
            let fill = (self.grenades - i as f32).clamp(0.0, 1.0);
            let color = if fill >= 1.0 { WHITE } else { [0.4, 0.4, 0.4, 0.6] };
            c.art(a, x, y, 5.0, false, color);
            if fill > 0.0 && fill < 1.0 {
                c.rect(x, y + a.h() * 5.0 + 4.0, a.w() * 5.0 * fill, 5.0, GOLD);
            }
        }
    }

    /// The weapons and passives carried, bottom left, each with its level.
    fn kit_tray(&self, c: &mut Canvas) {
        let rows: [Vec<(usize, u32)>; 2] = [self.kit.weapons.iter().map(|a| (a.def, a.level)).collect(), self.kit.passives.clone()];
        let size = 40.0;
        for (r, row) in rows.iter().enumerate() {
            let y = SCREEN_H - 24.0 - (2 - r) as f32 * (size + 10.0);
            powerup_icons(c, 24.0, y, size, row.iter().copied(), 1.0);
        }
    }

    /// The run's weapons over its passives, at their levels, at the top right of the end screen.
    fn build_panel(&self, c: &mut Canvas, a: f32) {
        let (size, gap) = (40.0, 8.0);
        let x = SCREEN_W - 24.0 - 6.0 * (size + gap) + gap;
        let top = 196.0;
        c.rect(x - 8.0, top - 8.0, SCREEN_W - 16.0 - x + 8.0, 30.0 + 2.0 * (size + 10.0) + 16.0, [0.0, 0.0, 0.0, 0.45 * a]);
        c.label("Build", x, top, 24.0, [1.0, 1.0, 1.0, 0.6 * a], Align::Left);
        let rows = [self.kit.weapons.iter().map(|w| (w.def, w.level)).collect::<Vec<_>>(), self.kit.passives.clone()];
        for (r, row) in rows.into_iter().enumerate() {
            powerup_icons(c, x, top + 30.0 + r as f32 * (size + 10.0), size, row, a);
        }
    }

    /// Draws the map and everything on it, with `cam` at the top-left and each art pixel `scale` screen pixels across.
    /// `fog` shows zombies as remembered, other moving things only while in sight, and leaves out bullets, blood,
    /// particles and the dead; terrain and props always show.
    pub(super) fn world(&self, c: &mut Canvas, cam: [f32; 2], scale: f32, tint: Color, fog: bool) {
        let known = |p: [f32; 2]| !fog || self.in_sight(p);
        let shade = |k: Color| [k[0] * tint[0], k[1] * tint[1], k[2] * tint[2], k[3]];
        let lit = shade(WHITE);
        let (view_w, view_h) = (SCREEN_W / scale, SCREEN_H / scale);
        let at = |x: f32, y: f32| ((x - cam[0]) * scale, (y - cam[1]) * scale);

        let (tx0, ty0) = ((cam[0] / TILE) as usize, (cam[1] / TILE) as usize);
        let (tx1, ty1) = (((cam[0] + view_w) / TILE).ceil() as usize, ((cam[1] + view_h) / TILE).ceil() as usize);
        for ty in ty0..ty1.min(self.map.h) {
            for tx in tx0..tx1.min(self.map.w) {
                let (ground, overlay) = self.map.tile(tx, ty);
                let (sx, sy) = at(tx as f32 * TILE, ty as f32 * TILE);
                c.art(ground, sx, sy, scale, false, lit);
                if let Some(o) = overlay {
                    c.art(o, sx, sy, scale, false, lit);
                }
            }
        }
        let visible = |x: f32, y: f32, w: f32, h: f32| x + w > cam[0] && x < cam[0] + view_w && y + h > cam[1] && y < cam[1] + view_h;
        let stains = self.stains.iter().filter(|_| !fog).map(|&(a, x, y, [r, g, b])| (a, x, y, [r, g, b, 1.0]));
        for (a, x, y, color) in self.map.decals.iter().map(|&(a, x, y)| (a, x, y, WHITE)).chain(stains) {
            if visible(x, y, a.w(), a.h()) {
                let (sx, sy) = at(x, y);
                c.art(a, sx, sy, scale, false, shade(color));
            }
        }
        if let Some(s) = self.shown.as_ref().filter(|s| s.stand == Stand::Ground && !s.frames.is_empty()) {
            let a = s.frame(self.t);
            let (sx, sy) = at(s.pos[0] - a.w() / 2.0, s.pos[1] - a.h() / 2.0);
            c.art(a, sx, sy, scale, false, lit);
        }
        for t in self.tells.iter().filter(|t| known(t.pos)) {
            let k = (t.t / TELL_TIME).min(1.0);
            let (sx, sy) = at(t.pos[0], t.pos[1]);
            c.circle(sx, sy, (2.0 + 3.0 * k) * scale, [0.2, 0.13, 0.07, 0.8]);
        }
        for z in self.burrowed.iter().filter(|z| z.rise == 0.0 && known(z.pos)) {
            let a = super::effects::effects().mound.looping(self.t + z.id as f32 * 0.37);
            let (sx, sy) = at(z.pos[0] - a.w() / 2.0, z.pos[1] - a.h() + 1.0);
            c.art(a, sx, sy, scale, z.dir[0] < 0.0, lit);
        }
        for p in &self.patches {
            let fade = ((p.life - p.age) / 0.5).clamp(0.0, 1.0);
            match p.hazard {
                Hazard::Tacks => {
                    let a = super::effects::effects().tacks.looping(self.t);
                    let (sx, sy) = at(p.pos[0] - a.w() / 2.0, p.pos[1] - a.h() / 2.0);
                    c.art(a, sx, sy, scale, false, shade([1.0, 1.0, 1.0, fade]));
                }
                Hazard::Fire => {
                    let fire = super::effects::effects().fire;
                    let a = fire.looping(self.t + p.pos[0] / fire.fps);
                    let (sx, sy) = at(p.pos[0] - a.w() / 2.0, p.pos[1] - a.h());
                    c.art(a, sx, sy, scale, false, [1.0, 1.0, 1.0, fade]);
                }
                Hazard::Flare => {
                    let (sx, sy) = at(p.pos[0], p.pos[1]);
                    c.circle(sx, sy, p.radius * scale, [1.0, 0.3, 0.2, 0.12 * fade]);
                    let a = super::effects::effects().flare.looping(self.t);
                    let (sx, sy) = at(p.pos[0] - a.w() / 2.0, p.pos[1] - a.h());
                    c.art(a, sx, sy, scale, false, WHITE);
                }
            }
        }
        for corpse in self.juice.corpses.iter().filter(|_| !fog) {
            let fade = 1.0 - corpse.sunk() / 8.0;
            let a = corpse.dead.map_or(corpse.art, |(dead, dir)| dead[Facing::of(dir).0.index()]);
            let (sx, sy) = at(corpse.pos[0], corpse.pos[1] - corpse.z - 3.0);
            let turn = if corpse.fall[0] < 0.0 { -1.0 } else { 1.0 };
            let angle = if corpse.z > 0.0 { corpse.spin } else { corpse.tipped() * PI / 2.0 * turn };
            c.art_rotated(a, sx, sy, scale, angle, shade([0.8, 0.8, 0.8, fade]));
        }
        for d in self.drops.iter().filter(|d| known(d.pos)) {
            let a = match d.kind {
                DropKind::Money(_) => super::effects::effects().money.once(d.age),
                DropKind::Health => super::effects::effects().health.looping(d.age),
                DropKind::Chest => super::effects::effects().chest.looping(d.age),
            };
            let (sx, sy) = at(d.pos[0] - a.w() / 2.0, d.pos[1] - a.h() - d.z);
            c.art(a, sx, sy, scale, false, lit);
        }

        let shown: Vec<(Mark, f32)> = if fog {
            self.marks.iter().map(|m| (*m, m.fade())).collect()
        } else {
            let live = self.zombies.iter().enumerate().filter(|&(i, _)| !self.lies_dead(i)).map(|(_, z)| z).chain(self.burrowed.iter().filter(|z| z.rise > 0.0));
            live.map(|z| (Mark::of(z), 1.0)).collect()
        };
        for (m, fade) in shown.iter().filter(|(m, _)| m.leader || m.boss) {
            let (sx, sy) = at(m.pos[0], m.pos[1]);
            let pulse = 0.35 + 0.25 * (self.t * 6.0).sin();
            let (r, ring) = if m.boss { (16.0, HARVESTER) } else { (9.0, GOLD) };
            c.circle(sx, sy, r * scale, [ring[0], ring[1], ring[2], pulse * fade]);
        }
        // Props, zombies and the player, drawn back to front by where they meet the ground.
        // The last fields are how much of each sprite shows, as zombies still in the ground show only their top,
        // and how bright, as a prop just hit flashes.
        let mut sprites: Vec<(f32, Art, f32, f32, bool, f32, Color)> = Vec::new();
        for p in &self.map.props {
            let a = p.frame(self.t);
            if visible(p.x, p.y, a.w(), a.h()) {
                let b = if p.flash > 0.0 { PROP_FLASH } else { 1.0 };
                sprites.push((p.base, a, p.x, p.y, false, 1.0, [b, b, b, 1.0]));
            }
        }
        for r in &self.barrels {
            let a = if r.flash > 0.0 { art::BARREL_FLASH } else { art::BARREL };
            if visible(r.pos[0], r.pos[1], TILE, TILE) {
                sprites.push((r.pos[1] + a.h(), a, r.pos[0], r.pos[1], false, 1.0, [1.0; 4]));
            }
        }
        for &(m, fade) in &shown {
            let a = m.art;
            sprites.push((m.pos[1], a, m.pos[0] - a.w() / 2.0, m.pos[1] - a.h() * m.rise + 2.0, m.flip, m.rise, [1.0, 1.0, 1.0, fade]));
        }
        if let Some(s) = self.shown.as_ref().filter(|s| s.stand != Stand::Ground && !s.frames.is_empty()) {
            let a = s.frame(self.t);
            let (x, y) = (s.pos[0] - a.w() / 2.0, s.pos[1] - a.h());
            sprites.push((s.pos[1], a, x, y, false, 1.0, [1.0; 4]));
            if let Some(at) = a.worn_at().filter(|_| s.worn) {
                let [gx, gy] = worn_corner(at, a, false);
                sprites.push((s.pos[1] + 0.01, art::GRENADE_WORN, x + gx, y + gy, false, 1.0, [1.0; 4]));
            }
        }
        for b in &self.birds {
            let a = b.art();
            sprites.push((b.pos[1], a, b.pos[0] - a.w() / 2.0, b.pos[1] - a.h() - b.z, b.flip, 1.0, [1.0; 4]));
        }
        for d in &self.decoys {
            let a = art::SCARECROW;
            sprites.push((d.pos[1], a, d.pos[0] - a.w() / 2.0, d.pos[1] - a.h() + 2.0, false, 1.0, [1.0; 4]));
        }
        for d in &self.dogs {
            let a = d.art();
            sprites.push((d.pos[1], a, d.pos[0] - a.w() / 2.0, d.pos[1] - a.h() + 2.0, d.heading[0] < 0.0, 1.0, [1.0; 4]));
        }
        for (i, (p, flip)) in self.crows().into_iter().enumerate() {
            let crow = super::effects::effects().crow;
            let a = crow.looping(self.t + i as f32 / crow.fps);
            sprites.push((p[1], a, p[0] - a.w() / 2.0, p[1] - a.h() - 12.0, flip, 1.0, [1.0; 4]));
        }
        let (facing, flip) = self.facing;
        let (a, rise) = player_pose(self, facing);
        if self.shown.as_ref().is_none_or(|s| s.player) {
            let (x, y) = (self.pos[0] - a.w() / 2.0, self.pos[1] - a.h() * rise + 2.0);
            sprites.push((self.pos[1], a, x, y, flip, rise, [1.0; 4]));
            if let Some((at, bright)) = player_worn(self, facing) {
                let [gx, gy] = worn_corner(at, a, flip);
                sprites.push((self.pos[1] + 0.01, art::GRENADE_WORN, x + gx, y + gy, flip, 1.0, [bright, bright, bright, 1.0]));
            }
        }
        sprites.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, a, x, y, flip, show, k) in sprites {
            let (sx, sy) = at(x, y);
            c.art_top(a, sx, sy, scale, flip, show, [lit[0] * k[0], lit[1] * k[1], lit[2] * k[2], lit[3] * k[3]]);
        }

        let own_shot = super::effects::effects().bullet.shot;
        for b in self.bullets.iter().filter(|_| !fog) {
            let Some(shot) = b.look.map_or(own_shot, |e| e.shot) else {
                let (sx, sy) = at(b.pos[0], b.pos[1]);
                let a = b.look.unwrap_or(super::effects::effects().bullet).looping(b.age);
                c.art_rotated(a, sx, sy, scale, b.vel[1].atan2(b.vel[0]), lit);
                continue;
            };
            let [r, g, bl] = shot.color;
            let tail = shot.tail(b.pos, b.vel, b.age);
            if let Some(frames) = shot.sprites[super::effects::TRACER] {
                let a = frames[(b.age * 12.0) as usize % frames.len()];
                let length = ((b.pos[0] - tail[0]).powi(2) + (b.pos[1] - tail[1]).powi(2)).sqrt();
                let (sx, sy) = at((b.pos[0] + tail[0]) / 2.0, (b.pos[1] + tail[1]) / 2.0);
                c.art_rotated(a, sx, sy, scale * length / a.w(), b.vel[1].atan2(b.vel[0]), lit);
                continue;
            }
            // Overlapping dots, close enough at the head to read as one line.
            let n = ((shot.length / (shot.size * 0.35)).ceil() as usize).clamp(4, 32);
            for i in 0..n {
                let k = i as f32 / (n - 1) as f32;
                let (width, bright) = super::effects::Shot::taper(k);
                let (sx, sy) = at(b.pos[0] + (tail[0] - b.pos[0]) * k, b.pos[1] + (tail[1] - b.pos[1]) * k);
                c.circle(sx, sy, shot.size * width * scale / 2.0, [lit[0] * r * bright, lit[1] * g * bright, lit[2] * bl * bright, lit[3]]);
            }
        }
        for s in self.spits.iter().filter(|s| known(s.pos)) {
            let (sx, sy) = at(s.pos[0], s.pos[1]);
            let [r, g, b] = s.tint();
            c.art_rotated(super::effects::effects().spit.looping(s.age), sx, sy, scale, s.vel[1].atan2(s.vel[0]), [lit[0] * r, lit[1] * g, lit[2] * b, lit[3]]);
        }
        for fx in &self.fx {
            let (sx, sy) = at(fx.pos[0], fx.pos[1]);
            let [r, g, b] = fx.tint;
            c.art_rotated(fx.frame(), sx, sy, scale, fx.angle, [lit[0] * r, lit[1] * g, lit[2] * b, lit[3] * fx.alpha()]);
        }
        for s in &self.swings {
            let (angle, r) = s.now();
            let (sx, sy) = at(self.pos[0] + angle.cos() * r, self.pos[1] - 6.0 + angle.sin() * r);
            c.art_rotated(s.art, sx, sy, scale, angle, [lit[0], lit[1], lit[2], lit[3] * s.alpha()]);
        }
        for b in &self.bolts {
            let fade = 1.0 - b.age / BOLT_LIFE;
            for w in b.points.windows(2) {
                for k in 0..8 {
                    let t = k as f32 / 8.0;
                    let jitter = ((k as f32 * 7.3 + b.age * 90.0).sin()) * 3.0;
                    let (sx, sy) = at(w[0][0] + (w[1][0] - w[0][0]) * t + jitter, w[0][1] + (w[1][1] - w[0][1]) * t - jitter);
                    c.circle(sx, sy, 1.2 * scale, [0.7, 0.85, 1.0, fade]);
                }
            }
        }
        for p in self.juice.particles.iter().filter(|_| !fog) {
            let s = p.size_now() * scale;
            let (sx, sy) = at(p.pos[0], p.pos[1] - p.z);
            match p.frame() {
                Some(a) => c.art_rotated(a, sx, sy, s / a.w(), 0.0, [1.0, 1.0, 1.0, p.alpha()]),
                None => c.rect(sx - s / 2.0, sy - s / 2.0, s, s, [p.color[0], p.color[1], p.color[2], p.alpha()]),
            }
        }
        if fog {
            self.shade_unseen(c, cam, scale);
        }
        for r in &self.juice.rings {
            let rr = r.now();
            let fade = 1.0 - r.age / r.life;
            for i in 0..32 {
                let a = i as f32 / 32.0 * TAU;
                let (sx, sy) = at(r.center[0] + a.cos() * rr, r.center[1] + a.sin() * rr);
                c.circle(sx, sy, 1.5 * scale * fade, [r.color[0], r.color[1], r.color[2], fade]);
            }
        }
        for g in &self.thrown {
            let (p, lift) = g.at();
            let k = (g.t / crate::tuning::tuning().grenade_flight).min(1.0);
            let (gx, gy) = at(p[0], p[1]);
            c.circle(gx, gy, (2.0 + 2.0 * k) * scale, [0.0, 0.0, 0.0, 0.4]);
            let (sx, sy) = at(p[0], p[1] - 6.0 - lift);
            c.art_rotated(g.art(), sx, sy, scale, g.t * 12.0, lit);
            if g.kind != Thrown::Grenade {
                continue;
            }
            for i in 0..8 {
                let a = g.t * 3.0 + i as f32 / 8.0 * TAU;
                let rr = crate::tuning::tuning().grenade_radius * (1.15 - 0.15 * k);
                let (sx, sy) = at(g.to[0] + a.cos() * rr, g.to[1] + a.sin() * rr);
                c.circle(sx, sy, 1.5 * scale, [1.0, 0.25, 0.15, 0.85]);
            }
        }
    }

    fn overlays(&self, c: &mut Canvas) {
        match &self.phase {
            Phase::Playing => {}
            Phase::LevelUp { choices, selected, t, picked } => {
                let fade = picked.map_or(1.0, |p| 1.0 - p / PICK_TIME);
                c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.55 * (t / 0.2).min(1.0) * fade]);
                if self.glory > 0.0 {
                    c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [1.0, 0.9, 0.6, self.glory / 0.25 * 0.5]);
                }
                // Embers drifting up behind the title.
                for i in 0..24 {
                    let seed = i as f32 * 12.9898;
                    let x = (seed.sin() * 43758.545).fract().abs() * SCREEN_W;
                    let y = SCREEN_H - ((t * (40.0 + i as f32 * 7.0) + seed * 50.0) % (SCREEN_H + 40.0));
                    c.circle(x, y, 3.0 + (i % 3) as f32 * 2.0, [1.0, 0.8, 0.3, 0.5 * fade]);
                }
                let scale = 12.0 * back_out((t / 0.3).min(1.0)) + (t * 4.0).sin() * 0.4;
                c.text_centered(150.0 - (scale - 12.0) * 2.5, scale, [GOLD[0], GOLD[1], GOLD[2], fade], "LEVEL UP");
                self.choices(c, choices, *selected, *t, *picked, 270.0);
                let keys: Vec<String> = [("{L1}: banish", self.banishes, LevelKey::Banish), ("{R1}: reroll", self.rerolls, LevelKey::Reroll), ("{Y}: favor", self.favors, LevelKey::Favor)]
                    .into_iter()
                    .filter(|k| level_key_shows(k.2, self.level))
                    .map(|(name, left, _)| format!("{}, {left} left", pad::hint(name)))
                    .collect();
                c.label(keys.join("      "), SCREEN_W / 2.0, 716.0, 26.0, [1.0, 1.0, 1.0, 0.7 * fade], Align::Center);
            }
            Phase::Over { t, won } => {
                // A death waits for the player to climb back out before the summary shows.
                let t = if *won { *t } else { t - TURNED };
                if t <= 0.0 {
                    return;
                }
                let k = (t / 0.6).min(1.0);
                c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.6 * k]);
                let (title, color) = if *won { ("SAFE AT HOME", GOLD) } else { ("YOU TURNED", RED) };
                let scale = 16.0 * back_out(k);
                // The middle stays clear, where the turned player stands, with the damage table to its left.
                let (title_y, line_y) = (130.0, 560.0);
                c.text_centered(title_y - scale * 2.5, scale, color, title);
                let a = ((t - 0.5) / 0.4).clamp(0.0, 1.0);
                self.damage_table(c, a);
                self.build_panel(c, a);
                let line = if self.t >= super::dawn() {
                    let how = if *won { "Home" } else { "Lasted" };
                    format!("{how} {} past dawn   Level {}   Kills {}", clock(self.t - super::dawn()), self.level, self.kills)
                } else {
                    format!("Survived {}   Level {}   Kills {}", clock(self.t), self.level, self.kills)
                };
                c.label(line, SCREEN_W / 2.0, line_y, 40.0, [1.0, 1.0, 1.0, a], Align::Center);
                c.label(pad::hint("{A}: try another farm      {B}: menu"), SCREEN_W / 2.0, line_y + 80.0, 32.0, [1.0, 1.0, 1.0, 0.7 * a], Align::Center);
                let names: Vec<&str> = self.newly.iter().map(|&i| book().defs[i].name.as_str()).collect();
                let news = [
                    (!names.is_empty()).then(|| format!("Found: {}", names.join(", "))),
                    match self.ready {
                        0 => None,
                        1 => Some("Something new can be found".to_string()),
                        n => Some(format!("{n} new powerups can be found")),
                    },
                ];
                for (k, line) in news.into_iter().flatten().enumerate() {
                    let y = line_y - 80.0 - k as f32 * 48.0;
                    c.label(line, SCREEN_W / 2.0, y, 34.0, [GOLD[0], GOLD[1], GOLD[2], a], Align::Center);
                }
            }
        }
    }

    /// The run's damage and damage a second by weapon, most first, then the whole run's, faded in by `a`.
    fn damage_table(&self, c: &mut Canvas, a: f32) {
        let tally = &self.juice.tally;
        let rows = tally.rows();
        let (x, top, size, row_h) = (24.0, 196.0, 24.0, 26.0);
        let (damage_x, dps_x) = (x + 290.0, x + 410.0);
        let lines = rows.len() as f32 + 2.0;
        c.rect(x - 8.0, top - 8.0, dps_x - x + 16.0, lines * row_h + 16.0, [0.0, 0.0, 0.0, 0.45 * a]);
        let dim = [1.0, 1.0, 1.0, 0.6 * a];
        c.label("Damage", damage_x, top, size, dim, Align::Right);
        c.label("Per sec", dps_x, top, size, dim, Align::Right);
        let line = |c: &mut Canvas, i: usize, name: &str, damage: f32, dps: f32, color: Color| {
            let y = top + (i as f32 + 1.0) * row_h;
            c.label(name, x, y, size, color, Align::Left);
            c.label(grouped(damage.round() as u64), damage_x, y, size, color, Align::Right);
            c.label(per_second(dps), dps_x, y, size, color, Align::Right);
        };
        let white = [1.0, 1.0, 1.0, a];
        for (i, r) in rows.iter().enumerate() {
            let name = r.by.map_or("Grenades, barrels", |d| book().defs[d].name.as_str());
            line(c, i, name, r.damage, r.dps, white);
        }
        line(c, rows.len(), "Whole run", tally.total(), tally.dps(), [GOLD[0], GOLD[1], GOLD[2], a]);
    }

    /// While X is held, a crosshair on the barrel the aimed weapons are going for.
    fn barrel_mark(&self, c: &mut Canvas, view_proj: glam::Mat4) {
        let Some(d) = self.barrel_reach().and_then(|reach| self.barrel_in_view(reach)) else { return };
        let Some(([x, y], dist)) = view3d::to_screen(view_proj, [self.pos[0] + d[0], self.pos[1] + d[1]], 8.0) else { return };
        let r = (90.0 / dist).clamp(18.0, 60.0) * (1.0 + 0.1 * (self.t * 10.0).sin());
        let red = [1.0, 0.3, 0.2, 0.9];
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            let (cx, cy) = (x + dx * r, y + dy * r);
            let (w, h) = if dx != 0.0 { (r * 0.6, 5.0) } else { (5.0, r * 0.6) };
            c.rect(cx - w / 2.0, cy - h / 2.0, w, h, red);
        }
        c.circle(x, y, 5.0, red);
    }

    /// After dawn, where the farmhouse is: over it when it's in view, else at the edge it lies past.
    fn way_home(&self, c: &mut Canvas, view_proj: glam::Mat4) {
        if self.t < super::dawn() || !matches!(self.phase, Phase::Playing | Phase::LevelUp { .. }) {
            return;
        }
        let d = [self.map.home[0] - self.pos[0], self.map.home[1] - self.pos[1]];
        let far = format!("{:.0}m", (d[0] * d[0] + d[1] * d[1]).sqrt() / TILE);
        let pulse = [GOLD[0], GOLD[1], GOLD[2], 0.75 + 0.25 * (self.t * 5.0).sin()];
        let shown = view3d::to_screen(view_proj, self.map.home, 30.0).filter(|([x, y], _)| (60.0..SCREEN_W - 60.0).contains(x) && (120.0..SCREEN_H).contains(y));
        if let Some(([x, y], _)) = shown {
            c.label(format!("HOME  {far}"), x, y - 40.0, 34.0, pulse, Align::Center);
            c.label("v", x, y - 6.0, 34.0, pulse, Align::Center);
            return;
        }
        let (fwd, right) = ([self.view_yaw.cos(), self.view_yaw.sin()], [-self.view_yaw.sin(), self.view_yaw.cos()]);
        let (ahead, side) = (d[0] * fwd[0] + d[1] * fwd[1], d[0] * right[0] + d[1] * right[1]);
        if ahead < 0.0 && side.abs() < -ahead {
            c.label(format!("HOME IS BEHIND YOU  {far}   ({} turns)", pad::label(Buttons::B)), SCREEN_W / 2.0, SCREEN_H - 70.0, 34.0, pulse, Align::Center);
        } else if side > 0.0 {
            c.label(format!("HOME  {far}  >"), SCREEN_W - 24.0, 140.0, 34.0, pulse, Align::Right);
        } else {
            c.label(format!("<  HOME  {far}"), 24.0, 140.0, 34.0, pulse, Align::Left);
        }
    }

    /// After dawn, the farmhouse's step on the map, pinned to the edge when it's off the map.
    fn home_on_map(&self, c: &mut Canvas, cam: [f32; 2]) {
        if self.t < super::dawn() {
            return;
        }
        let x = ((self.map.home[0] - cam[0]) * map_scale()).clamp(40.0, SCREEN_W - 40.0);
        let y = ((self.map.home[1] - cam[1]) * map_scale()).clamp(40.0, SCREEN_H - 40.0);
        let pulse = 0.5 + 0.4 * (self.t * 5.0).sin();
        c.circle(x, y, 14.0 * map_scale() / 2.0, [GOLD[0], GOLD[1], GOLD[2], pulse * 0.5]);
        c.circle(x, y, 5.0 * map_scale() / 2.0, [GOLD[0], GOLD[1], GOLD[2], 1.0]);
        c.label("HOME", x, y - 52.0, 30.0, GOLD, Align::Center);
    }

    /// While the truck driver holds a collection ticket, a coin bobbing over his head.
    fn ticket_mark(&self, c: &mut Canvas, view_proj: glam::Mat4) {
        if !matches!(self.phase, Phase::Playing | Phase::LevelUp { .. }) {
            return;
        }
        let Some(at) = self.ticket() else { return };
        let Some(([x, y], dist)) = view3d::to_screen(view_proj, at, 30.0 + 2.0 * (self.t * 4.0).sin()) else { return };
        let a = super::effects::effects().money.once(10.0);
        let scale = (10.0 / dist).clamp(3.0, 6.0);
        c.circle(x, y, a.w() * scale, [GOLD[0], GOLD[1], GOLD[2], 0.25 + 0.15 * (self.t * 5.0).sin()]);
        c.art(a, x - a.w() * scale / 2.0, y - a.h() * scale / 2.0, scale, false, WHITE);
    }

    /// While the truck driver holds a collection ticket, where he stands on the map, pinned to the edge when off it.
    fn ticket_on_map(&self, c: &mut Canvas, cam: [f32; 2]) {
        let Some(at) = self.ticket() else { return };
        let x = ((at[0] - cam[0]) * map_scale()).clamp(40.0, SCREEN_W - 40.0);
        let y = ((at[1] - cam[1]) * map_scale()).clamp(40.0, SCREEN_H - 40.0);
        let pulse = 0.5 + 0.4 * (self.t * 5.0).sin();
        c.circle(x, y, 14.0 * map_scale() / 2.0, [GOLD[0], GOLD[1], GOLD[2], pulse * 0.5]);
        c.circle(x, y, 5.0 * map_scale() / 2.0, GOLD);
        let a = super::effects::effects().money.once(10.0);
        c.art(a, x - a.w() * map_scale() / 2.0, y - 30.0 - a.h() * map_scale() / 2.0, map_scale(), false, WHITE);
    }

    /// Start's pause: the run's numbers over the dimmed view.
    fn paused_top(&self, c: &mut Canvas) {
        if self.menu.is_none() {
            return;
        }
        c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.6]);
        c.text_centered(250.0, 16.0, WHITE, "PAUSED");
        let line = format!("{}   Level {}   Kills {}", clock(self.t), self.level, self.kills);
        c.label(line, SCREEN_W / 2.0, 420.0, 40.0, WHITE, Align::Center);
    }

    /// Start's pause choices, in the rows the level-up cards use so a tap lands the same way.
    fn paused_bottom(&self, c: &mut Canvas) {
        let Some(at) = self.menu else { return };
        c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.6]);
        let how = pad::hint(if self.taps { "{Start} or {B}: resume.   Up / Down and {A}, or tap" } else { "{Start} or {B}: resume.   Up / Down and {A}" });
        c.label(how, SCREEN_W / 2.0, 60.0, 32.0, WHITE, Align::Center);
        for (i, name) in PAUSE_CHOICES.iter().enumerate() {
            let (x, y, w, h) = (212.0, CHOICE_TOP + i as f32 * PAUSE_ROW, 600.0, PAUSE_ROW - 20.0);
            let on = i == at;
            c.rect(x, y, w, h, if on { [1.0, 0.85, 0.3, 0.35] } else { [1.0, 1.0, 1.0, 0.1] });
            c.label(*name, SCREEN_W / 2.0, y + h / 2.0 - 26.0, 48.0, if on { GOLD } else { WHITE }, Align::Center);
        }
    }

    fn hud(&self, c: &mut Canvas) {
        // XP across the top, with a bright leading edge; it flashes while full.
        let k = (self.xp_shown / self.next_xp as f32).clamp(0.0, 1.0);
        c.rect(0.0, 0.0, SCREEN_W, 14.0, [0.0, 0.0, 0.0, 0.6]);
        let glint = (1.0 - self.chain.0 / 0.15).max(0.0);
        let bar = if self.leveling > 0.0 && (self.juice.clock * 16.0).sin() > 0.0 {
            WHITE
        } else {
            [GOLD[0], GOLD[1] + (1.0 - GOLD[1]) * glint, GOLD[2] + (1.0 - GOLD[2]) * glint, 1.0]
        };
        c.rect(0.0, 0.0, SCREEN_W * k, 14.0, bar);
        if k > 0.01 && k < 1.0 {
            c.rect(SCREEN_W * k - 8.0, 0.0, 8.0, 14.0, [1.0, 1.0, 0.85, 0.9]);
        }
        // Health, with a pale chunk trailing what was just lost; it throbs when low.
        let (bx, by, bw, bh) = (24.0, 30.0, 240.0, 22.0);
        let max = self.kit.max_hp();
        c.rect(bx - 3.0, by - 3.0, bw + 6.0, bh + 6.0, [0.0, 0.0, 0.0, 0.7]);
        c.rect(bx, by, bw * (self.hp_shown / max).clamp(0.0, 1.0), bh, [1.0, 0.9, 0.8, 0.85]);
        let health = self.hp / max;
        let throb = if health < 0.3 { 0.5 + 0.5 * (self.juice.clock * 10.0).sin() } else { 0.0 };
        c.rect(bx, by, bw * health.clamp(0.0, 1.0), bh, [RED[0] + 0.15 * throb, RED[1] + 0.1 * throb, RED[2], 1.0]);
        if self.range.is_none() {
            c.label(format!("LV {}", self.level), bx, by + 30.0, 32.0, WHITE, Align::Left);
        }
        if self.range.is_some() {
        } else if self.t >= super::dawn() {
            c.text_centered(30.0, 6.0, HARVESTER, &format!("DAWN +{}", clock(self.t - super::dawn())));
            c.label(format!("The dead +{:.0}%", (self.fury - 1.0) * 100.0), SCREEN_W / 2.0, 96.0, 30.0, HARVESTER, Align::Center);
        } else {
            c.text_centered(30.0, 6.0, WHITE, &clock(self.t));
        }
        let bump = 1.0 + 0.35 * (1.0 - self.kill_bump / 0.15).max(0.0);
        c.label(format!("{} kills", self.kills), SCREEN_W - 24.0, 28.0 - 8.0 * (bump - 1.0), 32.0 * bump, WHITE, Align::Right);
    }

    /// The upgrade cards: each slides in a beat after the last, the selected one breathes, rare ones shimmer gold,
    /// and a pick flashes white while the rest fade.
    fn choices(&self, c: &mut Canvas, choices: &[Choice; 3], selected: usize, t: f32, picked: Option<f32>, top: f32) {
        let fade = picked.map_or(1.0, |p| 1.0 - p / PICK_TIME);
        for (i, ch) in choices.iter().enumerate() {
            let k = back_out(((t - self.dealt[i] - i as f32 * 0.07) / 0.28).clamp(0.0, 1.0));
            let on = i == selected;
            let grow = if on && picked.is_none() { 4.0 + 4.0 * (t * 6.0).sin() } else { 0.0 };
            let (x, y) = (112.0 + (1.0 - k) * 1000.0 - grow, top + i as f32 * CHOICE_ROW - grow / 2.0);
            let (w, h) = (800.0 + grow * 2.0, CHOICE_ROW - 20.0 + grow);
            let alpha = if on { 1.0 } else { fade };
            let doomed = self.banishing && book().defs[ch.up].kind != Kind::Filler;
            let blessed = self.favoring && book().defs[ch.up].kind != Kind::Filler;
            if ch.rare || ch.found || doomed || blessed {
                let glint = 0.6 + 0.4 * (t * 5.0).sin();
                let edge = if doomed { RED } else if ch.found { FIND } else { GOLD };
                c.rect(x - 5.0, y - 5.0, w + 10.0, h + 10.0, [edge[0], edge[1], edge[2], glint * alpha]);
                let back = if doomed { [0.1, 0.03, 0.02] } else if ch.found { [0.02, 0.08, 0.04] } else { [0.1, 0.08, 0.02] };
                c.rect(x, y, w, h, [back[0], back[1], back[2], alpha]);
            }
            let def = &book().defs[ch.up];
            let from = self.kit.level(ch.up);
            let to = self.kit.raised(ch.up, if ch.rare { 2 } else { 1 });
            let fill = match picked {
                Some(p) if on => [1.0, 1.0, 1.0, 0.8 * (1.0 - p / PICK_TIME)],
                _ if on => [1.0, 0.85, 0.3, 0.35],
                _ => [1.0, 1.0, 1.0, 0.1 * alpha],
            };
            c.rect(x, y, w, h, fill);
            if ch.rare {
                let band = x + (t * 0.9).fract() * (w + 60.0) - 60.0;
                let (b0, b1) = (band.max(x), (band + 60.0).min(x + w));
                if b1 > b0 {
                    c.rect(b0, y, b1 - b0, h, [1.0, 1.0, 0.9, 0.12 * alpha]);
                }
                c.label(if to > def.max { "LEVEL BREAKER" } else { "RARE  x2" }, x + w - 28.0, y + 16.0, 30.0, [GOLD[0], GOLD[1], GOLD[2], alpha], Align::Right);
            }
            let icon = 6.0;
            c.rect(x + 16.0, y + (h - 16.0 * icon) / 2.0, 16.0 * icon, 16.0 * icon, [0.0, 0.0, 0.0, 0.35 * alpha]);
            c.art(def.icon, x + 16.0, y + (h - 16.0 * icon) / 2.0, icon, false, [1.0, 1.0, 1.0, alpha]);
            let left = x + 32.0 + 16.0 * icon;
            let name_color = if on { GOLD } else { WHITE };
            c.label(def.name.clone(), left, y + 12.0, 40.0, [name_color[0], name_color[1], name_color[2], alpha], Align::Left);
            c.label(def.change(from, to), left, y + 62.0, 26.0, [1.0, 1.0, 1.0, 0.75 * alpha], Align::Left);
            let tag = match def.kind {
                Kind::Filler => String::new(),
                _ if self.favored == Some(ch.up) => format!("FAVORED  LV {to}"),
                _ if from == 0 && to > 1 => format!("NEW  LV {to}"),
                _ if from == 0 => "NEW".into(),
                _ => format!("LV {to}"),
            };
            let zone = def.zone.map(Zone::name).unwrap_or(if def.kind == Kind::Passive { "PASSIVE" } else { "" });
            if ch.found {
                c.label("FOUND", x + w - 28.0, y + 16.0, 30.0, [FIND[0], FIND[1], FIND[2], alpha], Align::Right);
            }
            let tag_y = if ch.rare || ch.found { y + 52.0 } else { y + 16.0 };
            c.label(format!("{zone}  {tag}"), x + w - 28.0, tag_y, 26.0, [zone_color(def.zone)[0], zone_color(def.zone)[1], zone_color(def.zone)[2], alpha], Align::Right);
        }
    }
}

/// The edge and tag of a found powerup's card, apart from a rare one's gold.
const FIND: Color = [0.45, 1.0, 0.6, 1.0];
const HARVESTER: Color = [1.0, 0.45, 0.3, 1.0];

fn zone_color(zone: Option<Zone>) -> Color {
    match zone {
        Some(Zone::Ahead) => [1.0, 0.85, 0.45, 1.0],
        Some(Zone::Around) => [0.6, 0.9, 1.0, 1.0],
        Some(Zone::Behind) => [1.0, 0.55, 0.5, 1.0],
        Some(Zone::Map) => [0.7, 1.0, 0.6, 1.0],
        None => [0.8, 0.8, 0.85, 1.0],
    }
}

/// Eases from 0 to 1, overshooting a little before settling.
/// Powerup icons in a row from (x, y), `size` pixels square, each with its level in its corner, gold at its top level.
pub(crate) fn powerup_icons(c: &mut Canvas, x: f32, y: f32, size: f32, held: impl IntoIterator<Item = (usize, u32)>, alpha: f32) {
    let label = size / 2.0;
    for (i, (def, level)) in held.into_iter().enumerate() {
        let Some(d) = book().defs.get(def) else { continue };
        let x = x + i as f32 * (size + size / 5.0);
        c.rect(x - 2.0, y - 2.0, size + 4.0, size + 4.0, [0.0, 0.0, 0.0, 0.6 * alpha]);
        c.art(d.icon, x, y, size / 16.0, false, [1.0, 1.0, 1.0, alpha]);
        let color = if level >= d.max { GOLD } else { WHITE };
        c.label(level.to_string(), x + size - 2.0, y + size - label, label, [color[0], color[1], color[2], alpha], Align::Right);
    }
}

/// A whole number with commas between each three digits.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Damage a second, to a tenth below ten.
fn per_second(dps: f32) -> String {
    if dps < 10.0 { format!("{dps:.1}") } else { format!("{dps:.0}") }
}

fn back_out(k: f32) -> f32 {
    let c = 1.70158;
    let k = k - 1.0;
    1.0 + k * k * ((c + 1.0) * k + c)
}
