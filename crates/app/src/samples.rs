//! Recorded sounds that stand in for synth cues: `<cue>.wav`, or `<cue>-2.wav` and so on for variations,
//! in the sounds folder. Any cue without a file keeps its synth voice.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use last_harvest_game::Sfx;

use crate::log;

/// Stereo frames at the output's sample rate.
pub struct Clip {
    pub frames: Vec<[f32; 2]>,
}

/// Sound files by name, like `shot-2.ogg`, with their bytes.
pub type Files = Vec<(String, Vec<u8>)>;

#[derive(Default)]
pub struct Bank {
    clips: HashMap<String, Vec<Arc<Clip>>>,
}

/// Every name `cue` gives, which are the files held in memory.
const CUES: &[&str] = &[
    "start", "miss", "gameover", "turn", "shot", "blast", "slash", "thud", "crit", "boom", "throw",
    "pickup", "heal", "hurt", "heartbeat", "levelup", "pick", "pick-rare", "rise", "groan", "caw", "bell", "horde", "dawn", "zap", "bark",
];

/// The file name a cue looks for, and the playback rate it asks for. Pickups climb a pentatonic scale from one file.
pub fn cue(sfx: Sfx) -> (&'static str, f32) {
    let name = match sfx {
        Sfx::Tick(_) | Sfx::Left | Sfx::Right => return ("", 1.0),
        Sfx::Start => "start",
        Sfx::Miss => "miss",
        Sfx::GameOver => "gameover",
        Sfx::Turn => "turn",
        Sfx::Shot => "shot",
        Sfx::Blast => "blast",
        Sfx::Slash => "slash",
        Sfx::Thud(false) => "thud",
        Sfx::Thud(true) => "crit",
        Sfx::Kill(kind) => last_harvest_game::kill_sound(kind),
        Sfx::Boom { .. } => "boom",
        Sfx::Throw => "throw",
        // Sped up past an octave, a recorded sound turns into chipmunk noise, so a run tops out there.
        Sfx::Pickup(n) => {
            const SCALE: [f32; 6] = [0.0, 2.0, 4.0, 7.0, 9.0, 12.0];
            return ("pickup", 2f32.powf(SCALE[(n as usize).min(SCALE.len() - 1)] / 12.0));
        }
        Sfx::Heal => "heal",
        Sfx::Hurt => "hurt",
        Sfx::Heartbeat => "heartbeat",
        Sfx::LevelUp => "levelup",
        Sfx::Pick(false) => "pick",
        Sfx::Pick(true) => "pick-rare",
        Sfx::Rise { .. } => "rise",
        Sfx::Groan { .. } => "groan",
        Sfx::Caw { .. } => "caw",
        Sfx::Zap => "zap",
        Sfx::Bark => "bark",
        Sfx::Bell => "bell",
        Sfx::Horde => "horde",
        Sfx::Dawn => "dawn",
    };
    (name, 1.0)
}

/// Where the sounds live: `LAST_HARVEST_SOUNDS`, else `sounds` beside the binary, else `assets/private/sounds` for a dev run.
pub fn folder() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LAST_HARVEST_SOUNDS") {
        return Some(dir.into());
    }
    let beside = std::env::current_exe().ok()?.parent()?.join("sounds");
    [beside, PathBuf::from("assets/private/sounds")].into_iter().find(|d| d.is_dir())
}

/// The Ogg Vorbis and WAV files in `dir`.
pub fn read_folder(dir: &Path) -> Files {
    read_files(dir, &[".ogg", ".wav"])
}

