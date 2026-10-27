//! Builds `game_data.pak`, the data the game loads, and makes and reads enemy cards.
//!
//!   cargo run -p last-harvest-pack [-- <out.pak>]                     the pack, `target/game_data.pak` by default
//!   cargo run -p last-harvest-pack -- card <sheet.png> <enemy.txt> <name.enemy.png>
//!   cargo run -p last-harvest-pack -- player <sheet.png> <player.txt> <name.player.png>
//!   cargo run -p last-harvest-pack -- show <name.enemy.png | name.player.png>
//!   cargo run -p last-harvest-pack -- mod <folder> <name.pak>       a mod: the folder's files at their paths in it

use std::path::{Path, PathBuf};

use last_harvest_game::art;
use last_harvest_pack::{ATLAS, ENEMIES, FONT, MANIFEST, POWERUPS, SOUNDS, card, manifest, mod_manifest, write};

/// Files by path or name, with their bytes.
type Files = Vec<(String, Vec<u8>)>;

/// The files in `dir` ending in one of `exts`, by name, or none when it doesn't exist.
fn listed(dir: &Path, exts: &[&str]) -> std::io::Result<Files> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(Vec::new()) };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        if exts.iter().any(|ext| name.ends_with(ext)) {
            files.push((name, std::fs::read(&path)?));
        }
    }
    Ok(files)
}

/// Every file in the base pack, by its path inside the pack, sorted so the same inputs give the same bytes.
fn files(root: &Path) -> Result<Files, Box<dyn std::error::Error>> {
    let mut files = vec![
        (MANIFEST.to_string(), manifest().into_bytes()),
        (ATLAS.to_string(), art::pixels().to_vec()),
        (FONT.to_string(), std::fs::read(root.join("assets").join(FONT))?),
        (POWERUPS.to_string(), std::fs::read(root.join("crates/game").join(POWERUPS))?),
        (ENEMIES.to_string(), std::fs::read(root.join("crates/game").join(ENEMIES))?),
    ];
    for (name, bytes) in listed(&root.join("assets/private/sounds"), &[".ogg", ".wav"])? {
        files.push((format!("{SOUNDS}{name}"), bytes));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn decode_sheet(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let file = std::fs::File::open(path).map_err(|e| fail(&e))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|e| fail(&e))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| fail(&e))?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        other => return Err(fail(&format!("unexpected color type {other:?}"))),
    };
    Ok((info.width, info.height, rgba))
}

fn make_card(kind: card::Kind, sheet: &Path, text: &Path, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !out.to_string_lossy().ends_with(kind.extension()) {
        return Err(format!("{}: a card's name ends in {}, so the game finds it", out.display(), kind.extension()).into());
    }
    let (w, h, rgba) = decode_sheet(sheet)?;
    let text = std::fs::read_to_string(text)?;
    if kind == card::Kind::Player {
        last_harvest_game::player_fields(&text)?;
    }
    let bytes = card::write_as(kind, &text, w, h, &rgba)?;
    let name = out.display().to_string();
    let made = card::read_as(kind, &name, &bytes)?;
    std::fs::write(out, bytes)?;
    println!("{name}: {}x{} frames", made.frame[0], made.frame[1]);
    Ok(())
}

/// Every file under `dir`, by its path from there with `/` between folders.
fn tree(dir: &Path, under: &str, out: &mut Files) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        if name.starts_with('.') {
            continue;
        }
        let at = format!("{under}{name}");
        if path.is_dir() {
            tree(&path, &format!("{at}/"), out)?;
        } else {
            out.push((at, std::fs::read(&path)?));
        }
    }
    Ok(())
}

/// Packs a folder as a mod: its files at their paths, with its own manifest.toml or one named after the folder.
fn make_mod(dir: &Path, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut files = Files::new();
    tree(dir, "", &mut files)?;
    if !files.iter().any(|(p, _)| p == MANIFEST) {
        let id = dir.file_name().and_then(|n| n.to_str()).unwrap_or("mod");
        files.push((MANIFEST.to_string(), mod_manifest(id).into_bytes()));
    }
    for (path, bytes) in &files {
        if let Some(kind) = card::Kind::of(path) {
            card::read_as(kind, path, bytes)?;
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    write(std::fs::File::create(out)?, &files)?;
    last_harvest_pack::Pak::open(out)?;
    println!("{}: {} files", out.display(), files.len());
    Ok(())
}

fn show(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let kind = card::Kind::of(&path.to_string_lossy()).unwrap_or(card::Kind::Enemy);
    let card = card::read_as(kind, &path.display().to_string(), &std::fs::read(path)?)?;
    println!("{}x{} frames\n{}", card.frame[0], card.frame[1], card.text.trim_end());
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    match args.iter().map(|a| a.to_str().unwrap_or("")).collect::<Vec<_>>()[..] {
        ["card", _, _, _] => return make_card(card::Kind::Enemy, &args[1], &args[2], &args[3]),
        ["player", _, _, _] => return make_card(card::Kind::Player, &args[1], &args[2], &args[3]),
        ["show", _] => return show(&args[1]),
        ["mod", _, _] => return make_mod(&args[1], &args[2]),
        ["card", ..] | ["player", ..] | ["show", ..] | ["mod", ..] => {
            return Err("usage: card <sheet.png> <enemy.txt> <name.enemy.png> | player <sheet.png> <player.txt> <name.player.png> | show <card> | mod <folder> <name.pak>".into());
        }
        _ => {}
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap();
    let out = args.first().cloned().unwrap_or_else(|| root.join("target/game_data.pak"));
    let files = files(root)?;
    write(std::fs::File::create(&out)?, &files)?;
    let size = std::fs::metadata(&out)?.len();
    println!("{}: {} files, {:.1} MB", out.display(), files.len(), size as f64 / 1e6);
    Ok(())
}
