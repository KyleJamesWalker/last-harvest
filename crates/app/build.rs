//! Sets `LAST_HARVEST_BUILD`, the version the binary reports: `LAST_HARVEST_VERSION` from a release build, else
//! `dev` and the commit it was built from.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !text.is_empty()).then_some(text)
}

fn main() {
    println!("cargo:rerun-if-env-changed=LAST_HARVEST_VERSION");
    let version = match std::env::var("LAST_HARVEST_VERSION").ok().filter(|v| !v.is_empty()) {
        Some(v) => v,
        None => {
            for file in ["HEAD", "packed-refs"].into_iter().map(String::from).chain(git(&["symbolic-ref", "-q", "HEAD"])) {
                // A path that doesn't exist would rerun this on every build.
                if let Some(path) = git(&["rev-parse", "--git-path", &file]).filter(|p| std::path::Path::new(p).exists()) {
                    println!("cargo:rerun-if-changed={path}");
                }
            }
            git(&["rev-parse", "--short", "HEAD"]).map_or_else(|| "dev".to_string(), |sha| format!("dev-{sha}"))
        }
    };
    println!("cargo:rustc-env=LAST_HARVEST_BUILD={version}");
}
