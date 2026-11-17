//! The DPS sandbox's command line: plays whole nights headless, as fast as the cores allow, for each build and
//! seed, and prints what each weapon did, with CSV and JSON beside it and a diff of two CSVs.

use std::fmt::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use last_harvest::{VERSION, pak};
use last_harvest_game::Difficulty;
use last_harvest_game::sandbox::{self, Night, Pick, Plan, Walk};

const USAGE: &str = "\
usage: last-harvest-sandbox [options] [BUILD ...]
       last-harvest-sandbox --diff OLD.csv NEW.csv

Plays whole nights headless with a scripted player and a forced build, and prints kills, useful and raw damage,
hearts and level for each. With no BUILD, measures each weapon alone at each level and its breaker's.

  BUILD               powerups as id or id:level, comma separated; a level may be top or break: pistol:8,knife:top
  --weapons a,b       the matrix only for these weapons
  --seeds N           nights per build, each on its own seed (3)
  --seed S            the first seed (1)
  --difficulty D      easy, normal or hard (normal)
  --size WxH          the generated farm's size in tiles (72x54)
  --walk W            circle; eight, a loop each way; or avoid, turning away from the zombies on the map (circle)
  --turn K            how hard circle and eight turn, 0 to 1 of the turn buttons' speed (0.25)
  --turn-every S      seconds between turnarounds, 0 for none; avoid never turns round (6)
  --throw-every S     seconds between grenades lobbed straight ahead, 0 for none (12)
  --no-throw          the same as --throw-every 0
  --pick P            level-up cards: none, the build staying fixed; random; held, what the build holds, with
                      rerolls and banishes spent to find it; or first:a+b+c, the first named on offer, else as held
                      (none)
  --locked            with a pick, only the powerups a new player has unlocked come up; else every one
  --mortal            the player can die, which ends the night; else they stand back up and play on
  --secs S            stop this many seconds into the night; past dawn (600) the night goes on, the Harvester awake
  --jobs N            nights played at once (every core)
  --csv FILE          one row per build and weapon, and an `all` row per build, under a `#` line of the settings
  --json FILE         every night played
  --pak FILE          the data pack, instead of the built-in data or data/game_data.pak
  --mods DIR          a folder of mod packs to lay over it
  --diff OLD NEW      compare two --csv files by build and weapon
";

struct Args {
    builds: Vec<String>,
    weapons: Option<Vec<String>>,
    seeds: u64,
    seed: u64,
    plan: Plan,
    jobs: usize,
    csv: Option<String>,
    json: Option<String>,
    /// `--pick` as given, read once the data is loaded.
    pick: String,
    locked: bool,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("--help" | "-h") => return print!("{USAGE}"),
        Some("--version") => return println!("{VERSION}"),
        Some("--diff") => match &args[1..] {
            [old, new] => diff(old, new),
            _ => Err("--diff takes two CSV files".into()),
        },
        _ => parse(&args).and_then(run),
    };
    if let Err(e) = result {
        eprintln!("last-harvest-sandbox: {e}");
        std::process::exit(2);
    }
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut a = Args { builds: Vec::new(), weapons: None, seeds: 3, seed: 1, plan: Plan::new(Vec::new()), jobs: std::thread::available_parallelism().map_or(1, |n| n.get()), csv: None, json: None, pick: "none".into(), locked: false };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("{arg} needs a value"));
        fn num<T: std::str::FromStr>(arg: &str, v: String) -> Result<T, String> {
            v.parse().map_err(|_| format!("{arg}: {v} is not a number"))
        }
        match arg.as_str() {
            "--weapons" => a.weapons = Some(value()?.split(',').map(|s| s.trim().to_string()).collect()),
            "--seeds" => a.seeds = num::<u64>(arg, value()?)?.max(1),
            "--seed" => a.seed = num(arg, value()?)?,
            "--difficulty" => {
                let v = value()?;
                a.plan.difficulty = *Difficulty::ALL.iter().find(|d| d.key() == v).ok_or_else(|| format!("--difficulty: {v} is not easy, normal or hard"))?;
            }
            "--size" => {
                let v = value()?;
                let (w, h) = v.split_once('x').ok_or_else(|| format!("--size: {v} is not WxH"))?;
                a.plan.size = [num(arg, w.into())?, num(arg, h.into())?];
            }
            "--walk" => {
                let v = value()?;
                a.plan.walk = Walk::from_name(&v).ok_or_else(|| format!("--walk: {v} is not circle, eight or avoid"))?;
            }
            "--pick" => a.pick = value()?,
            "--locked" => a.locked = true,
            "--no-throw" => a.plan.throw_every = 0.0,
            "--turn" => a.plan.turn = num::<f32>(arg, value()?)?.clamp(0.0, 1.0),
            "--turn-every" => a.plan.turn_every = num(arg, value()?)?,
            "--throw-every" => a.plan.throw_every = num(arg, value()?)?,
            "--mortal" => a.plan.mortal = true,
            "--secs" => a.plan.until = Some(num(arg, value()?)?),
            "--jobs" => a.jobs = num::<usize>(arg, value()?)?.max(1),
            "--csv" => a.csv = Some(value()?),
            "--json" => a.json = Some(value()?),
            // SAFETY: no other thread has started yet.
            "--pak" => unsafe { std::env::set_var("LAST_HARVEST_PAK", value()?) },
            "--mods" => unsafe { std::env::set_var("LAST_HARVEST_MODS", value()?) },
            s if s.starts_with("--") => return Err(format!("unknown option {s}\n\n{USAGE}")),
            s => a.builds.push(s.to_string()),
        }
    }
    Ok(a)
}

