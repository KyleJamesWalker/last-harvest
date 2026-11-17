//! A run's damage by what dealt it, and how long each weapon was held, for the damage per second at its end.

/// Damage dealt this run, by weapon.
#[derive(Clone, Debug, Default)]
pub(super) struct Tally {
    /// The weapon damage dealt now goes to, by its place in the powerup book; None for grenades and barrels.
    pub by: Option<usize>,
    dealt: Vec<f32>,
    held: Vec<f32>,
    /// Grenades' and barrels' damage.
    other: f32,
    /// Seconds of play.
    pub secs: f32,
    /// Damage up to each zombie's remaining health, and killing blows, by the same keys as `dealt` and `other`.
    useful: Vec<f32>,
    useful_other: f32,
    kills: Vec<u32>,
    kills_other: u32,
    /// Hearts picked up.
    pub hearts: u32,
    /// The night's time the player first fell, even if the run was undying and stood them back up.
    pub fell: Option<f32>,
}

/// What one weapon, or grenades and barrels, did over a run.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dealt {
    pub raw: f32,
    pub useful: f32,
    pub kills: u32,
    /// Seconds held, or the whole run for grenades and barrels.
    pub held: f32,
}

/// One line of the end screen's table: what dealt the damage (None for grenades and barrels), how much, and how
/// much a second over the time it was held.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Row {
    pub by: Option<usize>,
    pub damage: f32,
    pub dps: f32,
}

impl Tally {
    /// A tally with room for `defs` powerups, so adding never allocates.
    pub fn sized(defs: usize) -> Self {
        Self { dealt: vec![0.0; defs], held: vec![0.0; defs], useful: vec![0.0; defs], kills: vec![0; defs], ..Self::default() }
    }

    pub fn add(&mut self, amount: f32) {
        match self.by {
            Some(d) => *grow(&mut self.dealt, d) += amount,
            None => self.other += amount,
        }
    }

    /// Counts `dt` seconds of play, held by each of `defs`.
    pub fn hold(&mut self, defs: impl Iterator<Item = usize>, dt: f32) {
        self.secs += dt;
        for d in defs {
            *grow(&mut self.held, d) += dt;
        }
    }

    /// A hit of `dealt` on a zombie with `hp` left: what of it was useful, and whether it was the killing blow.
    pub fn land(&mut self, dealt: f32, hp: f32) {
        let useful = dealt.min(hp.max(0.0));
        let kill = (hp > 0.0 && hp <= dealt) as u32;
        match self.by {
            Some(d) => {
                *grow(&mut self.useful, d) += useful;
                if d >= self.kills.len() {
                    self.kills.resize(d + 1, 0);
                }
                self.kills[d] += kill;
            }
            None => {
                self.useful_other += useful;
                self.kills_other += kill;
            }
        }
    }

    /// What weapon `by` did, or grenades and barrels for None.
    pub fn dealt(&self, by: Option<usize>) -> Dealt {
        let at = |v: &[f32], d: usize| v.get(d).copied().unwrap_or(0.0);
        match by {
            Some(d) => Dealt { raw: at(&self.dealt, d), useful: at(&self.useful, d), kills: self.kills.get(d).copied().unwrap_or(0), held: at(&self.held, d) },
            None => Dealt { raw: self.other, useful: self.useful_other, kills: self.kills_other, held: self.secs },
        }
    }

    pub fn total(&self) -> f32 {
        self.dealt.iter().sum::<f32>() + self.other
    }

    /// Damage a second over the whole run.
    pub fn dps(&self) -> f32 {
        per_sec(self.total(), self.secs)
    }

    /// Each weapon held or that did damage, and grenades and barrels if they did, most damage first.
    pub fn rows(&self) -> Vec<Row> {
        let n = self.held.len().max(self.dealt.len());
        let at = |v: &[f32], d: usize| v.get(d).copied().unwrap_or(0.0);
        let mut rows: Vec<Row> = (0..n)
            .map(|d| (d, at(&self.held, d), at(&self.dealt, d)))
            .filter(|&(_, held, damage)| held > 0.0 || damage > 0.0)
            .map(|(d, held, damage)| Row { by: Some(d), damage, dps: per_sec(damage, held) })
            .collect();
        if self.other > 0.0 {
            rows.push(Row { by: None, damage: self.other, dps: per_sec(self.other, self.secs) });
        }
        rows.sort_by(|a, b| b.damage.total_cmp(&a.damage));
        rows
    }
}

fn grow(v: &mut Vec<f32>, i: usize) -> &mut f32 {
    if i >= v.len() {
        v.resize(i + 1, 0.0);
    }
    &mut v[i]
}

fn per_sec(damage: f32, secs: f32) -> f32 {
    if secs > 0.0 { damage / secs } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_weapon_counts_over_the_time_it_was_held_and_the_run_over_all_of_it() {
        let mut t = Tally::sized(4);
        t.hold([0].into_iter(), 10.0);
        t.hold([0, 2].into_iter(), 10.0);
        t.by = Some(0);
        t.add(100.0);
        t.by = Some(2);
        t.add(300.0);
        t.by = None;
        t.add(40.0);
        assert_eq!(t.total(), 440.0);
        assert_eq!(t.dps(), 22.0);
        assert_eq!(t.rows(), [
            Row { by: Some(2), damage: 300.0, dps: 30.0 },
            Row { by: Some(0), damage: 100.0, dps: 5.0 },
            Row { by: None, damage: 40.0, dps: 2.0 },
        ]);
    }

    #[test]
    fn a_hit_is_useful_only_up_to_the_health_left_and_the_last_one_kills() {
        let mut t = Tally::sized(2);
        t.by = Some(1);
        t.land(150.0, 20.0);
        t.land(5.0, -130.0);
        t.land(8.0, 30.0);
        t.by = None;
        t.land(40.0, 40.0);
        assert_eq!(t.dealt(Some(1)), Dealt { raw: 0.0, useful: 28.0, kills: 1, held: 0.0 });
        assert_eq!(t.dealt(None), Dealt { raw: 0.0, useful: 40.0, kills: 1, held: 0.0 });
        assert_eq!(t.dealt(Some(0)), Dealt::default());
    }

    #[test]
    fn a_weapon_held_without_a_hit_still_shows_and_an_empty_run_is_zero() {
        let mut t = Tally::sized(2);
        assert_eq!((t.dps(), t.rows().len()), (0.0, 0));
        t.hold([1].into_iter(), 5.0);
        assert_eq!(t.rows(), [Row { by: Some(1), damage: 0.0, dps: 0.0 }]);
    }
}
