//! The hooks a store or console plugs into. Each hook is a trait whose methods all default to doing nothing, so a
//! backend overrides only what its platform offers, and `Platform` is the one handle the app holds. `NoOp` is the
//! public build's backend; `Recorder` keeps every call for tests to check. Nothing here calls a vendor's SDK: those
//! backends live outside this repository.
//!
//! No call blocks. Anything a platform answers later, such as a cloud file, is asked for and then polled.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

/// Achievements by the id `achievements.txt` gives them.
pub trait Achievements {
    /// Unlocks `id`. The game asks again for ones already unlocked, so a backend must take a repeat as a no-op.
    fn unlock(&mut self, _id: &str) {}
}

/// Files kept in the platform's cloud, by name.
pub trait CloudSaves {
    /// Starts fetching `name`; the answer comes from `poll_cloud`.
    fn fetch(&mut self, _name: &str) {}
    /// Replaces `name` in the cloud with `bytes`.
    fn store(&mut self, _name: &str, _bytes: &[u8]) {}
    /// The next fetch that has finished, if any.
    fn poll_cloud(&mut self) -> Option<CloudFile> {
        None
    }
}

/// A fetched cloud file: its bytes, or none when the cloud has no file by that name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudFile {
    pub name: String,
    pub bytes: Option<Vec<u8>>,
}

/// A sink for metrics events. Consent, batching and the offline queue are the caller's.
pub trait Metrics {
    fn send(&mut self, _event: &Event) {}
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub name: String,
    pub fields: Vec<(String, Field)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Text(String),
    Number(f64),
}

/// The platform's own overlay and store page.
pub trait StoreOverlay {
    /// Opens the game's store page in the overlay; false where the platform has none.
    fn open_store(&mut self) -> bool {
        false
    }
    /// The overlay is covering the game.
    fn overlay_open(&self) -> bool {
        false
    }
}

/// The game's buttons, as `Glyphs` names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Button {
    A,
    B,
    X,
    Y,
    L1,
    R1,
    L2,
    R2,
    Select,
    Start,
}

/// What the platform's input layer calls each game button.
pub trait InputGlyphs {
    /// The label to show for `button`, or none to keep the game's own.
    fn glyph(&self, _button: Button) -> Option<String> {
        None
    }
}

/// Every hook, behind the one handle the app holds.
pub trait Platform: Achievements + CloudSaves + Metrics + StoreOverlay + InputGlyphs {
    /// Called once a frame, for platforms that pump callbacks.
    fn tick(&mut self) {}
}

/// The public build's backend: every hook does nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoOp;

impl Achievements for NoOp {}
impl CloudSaves for NoOp {}
impl Metrics for NoOp {}
impl StoreOverlay for NoOp {}
impl InputGlyphs for NoOp {}
impl Platform for NoOp {}

/// One call a `Recorder` saw.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    Unlock(String),
    Fetch(String),
    Store(String, Vec<u8>),
    Send(Event),
    OpenStore,
}

#[derive(Default)]
struct Log {
    calls: Vec<Call>,
    ticks: u64,
    cloud: BTreeMap<String, Vec<u8>>,
    fetched: VecDeque<CloudFile>,
    glyphs: BTreeMap<Button, String>,
    overlay: bool,
}

/// A backend for tests: it keeps every call, and answers from what the test set up. Clones share one log, so a test
/// keeps a clone and hands the other to the code under test.
#[derive(Clone, Default)]
pub struct Recorder {
    log: Rc<RefCell<Log>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every call so far, in order.
    pub fn calls(&self) -> Vec<Call> {
        self.log.borrow().calls.clone()
    }

    /// Each achievement unlocked, once, sorted.
    pub fn unlocked(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .log
            .borrow()
            .calls
            .iter()
            .filter_map(|c| match c {
                Call::Unlock(id) => Some(id.clone()),
                _ => None,
            })
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn ticks(&self) -> u64 {
        self.log.borrow().ticks
    }

    /// What the cloud holds under `name`, from the start or a `store`.
    pub fn cloud(&self, name: &str) -> Option<Vec<u8>> {
        self.log.borrow().cloud.get(name).cloned()
    }

    pub fn put_cloud(&self, name: &str, bytes: &[u8]) {
        self.log
            .borrow_mut()
            .cloud
            .insert(name.into(), bytes.to_vec());
    }

    pub fn set_glyph(&self, button: Button, label: &str) {
        self.log.borrow_mut().glyphs.insert(button, label.into());
    }

    pub fn set_overlay(&self, open: bool) {
        self.log.borrow_mut().overlay = open;
    }

    fn push(&self, call: Call) {
        self.log.borrow_mut().calls.push(call);
    }
}

impl Achievements for Recorder {
    fn unlock(&mut self, id: &str) {
        self.push(Call::Unlock(id.into()));
    }
}

impl CloudSaves for Recorder {
    fn fetch(&mut self, name: &str) {
        self.push(Call::Fetch(name.into()));
        let bytes = self.cloud(name);
        self.log.borrow_mut().fetched.push_back(CloudFile {
            name: name.into(),
            bytes,
        });
    }