/// Every night one build played, by seed.
struct Cell {
    build: Vec<(usize, u32)>,
    nights: Vec<Night>,
}

fn run(mut a: Args) -> Result<(), String> {
    pak::load()?;
    a.plan.pick = Pick::from_name(&a.pick).map_err(|e| format!("--pick: {e}"))?;
    a.plan.unlocked = a.plan.pick != Pick::None && !a.locked;
    let builds = if a.builds.is_empty() { matrix(a.weapons.as_deref())? } else { a.builds.iter().map(|b| sandbox::parse_build(b)).collect::<Result<_, _>>()? };
    let jobs: Vec<(usize, u64)> = (0..builds.len()).flat_map(|b| (0..a.seeds).map(move |s| (b, s))).collect();
    let nights: Vec<Mutex<Option<Night>>> = jobs.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let started = Instant::now();
    eprintln!("sandbox: {} builds x {} seeds, {} nights on {} threads", builds.len(), a.seeds, jobs.len(), a.jobs);
    std::thread::scope(|s| {
        for _ in 0..a.jobs.min(jobs.len()) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(b, seed)) = jobs.get(i) else { break };
                    let plan = Plan { seed: a.seed + seed, build: builds[b].clone(), ..a.plan.clone() };
                    let night = sandbox::night(&plan);
                    *nights[i].lock().unwrap() = Some(night);
                    if (i + 1).is_multiple_of(25) {
                        eprintln!("sandbox: {} of {} nights, {:.0} s", i + 1, jobs.len(), started.elapsed().as_secs_f32());
                    }
                }
            });
        }
    });
    let mut nights = nights.into_iter().map(|n| n.into_inner().unwrap().unwrap());
    let cells: Vec<Cell> = builds.into_iter().map(|build| Cell { build, nights: nights.by_ref().take(a.seeds as usize).collect() }).collect();
    eprintln!("sandbox: done in {:.0} s", started.elapsed().as_secs_f32());
    print!("{}", table(&cells, &a.plan));
    if let Some(path) = &a.csv {
        std::fs::write(path, csv(&cells, &a.plan)).map_err(|e| format!("{path}: {e}"))?;
    }
    if let Some(path) = &a.json {
        std::fs::write(path, json(&cells, &a)).map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(())
}