/// The files in `dir` ending in one of `exts`, sorted by name; none when it can't be read.
pub fn read_files(dir: &Path, exts: &[&str]) -> Files {
    let Ok(entries) = std::fs::read_dir(dir) else { return Files::new() };
    let mut files: Files = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let read = || std::fs::read(e.path()).inspect_err(|err| log!("{}: {err}", e.path().display())).ok();
            exts.iter().any(|ext| name.ends_with(ext)).then(read).flatten().map(|bytes| (name, bytes))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

impl Bank {
    pub fn load(files: &[(String, Vec<u8>)], rate: f32) -> Self {
        let mut bank = Bank::default();
        for (file, bytes) in files {
            let Some((stem, ext)) = file.rsplit_once('.') else { continue };
            if ext != "wav" && ext != "ogg" {
                continue;
            }
            // "shot-2" is a variation of "shot"; "kill-big" is its own cue.
            let name = match stem.rsplit_once('-') {
                Some((base, n)) if n.parse::<u32>().is_ok() => base,
                _ => stem,
            };
            // Loops like music and ambience are long and played another way; only cues are held in memory.
            if !CUES.contains(&name) && !last_harvest_game::kill_sounds().any(|k| k == name) {
                continue;
            }
            match if ext == "ogg" { decode_ogg(bytes, rate) } else { decode(bytes, rate) } {
                Ok(clip) => bank.clips.entry(name.to_string()).or_default().push(Arc::new(clip)),
                Err(e) => {
                    log!("audio: {file}: {e}");
                    last_harvest_game::data::note(format!("{file}: {e}"));
                }
            }
        }
        let mut names: Vec<&String> = bank.clips.keys().collect();
        names.sort();
        log!("audio: {} recorded cues: {names:?}", names.len());
        bank
    }

    /// One of the cue's clips, chosen by `pick` in 0..1.
    pub fn get(&self, name: &str, pick: f32) -> Option<Arc<Clip>> {
        let clips = self.clips.get(name)?;
        Some(clips[((pick * clips.len() as f32) as usize).min(clips.len() - 1)].clone())
    }
}

/// Reads a PCM (8, 16, 24 or 32-bit) or float WAV, mono or stereo, resampled to `rate`.
pub fn decode(bytes: &[u8], rate: f32) -> Result<Clip, String> {
    let cut = || "a chunk is cut short".to_string();
    let u16_at = |i: usize| bytes.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(cut);
    let u32_at = |i: usize| bytes.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(cut);
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let (mut fmt, mut data) = (None, None);
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let size = u32_at(i + 4)? as usize;
        let body = i + 8..(i + 8).saturating_add(size).min(bytes.len());
        match &bytes[i..i + 4] {
            b"fmt " if size >= 16 => {
                // An extensible header keeps the real format in the first two bytes of its subformat GUID.
                let format = if u16_at(i + 8)? == 0xFFFE && size >= 40 { u16_at(i + 32)? } else { u16_at(i + 8)? };
                fmt = Some((format, u16_at(i + 10)? as usize, u32_at(i + 12)? as f32, u16_at(i + 22)? as usize));
            }
            b"data" => data = Some(body),
            _ => {}
        }
        i = i.saturating_add(8 + size + (size & 1));
    }
    let (format, channels, source_rate, bits) = fmt.ok_or("no fmt chunk")?;
    let data = &bytes[data.ok_or("no data chunk")?];
    let width = bits / 8;
    if channels == 0 || !matches!((format, bits), (1, 8 | 16 | 24 | 32) | (3, 32)) {
        return Err(format!("unsupported format {format} with {bits} bits"));
    }
    let float = format == 3;
    let sample = |at: usize| -> f32 {
        let b = &data[at..at + width];
        match (float, width) {
            (true, _) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            (false, 1) => (b[0] as f32 - 128.0) / 128.0,
            (false, 2) => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
            (false, 3) => i32::from_le_bytes([0, b[0], b[1], b[2]]) as f32 / 2147483648.0,
            (false, _) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2147483648.0,
        }
    };
    let frame = width * channels;
    let source_rate = checked_rate(source_rate)?;
    if (data.len() / frame) as f32 > MAX_SECONDS * source_rate {
        return Err(too_long());
    }
    let source: Vec<[f32; 2]> = (0..data.len() / frame)
        .map(|f| {
            let l = sample(f * frame);
            let r = if channels > 1 { sample(f * frame + width) } else { l };
            [l, r]
        })
        .collect();
    resample(&source, source_rate, rate)
}

