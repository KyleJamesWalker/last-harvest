//! Local co-op: each viewed player's own controls, cards and camera feel, and seeing the run as one of them.

use super::player::{ME, PlayerId};
use super::single::Aiming;
use super::{Choice, Harvest, Phase};
use crate::Input;
use crate::sfx::Sfx;

/// A level-up's cards as one player sees them: the three on offer, the one selected, and the times as in `Phase::LevelUp`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Cards {
    pub choices: [Choice; 3],
    pub selected: usize,
    pub t: f32,
    pub picked: Option<f32>,
}

/// What a viewed player keeps of their own while another player's is in `Harvest`'s fields.
#[derive(Clone, Debug, Default)]
pub(super) struct Local {
    turn_ready: bool,
    targeting: bool,
    lean: f32,
    quick: Option<f32>,
    aiming: Option<Aiming>,
    glance: bool,
    last_throw: Option<[f32; 2]>,
    since_throw: f32,
    touch_map: bool,
    queued_touch: Option<[f32; 2]>,
    hp_shown: f32,
    dealt: [f32; 3],
    banishing: bool,
    favoring: bool,
    banished: u64,
    banishes: u32,
    rerolls: u32,
    favored: Option<usize>,
    favors: u32,
    pub cards: Option<Cards>,
    feel: [f32; 6],
}

impl Local {
    /// A new player's start: the run's banishes, rerolls and favors, and a full health bar.
    pub fn of(h: &Harvest, p: PlayerId) -> Self {
        Self { banishes: h.banishes, rerolls: h.rerolls, favors: h.favors, hp_shown: h.players[p].hp, ..Self::default() }
    }
}

impl Harvest {
    /// Shows the run to `views` players, each in their own view with their own controls and cards; players past them
    /// have no view. More than one never freezes the shared world for one player's hit.
    pub fn with_views(mut self, views: usize) -> Self {
        self.views = views.clamp(1, self.players.len());
        self.locals = (0..self.players.len()).map(|p| Local::of(&self, p)).collect();
        self.juice.still = self.views > 1;
        self
    }

    /// How many players have a view of their own.
    pub fn views(&self) -> usize {
        self.views
    }

    /// Players 0 and `v` trade places everywhere a player is named, so the run reads as `v` saw it from ME; again
    /// trades them back.
    pub(super) fn relabel(&mut self, v: PlayerId) {
        if v == ME || v >= self.players.len() {
            return;
        }
        let swap = |p: &mut PlayerId| {
            if *p == ME {
                *p = v;
            } else if *p == v {
                *p = ME;
            }
        };
        self.players.swap(ME, v);
        if v < self.inputs.len() {
            self.inputs.swap(ME, v);
        }
        for (p, _) in &mut self.bots {
            swap(p);
        }
        for z in self.zombies.iter_mut().chain(self.burrowed.iter_mut()) {
            swap(&mut z.burn_owner);
            if let Some(q) = &mut z.led {
                swap(q);
            }
        }
        for t in &mut self.tells {
            if let Some(q) = &mut t.led {
                swap(q);
            }
        }
        for b in &mut self.bullets {
            swap(&mut b.owner);
        }
        for d in &mut self.drops {
            swap(&mut d.to);
        }
        for g in &mut self.thrown {
            swap(&mut g.owner);
        }
        for b in &mut self.barrels {
            swap(&mut b.by);
        }
        for p in &mut self.patches {
            p.relabel(v);
        }
        for d in &mut self.decoys {
            d.relabel(v);
        }
        swap(&mut self.actor);
        swap(&mut self.paused_by);
        self.juice.tally.relabel(v);
        self.trade(v);
        self.swapped = if self.swapped == v { ME } else { v };
    }

    /// The number a player goes by, from 1, whoever is ME just now.
    pub(super) fn number(&self, p: PlayerId) -> usize {
        let s = self.swapped;
        (if p == ME { s } else if p == s { ME } else { p }) + 1
    }

