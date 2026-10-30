//! Builds `game_data.pak`, the data the game loads, and makes and reads enemy cards.
//!
//!   cargo run -p last-harvest-pack [-- <out.pak>]                     the pack, `target/game_data.pak` by default
//!   cargo run -p last-harvest-pack -- card <sheet.png> <enemy.txt> <name.enemy.png>
//!   cargo run -p last-harvest-pack -- player <sheet.png> <player.txt> <name.player.png>
//!   cargo run -p last-harvest-pack -- show <name.enemy.png | name.player.png>
//!   cargo run -p last-harvest-pack -- mod <folder> <name.pak>       a mod: the folder's files at their paths in it

use std::path::{Path, PathBuf};

use last_harvest_game::art;
use last_harvest_pack::{ATLAS, ENEMIES, FONT, MANIFEST, POWERUPS, PROPS_TABLE, SOUNDS, card, manifest, mod_manifest, write};

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
        (PROPS_TABLE.to_string(), std::fs::read(root.join("crates/game").join(PROPS_TABLE))?),
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

fn make_prop(sheet: &Path, text: &Path, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let ext = last_harvest_pack::prop::EXTENSION;
    if !out.to_string_lossy().ends_with(ext) {
        return Err(format!("{}: a prop's name ends in {ext}, so the game finds it", out.display()).into());
    }
    let (w, h, rgba) = decode_sheet(sheet)?;
    let bytes = last_harvest_pack::prop::write(&std::fs::read_to_string(text)?, w, h, &rgba)?;
    let name = out.display().to_string();
    let made = last_harvest_pack::prop::read(&name, &bytes)?;
    std::fs::write(out, bytes)?;
    println!("{name}: {}x{} frames", made.frame[0], made.frame[1]);
    Ok(())
}

fn make_card(kind: card::Kind, sheet: &Path, text: &Path, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !out.to_string_lossy().ends_with(kind.extension()) {
        return Err(format!("{}: a card's name ends in {}, so the game finds it", out.display(), kind.extension()).into());
    }
    let (w, h, rgba) = decode_sheet(sheet)?;
    let text = std::fs::read_to_string(text)?;
    match kind {
        card::Kind::Enemy => _ = last_harvest_game::card_fields(&text)?,
        card::Kind::Player => _ = last_harvest_game::player_fields(&text)?,
    }
    // A sheet both five and six frames tall reads as five; a dead row needs one six only divides.
    let rows = if h % 5 == 0 { 5 } else { 6 };
    let bytes = card::write_as(kind, &text, w, h, rows, &rgba)?;
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
    load_props(files.iter().map(|(p, b)| (p.as_str(), b.as_slice())))?;
    for (path, bytes) in &files {
        if let Some(kind) = card::Kind::of(path) {
            card::read_as(kind, path, bytes)?;
        }
        if path.ends_with(last_harvest_pack::prop::EXTENSION) {
            last_harvest_pack::prop::read(path, bytes)?;
        }
        if let Some(file) = map_file(path) {
            last_harvest_game::check_map(file, String::from_utf8(bytes.clone()).map_err(|_| format!("{path}: not text"))?)?;
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    write(std::fs::File::create(out)?, &files)?;
    last_harvest_pack::Pak::open(out)?.check_mod().inspect_err(|_| _ = std::fs::remove_file(out))?;
    println!("{}: {} files", out.display(), files.len());
    Ok(())
}

/// Lists a pack's files and whether the game would take it as a mod, reading each card in it.
/// Reads the props among `files` and loads them so maps that place them can be checked.
fn load_props<'a>(files: impl Iterator<Item = (&'a str, &'a [u8])>) -> Result<(), String> {
    let props = files.filter(|(p, _)| p.ends_with(last_harvest_pack::prop::EXTENSION)).map(|(p, b)| last_harvest_pack::prop::read(p, b)).collect::<Result<Vec<_>, _>>()?;
    match last_harvest_game::load_props_for_checks(&props).into_iter().next() {
        Some(problem) => Err(problem),
        None => Ok(()),
    }
}

fn show_pak(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let pak = last_harvest_pack::Pak::open(path)?;
    if let Err(e) = load_props(pak.paths().into_iter().map(|p| (p, pak.get(p).unwrap_or_default()))) {
        println!("A prop won't load, so maps that place it won't either: {e}\n");
    }
    for file in pak.paths() {
        let size = pak.get(file).map_or(0, <[u8]>::len);
        let bytes = pak.get(file).unwrap_or_default();
        let note = match (card::Kind::of(file), map_file(file)) {
            (Some(kind), _) => card::read_as(kind, file, bytes).map_or_else(|e| format!("  not a card: {e}"), |c| format!("  {}x{} frames", c.frame[0], c.frame[1])),
            (_, Some(name)) => map_note(name, bytes),
            _ if file.ends_with(last_harvest_pack::prop::EXTENSION) => {
                last_harvest_pack::prop::read(file, bytes).map_or_else(|e| format!("  not a prop: {e}"), |p| format!("  prop: {}x{} frames", p.frame[0], p.frame[1]))
            }
            _ => String::new(),
        };
        println!("{size:>9}  {file}{note}");
    }
    match pak.check_mod() {
        Ok(()) => println!("\nThe game takes it as a mod."),
        Err(e) => println!("\nThe game leaves it out as a mod: {e}"),
    }
    Ok(())
}

/// A map's file name, for a path in a pack's `maps/` or a map file anywhere.
fn map_file(path: &str) -> Option<&str> {
    let name = path.strip_prefix(last_harvest_pack::MAPS).unwrap_or(path);
    let name = name.rsplit('/').next().unwrap_or(name);
    name.ends_with(last_harvest_game::MAP_EXTENSION).then_some(name)
}

fn map_note(name: &str, bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes).map_err(|_| "not text".to_string()).and_then(|t| last_harvest_game::check_map(name, t.to_string())) {
        Ok(m) => format!("  map: {}", m.name),
        Err(e) => format!("  not a map: {e}"),
    }
}

