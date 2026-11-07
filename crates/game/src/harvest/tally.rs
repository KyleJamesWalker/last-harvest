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
        Self { dealt: vec![0.0; defs], held: vec![0.0; defs], ..Self::default() }
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
    fn a_weapon_held_without_a_hit_still_shows_and_an_empty_run_is_zero() {
        let mut t = Tally::sized(2);
        assert_eq!((t.dps(), t.rows().len()), (0.0, 0));
        t.hold([1].into_iter(), 5.0);
        assert_eq!(t.rows(), [Row { by: Some(1), damage: 0.0, dps: 0.0 }]);
    }
}
