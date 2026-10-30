//! Earlier versions of a sprite, read from the git repository its folder lives in. Nothing here writes to git.

#[derive(Clone, Debug, PartialEq)]
pub struct Commit {
    pub id: String,
    pub date: String,
    pub subject: String,
}

/// Separates a commit's fields in `git log` output and on the wire.
pub const SEP: char = '\u{1f}';

impl Commit {
    /// One commit per line, its fields joined by `SEP`.
    pub fn parse(text: &str) -> Vec<Commit> {
        text.lines()
            .filter_map(|l| {
                let mut f = l.splitn(3, SEP);
                Some(Commit { id: f.next()?.into(), date: f.next()?.into(), subject: f.next().unwrap_or("").into() })
            })
            .collect()
    }

    pub fn line(&self) -> String {
        format!("{}{SEP}{}{SEP}{}", self.id, self.date, self.subject.replace(['\n', SEP], " "))
    }
}

/// A commit id as `git log --format=%h` prints it, safe to hand back to git.
pub fn valid_id(id: &str) -> bool {
    (4..=40).contains(&id.len()) && id.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(not(target_arch = "wasm32"))]
pub use git::Repo;

#[cfg(not(target_arch = "wasm32"))]
mod git {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use super::Commit;

    /// The sprites folder, when it sits in a git working tree.
    pub struct Repo {
        dir: PathBuf,
    }

    impl Repo {
        pub fn open(dir: &Path) -> Option<Self> {
            let out = git(dir, &["rev-parse", "--is-inside-work-tree"])?;
            (out.trim_ascii() == b"true").then(|| Self { dir: dir.to_path_buf() })
        }

        /// Commits that touched the sprite's folder, newest first.
        pub fn log(&self, name: &str) -> Vec<Commit> {
            let Some(out) = git(&self.dir, &["log", "--format=%h%x1f%ad%x1f%s", "--date=format:%Y-%m-%d %H:%M", "--", name]) else { return Vec::new() };
            Commit::parse(&String::from_utf8_lossy(&out))
        }

        /// Whether the sprite's files differ from the last commit.
        pub fn changed(&self, name: &str) -> bool {
            git(&self.dir, &["status", "--porcelain", "--", name]).is_some_and(|out| !out.is_empty())
        }

        /// A file of the sprite's folder as it was at commit `id`, if it existed then.
        pub fn read(&self, id: &str, name: &str, file: &str) -> Option<Vec<u8>> {
            self.show(id, &format!("{name}/{file}"))
        }

        /// A file under the folder as it was at commit `id`, if it existed then.
        pub fn show(&self, id: &str, path: &str) -> Option<Vec<u8>> {
            git(&self.dir, &["show", &format!("{id}:./{path}")])
        }
    }

    /// Runs git in `dir` and returns what it printed, or None if it failed.
    pub(super) fn git(dir: &Path, args: &[&str]) -> Option<Vec<u8>> {
        let out = Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
        out.status.success().then_some(out.stdout)
    }
}

#[cfg(test)]
pub mod tests {
    use std::path::{Path, PathBuf};

    use super::git::git;
    use super::*;

    /// A throwaway repository with a `sprites` folder, for tests.
    pub fn repo(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("lh-history-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sprites")).unwrap();
        for args in [&["init", "-q"][..], &["config", "user.email", "test@example.com"], &["config", "user.name", "Test"], &["config", "commit.gpgsign", "false"]] {
            git(&root, args).unwrap();
        }
        root
    }

    pub fn commit(root: &Path, message: &str) {
        git(root, &["add", "-A"]).unwrap();
        git(root, &["commit", "-q", "-m", message]).unwrap();
    }

    #[test]
    fn lists_and_reads_a_sprites_versions() {
        let root = repo("log");
        let dir = root.join("sprites");
        std::fs::create_dir_all(dir.join("THING")).unwrap();
        std::fs::write(dir.join("THING/stand.txt"), "slab 4\n").unwrap();
        commit(&root, "first");
        std::fs::write(dir.join("THING/stand.txt"), "slab 9\n").unwrap();
        commit(&root, "second");
        std::fs::write(dir.join("THING/stand.txt"), "figure\n").unwrap();
        let r = Repo::open(&dir).unwrap();
        let log = r.log("THING");
        assert_eq!(log.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["second", "first"]);
        assert_eq!(r.read(&log[1].id, "THING", "stand.txt").as_deref(), Some(&b"slab 4\n"[..]));
        assert_eq!(r.read(&log[1].id, "THING", "east.png"), None);
        assert!(r.changed("THING"));
        assert!(Repo::open(&std::env::temp_dir().join("lh-not-a-repo-anywhere")).is_none());
        assert!(log.iter().all(|c| valid_id(&c.id)));
        assert_eq!(Commit::parse(&log.iter().map(Commit::line).collect::<Vec<_>>().join("\n")), log);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