/// Each weapon alone at each of its levels and its breaker's.
fn matrix(only: Option<&[String]>) -> Result<Vec<Vec<(usize, u32)>>, String> {
    let all = sandbox::weapon_levels();
    if let Some(bad) = only.into_iter().flatten().find(|w| !all.iter().any(|(id, _)| id == *w)) {
        return Err(format!("--weapons: {bad} is not a weapon"));
    }
    all.into_iter()
        .filter(|(id, _)| only.is_none_or(|o| o.contains(id)))
        .flat_map(|(id, top)| (1..=top).map(move |l| sandbox::parse_build(&format!("{id}:{l}"))))
        .collect()
}

fn mean(v: impl IntoIterator<Item = f32>) -> f32 {
    let v: Vec<f32> = v.into_iter().collect();
    if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 }
}

/// The sample standard deviation.
fn spread(v: impl IntoIterator<Item = f32>) -> f32 {
    let v: Vec<f32> = v.into_iter().collect();
    if v.len() < 2 {
        return 0.0;
    }
    let m = mean(v.iter().copied());
    (v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / (v.len() - 1) as f32).sqrt()
}

/// One build's nights, or one weapon's share of them, averaged over the seeds.
struct Row {
    weapon: String,
    level: u32,
    secs: f32,
    kills: f32,
    kills_sd: f32,
    useful: f32,
    raw: f32,
    /// Useful and raw damage a second held.
    dps: f32,
    raw_dps: f32,
}

impl Row {
    fn per_min(&self) -> f32 {
        if self.secs > 0.0 { self.kills * 60.0 / self.secs } else { 0.0 }
    }

    fn overkill(&self) -> f32 {
        if self.raw > 0.0 { 1.0 - self.useful / self.raw } else { 0.0 }
    }
}

impl Cell {
    fn all(&self) -> Row {
        let n = &self.nights;
        let sum = |night: &Night, f: fn(&sandbox::Line) -> f32| night.lines.iter().map(f).sum::<f32>();
        let secs = mean(n.iter().map(|x| x.secs));
        let (useful, raw) = (mean(n.iter().map(|x| sum(x, |l| l.useful))), mean(n.iter().map(|x| sum(x, |l| l.raw))));
        let per = |v: f32| if secs > 0.0 { v / secs } else { 0.0 };
        Row { weapon: "all".into(), level: 0, secs, kills: mean(n.iter().map(|x| x.kills as f32)), kills_sd: spread(n.iter().map(|x| x.kills as f32)), useful, raw, dps: per(useful), raw_dps: per(raw) }
    }

    /// Each weapon any night ended with, grenades last; a night without it counts nothing for it, and its level is
    /// the mean of the nights that had it, as a pick can raise it.
    fn lines(&self) -> Vec<Row> {
        let mut ids: Vec<&str> = Vec::new();
        for l in self.nights.iter().flat_map(|x| &x.lines).filter(|l| l.id != "grenades") {
            if !ids.contains(&l.id.as_str()) {
                ids.push(&l.id);
            }
        }
        ids.push("grenades");
        ids.into_iter()
            .map(|id| {
                let line = |x: &'_ Night| x.lines.iter().find(|l| l.id == id).cloned();
                let of = |f: fn(&sandbox::Line) -> f32| mean(self.nights.iter().map(|x| line(x).as_ref().map_or(0.0, f)));
                let (held, useful, raw) = (of(|l| l.held), of(|l| l.useful), of(|l| l.raw));
                let per = |v: f32| if held > 0.0 { v / held } else { 0.0 };
                let kills: Vec<f32> = self.nights.iter().map(|x| line(x).map_or(0.0, |l| l.kills as f32)).collect();
                let level = mean(self.nights.iter().filter_map(line).map(|l| l.level as f32)).round() as u32;
                Row { weapon: id.to_string(), level, secs: held, kills: mean(kills.iter().copied()), kills_sd: spread(kills), useful, raw, dps: per(useful), raw_dps: per(raw) }
            })
            .collect()
    }

    fn name(&self) -> String {
        sandbox::build_text(&self.build)
    }

