//! A pack's own farms, `farms/<id>.txt` and `farms/<id>.ground.txt`, which a player card picks, and the one in use.

use std::sync::{OnceLock, RwLock};

use super::farm::{self, Farm};
use super::ground::{self, Ground};
use crate::data::FarmFile;

/// The most a farm's id can be, so a player card names it inline.
pub const ID_MAX: usize = 24;

struct Named {
    id: String,
    farm: &'static Farm,
    ground: &'static [Ground],
}

static FARMS: OnceLock<Vec<Named>> = OnceLock::new();

/// The game's ground with every farm's laid over it, so a hand-made map may paint any of their codes.
static ALL: OnceLock<&'static [Ground]> = OnceLock::new();

#[derive(Clone, Copy)]
enum Active {
    Game,
    Named(usize),
    All,
}

static ACTIVE: RwLock<Active> = RwLock::new(Active::Game);

/// Whether `id` can name a farm.
pub fn valid_id(id: &str) -> bool {
    (1..=ID_MAX).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `base` with each of `top`'s kinds in place of the one with its code, and the new codes after.
pub fn lay_over(base: &[Ground], top: Vec<Ground>) -> Vec<Ground> {
    let mut out = base.to_vec();
    for g in top {
        match out.iter_mut().find(|o| o.code == g.code) {
            Some(o) => *o = g,
            None => out.push(g),
        }
    }
    out
}

/// A farm as it reads over the game's: its rules, else the game's, and its ground laid over the game's.
fn read(f: &FarmFile) -> Result<(Farm, Vec<Ground>), String> {
    let mut kinds = ground::ground().to_vec();
    if let Some(text) = &f.ground {
        let (top, missing) = ground::parse_noting(text).map_err(|e| format!("farms/{}.ground.txt:{e}", f.id))?;
        for name in missing {
            crate::data::note(format!("farms/{}.ground.txt: no prop called {name}, so its crop is left off", f.id));
        }
        kinds = lay_over(&kinds, top);
    }
    let rules = match &f.farm {
        Some(text) => farm::parse(text).map_err(|e| format!("farms/{}.txt:{e}", f.id))?,
        None => farm::farm().clone(),
    };
    farm::check_codes(&rules, &kinds).map_err(|e| format!("farms/{}: {e}", f.id))?;
    Ok((rules, kinds))
}

/// Reads the pack's farms over the game's farm and ground, once, and returns a line for each left out.
pub fn install(files: &[FarmFile]) -> Vec<String> {
    let mut notes = Vec::new();
    let mut named: Vec<Named> = Vec::new();
    for f in files {
        if !valid_id(&f.id) {
            notes.push(format!("farms/{}: a farm's id is 1 to {ID_MAX} of a-z, 0-9 and _", f.id));
            continue;
        }
        match read(f) {
            Ok((rules, kinds)) => {
                named.retain(|n| n.id != f.id);
                named.push(Named { id: f.id.clone(), farm: Box::leak(Box::new(rules)), ground: kinds.leak() });
            }
            Err(e) => notes.push(e),
        }
    }
    let all = named.iter().fold(ground::ground().to_vec(), |all, n| lay_over(&all, n.ground.to_vec()));
    ALL.set(all.leak()).ok();
    FARMS.set(named).ok();
    notes
}

/// The farms a player card can pick, by id.
pub fn ids() -> Vec<String> {
    FARMS.get().map_or(Vec::new(), |f| f.iter().map(|n| n.id.clone()).collect())
}

/// Lays out and paints with the farm `id` until told otherwise; the game's own for None, or an id no pack brings.
pub fn use_farm(id: Option<&str>) {
    let at = id.and_then(|id| FARMS.get()?.iter().position(|n| n.id == id));
    *ACTIVE.write().unwrap_or_else(|e| e.into_inner()) = at.map_or(Active::Game, Active::Named);
}

/// Runs `f` with every farm's ground at once, as a hand-made map is checked, then goes back to the game's.
pub(crate) fn with_every_ground<T>(f: impl FnOnce() -> T) -> T {
    *ACTIVE.write().unwrap_or_else(|e| e.into_inner()) = Active::All;
    let out = f();
    *ACTIVE.write().unwrap_or_else(|e| e.into_inner()) = Active::Game;
    out
}

fn active() -> Active {
    *ACTIVE.read().unwrap_or_else(|e| e.into_inner())
}

/// The farm in use's rules, where it isn't the game's.
pub(super) fn active_farm() -> Option<&'static Farm> {
    match active() {
        Active::Named(i) => FARMS.get().map(|f| f[i].farm),
        _ => None,
    }
}

/// The farm in use's ground, where it isn't the game's.
pub(super) fn active_ground() -> Option<&'static [Ground]> {
    match active() {
        Active::Named(i) => FARMS.get().map(|f| f[i].ground),
        Active::All => ALL.get().copied(),
        Active::Game => None,
    }
}
