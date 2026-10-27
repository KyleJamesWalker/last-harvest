//! Builds `game_data.pak`, the data the game loads, and makes and reads enemy cards.
//!
//!   cargo run -p last-harvest-pack [-- <out.pak>]                     the pack, `target/game_data.pak` by default
//!   cargo run -p last-harvest-pack -- card <sheet.png> <enemy.txt> <card.png>
//!   cargo run -p last-harvest-pack -- show <card.png>

use std::path::{Path, PathBuf};

use last_harvest_game::art;
use last_harvest_pack::{ATLAS, ENEMIES, ENEMY_CARDS, FONT, MANIFEST, POWERUPS, SOUNDS, card, manifest, write};

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
    for (name, bytes) in listed(&root.join("assets/private/enemies"), &[".png"])? {
        card::read(&name, &bytes)?;
        files.push((format!("{ENEMY_CARDS}{name}"), bytes));
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

fn make_card(sheet: &Path, text: &Path, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let (w, h, rgba) = decode_sheet(sheet)?;
    let text = std::fs::read_to_string(text)?;
    let bytes = card::write(&text, w, h, &rgba)?;
    let name = out.display().to_string();
    let made = card::read(&name, &bytes)?;
    std::fs::write(out, bytes)?;
    println!("{name}: {}x{} frames", made.frame[0], made.frame[1]);
    Ok(())
}

fn show(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let card = card::read(&path.display().to_string(), &std::fs::read(path)?)?;
    println!("{}x{} frames\n{}", card.frame[0], card.frame[1], card.text.trim_end());
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    match args.iter().map(|a| a.to_str().unwrap_or("")).collect::<Vec<_>>()[..] {
        ["card", _, _, _] => return make_card(&args[1], &args[2], &args[3]),
        ["show", _] => return show(&args[1]),
        ["card", ..] | ["show", ..] => return Err("usage: card <sheet.png> <enemy.txt> <card.png> | show <card.png>".into()),
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