    fn hearts(&self) -> f32 {
        mean(self.nights.iter().map(|x| x.hearts as f32))
    }

    fn level(&self) -> f32 {
        mean(self.nights.iter().map(|x| x.level as f32))
    }

    /// The mean level at the end of minute `m`, counting from 1, over the nights that got that far.
    fn level_at(&self, m: usize) -> Option<f32> {
        let at: Vec<f32> = self.nights.iter().filter_map(|x| x.minutes.get(m - 1)).map(|x| x.level as f32).collect();
        (!at.is_empty()).then(|| mean(at))
    }

    fn by_minute(&self, f: fn(&sandbox::Minute) -> u32) -> String {
        let most = self.nights.iter().map(|x| x.minutes.len()).max().unwrap_or(0);
        (0..most).map(|m| format!("{:.1}", mean(self.nights.iter().filter_map(|x| x.minutes.get(m)).map(|x| f(x) as f32)))).collect::<Vec<_>>().join(";")
    }

    /// The nights the player fell, and when on average.
    fn fell(&self) -> (usize, Option<f32>) {
        let fell: Vec<f32> = self.nights.iter().filter_map(|x| x.fell).collect();
        (fell.len(), (!fell.is_empty()).then(|| mean(fell.iter().copied())))
    }

    /// The mean time to the first fall, a night that never fell counting as its whole length: survival restricted to
    /// the night, censored where it never fell.
    fn survival(&self) -> f32 {
        mean(self.nights.iter().map(|x| x.fell.unwrap_or(x.secs)))
    }

    fn spawned(&self) -> f32 {
        mean(self.nights.iter().map(|x| x.spawned as f32))
    }

    /// The share of the night spent at the zombie cap.
    fn capped(&self) -> f32 {
        mean(self.nights.iter().map(|x| if x.secs > 0.0 { x.capped / x.secs } else { 0.0 }))
    }

    /// Kills held back by the spawn rate, not the build: the field never filled and nearly all that rose died.
    fn at_ceiling(&self) -> bool {
        let spawned = self.spawned();
        self.capped() < 0.02 && spawned > 0.0 && self.all().kills >= 0.9 * spawned
    }
}

fn table(cells: &[Cell], plan: &Plan) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{} nights, {}", cells.first().map_or(0, |c| c.nights.len()), settings(plan));
    let _ = writeln!(s, "{:<28} {:>11} {:>6} {:>9} {:>9} {:>5} {:>6} {:>7} {:>6} {:>5} {:>5} {:>8} {:>5} {:>4}", "build", "kills", "k/min", "useful", "raw", "over", "dps", "raw dps", "hearts", "lvl5", "lvl", "fell", "cap", "ceil");
    for c in cells {
        let all = c.all();
        let level5 = c.level_at(5).map_or("-".into(), |l| format!("{l:.1}"));
        let fell = match c.fell() {
            (0, _) => "-".into(),
            (n, Some(t)) => format!("{n}@{t:.0}s"),
            (n, None) => format!("{n}"),
        };
        let _ = writeln!(
            s,
            "{:<28} {:>11} {:>6.1} {:>9.0} {:>9.0} {:>4.0}% {:>6.1} {:>7.1} {:>6.0} {:>5} {:>5.1} {:>8} {:>4.0}% {:>4}",
            c.name(),
            format!("{:.0}±{:.0}", all.kills, all.kills_sd),
            all.per_min(),
            all.useful,
            all.raw,
            all.overkill() * 100.0,
            all.dps,
            all.raw_dps,
            c.hearts(),
            level5,
            c.level(),
            fell,
            c.capped() * 100.0,
            if c.at_ceiling() { "yes" } else { "" }
        );
        let lines = c.lines();
        if lines.iter().filter(|l| l.raw > 0.0 || l.weapon != "grenades").count() > 1 {
            for l in lines.iter().filter(|l| l.raw > 0.0) {
                let _ = writeln!(s, "  {:<26} {:>11} {:>6.1} {:>9.0} {:>9.0} {:>4.0}% {:>6.1} {:>7.1}", if l.level > 0 { format!("{}:{}", l.weapon, l.level) } else { l.weapon.clone() }, format!("{:.0}±{:.0}", l.kills, l.kills_sd), l.per_min(), l.useful, l.raw, l.overkill() * 100.0, l.dps, l.raw_dps);
            }
        }
    }
    s
}

