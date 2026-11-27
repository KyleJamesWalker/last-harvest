//! Play stats and crash reports for a metrics backend outside this repository, kept and sent only while the player
//! shares them (`Save::metrics`) and the backend has a sink (`Metrics::collects`).
//!
//! Every event carries `schema` (`SCHEMA`), `version` (the release) and `device` (`<os>-<arch>`), and no name or id
//! of the player or the device. The events:
//!
//! | Event | Sent | Fields |
//! |---|---|---|
//! | `install` | once, when the player first shares | the common ones |
//! | `playtime` | each `PLAYTIME_CHUNK` seconds the game runs | `secs` |
//! | `run_end` | as a run ends | `difficulty`, `won`, `secs`, `kills`, `level`, `dps`, `broke`, `build` (`id:level` with spaces between) |
//! | `build_pick` | as a run ends, one per powerup it ended with | `powerup`, `level`, `difficulty`, `won`, `secs` |
//!
//! `won` and `broke` are 1 or 0. Events wait in an outbox, `metrics.txt` in the store folder, so they outlast a
//! night offline and a power-off; the backend gets them a batch at a time, and each batch stays until the backend
//! answers its id. A panic writes a crash report into `crashes/` for the next start to send. Withdrawing consent
//! deletes the outbox and every crash report not yet sent.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use last_harvest_game::{Ask, Consent, Run};
use last_harvest_platform::{Event, Field, Metrics};

pub const SCHEMA: f64 = 1.0;
/// Most events in one batch.
pub const BATCH: usize = 50;
/// Events the outbox keeps; past that the oldest go.
pub const MAX_EVENTS: usize = 2000;
/// Crash reports the outbox keeps; past that the oldest go.
pub const MAX_CRASHES: usize = 4;
/// Longest crash report, in bytes; the rest is cut.
pub const CRASH_MAX: usize = 16 * 1024;
pub const PLAYTIME_CHUNK: f32 = 300.0;
/// Seconds a smaller batch waits for more events before it goes.
pub const FLUSH_EVERY: f32 = 60.0;
/// Seconds before a batch the backend hasn't answered is sent again.
pub const RETRY: f32 = 60.0;

const OUTBOX: &str = "metrics.txt";
const CRASHES: &str = "crashes";

/// Whether a panic may write a crash report; the panic hook can't reach the save.
static SHARING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug, PartialEq)]
enum Item {
    Event(Event),
    Crash(String),
}

/// A batch handed to the backend and not yet answered: its id, how many items from the front it holds, and its age.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Flight {
    id: u64,
    items: usize,
    age: f32,
}

/// The outbox, and the one gate between the game and the metrics backend.
pub struct Telemetry {
    store: Option<PathBuf>,
    items: VecDeque<Item>,
    next_id: u64,
    flight: Option<Flight>,
    since_flush: f32,
    installed: bool,
    playtime: f32,
    loaded: bool,
    dirty: bool,
}

impl Telemetry {
    /// An outbox kept in `store`, or in memory alone for none.
    pub fn new(store: Option<PathBuf>) -> Self {
        Self {
            store,
            items: VecDeque::new(),
            next_id: 1,
            flight: None,
            since_flush: 0.0,
            installed: false,
            playtime: 0.0,
            loaded: false,
            dirty: false,
        }
    }

    /// Once a frame: keeps what this frame's asks and `dt` seconds of play add, if the player shares, then hands the
    /// backend its next batch and drops what it answered.
    pub fn frame(&mut self, consent: Consent, asks: &[Ask], backend: &mut dyn Metrics, dt: f32) {
        let sharing = consent == Consent::Yes && backend.collects();
        SHARING.store(sharing, Ordering::Relaxed);
        if !sharing {
            if consent == Consent::No {
                self.purge();
            }
            return;
        }
        if !self.loaded {
            self.load();
        }
        let mut checkpoint = false;
        if !self.installed {
            self.installed = true;
            self.push(Item::Event(install()));
            checkpoint = true;
        }
        for ask in asks {
            if let Ask::RunEnded(run) = ask {
                self.push(Item::Event(run_end(run)));
                for e in build_picks(run) {
                    self.push(Item::Event(e));
                }
                checkpoint = true;
            }
        }
        self.playtime += dt;
        if self.playtime >= PLAYTIME_CHUNK {
            self.playtime -= PLAYTIME_CHUNK;
            self.push(Item::Event(playtime(PLAYTIME_CHUNK)));
            checkpoint = true;
        }
        checkpoint |= self.deliver(backend, dt);
        if checkpoint {
            self.write();
        }
    }