/// Reads an Ogg Vorbis file, mono or stereo, resampled to `rate`.
pub fn decode_ogg(bytes: &[u8], rate: f32) -> Result<Clip, String> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let channels = reader.ident_hdr.audio_channels as usize;
    let source_rate = reader.ident_hdr.audio_sample_rate as f32;
    if channels == 0 {
        return Err("no channels".into());
    }
    let source_rate = checked_rate(source_rate)?;
    let mut source = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl().map_err(|e| e.to_string())? {
        source.extend(packet.chunks_exact(channels).map(|f| {
            let s = |v: i16| v as f32 / 32768.0;
            [s(f[0]), s(f[(channels > 1) as usize])]
        }));
        // A packet of a byte or two can decode to thousands of frames, so stop as soon as it's too long.
        if source.len() as f32 > MAX_SECONDS * source_rate {
            return Err(too_long());
        }
    }
    resample(&source, source_rate, rate)
}

// A sound may come from anyone's mod: a made-up rate or length could ask for more frames than there's memory.
const RATES: std::ops::RangeInclusive<f32> = 1000.0..=384_000.0;
const MAX_SECONDS: f32 = 30.0;

fn checked_rate(rate: f32) -> Result<f32, String> {
    if RATES.contains(&rate) { Ok(rate) } else { Err(format!("a rate of {rate} Hz")) }
}

fn too_long() -> String {
    format!("longer than {MAX_SECONDS} seconds, too long for a sound effect")
}