    fn store(&mut self, name: &str, bytes: &[u8]) {
        self.push(Call::Store(name.into(), bytes.to_vec()));
        self.put_cloud(name, bytes);
    }

    fn poll_cloud(&mut self) -> Option<CloudFile> {
        self.log.borrow_mut().fetched.pop_front()
    }
}

impl Metrics for Recorder {
    fn send(&mut self, event: &Event) {
        self.push(Call::Send(event.clone()));
    }
}

impl StoreOverlay for Recorder {
    fn open_store(&mut self) -> bool {
        self.push(Call::OpenStore);
        true
    }

    fn overlay_open(&self) -> bool {
        self.log.borrow().overlay
    }
}

impl InputGlyphs for Recorder {
    fn glyph(&self, button: Button) -> Option<String> {
        self.log.borrow().glyphs.get(&button).cloned()
    }
}

impl Platform for Recorder {
    fn tick(&mut self) {
        self.log.borrow_mut().ticks += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exercise(p: &mut dyn Platform) {
        p.tick();
        p.unlock("first_night");
        p.fetch("save.txt");
        p.store("save.txt", b"volume=6\n");
        p.send(&Event {
            name: "run_end".into(),
            fields: vec![("kills".into(), Field::Number(12.0))],
        });
        p.open_store();
    }

    #[test]
    fn the_no_op_backend_answers_nothing() {
        let mut p = NoOp;
        exercise(&mut p);
        assert_eq!(p.poll_cloud(), None);
        assert!(!p.open_store() && !p.overlay_open());
        assert_eq!(p.glyph(Button::A), None);
    }

    #[test]
    fn the_recorder_keeps_every_call_in_order_for_its_clones() {
        let seen = Recorder::new();
        let mut p: Box<dyn Platform> = Box::new(seen.clone());
        exercise(p.as_mut());
        p.unlock("first_night");
        assert_eq!(seen.ticks(), 1);
        assert_eq!(
            seen.calls()[..3],
            [
                Call::Unlock("first_night".into()),
                Call::Fetch("save.txt".into()),
                Call::Store("save.txt".into(), b"volume=6\n".to_vec()),
            ]
        );
        assert!(matches!(seen.calls()[3], Call::Send(ref e) if e.name == "run_end"));
        assert_eq!(seen.calls()[4], Call::OpenStore);
        assert_eq!(seen.unlocked(), ["first_night"], "a repeat unlocks once");
    }

    #[test]
    fn the_recorder_answers_a_fetch_from_its_cloud_on_the_next_poll() {
        let seen = Recorder::new();
        seen.put_cloud("save.txt", b"old");
        let mut p = seen.clone();
        assert_eq!(p.poll_cloud(), None, "nothing until a fetch");
        p.fetch("save.txt");
        p.fetch("other.txt");
        assert_eq!(
            p.poll_cloud(),
            Some(CloudFile {
                name: "save.txt".into(),
                bytes: Some(b"old".to_vec())
            })
        );
        assert_eq!(p.poll_cloud().unwrap().bytes, None);
        p.store("save.txt", b"new");
        assert_eq!(seen.cloud("save.txt").as_deref(), Some(&b"new"[..]));
    }

    #[test]
    fn the_recorder_answers_glyphs_and_the_overlay_as_set() {
        let seen = Recorder::new();
        seen.set_glyph(Button::A, "Cross");
        seen.set_overlay(true);
        let p: Box<dyn Platform> = Box::new(seen.clone());
        assert_eq!(p.glyph(Button::A).as_deref(), Some("Cross"));
        assert_eq!(p.glyph(Button::B), None);
        assert!(p.overlay_open());
    }
}