    /// Swaps the fields ME's controls, cards and camera live in with player `v`'s keeping.
    fn trade(&mut self, v: PlayerId) {
        let Some(l) = self.locals.get_mut(v) else { return };
        std::mem::swap(&mut self.turn_ready, &mut l.turn_ready);
        std::mem::swap(&mut self.targeting, &mut l.targeting);
        std::mem::swap(&mut self.lean, &mut l.lean);
        std::mem::swap(&mut self.quick, &mut l.quick);
        std::mem::swap(&mut self.aiming, &mut l.aiming);
        std::mem::swap(&mut self.glance, &mut l.glance);
        std::mem::swap(&mut self.last_throw, &mut l.last_throw);
        std::mem::swap(&mut self.since_throw, &mut l.since_throw);
        std::mem::swap(&mut self.touch_map, &mut l.touch_map);
        std::mem::swap(&mut self.queued_touch, &mut l.queued_touch);
        std::mem::swap(&mut self.hp_shown, &mut l.hp_shown);
        std::mem::swap(&mut self.dealt, &mut l.dealt);
        std::mem::swap(&mut self.banishing, &mut l.banishing);
        std::mem::swap(&mut self.favoring, &mut l.favoring);
        std::mem::swap(&mut self.banished, &mut l.banished);
        std::mem::swap(&mut self.banishes, &mut l.banishes);
        std::mem::swap(&mut self.rerolls, &mut l.rerolls);
        std::mem::swap(&mut self.favored, &mut l.favored);
        std::mem::swap(&mut self.favors, &mut l.favors);
        if let (Phase::LevelUp { choices, selected, t, picked }, Some(c)) = (&mut self.phase, &mut l.cards) {
            std::mem::swap(choices, &mut c.choices);
            std::mem::swap(selected, &mut c.selected);
            std::mem::swap(t, &mut c.t);
            std::mem::swap(picked, &mut c.picked);
        }
        let mine = self.juice.feel();
        self.juice.set_feel(l.feel);
        l.feel = mine;
    }

    /// Runs `f` on player `p`'s camera feel: shake, kick, punch and the red flash; nothing for a player without a view.
    pub(super) fn feel_for(&mut self, p: PlayerId, f: impl FnOnce(&mut super::juice::Juice)) {
        if p == ME {
            return f(&mut self.juice);
        }
        if p >= self.views {
            return;
        }
        let mine = self.juice.feel();
        self.juice.set_feel(self.locals[p].feel);
        f(&mut self.juice);
        self.locals[p].feel = self.juice.feel();
        self.juice.set_feel(mine);
    }

    /// The player view `v` follows: themselves, leading the dead included, or else once turned while others stand, the
    /// first who still does.
    pub fn followed(&self, v: PlayerId) -> PlayerId {
        if self.players[v].alive() || self.players[v].lead.is_some() || matches!(self.phase, Phase::Over { .. }) {
            return v;
        }
        (0..self.players.len()).find(|&p| self.players[p].alive()).unwrap_or(v)
    }

    /// Draws view `v` of a co-op run: what its player sees, or once turned, a living teammate's view under a banner.
    pub fn draw_view(&mut self, v: PlayerId, f: &mut crate::Frame, clean: bool) {
        let seen = self.followed(v);
        self.relabel(seen);
        self.draw(f, clean);
        if seen != v {
            let name = format!("YOU TURNED  -  WATCHING P{}", seen + 1);
            f.top.anchor([0.5, 0.0]);
            f.top.rect(0.0, 150.0, crate::SCREEN_W, 56.0, [0.0, 0.0, 0.0, 0.5]);
            f.top.label(name, crate::SCREEN_W / 2.0, 158.0, 36.0, super::RED, crate::Align::Center);
        }
        self.relabel(seen);
    }

    /// Whether player `p` holds X, so their aimed weapons go for barrels first.
    pub(super) fn targeting_of(&self, p: PlayerId) -> bool {
        if p == ME { self.targeting } else { self.locals.get(p).is_some_and(|l| l.targeting) }
    }

    /// Player `p`'s turn buttons count once let go after the A that started the run or ended a level-up.
    pub(super) fn turn_ready_mut(&mut self, p: PlayerId) -> Option<&mut bool> {
        if p == ME { Some(&mut self.turn_ready) } else { self.locals.get_mut(p).filter(|_| p < self.views).map(|l| &mut l.turn_ready) }
    }

    /// What every viewed player but ME presses this step, by place in `players`; scripted players steer themselves.
    pub fn set_inputs(&mut self, inputs: &[Input]) {
        for (i, input) in inputs.iter().enumerate() {
            let p = i + 1;
            if p < self.views && !self.bots.iter().any(|(b, _)| *b == p) {
                self.steer(p, *input);
            }
        }
    }