    /// Keeps the outbox and the play time not yet sent, at a clean exit.
    pub fn exiting(&mut self) {
        if self.loaded {
            self.dirty = true;
            self.write();
        }
    }

    /// Takes the backend's answers, sends a batch again that went unanswered, and sends the next; true once an
    /// answer took items out.
    fn deliver(&mut self, backend: &mut dyn Metrics, dt: f32) -> bool {
        let mut done = false;
        while let Some(id) = backend.poll_sent() {
            if let Some(f) = self.flight.filter(|f| f.id == id) {
                self.items.drain(..f.items);
                self.flight = None;
                self.dirty = true;
                done = true;
            }
        }
        self.since_flush += dt;
        match &mut self.flight {
            Some(f) => {
                f.age += dt;
                if f.age >= RETRY {
                    f.age = 0.0;
                    let f = *f;
                    self.send(backend, f.id, f.items);
                }
            }
            None => {
                let events = self
                    .items
                    .iter()
                    .take_while(|i| matches!(i, Item::Event(_)))
                    .take(BATCH)
                    .count();
                let due = match self.items.front() {
                    Some(Item::Crash(_)) => Some(1),
                    Some(_) if events == BATCH || self.since_flush >= FLUSH_EVERY => Some(events),
                    _ => None,
                };
                if let Some(n) = due {
                    let id = self.next_id;
                    self.next_id += 1;
                    self.dirty = true;
                    self.flight = Some(Flight {
                        id,
                        items: n,
                        age: 0.0,
                    });
                    self.send(backend, id, n);
                    self.since_flush = 0.0;
                }
            }
        }
        done
    }

    fn send(&self, backend: &mut dyn Metrics, id: u64, n: usize) {
        match self.items.front() {
            Some(Item::Crash(report)) => backend.send_crash(id, report),
            Some(Item::Event(_)) => {
                let events: Vec<Event> = self
                    .items
                    .iter()
                    .take(n)
                    .filter_map(|i| match i {
                        Item::Event(e) => Some(e.clone()),
                        Item::Crash(_) => None,
                    })
                    .collect();
                backend.send(id, &events);
            }
            None => {}
        }
    }

    /// Adds an item, dropping the oldest of its kind past the cap; a batch in flight that loses one is sent anew.
    fn push(&mut self, item: Item) {
        let crash = matches!(item, Item::Crash(_));
        self.items.push_back(item);
        let (cap, kind) = if crash {
            (MAX_CRASHES, true)
        } else {
            (MAX_EVENTS, false)
        };
        let count = |items: &VecDeque<Item>| {
            items
                .iter()
                .filter(|i| matches!(i, Item::Crash(_)) == kind)
                .count()
        };
        while count(&self.items) > cap {
            let at = self
                .items
                .iter()
                .position(|i| matches!(i, Item::Crash(_)) == kind)
                .expect("over the cap, so one is there");
            if self.flight.is_some_and(|f| at < f.items) {
                self.flight = None;
            }
            self.items.remove(at);
        }
        self.dirty = true;
    }

    /// Reads the outbox and the crash reports a panic left, which then live in the outbox alone.
    fn load(&mut self) {
        self.loaded = true;
        let Some(dir) = self.store.clone() else {
            return;
        };
        if let Ok(text) = fs::read_to_string(dir.join(OUTBOX)) {
            self.read_text(&text);
        }
        let staged = staged_crashes(&dir.join(CRASHES));
        for (_, report) in &staged {
            self.push(Item::Crash(report.clone()));
        }
        if !staged.is_empty() {
            self.dirty = true;
            if self.write() {
                for (path, _) in staged {
                    fs::remove_file(path).ok();
                }
            }
        }
    }

    /// Writes the outbox if it changed; true once it is on disk.
    fn write(&mut self) -> bool {
        if !self.dirty {
            return true;
        }
        let Some(dir) = &self.store else {
            self.dirty = false;
            return true;
        };
        let path = dir.join(OUTBOX);
        let tmp = path.with_extension("tmp");
        match fs::write(&tmp, self.to_text()).and_then(|()| fs::rename(&tmp, &path)) {
            Ok(()) => {
                self.dirty = false;
                true
            }
            Err(e) => {
                crate::log!("metrics: {}: {e}", path.display());
                false
            }
        }
    }