/// The settings a run played under, as `key=value` words.
fn settings(p: &Plan) -> String {
    format!(
        "difficulty={} size={}x{} walk={} turn={} turn_every={} throw_every={} pick={} unlocked={} mortal={} secs={} step={:.5}",
        p.difficulty.key(),
        p.size[0],
        p.size[1],
        p.walk.name(),
        p.turn,
        p.turn_every,
        p.throw_every,
        p.pick.name(),
        p.unlocked,
        p.mortal,
        p.until.unwrap_or(600.0),
        p.step
    )
}

const CSV_HEAD: &str = "build,weapon,level,seeds,secs,kills,kills_sd,kills_per_min,useful,raw,overkill,dps,raw_dps,hearts,level_reached,fell_nights,fell_at,dawn_nights,spawned,capped,ceiling,kills_by_min,hearts_by_min,level_by_min,survival,censored";

fn csv(cells: &[Cell], plan: &Plan) -> String {
    let mut s = format!("# last-harvest-sandbox {VERSION} {}\n{CSV_HEAD}\n", settings(plan));
    for c in cells {
        let name = c.name();
        let (fell, at) = c.fell();
        let dawns = c.nights.iter().filter(|x| x.dawn).count();
        let all = c.all();
        let row = |s: &mut String, r: &Row, extra: &str| {
            let _ = writeln!(s, "\"{name}\",{},{},{},{:.1},{:.2},{:.2},{:.2},{:.1},{:.1},{:.4},{:.2},{:.2},{extra}", r.weapon, r.level, c.nights.len(), r.secs, r.kills, r.kills_sd, r.per_min(), r.useful, r.raw, r.overkill(), r.dps, r.raw_dps);
        };
        let extra = format!(
            "{:.1},{:.2},{fell},{},{dawns},{:.1},{:.4},{},{},{},{},{:.1},{}",
            c.hearts(),
            c.level(),
            at.map_or(String::new(), |t| format!("{t:.1}")),
            c.spawned(),
            c.capped(),
            c.at_ceiling(),
            c.by_minute(|m| m.kills),
            c.by_minute(|m| m.hearts),
            c.by_minute(|m| m.level),
            c.survival(),
            c.nights.len() - fell
        );
        row(&mut s, &all, &extra);
        for l in c.lines() {
            row(&mut s, &l, ",,,,,,,,,,,,");
        }
    }
    s
}

fn json(cells: &[Cell], a: &Args) -> String {
    let p = &a.plan;
    let mut s = format!(
        "{{\"version\":\"{VERSION}\",\"difficulty\":\"{}\",\"size\":[{},{}],\"walk\":\"{}\",\"turn\":{},\"turn_every\":{},\"throw_every\":{},\"pick\":\"{}\",\"unlocked\":{},\"mortal\":{},\"secs\":{},\"step\":{},\"builds\":[",
        p.difficulty.key(),
        p.size[0],
        p.size[1],
        p.walk.name(),
        p.turn,
        p.turn_every,
        p.throw_every,
        p.pick.name(),
        p.unlocked,
        p.mortal,
        p.until.unwrap_or(600.0),
        p.step
    );
    for (i, c) in cells.iter().enumerate() {
        let _ = write!(s, "{}{{\"build\":\"{}\",\"nights\":[", if i > 0 { "," } else { "" }, c.name());
        for (j, (n, seed)) in c.nights.iter().zip(a.seed..).enumerate() {
            let minutes: Vec<String> = n.minutes.iter().map(|m| format!("{{\"kills\":{},\"hearts\":{},\"level\":{}}}", m.kills, m.hearts, m.level)).collect();
            let took: Vec<String> = n.took.iter().map(|t| format!("{{\"secs\":{:.2},\"id\":\"{}\",\"rare\":{}}}", t.secs, sandbox::powerup_id(t.up), t.rare)).collect();
            let lines: Vec<String> = n.lines.iter().map(|l| format!("{{\"weapon\":\"{}\",\"level\":{},\"raw\":{:.1},\"useful\":{:.1},\"kills\":{},\"held\":{:.2}}}", l.id, l.level, l.raw, l.useful, l.kills, l.held)).collect();
            let _ = write!(
                s,
                "{}{{\"seed\":{seed},\"secs\":{:.2},\"dawn\":{},\"fell\":{},\"kills\":{},\"hearts\":{},\"level\":{},\"spawned\":{},\"capped\":{:.2},\"build\":\"{}\",\"took\":[{}],\"minutes\":[{}],\"weapons\":[{}]}}",
                if j > 0 { "," } else { "" },
                n.secs,
                n.dawn,
                n.fell.map_or("null".into(), |t| format!("{t:.2}")),
                n.kills,
                n.hearts,
                n.level,
                n.spawned,
                n.capped,
                sandbox::build_text(&n.build),
                took.join(","),
                minutes.join(","),
                lines.join(",")
            );
        }
        s += "]}";
    }
    s += "]}\n";
    s
}