fn show(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if path.extension().is_some_and(|e| e == "pak") {
        return show_pak(path);
    }
    let shown = path.to_string_lossy();
    if shown.ends_with(last_harvest_pack::prop::EXTENSION) {
        let p = last_harvest_pack::prop::read(&shown, &std::fs::read(path)?)?;
        println!("{}x{} frames\n{}", p.frame[0], p.frame[1], p.text.trim_end());
        return Ok(());
    }
    if let Some(name) = map_file(&shown) {
        println!("{}", map_note(name, &std::fs::read(path)?).trim_start());
        return Ok(());
    }
    let kind = card::Kind::of(&path.to_string_lossy()).unwrap_or(card::Kind::Enemy);
    let card = card::read_as(kind, &path.display().to_string(), &std::fs::read(path)?)?;
    println!("{}x{} frames\n{}", card.frame[0], card.frame[1], card.text.trim_end());
    match kind {
        card::Kind::Enemy => {
            let f = last_harvest_game::card_fields(&card.text)?;
            println!("\nAbout: {}", if f.about.is_empty() { "none" } else { &f.about });
            show_mix(&card.text)?
        }
        card::Kind::Player => {
            let f = last_harvest_game::player_fields(&card.text)?;
            println!("\nAbout: {}", if f.about.is_empty() { "none" } else { &f.about });
            println!("\nStarts with:");
            for (d, start) in last_harvest_game::Difficulty::ALL.into_iter().zip(&f.start) {
                let weapons = start.clone().unwrap_or_else(|| last_harvest_game::default_start(d));
                let how = if start.is_some() { "" } else { " (the game's own)" };
                println!("  {:<7}{}{how}", d.name(), if weapons.is_empty() { "nothing".to_string() } else { weapons.join(", ") });
            }
        }
    }
    Ok(())
}

/// Prints how a night's spawns split between the game's kinds and this card's, a few times through the night.
fn show_mix(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let minutes = [0.5, 2.5, 5.0, 9.5];
    let mix = last_harvest_game::spawn_mix(&[text], &minutes)?;
    println!("\nSpawn mix{}:", if mix.replaced.is_empty() { String::new() } else { format!(", replacing the game's {}", mix.replaced.join(", ")) });
    print!("  {:<14}", "");
    minutes.iter().for_each(|m| print!("{:>8}", format!("{m} min")));
    println!();
    for (k, (_, name)) in mix.kinds.iter().enumerate() {
        print!("  {:<14}", name.chars().take(14).collect::<String>());
        mix.shares.iter().for_each(|s| print!("{:>7.0}%", s[k] * 100.0));
        let never = mix.shares.iter().all(|s| s[k] <= 0.0);
        println!("{}", if never { "   never spawns" } else { "" });
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    match args.iter().map(|a| a.to_str().unwrap_or("")).collect::<Vec<_>>()[..] {
        ["card", _, _, _] => return make_card(card::Kind::Enemy, &args[1], &args[2], &args[3]),
        ["player", _, _, _] => return make_card(card::Kind::Player, &args[1], &args[2], &args[3]),
        ["show", _] => return show(&args[1]),
        ["mod", _, _] => return make_mod(&args[1], &args[2]),
        ["prop", _, _, _] => return make_prop(&args[1], &args[2], &args[3]),
        ["card", ..] | ["player", ..] | ["prop", ..] | ["show", ..] | ["mod", ..] => {
            return Err("usage: card <sheet.png> <enemy.txt> <name.enemy.png> | player <sheet.png> <player.txt> <name.player.png> | prop <sheet.png> <prop.txt> <name.prop.png> | show <card, prop, map or pak> | mod <folder> <name.pak>".into());
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