    /// Forgets everything kept and deletes it from disk.
    fn purge(&mut self) {
        if let Some(dir) = &self.store
            && (self.loaded || dir.join(OUTBOX).exists() || dir.join(CRASHES).exists())
        {
            fs::remove_file(dir.join(OUTBOX)).ok();
            fs::remove_dir_all(dir.join(CRASHES)).ok();
        }
        *self = Self::new(self.store.take());
    }

    fn to_text(&self) -> String {
        let mut text = format!(
            "schema={SCHEMA}\ninstalled={}\nnext={}\nplaytime={:.0}\n",
            if self.installed { "yes" } else { "no" },
            self.next_id,
            self.playtime
        );
        for item in &self.items {
            match item {
                Item::Event(e) => {
                    text += "event=";
                    text += &escape(&e.name);
                    for (key, field) in &e.fields {
                        text += "\t";
                        text += &escape(key);
                        text += &match field {
                            Field::Text(t) => format!("=t:{}", escape(t)),
                            Field::Number(n) => format!("=n:{n}"),
                        };
                    }
                    text += "\n";
                }
                Item::Crash(r) => text += &format!("crash={}\n", escape(r)),
            }
        }
        text
    }

    /// Lines that won't read are skipped.
    fn read_text(&mut self, text: &str) {
        for (key, value) in text.lines().filter_map(|l| l.split_once('=')) {
            match key {
                "installed" => self.installed = value == "yes",
                "next" => self.next_id = value.parse().unwrap_or(self.next_id),
                "playtime" => {
                    self.playtime = value
                        .parse::<f32>()
                        .unwrap_or(0.0)
                        .clamp(0.0, PLAYTIME_CHUNK)
                }
                "event" => {
                    if let Some(e) = event_from_text(value) {
                        self.push(Item::Event(e));
                    }
                }
                "crash" => self.push(Item::Crash(unescape(value))),
                _ => {}
            }
        }
        self.dirty = false;
    }

    #[cfg(test)]
    fn kept(&self) -> usize {
        self.items.len()
    }
}

fn event_from_text(text: &str) -> Option<Event> {
    let mut parts = text.split('\t');
    let name = unescape(parts.next().filter(|n| !n.is_empty())?);
    let fields = parts
        .map(|p| {
            let (key, value) = p.split_once('=')?;
            let field = match value.split_once(':')? {
                ("t", t) => Field::Text(unescape(t)),
                ("n", n) => Field::Number(n.parse().ok()?),
                _ => return None,
            };
            Some((unescape(key), field))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Event { name, fields })
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out += "\\\\",
            '\n' => out += "\\n",
            '\t' => out += "\\t",
            '\r' => out += "\\r",
            _ => out.push(ch),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn device() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn event(name: &str, fields: Vec<(&str, Field)>) -> Event {
    let mut all = vec![
        ("schema".to_string(), Field::Number(SCHEMA)),
        ("version".to_string(), Field::Text(crate::VERSION.into())),
        ("device".to_string(), Field::Text(device())),
    ];
    all.extend(fields.into_iter().map(|(k, f)| (k.to_string(), f)));
    Event {
        name: name.into(),
        fields: all,
    }
}

fn number(n: impl Into<f64>) -> Field {
    Field::Number(n.into())
}

pub fn install() -> Event {
    event("install", vec![])
}

pub fn playtime(secs: f32) -> Event {
    event("playtime", vec![("secs", number(secs.round()))])
}

/// The powerups a run ended with, by id, and each one's level.
fn kit(run: &Run) -> Vec<(&'static str, u8)> {
    run.kit
        .iter()
        .flatten()
        .filter_map(|&(def, level)| Some((last_harvest_game::powerup_id(def as usize)?, level)))
        .collect()
}

pub fn run_end(run: &Run) -> Event {
    let build: Vec<String> = kit(run)
        .into_iter()
        .map(|(id, level)| format!("{id}:{level}"))
        .collect();
    event(
        "run_end",
        vec![
            ("difficulty", Field::Text(run.difficulty.key().into())),
            ("won", number(u8::from(run.won))),
            ("secs", number(run.secs)),
            ("kills", number(run.kills)),
            ("level", number(run.level)),
            ("dps", number(run.dps)),
            ("broke", number(u8::from(run.broke))),
            ("build", Field::Text(build.join(" "))),
        ],
    )
}

pub fn build_picks(run: &Run) -> Vec<Event> {
    kit(run)
        .into_iter()
        .map(|(id, level)| {
            event(
                "build_pick",
                vec![
                    ("powerup", Field::Text(id.into())),
                    ("level", number(level)),
                    ("difficulty", Field::Text(run.difficulty.key().into())),
                    ("won", number(u8::from(run.won))),
                    ("secs", number(run.secs)),
                ],
            )
        })
        .collect()
}

/// A crash report: the release, the device, the panic and a backtrace, cut to `CRASH_MAX` bytes.
pub fn crash_report(panic: &str, backtrace: &str) -> String {
    let mut report = format!(
        "version={}\ndevice={}\n{panic}\n{backtrace}\n",
        crate::VERSION,
        device()
    );
    if report.len() > CRASH_MAX {
        let mut cut = CRASH_MAX;
        while !report.is_char_boundary(cut) {
            cut -= 1;
        }
        report.truncate(cut);
    }
    report
}

/// Writes `report` into `crashes/` under `store` for the next start to send.
pub fn stage_crash(store: &Path, report: &str, unix: u64) -> std::io::Result<PathBuf> {
    let dir = store.join(CRASHES);
    fs::create_dir_all(&dir)?;
    let path = (0..100)
        .map(|n| dir.join(format!("crash-{unix}-{n:02}.txt")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("crash-{unix}-99.txt")));
    fs::write(&path, report)?;
    Ok(path)
}

/// The crash reports waiting in `dir`, oldest first.
fn staged_crashes(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "txt"))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|p| Some((p.clone(), fs::read_to_string(&p).ok()?)))
        .collect()
}

