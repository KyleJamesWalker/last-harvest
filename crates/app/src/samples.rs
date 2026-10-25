//! Recorded sounds that stand in for synth cues: `<cue>.wav`, or `<cue>-2.wav` and so on for variations,
//! in the sounds folder. Any cue without a file keeps its synth voice.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use last_harvest_game::Sfx;

/// Stereo frames at the output's sample rate.
pub struct Clip {
    pub frames: Vec<[f32; 2]>,
}

#[derive(Default)]
pub struct Bank {
    clips: HashMap<String, Vec<Arc<Clip>>>,
}

/// The file name a cue looks for, and the playback rate it asks for. Pickups climb a pentatonic scale from one file.
pub fn cue(sfx: Sfx) -> (&'static str, f32) {
    let name = match sfx {
        Sfx::Tick(_) | Sfx::Left | Sfx::Right => return ("", 1.0),
        Sfx::Start => "start",
        Sfx::Miss => "miss",
        Sfx::GameOver => "gameover",
        Sfx::Shot => "shot",
        Sfx::Blast => "blast",
        Sfx::Slash => "slash",
        Sfx::Thud(false) => "thud",
        Sfx::Thud(true) => "crit",
        Sfx::Kill(0) => "kill-skinny",
        Sfx::Kill(1) => "kill-kid",
        Sfx::Kill(_) => "kill-big",
        Sfx::Boom { .. } => "boom",
        Sfx::Throw => "throw",
        Sfx::Pickup(n) => {
            const SCALE: [f32; 5] = [0.0, 2.0, 4.0, 7.0, 9.0];
            let n = n as usize;
            return ("pickup", 2f32.powf((SCALE[n % 5] + 12.0 * (n / 5) as f32) / 12.0));
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

impl Bank {
    pub fn load(dir: &Path, rate: f32) -> Self {
        let mut bank = Bank::default();
        let Ok(entries) = std::fs::read_dir(dir) else { return bank };
        for path in entries.flatten().map(|e| e.path()) {
            if path.extension().and_then(|e| e.to_str()) != Some("wav") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            // "shot-2" is a variation of "shot"; "kill-big" is its own cue.
            let name = match stem.rsplit_once('-') {
                Some((base, n)) if n.parse::<u32>().is_ok() => base,
                _ => stem,
            };
            match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| decode(&b, rate)) {
                Ok(clip) => bank.clips.entry(name.to_string()).or_default().push(Arc::new(clip)),
                Err(e) => eprintln!("audio: {}: {e}", path.display()),
            }
        }
        let mut names: Vec<&String> = bank.clips.keys().collect();
        names.sort();
        eprintln!("audio: {} recorded cues from {}: {names:?}", names.len(), dir.display());
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
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let (mut fmt, mut data) = (None, None);
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let size = u32_at(i + 4) as usize;
        let body = i + 8..(i + 8 + size).min(bytes.len());
        match &bytes[i..i + 4] {
            b"fmt " if size >= 16 => {
                // An extensible header keeps the real format in the first two bytes of its subformat GUID.
                let format = if u16_at(i + 8) == 0xFFFE && size >= 40 { u16_at(i + 32) } else { u16_at(i + 8) };
                fmt = Some((format, u16_at(i + 10) as usize, u32_at(i + 12) as f32, u16_at(i + 22) as usize));
            }
            b"data" => data = Some(body),
            _ => {}
        }
        i += 8 + size + (size & 1);
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
    let source: Vec<[f32; 2]> = (0..data.len() / frame)
        .map(|f| {
            let l = sample(f * frame);
            let r = if channels > 1 { sample(f * frame + width) } else { l };
            [l, r]
        })
        .collect();
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
        let bank = Bank::load(&dir, 48_000.0);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(bank.clips["shot"].len(), 2);
        assert_eq!(bank.clips["kill-big"].len(), 1);
        assert!(bank.get("kill", 0.0).is_none());
        assert_eq!(cue(Sfx::Kill(2)).0, "kill-big");
    }
}