    /// A co-op step: every viewed player's controls, one shared world, a pause anyone opens for all, and a level-up
    /// that waits for everyone's pick.
    pub(super) fn update_coop(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>) -> super::Flow {
        let mut flow = super::Flow::default();
        if let Some(i) = self.inputs.get_mut(ME) {
            *i = *input;
        }
        let views = self.views;
        for v in 0..views {
            self.relabel(v);
            self.quick_off();
            self.relabel(v);
        }
        if let Some(at) = self.menu {
            let own = self.inputs.get(self.paused_by).copied().unwrap_or_default();
            self.pause_menu(at, &own, sounds, &mut flow);
            return flow;
        }
        if matches!(self.phase, Phase::Playing)
            && let Some(v) = (0..views).find(|&v| self.inputs[v].pressed.contains(crate::Buttons::START))
        {
            self.menu = Some(0);
            self.paused_by = v;
            sounds.push(Sfx::Tick(0));
            return flow;
        }
        self.juice.tick_real(dt);
        for v in 1..views {
            self.feel_for(v, |j| j.tick_feel(dt));
        }
        self.glory = (self.glory - dt).max(0.0);
        match self.phase {
            Phase::Playing => {
                self.drive_bots(sounds);
                for v in 0..views {
                    self.relabel(v);
                    let own = self.inputs[ME];
                    let own = if self.single { self.single_input(&own, dt, sounds).0 } else { own };
                    self.inputs[ME] = own;
                    self.relabel(v);
                }
                let own = self.inputs[ME];
                self.play(&own, dt, sounds, &mut flow);
            }
            Phase::LevelUp { .. } => self.choose_all(dt, sounds),
            Phase::Over { t: before, won } => {
                let t = before + dt;
                self.phase = Phase::Over { t, won };
                self.juice.tick(if t < super::TURN_SINK { dt * 0.4 } else { dt });
                self.wind_down(dt, sounds, &mut flow);
                for p in 0..self.players.len() {
                    if self.players[p].down.is_some() {
                        self.turning(p, dt, sounds);
                    }
                }
                for f in &mut self.fx {
                    f.t += dt;
                }
                self.fx.retain(|f| !f.done());
                let ready = if won { 1.0 } else { super::TURNED + 0.6 };
                let pressed = (0..views).fold(crate::Buttons::NONE, |b, v| b | self.inputs[v].pressed);
                if t > ready && pressed.any(crate::Buttons::A | crate::Buttons::START) {
                    self.restart(sounds);
                } else if t > ready && pressed.contains(crate::Buttons::B) {
                    flow.quit = true;
                }
            }
        }
        flow.found = std::mem::take(&mut self.found_now);
        flow
    }

    /// The cards come up: every other viewed player gets their own draw from their own kit, after ME's.
    pub(super) fn deal_others(&mut self) {
        for v in 1..self.views {
            if !self.players[v].alive() {
                continue;
            }
            self.relabel(v);
            let choices = self.roll_choices(&[]);
            self.relabel(v);
            let l = &mut self.locals[v];
            (l.dealt, l.banishing, l.favoring) = ([0.0; 3], false, false);
            l.cards = Some(Cards { choices, selected: 0, t: 0.0, picked: None });
        }
    }

    /// Each living viewed player picks from their own cards with their own pad; play resumes once all have.
    fn choose_all(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let mut done = true;
        for v in 0..self.views {
            if !self.players[v].alive() || (v != ME && self.locals[v].cards.is_none()) {
                continue;
            }
            self.relabel(v);
            let mut own = self.inputs[ME];
            if self.bots.iter().any(|(b, _)| *b == ME) {
                own = bot_pick(self);
            }
            self.choose(&own, dt, sounds);
            let Phase::LevelUp { picked, .. } = self.phase else { unreachable!("choose keeps the cards up in co-op") };
            done &= picked.is_some_and(|p| p >= super::PICK_TIME);
            self.relabel(v);
        }
        if done {
            self.phase = Phase::Playing;
            self.turn_ready = false;
            for l in &mut self.locals {
                l.cards = None;
                l.turn_ready = false;
            }
        }
    }
}

/// A scripted player with a view takes its first card once the cards are in.
fn bot_pick(h: &Harvest) -> Input {
    let Phase::LevelUp { t, picked: None, .. } = h.phase else { return Input::default() };
    let ready = super::CARDS_IN + h.dealt.iter().copied().fold(0.0, f32::max) + 0.8;
    if t > ready { Input { pressed: crate::Buttons::X, held: crate::Buttons::X, ..Input::default() } } else { Input::default() }
}

