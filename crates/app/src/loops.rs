//! Long tracks that repeat under the effects, like music and the night's ambience. Each decodes on its own
//! thread a little ahead of playback, so only a fraction of a second is ever held in memory.

use std::io::Cursor;
use std::path::Path;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};

use lewton::inside_ogg::OggStreamReader;

/// Output frames per chunk the decoder hands over; a few are queued ahead.
const CHUNK: usize = 4096;
const QUEUED: usize = 4;
/// Output frames held back from the end of each pass, since Vorbis pads the end with silence to trim.
const HOLD: usize = 2048;

pub struct Loop {
    rx: Receiver<Vec<[f32; 2]>>,
    buf: Vec<[f32; 2]>,
    at: usize,
}

impl Loop {
    /// Starts decoding an Ogg Vorbis file over and over at `rate`, or None if it can't be read.
    pub fn open(path: &Path, rate: f32) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        OggStreamReader::new(Cursor::new(&bytes)).ok()?;
        let (tx, rx) = sync_channel(QUEUED);
        std::thread::Builder::new().name("audio-loop".into()).spawn(move || repeat(&bytes, rate, &tx)).ok()?;
        Some(Self { rx, buf: Vec::new(), at: 0 })
    }

    /// The next frame, or silence if the decoder hasn't kept up.
    pub fn next(&mut self) -> [f32; 2] {
        if self.at >= self.buf.len() {
            match self.rx.try_recv() {
                Ok(chunk) => (self.buf, self.at) = (chunk, 0),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return [0.0; 2],
            }
        }
        self.at += 1;
        self.buf[self.at - 1]
    }
}

/// Decodes the file from the top, again and again, until the player goes away.
fn repeat(bytes: &[u8], rate: f32, tx: &SyncSender<Vec<[f32; 2]>>) {
    loop {
        let Ok(mut reader) = OggStreamReader::new(Cursor::new(bytes)) else { return };
        let channels = reader.ident_hdr.audio_channels as usize;
        let step = reader.ident_hdr.audio_sample_rate as f32 / rate;
        let mut resample = Resample { step, at: 0.0, prev: None };
        let (mut out, mut decoded) = (Vec::with_capacity(CHUNK + HOLD), 0u64);
        while let Ok(Some(packet)) = reader.read_dec_packet_itl() {
            for f in packet.chunks_exact(channels) {
                let s = |v: i16| v as f32 / 32768.0;
                resample.push([s(f[0]), s(f[(channels > 1) as usize])], &mut out);
            }
            decoded += (packet.len() / channels) as u64;
            if out.len() >= CHUNK + HOLD && tx.send(out.drain(..CHUNK).collect()).is_err() {
                return;
            }
        }
        // The last page's granule position is the true length: drop the padding past it.
        if let Some(length) = reader.get_last_absgp() {
            let extra = (decoded.saturating_sub(length) as f32 / step) as usize;
            out.truncate(out.len().saturating_sub(extra));
        }
        if !out.is_empty() && tx.send(out).is_err() {
            return;
        }
    }
}

/// Linear resampling across a stream of frames, carrying its place from one frame to the next.
struct Resample {
    step: f32,
    at: f32,
    prev: Option<[f32; 2]>,
}

impl Resample {
    fn push(&mut self, cur: [f32; 2], out: &mut Vec<[f32; 2]>) {
        let Some(prev) = self.prev.replace(cur) else { return };
        while self.at < 1.0 {
            let t = self.at;
            out.push([prev[0] + (cur[0] - prev[0]) * t, prev[1] + (cur[1] - prev[1]) * t]);
            self.at += self.step;
        }
        self.at -= 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_keeps_the_rate_across_frames() {
        let mut r = Resample { step: 0.5, at: 0.0, prev: None };
        let mut out = Vec::new();
        for i in 0..5 {
            r.push([i as f32; 2], &mut out);
        }
        assert_eq!(out.iter().map(|f| f[0]).collect::<Vec<_>>(), [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5]);
    }

    #[test]
    fn a_loop_plays_its_file_again_and_again() {
        let dir = std::env::temp_dir().join(format!("lh-loop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (wav, ogg) = (dir.join("tone.wav"), dir.join("tone.ogg"));
        let made = std::process::Command::new("sox")
            .args(["-n", "-r", "22050", "-c", "1"])
            .arg(&wav)
            .args(["synth", "0.2", "sine", "440"])
            .status()
            .is_ok_and(|s| s.success())
            && std::process::Command::new("sox").arg(&wav).arg(&ogg).status().is_ok_and(|s| s.success());
        if !made {
            eprintln!("sox isn't installed; skipping");
            return;
        }
        let mut lp = Loop::open(&ogg, 44_100.0).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        // Three passes' worth, giving the decoder thread a moment whenever it falls behind.
        let (mut loud, mut taken) = (0, 0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while taken < 3 * 8820 && std::time::Instant::now() < deadline {
            let f = lp.next();
            if f == [0.0; 2] && lp.at >= lp.buf.len() {
                std::thread::sleep(std::time::Duration::from_millis(1));
                continue;
            }
            taken += 1;
            loud += (f[0].abs() > 0.3) as usize;
        }
        assert_eq!(taken, 3 * 8820, "it keeps coming");
        assert!(loud > taken / 3, "{loud} of {taken}");
    }
}