fn resample(source: &[[f32; 2]], source_rate: f32, rate: f32) -> Result<Clip, String> {
    if source.is_empty() {
        return Err("no samples".into());
    }
    let step = source_rate / rate;
    let n = (source.len() as f32 / step) as usize;
    let frames = (0..n)
        .map(|k| {
            let at = k as f32 * step;
            let (i, t) = (at as usize, at.fract());
            let (a, b) = (source[i], source[(i + 1).min(source.len() - 1)]);
            [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
        })
        .collect();
    Ok(Clip { frames })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(channels: u16, rate: u32, samples: &[i16]) -> Vec<u8> {
        let data = (samples.len() * 2) as u32;
        let mut b = Vec::new();
        b.extend(b"RIFF");
        b.extend((36 + data).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(channels.to_le_bytes());
        b.extend(rate.to_le_bytes());
        b.extend((rate * channels as u32 * 2).to_le_bytes());
        b.extend((channels * 2).to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend(data.to_le_bytes());
        for s in samples {
            b.extend(s.to_le_bytes());
        }
        b
    }

    #[test]
    fn a_wav_cut_short_or_with_a_made_up_rate_is_refused() {
        let good = wav(1, 24_000, &[0, 16384, -16384, 0]);
        let fmt = good.windows(4).position(|w| w == b"fmt ").unwrap();
        assert!(decode(&good[..fmt + 14], 48_000.0).err().unwrap().contains("cut short"));
        let mut zero = good.clone();
        zero[fmt + 12..fmt + 16].copy_from_slice(&0u32.to_le_bytes());
        assert!(decode(&zero, 48_000.0).err().unwrap().contains("rate"));
        let mut slow = good;
        slow[fmt + 12..fmt + 16].copy_from_slice(&1000u32.to_le_bytes());
        let long = wav(1, 1000, &vec![0; 40_000]);
        assert!(decode(&long, 48_000.0).err().unwrap().contains("too long"));
        assert!(decode(&slow, 48_000.0).is_ok());
    }

    #[test]
    fn decodes_mono_16_bit_and_resamples() {
        let clip = decode(&wav(1, 24_000, &[0, 16384, -16384, 0]), 48_000.0).unwrap();
        assert_eq!(clip.frames.len(), 8);
        assert_eq!(clip.frames[2], [0.5, 0.5]);
        assert_eq!(clip.frames[1], [0.25, 0.25]);
    }

    #[test]
    fn keeps_stereo_channels_apart() {
        let clip = decode(&wav(2, 48_000, &[16384, -16384]), 48_000.0).unwrap();
        assert_eq!(clip.frames, vec![[0.5, -0.5]]);
    }

    #[test]
    fn rejects_files_that_are_not_wav() {
        assert!(decode(b"OggS....", 48_000.0).is_err());
    }

    #[test]
    fn variations_share_a_cue_and_names_with_dashes_stay_whole() {
        let dir = std::env::temp_dir().join(format!("lh-sounds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["shot.wav", "shot-2.wav", "kill-big.wav", "notes.txt"] {
            std::fs::write(dir.join(name), wav(1, 48_000, &[1000; 4])).unwrap();
        }
        let bank = Bank::load(&read_folder(&dir), 48_000.0);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(bank.clips["shot"].len(), 2);
        assert_eq!(bank.clips["kill-big"].len(), 1);
        assert!(bank.get("kill", 0.0).is_none());
        assert_eq!(cue(Sfx::Kill(0)).0, "kill-big");
    }

    #[test]
    fn every_cue_name_is_one_the_bank_keeps() {
        let all = [
            Sfx::Start, Sfx::Miss, Sfx::GameOver, Sfx::Turn, Sfx::Shot, Sfx::Blast, Sfx::Slash, Sfx::Thud(false), Sfx::Thud(true),
            Sfx::Kill(0), Sfx::Kill(1), Sfx::Kill(2), Sfx::Boom { pan: 0, gain: 0 }, Sfx::Throw, Sfx::Pickup(0), Sfx::Heal, Sfx::Hurt,
            Sfx::Heartbeat, Sfx::LevelUp, Sfx::Pick(false), Sfx::Pick(true), Sfx::Rise { pan: 0, gain: 0 }, Sfx::Groan { pan: 0, gain: 0 },
            Sfx::Caw { pan: 0, gain: 0 }, Sfx::Bell, Sfx::Horde, Sfx::Dawn, Sfx::Zap, Sfx::Bark,
        ];
        for sfx in all {
            let name = cue(sfx).0;
            assert!(CUES.contains(&name) || last_harvest_game::kill_sounds().any(|k| k == name), "{sfx:?}");
        }
    }

    #[test]
    fn ogg_files_load_and_loops_are_left_out() {
        let dir = std::env::temp_dir().join(format!("lh-ogg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tone = dir.join("tone.wav");
        std::fs::write(&tone, wav(1, 22_050, &(0..2205).map(|i| ((i as f32 * 0.1).sin() * 8000.0) as i16).collect::<Vec<_>>())).unwrap();
        let ok = std::process::Command::new("sox").arg(&tone).arg(dir.join("shot.ogg")).status().is_ok_and(|s| s.success());
        if !ok {
            eprintln!("sox isn't installed; skipping");
            return;
        }
        std::fs::copy(dir.join("shot.ogg"), dir.join("music.ogg")).unwrap();
        let bank = Bank::load(&read_folder(&dir), 48_000.0);
        std::fs::remove_dir_all(&dir).unwrap();
        let clip = bank.get("shot", 0.0).expect("the ogg cue loads");
        // Vorbis pads the stream's end with a few milliseconds the decoder doesn't trim.
        assert!((4800..5800).contains(&clip.frames.len()), "{}", clip.frames.len());
        assert!(clip.frames.iter().any(|f| f[0].abs() > 0.1));
        assert!(bank.get("music", 0.0).is_none());
    }

    #[test]
    fn a_pickup_run_climbs_one_octave_and_holds() {
        let rate = |n| cue(Sfx::Pickup(n)).1;
        assert_eq!(rate(0), 1.0);
        assert!((1..6).all(|n| rate(n) > rate(n - 1)));
        assert_eq!(rate(5), 2.0);
        assert_eq!(rate(20), 2.0);
    }
}