#[cfg(test)]
mod tests {
    use super::super::map::{MapSource, TILE};
    use super::super::*;

    const DT: f32 = 1.0 / 60.0;

    fn pair() -> Harvest {
        let mut h = Harvest::calm_on(5, &MapSource::new("Open", [40, 40])).unwrap().with_players(2).with_views(2);
        h.birds.clear();
        h.players[0].pos = [18.5 * TILE, 20.5 * TILE];
        h.players[1].pos = [22.5 * TILE, 20.5 * TILE];
        h
    }

    /// One step with player 1 pressing `one` and player 2 `two`.
    fn step(h: &mut Harvest, one: Buttons, two: Buttons) {
        h.set_inputs(&[Input { held: two, pressed: two, ..Input::default() }]);
        h.update(&Input { held: one, pressed: one, ..Input::default() }, DT, &mut Vec::new());
    }

    fn idle(h: &mut Harvest, secs: f32) {
        for _ in 0..(secs / DT) as u32 {
            step(h, Buttons::NONE, Buttons::NONE);
        }
    }

    fn level_up(h: &mut Harvest) {
        h.xp = h.next_xp;
        for _ in 0..600 {
            if matches!(h.phase, Phase::LevelUp { .. }) {
                break;
            }
            idle(h, DT);
        }
        assert!(matches!(h.phase, Phase::LevelUp { .. }), "the cards came up");
        idle(h, CARDS_IN + 0.1);
    }

    #[test]
    fn relabeling_twice_puts_every_player_back() {
        let mut h = pair();
        h.thrown.push(Grenade { from: [0.0; 2], to: [0.0; 2], t: 0.0, kind: Thrown::Grenade, by: None, owner: 1 });
        h.juice.tally.book_mut(1).hearts = 7;
        let (a, b) = (h.players[0].pos, h.players[1].pos);
        h.relabel(1);
        assert_eq!((h.players[0].pos, h.thrown[0].owner, h.juice.tally.book(ME).hearts, h.number(ME), h.number(1)), (b, ME, 7, 2, 1));
        h.relabel(1);
        assert_eq!((h.players[0].pos, h.players[1].pos, h.thrown[0].owner, h.juice.tally.book(1).hearts, h.number(ME)), (a, b, 1, 7, 1));
    }

    #[test]
    fn each_player_walks_by_their_own_pad() {
        let mut h = pair();
        let (a, b) = (h.players[0].pos, h.players[1].pos);
        for _ in 0..30 {
            step(&mut h, Buttons::NONE, Buttons::UP);
        }
        assert_eq!(h.players[0].pos, a, "player 1 held nothing");
        assert!(len(sub(h.players[1].pos, b)) > 10.0, "player 2 walked");
    }

    #[test]
    fn a_shared_level_up_deals_each_player_their_own_cards_and_waits_for_both_picks() {
        let mut h = pair();
        let shoes = powerups::book().defs.iter().position(|d| d.id == "running_shoes").unwrap();
        h.players[1].kit.raise(shoes, 1);
        level_up(&mut h);
        assert!(h.locals[1].cards.is_some(), "player 2 has their own draw");
        let [one, two] = [h.players[0].kit.clone(), h.players[1].kit.clone()];
        step(&mut h, Buttons::X, Buttons::NONE);
        idle(&mut h, PICK_TIME + 0.1);
        assert!(matches!(h.phase, Phase::LevelUp { .. }), "play waits for player 2");
        let levels = |k: &weapons::Kit| k.weapons.iter().map(|w| w.level).sum::<u32>() + k.passives.iter().map(|p| p.1).sum::<u32>();
        assert!(levels(&h.players[0].kit) > levels(&one), "player 1's card went to player 1");
        assert_eq!(levels(&h.players[1].kit), levels(&two), "and nothing yet to player 2");
        step(&mut h, Buttons::X, Buttons::X);
        idle(&mut h, PICK_TIME + 0.1);
        assert!(matches!(h.phase, Phase::Playing), "both picked");
        assert!(h.locals.iter().all(|l| l.cards.is_none()));
    }

    #[test]
    fn rerolls_and_banishes_are_each_players_own() {
        let mut h = pair();
        level_up(&mut h);
        let rerolls = h.rerolls;
        step(&mut h, Buttons::NONE, Buttons::R1);
        assert_eq!((h.rerolls, h.locals[1].rerolls), (rerolls, rerolls - 1), "player 2's reroll is theirs");
    }

