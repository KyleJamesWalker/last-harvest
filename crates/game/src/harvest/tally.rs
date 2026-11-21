//! A run's damage by what dealt it, and how long each weapon was held, for the damage per second at its end.

use super::player::PlayerId;

/// Damage dealt this run, by player and weapon.
#[derive(Clone, Debug, Default)]
pub(super) struct Tally {
    /// The weapon damage dealt now goes to, by its place in the powerup book; None for grenades and barrels.
    pub by: Option<usize>,
    /// The player whose book damage dealt now goes in.
    pub owner: PlayerId,
    books: Vec<Book>,
    /// Defs each new book has room for.
    defs: usize,
    /// Seconds of play.
    pub secs: f32,
    /// Set once the run is over: what's still in flight lands, but the run's numbers are final.
    pub shut: bool,
}

/// One player's damage this run, by weapon.
#[derive(Clone, Debug, Default)]
pub(super) struct Book {
    dealt: Vec<f32>,
    held: Vec<f32>,
    /// Grenades' and barrels' damage.
    other: f32,
    /// Damage up to each zombie's remaining health, and killing blows, by the same keys as `dealt` and `other`.
    useful: Vec<f32>,
    useful_other: f32,
    kills: Vec<u32>,
    kills_other: u32,
    /// Hearts picked up.
    pub hearts: u32,
    /// The night's time the player first fell, even if the run was undying and stood them back up.
    pub fell: Option<f32>,
    /// Turned: seconds spent leading the dead, and the bites their swarm landed.
    pub led: f32,
    pub bites: u32,
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

impl Book {
    fn sized(defs: usize) -> Self {
        Self { dealt: vec![0.0; defs], held: vec![0.0; defs], useful: vec![0.0; defs], kills: vec![0; defs], ..Self::default() }
    }
}

impl Tally {
    /// A tally with room for `defs` powerups for each of `players`, so adding never allocates.
    pub fn sized(defs: usize, players: usize) -> Self {
        Self { books: (0..players).map(|_| Book::sized(defs)).collect(), defs, ..Self::default() }
    }

    /// Players 0 and `v` trade books, as `Harvest::relabel` trades them everywhere.
    pub fn relabel(&mut self, v: PlayerId) {
        if v >= self.books.len() {
            let defs = self.defs;
            self.books.resize_with(v + 1, || Book::sized(defs));
        }
        self.books.swap(0, v);
        self.owner = if self.owner == 0 { v } else if self.owner == v { 0 } else { self.owner };
    }

    /// Damage dealt from now on goes to `owner`'s weapon `by`.
    pub fn credit(&mut self, owner: PlayerId, by: Option<usize>) {
        self.owner = owner;
        self.by = by;
    }

    pub fn book(&self, p: PlayerId) -> &Book {
        static EMPTY: Book = Book { dealt: Vec::new(), held: Vec::new(), other: 0.0, useful: Vec::new(), useful_other: 0.0, kills: Vec::new(), kills_other: 0, hearts: 0, fell: None, led: 0.0, bites: 0 };
        self.books.get(p).unwrap_or(&EMPTY)
    }

    pub fn book_mut(&mut self, p: PlayerId) -> &mut Book {
        if p >= self.books.len() {
            let defs = self.defs;
            self.books.resize_with(p + 1, || Book::sized(defs));
        }
        &mut self.books[p]
    }

    pub fn add(&mut self, amount: f32) {
        if self.shut {
            return;
        }
        let by = self.by;
        let b = self.book_mut(self.owner);
        match by {
            Some(d) => *grow(&mut b.dealt, d) += amount,
            None => b.other += amount,
        }
    }

    /// Counts `dt` seconds of play.
    pub fn tick(&mut self, dt: f32) {
        self.secs += dt;
    }

    /// Counts `dt` seconds held by each of `defs` in player `p`'s hands.
    pub fn hold(&mut self, p: PlayerId, defs: impl Iterator<Item = usize>, dt: f32) {
        let b = self.book_mut(p);
        for d in defs {
            *grow(&mut b.held, d) += dt;
        }
    }

