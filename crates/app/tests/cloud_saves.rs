//! The save syncs through the platform's cloud-saves hook: fetched at launch, merged when it arrives, stored after.

use last_harvest::cloud::{CloudSave, NAME};
use last_harvest::platform::{Call, CloudFile, CloudSaves, Recorder};
use last_harvest_game::{Difficulty, Game, Records, Run, Save};

fn run(secs: u32) -> Run {
    Run {
        secs,
        kills: 7,
        level: 3,
        won: false,
        broke: false,
        difficulty: Difficulty::NORMAL,
        dps: 40,
        kit: Default::default(),
    }
}

fn with_records(mut save: Save, d: Difficulty, r: Records) -> Save {
    *save.records_mut(d).unwrap() = r;
    save
}

/// This device's save: its own settings, and some progress on normal and hard.
fn local() -> Save {
    let mut save = Save {
        volume: 2,
        how_to_seen: true,
        mods_off: last_harvest_game::ModsOff::parse("this_device"),
        ..Save::default()
    };
    save = with_records(
        save,
        Difficulty::NORMAL,
        Records {
            kills: 100,
            nights: 5,
            found: 0b01,
            ..Records::default()
        },
    );
    save = with_records(
        save,
        Difficulty::HARD,
        Records {
            best: 300,
            nights: 1,
            ..Records::default()
        },
    );
    save.record(run(120));
    save
}

/// Launches a game on `save`, fetches and polls once, the way a shell does in its first frame.
fn launch(save: Save, cloud: &mut dyn CloudSaves) -> (Game, CloudSave, bool) {
    let mut game = Game::new(1);
    game.load(save);
    let mut sync = CloudSave::default();
    sync.fetch(cloud);
    let arrived = sync.poll(&mut game, cloud);
    (game, sync, arrived)
}

fn stored(seen: &Recorder) -> Vec<Vec<u8>> {
    seen.calls()
        .into_iter()
        .filter_map(|c| match c {
            Call::Store(name, bytes) => {
                assert_eq!(
                    name, NAME,
                    "only the save goes to the cloud; replays stay local"
                );
                Some(bytes)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn with_no_cloud_copy_the_local_save_stays_and_is_stored() {
    let seen = Recorder::new();
    let mut cloud = seen.clone();
    let (game, sync, arrived) = launch(local(), &mut cloud);
    assert!(arrived);
    assert_eq!(game.save(), local());
    assert_eq!(stored(&seen), [local().to_text().into_bytes()]);
    sync.store(b"next write", &mut cloud);
    assert_eq!(
        stored(&seen).len(),
        2,
        "every write after the answer is stored"
    );
}

#[test]
fn a_newer_cloud_copy_brings_its_progress_and_leaves_this_devices_settings() {
    let mut newer = with_records(
        Save {
            volume: 9,
            mods_off: last_harvest_game::ModsOff::parse("other_device"),
            how_to_seen: true,
            ..Save::default()
        },
        Difficulty::NORMAL,
        Records {
            kills: 900,
            nights: 40,
            dawns: 3,
            found: 0b11,
            ..Records::default()
        },
    );
    newer.record(run(600));
    let seen = Recorder::new();
    seen.put_cloud(NAME, newer.to_text().as_bytes());
    let mut cloud = seen.clone();
    let fresh = Save {
        volume: 4,
        ..Save::default()
    };
    let (game, _, arrived) = launch(fresh, &mut cloud);
    assert!(arrived);
    let got = game.save();
    assert_eq!(
        got.records(Difficulty::NORMAL),
        newer.records(Difficulty::NORMAL)
    );
    assert_eq!(got.history, newer.history);
    assert!(got.how_to_seen);
    assert_eq!(got.volume, 4, "settings stay this device's");
    assert_eq!(
        got.mods_off, fresh.mods_off,
        "and so does the mods-off list"
    );
    assert_eq!(seen.cloud(NAME), Some(got.to_text().into_bytes()));
}

#[test]
fn when_each_copy_holds_progress_the_other_lacks_nothing_is_lost() {
    let mut other = with_records(
        Save::default(),
        Difficulty::NORMAL,
        Records {
            kills: 50,
            nights: 9,
            found: 0b10,
            ..Records::default()
        },
    );
    other = with_records(
        other,
        Difficulty::EASY,
        Records {
            dawns: 2,
            nights: 2,
            ..Records::default()
        },
    );
    other.record(run(120));
    other.record(run(480));
    let seen = Recorder::new();
    seen.put_cloud(NAME, other.to_text().as_bytes());
    let mut cloud = seen.clone();
    let mut game = Game::new(1);
    game.load(local());
    let mut sync = CloudSave::default();
    sync.fetch(&mut cloud);
    sync.store(b"written before the answer", &mut cloud);
    assert!(
        stored(&seen).is_empty(),
        "nothing is stored before the cloud answers"
    );
    assert!(sync.poll(&mut game, &mut cloud));

    let got = game.save();
    assert_eq!(
        got.records(Difficulty::NORMAL),
        Records {
            kills: 100,
            nights: 9,
            found: 0b11,
            best_dps: 40,
            ..Records::default()
        }
    );
    assert_eq!(
        got.records(Difficulty::HARD),
        local().records(Difficulty::HARD)
    );
    assert_eq!(
        got.records(Difficulty::EASY),
        other.records(Difficulty::EASY)
    );
    assert_eq!(
        got.history
            .iter()
            .flatten()
            .map(|r| r.secs)
            .collect::<Vec<_>>(),
        [120, 480]
    );
    assert_eq!(got.volume, local().volume);
    assert_eq!(stored(&seen), [got.to_text().into_bytes()]);
    assert!(!sync.poll(&mut game, &mut cloud), "it merges once");
}

/// A backend whose fetch never answers.
struct Silent(Recorder);

impl CloudSaves for Silent {
    fn fetch(&mut self, name: &str) {
        self.0.fetch(name);
    }
    fn store(&mut self, name: &str, bytes: &[u8]) {
        self.0.store(name, bytes);
    }
    fn poll_cloud(&mut self) -> Option<CloudFile> {
        None
    }
}

#[test]
fn a_fetch_that_never_answers_leaves_the_game_as_it_was_and_the_cloud_untouched() {
    let seen = Recorder::new();
    seen.put_cloud(NAME, b"another device's progress");
    let mut cloud = Silent(seen.clone());
    let (mut game, mut sync, arrived) = launch(local(), &mut cloud);
    assert!(!arrived);
    for _ in 0..600 {
        assert!(!sync.poll(&mut game, &mut cloud));
    }
    assert_eq!(game.save(), local(), "the game starts on the local save");
    sync.store(local().to_text().as_bytes(), &mut cloud);
    assert_eq!(seen.calls(), [Call::Fetch(NAME.into())]);
    assert_eq!(
        seen.cloud(NAME).as_deref(),
        Some(&b"another device's progress"[..])
    );
}