/// A CSV row's build and weapon, and its columns by name.
type CsvRow = ((String, String), Vec<(String, String)>);

/// A CSV the sandbox wrote.
fn read_csv(path: &str) -> Result<Vec<CsvRow>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut lines = text.lines().filter(|l| !l.starts_with('#'));
    let head: Vec<String> = lines.next().ok_or(format!("{path}: empty"))?.split(',').map(String::from).collect();
    Ok(lines
        .filter_map(|line| {
            let (build, rest) = line.strip_prefix('"')?.split_once("\",")?;
            let cols: Vec<(String, String)> = head.iter().skip(1).cloned().zip(rest.split(',').map(String::from)).collect();
            let weapon = cols.first()?.1.clone();
            Some(((build.to_string(), weapon), cols))
        })
        .collect())
}

fn diff(old: &str, new: &str) -> Result<(), String> {
    let (a, b) = (read_csv(old)?, read_csv(new)?);
    let get = |cols: &[(String, String)], k: &str| cols.iter().find(|(c, _)| c == k).and_then(|(_, v)| v.parse::<f32>().ok()).unwrap_or(0.0);
    let change = |x: f32, y: f32| if x != 0.0 { format!("{:+.0}%", (y - x) / x * 100.0) } else if y != 0.0 { "new".into() } else { "".into() };
    println!("{:<28} {:<10} {:>17} {:>6} {:>17} {:>6} {:>13}", "build", "weapon", "kills", "", "dps", "", "level");
    for (key, cols) in &b {
        let Some((_, was)) = a.iter().find(|(k, _)| k == key) else {
            println!("{:<28} {:<10} only in {new}", key.0, key.1);
            continue;
        };
        let (k0, k1, d0, d1) = (get(was, "kills"), get(cols, "kills"), get(was, "dps"), get(cols, "dps"));
        let level = if key.1 == "all" { format!("{:.1} -> {:.1}", get(was, "level_reached"), get(cols, "level_reached")) } else { String::new() };
        println!("{:<28} {:<10} {:>17} {:>6} {:>17} {:>6} {:>13}", key.0, key.1, format!("{k0:.0} -> {k1:.0}"), change(k0, k1), format!("{d0:.1} -> {d1:.1}"), change(d0, d1), level);
    }
    for (key, _) in a.iter().filter(|(k, _)| !b.iter().any(|(o, _)| o == k)) {
        println!("{:<28} {:<10} only in {old}", key.0, key.1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_is_the_sample_deviation_and_none_for_one() {
        assert_eq!(mean([2.0, 4.0, 6.0]), 4.0);
        assert_eq!(spread([2.0, 4.0, 6.0]), 2.0);
        assert_eq!(spread([5.0]), 0.0);
    }
}