    #[test]
    fn start_from_either_pauses_everyone_and_only_they_work_the_menu() {
        let mut h = pair();
        step(&mut h, Buttons::NONE, Buttons::START);
        assert_eq!((h.menu, h.paused_by), (Some(0), 1));
        let t = h.t;
        idle(&mut h, 0.5);
        assert_eq!(h.t, t, "the whole night waits");
        step(&mut h, Buttons::B, Buttons::NONE);
        assert!(h.menu.is_some(), "player 1 can't resume player 2's pause");
        step(&mut h, Buttons::NONE, Buttons::B);
        assert!(h.menu.is_none());
    }

    #[test]
    fn a_hit_shakes_only_its_player_and_never_freezes_the_shared_night() {
        let mut h = pair();
        h.ouch(1, 0.5);
        assert!(!h.juice.frozen() && h.juice.trauma == 0.0, "player 1's camera is still");
        h.relabel(1);
        assert!(h.juice.trauma > 0.0 && h.juice.hurt > 0.0, "player 2's shakes and flashes red");
        h.relabel(1);
        let t = h.t;
        idle(&mut h, 0.1);
        assert!(h.t > t + 0.05);
    }

    #[test]
    fn a_turned_player_watches_a_teammate_and_the_end_shows_each_players_numbers() {
        let mut h = pair();
        h.players[0].hp = 0.0;
        idle(&mut h, DT);
        assert_eq!(h.followed(0), 1);
        let mut f = crate::Frame::default();
        h.draw_view(0, &mut f, false);
        let says = |f: &crate::Frame, s: &str| f.top.shapes.iter().any(|x| matches!(x, crate::Shape::Text { text, .. } if text.contains(s)));
        assert!(says(&f, "WATCHING P2"));
        h.players[1].hp = 0.0;
        idle(&mut h, TURNED + 1.5);
        let mut f = crate::Frame::default();
        h.draw(&mut f, false);
        let both = if matches!(h.phase, Phase::Over { .. }) { [&f.top, &f.bottom].iter().any(|c| c.shapes.iter().any(|x| matches!(x, crate::Shape::Text { text, .. } if text.contains("P1 ") && text.contains("P2 ")))) } else { false };
        assert!(both, "the end screen names each player's kills");
    }

    #[test]
    fn a_narrow_view_pins_the_hud_to_its_edges_and_spans_full_screen_rects() {
        let mut c = crate::Canvas::default();
        c.rect(0.0, 0.0, crate::SCREEN_W, crate::SCREEN_H, [0.0; 4]);
        c.label("middle", 512.0, 400.0, 10.0, [1.0; 4], crate::Align::Center);
        c.anchor([1.0, 0.0]);
        c.label("kills", 1000.0, 28.0, 10.0, [1.0; 4], crate::Align::Right);
        c.anchor([0.0, 1.0]);
        c.rect(24.0, 700.0, 40.0, 40.0, [1.0; 4]);
        c.refit([1024.0, 1152.0]);
        let at = |i: usize| match &c.shapes[i] {
            crate::Shape::Rect { x, y, w, h, .. } => [*x, *y, *w, *h],
            crate::Shape::Text { x, y, .. } => [*x, *y, 0.0, 0.0],
            _ => unreachable!(),
        };
        assert_eq!(at(0), [0.0, 0.0, 1024.0, 1152.0], "a full-screen dim covers the whole view");
        assert_eq!(at(1)[..2], [512.0, 592.0], "the middle stays in the middle");
        assert_eq!(at(2)[..2], [1000.0, 28.0], "the top right stays top right");
        assert_eq!(at(3), [24.0, 1084.0, 40.0, 40.0], "the bottom left goes to the new bottom");
        assert_eq!(c.size, Some([1024.0, 1152.0]));
    }

    #[test]
    fn more_players_raise_the_cap_and_the_spawns() {
        let t = crate::tuning::tuning();
        let one = Harvest::new(1);
        let two = Harvest::new(1).with_players(2);
        assert_eq!(one.cap(), t.max_zombies as usize);
        assert_eq!(two.cap(), (t.max_zombies * (1.0 + t.max_per_player)) as usize);
        let owed = |mut h: Harvest| {
            h.spawn(1.0);
            h.owed + h.tells.len() as f32
        };
        let (a, b) = (owed(Harvest::new(1)), owed(Harvest::new(1).with_players(2)));
        assert!((b / a - (1.0 + t.spawn_per_player)).abs() < 0.05, "{a} then {b}");
    }
}
