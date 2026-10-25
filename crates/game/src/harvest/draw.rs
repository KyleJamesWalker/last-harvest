//! Both screens' 2D drawing: the map below, and the HUD, numbers and overlays over the 3D view above.

use std::f32::consts::{PI, TAU};

use super::juice::{self, Tone};
use super::map::TILE;
use crate::art::Stand;
use super::{CHOICE_ROW, CHOICE_TOP, Choice, DropKind, GOLD, GRENADE_FLIGHT, GRENADE_RADIUS, GRENADES, Harvest, PICK_TIME, PLAYER, Phase, RED, SCALE, TELL_TIME, WHITE, art_for, clock, view3d};
use crate::art::{self, Art};
use crate::{Align, Canvas, Color, Frame, SCREEN_H, SCREEN_W};

impl Harvest {
    /// The top screen is the 3D view; the bottom is the map, where a tap throws a grenade.
    pub fn draw(&self, f: &mut Frame) {
        let scene = view3d::scene(self);
        f.top.clear = [scene.fog[0], scene.fog[1], scene.fog[2], 1.0];
        self.popups(&mut f.top, &scene);
        self.motes(&mut f.top, &scene);
        f.top.cards = Some(scene);
        self.vignette(&mut f.top);
        self.hud(&mut f.top);
        self.banner(&mut f.top);
        self.overlays(&mut f.top);

        let c = &mut f.bottom;
        c.clear = [0.0, 0.0, 0.0, 1.0];
        // The sky's wash multiplied into the map's colors: the same look as a translucent layer, without its fill cost.
        let wash = view3d::sky(self).wash;
        let tint = [0, 1, 2, 3].map(|i| if i == 3 { 1.0 } else { 1.0 - wash[3] + wash[3] * wash[i] });
        if self.overview {
            let scale = (SCREEN_W / self.map.width_px()).min(SCREEN_H / self.map.height_px());
            self.world(c, [0.0, 0.0], scale, tint);
            c.label("Whole map (release L1 to play)", SCREEN_W / 2.0, SCREEN_H - 40.0, 28.0, WHITE, Align::Center);
            return;
        }
        let cam = self.camera();
        self.world(c, cam, SCALE, tint);
        self.view_cone(c, cam);
        self.grenade_tray(c);
        if let Phase::LevelUp { choices, selected, t, picked } = &self.phase {
            let fade = picked.map_or(1.0, |p| 1.0 - p / PICK_TIME);
            c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.6 * (t / 0.2).min(1.0) * fade]);
            c.label("Tap an upgrade, or Up / Down and X", SCREEN_W / 2.0, 60.0, 32.0, [1.0, 1.0, 1.0, fade], Align::Center);
            self.choices(c, choices, *selected, *t, *picked, CHOICE_TOP);
        }
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
        let health = self.hp / self.gear.max_hp;
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
    fn view_cone(&self, c: &mut Canvas, cam: [f32; 2]) {
        let half = 0.6;
        for edge in [-half, half] {
            let a = self.view_yaw + edge;
            for k in 1..9 {
                let r = k as f32 * 14.0;
                let (x, y) = ((self.pos[0] + a.cos() * r - cam[0]) * SCALE, (self.pos[1] - 6.0 + a.sin() * r - cam[1]) * SCALE);
                c.circle(x, y, 5.0, [1.0, 0.9, 0.6, 0.8 - k as f32 * 0.08]);
            }
        }
    }

    /// Grenades ready, bottom right, with the next one filling in.
    fn grenade_tray(&self, c: &mut Canvas) {
        let a = art::GRENADE;
        let slot = a.w() * 5.0 + 12.0;
        let panel = GRENADES * slot + 24.0;
        c.rect(SCREEN_W - 40.0 - panel + 12.0, SCREEN_H - 40.0 - a.h() * 5.0, panel, a.h() * 5.0 + 34.0, [0.0, 0.0, 0.0, 0.6]);
        for i in 0..GRENADES as usize {
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

    /// Draws the map and everything on it, with `cam` at the top-left and each art pixel `scale` screen pixels across.
    pub(super) fn world(&self, c: &mut Canvas, cam: [f32; 2], scale: f32, tint: Color) {
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
        let stains = self.stains.iter().map(|&(a, x, y, [r, g, b])| (a, x, y, [r, g, b, 1.0]));
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
        for t in &self.tells {
            let k = (t.t / TELL_TIME).min(1.0);
            let (sx, sy) = at(t.pos[0], t.pos[1]);
            c.circle(sx, sy, (2.0 + 3.0 * k) * scale, [0.2, 0.13, 0.07, 0.8]);
        }
        for corpse in &self.juice.corpses {
            let (sx, sy) = at(corpse.pos[0], corpse.pos[1] - corpse.z - 3.0);
            let turn = if corpse.fall[0] < 0.0 { -1.0 } else { 1.0 };
            let angle = if corpse.z > 0.0 { corpse.spin } else { corpse.tipped() * PI / 2.0 * turn };
            let fade = 1.0 - corpse.sunk() / 8.0;
            c.art_rotated(corpse.art, sx, sy, scale, angle, shade([0.8, 0.8, 0.8, fade]));
        }
        for d in &self.drops {
            let a = match d.kind {
                DropKind::Money(_) => art::MONEY[((d.age * 10.0) as usize).min(3)],
                DropKind::Health => art::HEALTH,
            };
            let (sx, sy) = at(d.pos[0] - a.w() / 2.0, d.pos[1] - a.h() - d.z);
            c.art(a, sx, sy, scale, false, lit);
        }

        // Props, zombies and the player, drawn back to front by where they meet the ground.
        // The last field is how much of each sprite shows: zombies still in the ground show only their top.
        let mut sprites: Vec<(f32, Art, f32, f32, bool, f32)> = Vec::new();
        for p in &self.map.props {
            let a = p.frame(self.t);
            if visible(p.x, p.y, a.w(), a.h()) {
                sprites.push((p.base, a, p.x, p.y, false, 1.0));
            }
        }
        for r in &self.barrels {
            let a = if r.flash > 0.0 { art::BARREL_FLASH } else { art::BARREL };
            if visible(r.pos[0], r.pos[1], TILE, TILE) {
                sprites.push((r.pos[1] + a.h(), a, r.pos[0], r.pos[1], false, 1.0));
            }
        }
        for z in &self.zombies {
            let (facing, flip) = z.facing;
            let a = z.art(facing);
            sprites.push((z.pos[1], a, z.pos[0] - a.w() / 2.0, z.pos[1] - a.h() * z.rise + 2.0, flip, z.rise));
        }
        if let Some(s) = self.shown.as_ref().filter(|s| s.stand != Stand::Ground && !s.frames.is_empty()) {
            let a = s.frame(self.t);
            sprites.push((s.pos[1], a, s.pos[0] - a.w() / 2.0, s.pos[1] - a.h(), false, 1.0));
        }
        for b in &self.birds {
            let a = b.art();
            sprites.push((b.pos[1], a, b.pos[0] - a.w() / 2.0, b.pos[1] - a.h() - b.z, b.flip, 1.0));
        }
        let (facing, flip) = self.facing;
        let a = art_for(self, &PLAYER, facing);
        sprites.push((self.pos[1], a, self.pos[0] - a.w() / 2.0, self.pos[1] - a.h() + 2.0, flip, 1.0));
        sprites.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, a, x, y, flip, show) in sprites {
            let (sx, sy) = at(x, y);
            c.art_top(a, sx, sy, scale, flip, show, lit);
        }

        for b in &self.bullets {
            let (sx, sy) = at(b.pos[0], b.pos[1]);
            c.art_rotated(art::BULLET, sx, sy, scale, b.vel[1].atan2(b.vel[0]), lit);
        }
        for fx in &self.fx {
            let a = fx.frames[((fx.t * fx.fps) as usize).min(fx.frames.len() - 1)];
            let (sx, sy) = at(fx.pos[0], fx.pos[1]);
            c.art_rotated(a, sx, sy, scale, fx.angle, lit);
        }
        for p in &self.juice.particles {
            let s = p.size_now() * scale;
            let (sx, sy) = at(p.pos[0], p.pos[1] - p.z);
            c.rect(sx - s / 2.0, sy - s / 2.0, s, s, [p.color[0], p.color[1], p.color[2], 1.0]);
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
            let k = (g.t / GRENADE_FLIGHT).min(1.0);
            let (gx, gy) = at(p[0], p[1]);
            c.circle(gx, gy, (2.0 + 2.0 * k) * scale, [0.0, 0.0, 0.0, 0.4]);
            for i in 0..8 {
                let a = g.t * 3.0 + i as f32 / 8.0 * TAU;
                let rr = GRENADE_RADIUS * (1.15 - 0.15 * k);
                let (sx, sy) = at(g.to[0] + a.cos() * rr, g.to[1] + a.sin() * rr);
                c.circle(sx, sy, 1.5 * scale, [1.0, 0.25, 0.15, 0.85]);
            }
            let (sx, sy) = at(p[0], p[1] - 6.0 - lift);
            c.art_rotated(art::GRENADE, sx, sy, scale, g.t * 12.0, lit);
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
            }
            Phase::Over { t, won } => {
                let k = (t / 0.6).min(1.0);
                c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.6 * k]);
                let (title, color) = if *won { ("DAWN", GOLD) } else { ("YOU DIED", RED) };
                let scale = 16.0 * back_out(k);
                c.text_centered(260.0 - scale * 2.5, scale, color, title);
                let a = ((t - 0.5) / 0.4).clamp(0.0, 1.0);
                let line = format!("Survived {}   Level {}   Kills {}", clock(self.t), self.level, self.kills);
                c.label(line, SCREEN_W / 2.0, 360.0, 40.0, [1.0, 1.0, 1.0, a], Align::Center);
                c.label("A: try another farm      B: menu", SCREEN_W / 2.0, 440.0, 32.0, [1.0, 1.0, 1.0, 0.7 * a], Align::Center);
            }
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
        let max = self.gear.max_hp;
        c.rect(bx - 3.0, by - 3.0, bw + 6.0, bh + 6.0, [0.0, 0.0, 0.0, 0.7]);
        c.rect(bx, by, bw * (self.hp_shown / max).clamp(0.0, 1.0), bh, [1.0, 0.9, 0.8, 0.85]);
        let health = self.hp / max;
        let throb = if health < 0.3 { 0.5 + 0.5 * (self.juice.clock * 10.0).sin() } else { 0.0 };
        c.rect(bx, by, bw * health.clamp(0.0, 1.0), bh, [RED[0] + 0.15 * throb, RED[1] + 0.1 * throb, RED[2], 1.0]);
        c.label(format!("LV {}", self.level), bx, by + 30.0, 32.0, WHITE, Align::Left);
        c.text_centered(30.0, 6.0, WHITE, &clock(self.t));
        let bump = 1.0 + 0.35 * (1.0 - self.kill_bump / 0.15).max(0.0);
        c.label(format!("{} kills", self.kills), SCREEN_W - 24.0, 28.0 - 8.0 * (bump - 1.0), 32.0 * bump, WHITE, Align::Right);
    }

    /// The upgrade cards: each slides in a beat after the last, the selected one breathes, rare ones shimmer gold,
    /// and a pick flashes white while the rest fade.
    fn choices(&self, c: &mut Canvas, choices: &[Choice; 3], selected: usize, t: f32, picked: Option<f32>, top: f32) {
        let fade = picked.map_or(1.0, |p| 1.0 - p / PICK_TIME);
        for (i, ch) in choices.iter().enumerate() {
            let k = back_out(((t - i as f32 * 0.07) / 0.28).clamp(0.0, 1.0));
            let on = i == selected;
            let grow = if on && picked.is_none() { 4.0 + 4.0 * (t * 6.0).sin() } else { 0.0 };
            let (x, y) = (112.0 + (1.0 - k) * 1000.0 - grow, top + i as f32 * CHOICE_ROW - grow / 2.0);
            let (w, h) = (800.0 + grow * 2.0, CHOICE_ROW - 20.0 + grow);
            let alpha = if on { 1.0 } else { fade };
            if ch.rare {
                let glint = 0.6 + 0.4 * (t * 5.0).sin();
                c.rect(x - 5.0, y - 5.0, w + 10.0, h + 10.0, [GOLD[0], GOLD[1], GOLD[2], glint * alpha]);
                c.rect(x, y, w, h, [0.1, 0.08, 0.02, alpha]);
            }
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
                c.label("RARE  x2", x + w - 28.0, y + 16.0, 30.0, [GOLD[0], GOLD[1], GOLD[2], alpha], Align::Right);
            }
            let (name, blurb) = ch.up.text(&self.gear);
            let name_color = if on { GOLD } else { WHITE };
            c.label(name, x + 28.0, y + 12.0, 40.0, [name_color[0], name_color[1], name_color[2], alpha], Align::Left);
            c.label(blurb, x + 28.0, y + 62.0, 28.0, [1.0, 1.0, 1.0, 0.75 * alpha], Align::Left);
        }
    }
}

/// Eases from 0 to 1, overshooting a little before settling.
fn back_out(k: f32) -> f32 {
    let c = 1.70158;
    let k = k - 1.0;
    1.0 + k * k * ((c + 1.0) * k + c)
}