    /// A hit of `dealt` on a zombie with `hp` left: what of it was useful, and whether it was the killing blow.
    pub fn land(&mut self, dealt: f32, hp: f32) {
        if self.shut {
            return;
        }
        let useful = dealt.min(hp.max(0.0));
        let kill = (hp > 0.0 && hp <= dealt) as u32;
        let by = self.by;
        let b = self.book_mut(self.owner);
        match by {
            Some(d) => {
                *grow(&mut b.useful, d) += useful;
                if d >= b.kills.len() {
                    b.kills.resize(d + 1, 0);
                }
                b.kills[d] += kill;
            }
            None => {
                b.useful_other += useful;
                b.kills_other += kill;
            }
        }
    }

    /// What player `p`'s weapon `by` did, or their grenades and barrels for None.
    pub fn dealt(&self, p: PlayerId, by: Option<usize>) -> Dealt {
        let b = self.book(p);
        let at = |v: &[f32], d: usize| v.get(d).copied().unwrap_or(0.0);
        match by {
            Some(d) => Dealt { raw: at(&b.dealt, d), useful: at(&b.useful, d), kills: b.kills.get(d).copied().unwrap_or(0), held: at(&b.held, d) },
            None => Dealt { raw: b.other, useful: b.useful_other, kills: b.kills_other, held: self.secs },
        }
    }

    pub fn total(&self, p: PlayerId) -> f32 {
        let b = self.book(p);
        b.dealt.iter().sum::<f32>() + b.other
    }

    /// Player `p`'s killing blows and useful damage over the run, every weapon's and their grenades' and barrels'.
    pub fn sum(&self, p: PlayerId) -> (u32, f32) {
        let b = self.book(p);
        (b.kills.iter().sum::<u32>() + b.kills_other, b.useful.iter().sum::<f32>() + b.useful_other)
    }

    /// Player `p`'s damage a second over the whole run.
    pub fn dps(&self, p: PlayerId) -> f32 {
        per_sec(self.total(p), self.secs)
    }

    /// Each weapon player `p` held or that did damage, and grenades and barrels if they did, most damage first.
    pub fn rows(&self, p: PlayerId) -> Vec<Row> {
        let b = self.book(p);
        let n = b.held.len().max(b.dealt.len());
        let at = |v: &[f32], d: usize| v.get(d).copied().unwrap_or(0.0);
        let mut rows: Vec<Row> = (0..n)
            .map(|d| (d, at(&b.held, d), at(&b.dealt, d)))
            .filter(|&(_, held, damage)| held > 0.0 || damage > 0.0)
            .map(|(d, held, damage)| Row { by: Some(d), damage, dps: per_sec(damage, held) })
            .collect();
        if b.other > 0.0 {
            rows.push(Row { by: None, damage: b.other, dps: per_sec(b.other, self.secs) });
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
        let mut t = Tally::sized(4, 1);
        t.tick(10.0);
        t.hold(0, [0].into_iter(), 10.0);
        t.tick(10.0);
        t.hold(0, [0, 2].into_iter(), 10.0);
        t.by = Some(0);
        t.add(100.0);
        t.by = Some(2);
        t.add(300.0);
        t.by = None;
        t.add(40.0);
        assert_eq!(t.total(0), 440.0);
        assert_eq!(t.dps(0), 22.0);
        assert_eq!(t.rows(0), [
            Row { by: Some(2), damage: 300.0, dps: 30.0 },
            Row { by: Some(0), damage: 100.0, dps: 5.0 },
            Row { by: None, damage: 40.0, dps: 2.0 },
        ]);
    }

    #[test]
    fn a_hit_is_useful_only_up_to_the_health_left_and_the_last_one_kills() {
        let mut t = Tally::sized(2, 1);
        t.by = Some(1);
        t.land(150.0, 20.0);
        t.land(5.0, -130.0);
        t.land(8.0, 30.0);
        t.by = None;
        t.land(40.0, 40.0);
        assert_eq!(t.dealt(0, Some(1)), Dealt { raw: 0.0, useful: 28.0, kills: 1, held: 0.0 });
        assert_eq!(t.dealt(0, None), Dealt { raw: 0.0, useful: 40.0, kills: 1, held: 0.0 });
        assert_eq!(t.dealt(0, Some(0)), Dealt::default());
    }

    #[test]
    fn a_weapon_held_without_a_hit_still_shows_and_an_empty_run_is_zero() {
        let mut t = Tally::sized(2, 1);
        assert_eq!((t.dps(0), t.rows(0).len()), (0.0, 0));
        t.tick(5.0);
        t.hold(0, [1].into_iter(), 5.0);
        assert_eq!(t.rows(0), [Row { by: Some(1), damage: 0.0, dps: 0.0 }]);
    }
}