/// Chains a panic hook over the current one that, while the player shares, stages a crash report under `store`.
/// Only the first call installs it, as Android may start the game again in one process.
#[cfg(not(target_arch = "wasm32"))]
pub fn install_crash_hook(store: PathBuf) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let before = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if SHARING.load(Ordering::Relaxed) {
                let report = crash_report(
                    &info.to_string(),
                    &std::backtrace::Backtrace::force_capture().to_string(),
                );
                stage_crash(&store, &report, crate::log::unix_now()).ok();
            }
            before(info);
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use last_harvest_platform::{Call, NoOp, Recorder};

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lh-metrics-{name}-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ping(n: u32) -> Event {
        event("ping", vec![("n", number(n))])
    }

    #[test]
    fn nothing_is_kept_or_sent_until_the_player_shares() {
        let dir = temp("unasked");
        for consent in [Consent::Unasked, Consent::No] {
            let seen = Recorder::new();
            let mut backend = seen.clone();
            let mut t = Telemetry::new(Some(dir.clone()));
            for _ in 0..100 {
                t.frame(consent, &[], &mut backend, PLAYTIME_CHUNK);
            }
            t.exiting();
            assert!(seen.calls().is_empty(), "{consent:?}");
            assert_eq!(t.kept(), 0);
        }
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0, "no file");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_backend_with_no_sink_gets_nothing_and_nothing_grows() {
        let dir = temp("noop");
        let mut t = Telemetry::new(Some(dir.clone()));
        for _ in 0..100 {
            t.frame(Consent::Yes, &[], &mut NoOp, PLAYTIME_CHUNK);
        }
        t.exiting();
        assert_eq!(t.kept(), 0);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn events_kept_offline_outlast_a_restart_and_go_once_in_order_when_online() {
        let dir = temp("offline");
        let seen = Recorder::new();
        let mut backend = seen.clone();
        seen.set_online(false);
        let mut t = Telemetry::new(Some(dir.clone()));
        t.frame(Consent::Yes, &[], &mut backend, 1.0);
        for n in 0..(BATCH as u32 + 10) {
            t.push(Item::Event(ping(n)));
        }
        for _ in 0..10 {
            t.frame(Consent::Yes, &[], &mut backend, RETRY);
        }
        assert!(seen.delivered().is_empty());
        t.exiting();
        drop(t);

        seen.set_online(true);
        let mut t = Telemetry::new(Some(dir.clone()));
        for _ in 0..4 {
            t.frame(Consent::Yes, &[], &mut backend, FLUSH_EVERY);
        }
        let names: Vec<String> = seen.delivered().iter().map(|e| e.name.clone()).collect();
        assert_eq!(names[0], "install", "installed once, before the rest");
        assert_eq!(names.iter().filter(|n| *n == "install").count(), 1);
        let pings: Vec<Field> = seen
            .delivered()
            .iter()
            .filter(|e| e.name == "ping")
            .map(|e| e.field("n").unwrap().clone())
            .collect();
        assert_eq!(
            pings,
            (0..(BATCH as u32 + 10)).map(number).collect::<Vec<_>>(),
            "each once, in order"
        );
        assert!(
            seen.calls()
                .iter()
                .all(|c| !matches!(c, Call::Send(_, e) if e.len() > BATCH)),
            "batched"
        );
        assert_eq!(t.kept(), 0);
        let mut again = Telemetry::new(Some(dir.clone()));
        again.load();
        assert_eq!(again.kept(), 0, "the delivered are gone from disk too");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn play_time_goes_in_chunks_with_the_common_fields() {
        let seen = Recorder::new();
        let mut backend = seen.clone();
        let mut t = Telemetry::new(None);
        t.frame(Consent::Yes, &[], &mut backend, PLAYTIME_CHUNK - 1.0);
        t.frame(Consent::Yes, &[], &mut backend, 2.0);
        t.frame(Consent::Yes, &[], &mut backend, FLUSH_EVERY);
        let got = seen.delivered();
        let names: Vec<&str> = got.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["install", "playtime"]);
        assert_eq!(got[1].field("secs"), Some(&number(PLAYTIME_CHUNK)));
        for e in &got {
            assert_eq!(e.field("schema"), Some(&number(SCHEMA)));
            assert!(e.field("version").is_some() && e.field("device").is_some());
        }
    }

    #[test]
    fn the_outbox_drops_its_oldest_past_the_cap() {
        let mut t = Telemetry::new(None);
        for n in 0..(MAX_EVENTS as u32 + 5) {
            t.push(Item::Event(ping(n)));
        }
        for n in 0..(MAX_CRASHES + 2) {
            t.push(Item::Crash(format!("crash {n}")));
        }
        assert_eq!(t.kept(), MAX_EVENTS + MAX_CRASHES);
        assert_eq!(t.items.front(), Some(&Item::Event(ping(5))));
        assert!(t.items.contains(&Item::Crash("crash 2".into())));
        assert!(!t.items.contains(&Item::Crash("crash 1".into())));
    }

    #[test]
    fn withdrawing_consent_deletes_the_outbox_and_staged_crashes() {
        let dir = temp("purge");
        let seen = Recorder::new();
        seen.set_online(false);
        let mut backend = seen.clone();
        let mut t = Telemetry::new(Some(dir.clone()));
        t.frame(Consent::Yes, &[], &mut backend, PLAYTIME_CHUNK);
        stage_crash(&dir, "boom", 1).unwrap();
        assert!(dir.join(OUTBOX).exists());
        t.frame(Consent::No, &[], &mut backend, 1.0);
        assert_eq!(t.kept(), 0);
        assert!(!dir.join(OUTBOX).exists() && !dir.join(CRASHES).exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_staged_crash_goes_alone_on_the_next_start_and_is_then_deleted() {
        let dir = temp("crash");
        let report = crash_report("panicked at src/x.rs:1:1:\nboom", "0: main\n\tat x");
        assert!(report.contains("boom") && report.contains("version="));
        let staged = stage_crash(&dir, &report, 1795237918).unwrap();
        let seen = Recorder::new();
        let mut backend = seen.clone();
        let mut t = Telemetry::new(Some(dir.clone()));
        t.frame(Consent::Yes, &[], &mut backend, 0.0);
        assert!(!staged.exists(), "moved into the outbox");
        for _ in 0..3 {
            t.frame(Consent::Yes, &[], &mut backend, FLUSH_EVERY);
        }
        assert_eq!(seen.crashes(), [report]);
        assert!(
            seen.calls()
                .iter()
                .any(|c| matches!(c, Call::SendCrash(..)))
        );
        assert_eq!(t.kept(), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_long_crash_report_is_cut_on_a_character() {
        let report = crash_report(&"é".repeat(CRASH_MAX), "");
        assert!(report.len() <= CRASH_MAX && report.len() > CRASH_MAX - 4);
    }

    #[test]
    fn the_outbox_text_round_trips_tabs_newlines_and_numbers() {
        let mut t = Telemetry::new(None);
        t.installed = true;
        t.next_id = 42;
        t.push(Item::Event(Event {
            name: "odd\tname".into(),
            fields: vec![
                ("a b".into(), Field::Text("x\\y\nz:\tw=v".into())),
                ("n".into(), Field::Number(-1.5)),
            ],
        }));
        t.push(Item::Crash("line 1\nline\t2\\".into()));
        let mut back = Telemetry::new(None);
        back.read_text(&t.to_text());
        assert_eq!(back.items, t.items);
        assert_eq!((back.installed, back.next_id), (true, 42));
        back.read_text("event=\nevent=x\tbad\ncrash\njunk=1\n");
        assert_eq!(back.kept(), 2, "bad lines are skipped");
    }
}
