//! Save file and crash log, kept next to the binary. `LAST_HARVEST_SAVE` moves the save, and the replays and screenshots with it.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use last_harvest::log;
use last_harvest_game::Save;

/// Batches bursts of changes, such as holding a volume key, into one write.
const SETTLE: Duration = Duration::from_secs(1);

fn home() -> PathBuf {
    last_harvest::home().unwrap_or_else(|| PathBuf::from("."))
}

/// The folder replays and screenshots go in: the save's, else the binary's.
fn dir() -> PathBuf {
    folder_of(std::env::var_os("LAST_HARVEST_SAVE").as_deref().map(Path::new), home())
}

fn folder_of(save: Option<&Path>, home: PathBuf) -> PathBuf {
    match save.and_then(Path::parent) {
        Some(p) if p.as_os_str().is_empty() => PathBuf::from("."),
        Some(p) => p.to_path_buf(),
        None => home,
    }
}

/// The next unused `screenshots/<kind>-NNNN.png` beside the save, creating the folder.
pub fn next_screenshot(kind: &str) -> Option<PathBuf> {
    let shots = dir().join("screenshots");
    if let Err(e) = fs::create_dir_all(&shots) {
        log!("screenshot: {}: {e}", shots.display());
        return None;
    }
    (1..10_000).map(|n| shots.join(format!("{kind}-{n:04}.png"))).find(|p| !p.exists())
}

/// Writes each replay into `replays/` beside the save, keeping the last ten.
pub fn keep_replays(raws: Vec<Vec<u8>>) {
    let dir = dir().join("replays");
    for raw in raws {
        match last_harvest::replays::keep(&dir, &raw, log::unix_now()) {
            Ok(path) => log!("replay: {} ({} bytes before gzip)", path.display(), raw.len()),
            Err(e) => log!("replay: {}: {e}", dir.display()),
        }
    }
}

/// The replays kept beside the save, newest first.
pub fn saved_replays() -> Vec<(String, Vec<u8>)> {
    last_harvest::replays::saved(&dir().join("replays"))
}

pub struct Saver {
    path: PathBuf,
    written: Save,
    changed_at: Option<Instant>,
}

impl Saver {
    pub fn load() -> (Self, Save) {
        let path = std::env::var_os("LAST_HARVEST_SAVE").map_or_else(|| home().join("save.txt"), PathBuf::from);
        let save = read(&path);
        log!("save: {} {save:?}", path.display());
        (Self { path, written: save, changed_at: None }, save)
    }

    /// Writes `save` once it has stopped changing, or right away with `now`.
    pub fn update(&mut self, save: Save, now: bool) {
        if save == self.written {
            self.changed_at = None;
            return;
        }
        let since = *self.changed_at.get_or_insert_with(Instant::now);
        if now || since.elapsed() >= SETTLE {
            match write_atomic(&self.path, save.to_text().as_bytes()) {
                Ok(()) => self.written = save,
                Err(e) => log!("save: {}: {e}", self.path.display()),
            }
            self.changed_at = None;
        }
    }
}

/// The save at `path`, or the defaults; one that won't read is moved aside to `<path>.bad` so the next write keeps it.
fn read(path: &Path) -> Save {
    match fs::read_to_string(path) {
        Ok(text) => Save::parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Save::default(),
        Err(e) => {
            let mut bad = path.as_os_str().to_owned();
            bad.push(".bad");
            let bad = PathBuf::from(bad);
            match fs::rename(path, &bad) {
                Ok(()) => log!("save: {}: {e}; moved it to {} and starting fresh", path.display(), bad.display()),
                Err(moved) => log!("save: {}: {e}; couldn't move it aside: {moved}", path.display()),
            }
            Save::default()
        }
    }
}

/// The SD card is vfat and may lose power at any moment, so write a temp file, sync it, then rename.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut f = File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(&tmp, path)?;
    if let Some(parent) = path.parent() {
        File::open(parent).and_then(|d| d.sync_all()).ok();
    }
    Ok(())
}

/// Appends each panic, with a backtrace, to `crash.log` before the default hook prints it.
pub fn install_crash_log() {
    let path = home().join("crash.log");
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = format!("--- crash at unix {}\n{info}\n{}\n", log::unix_now(), std::backtrace::Backtrace::force_capture());
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
            f.write_all(report.as_bytes()).ok();
            f.sync_all().ok();
        }
        default(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replays_and_screenshots_follow_the_save_the_environment_names() {
        let home = PathBuf::from("/bin/dir");
        assert_eq!(folder_of(None, home.clone()), home);
        assert_eq!(folder_of(Some(Path::new("/tmp/x/save.txt")), home.clone()), Path::new("/tmp/x"));
        assert_eq!(folder_of(Some(Path::new("save.txt")), home), Path::new("."));
    }

    #[test]
    fn a_save_that_wont_read_is_moved_aside_before_anything_overwrites_it() {
        let dir = std::env::temp_dir().join(format!("lh-save-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("save.txt");
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert!(read(&path) == Save::default());
        assert!(!path.exists());
        assert_eq!(fs::read(dir.join("save.txt.bad")).unwrap(), [0xff, 0xfe, 0x00]);
        assert!(read(&dir.join("none.txt")) == Save::default(), "no save is the defaults");
        fs::remove_dir_all(&dir).unwrap();
    }
}
