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
    windows_icon();
}

/// Builds the icon into a Windows binary; without the resource compiler, it goes without one.
fn windows_icon() {
    const ICO: &str = "../../assets/private/icons/game.ico";
    println!("cargo:rerun-if-changed={ICO}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon(ICO);
    if let Err(e) = res.compile() {
        println!("cargo:warning=no icon in the Windows binary: {e}");
    }
}
