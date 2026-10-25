use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SizedSample};
use last_harvest_game::Sfx;

use crate::samples::{self, Bank, Clip};

const MIX: f32 = 0.25;
const ATTACK: f32 = 0.005;
/// Oldest voices give way past this, so a swarm can't pile up hundreds.
const MAX_VOICES: usize = 48;
/// Each cue plays up to this fraction sharp or flat, so repeats don't drone.
const JITTER: f32 = 0.05;
const DRONE_GAIN: f32 = 0.5;

#[derive(Clone, Copy)]
enum Wave {
    Square,
    Triangle,
    Sine,
    /// Random steps at the note's frequency: hiss when high, rumble and grit when low.
    Noise,
}

/// One synth voice: a pitch sweep from `f0` to `f1` over `dur` seconds, starting after `delay`.
/// `pan` runs from -1 (left speaker only) to 1 (right speaker only); `curve` above 1 makes the fade percussive.
#[derive(Clone, Copy)]
struct Note {
    delay: f32,
    dur: f32,
    f0: f32,
    f1: f32,
    wave: Wave,
    gain: f32,
    pan: f32,
    curve: f32,
}

const fn note(delay: f32, dur: f32, f0: f32, f1: f32, wave: Wave, gain: f32) -> Note {
    Note { delay, dur, f0, f1, wave, gain, pan: 0.0, curve: 1.0 }
}

/// A note that hits hard and falls away fast.
const fn hit(delay: f32, dur: f32, f0: f32, f1: f32, wave: Wave, gain: f32) -> Note {
    Note { curve: 2.5, ..note(delay, dur, f0, f1, wave, gain) }
}

fn arpeggio(freqs: &[f32], step: f32, wave: Wave) -> Vec<Note> {
    freqs.iter().enumerate().map(|(i, &f)| note(i as f32 * step, step * 1.4, f, f, wave, 0.8)).collect()
}

fn panned(notes: Vec<Note>, pan: f32) -> Vec<Note> {
    notes.into_iter().map(|n| Note { pan, ..n }).collect()
}

/// Places a cue in the world: `pan` from -100 to 100 and `gain` from 0 to 255.
fn placed(notes: Vec<Note>, pan: i8, gain: u8) -> Vec<Note> {
    let g = gain as f32 / 255.0;
    notes.into_iter().map(|n| Note { pan: pan as f32 / 100.0, gain: n.gain * g, ..n }).collect()
}

/// Steps up a pentatonic scale from A5.
fn step(n: u8) -> f32 {
    const SCALE: [f32; 5] = [0.0, 2.0, 4.0, 7.0, 9.0];
    let n = n as usize;
    880.0 * 2f32.powf((SCALE[n % 5] + 12.0 * (n / 5) as f32) / 12.0)
}

fn notes(sfx: Sfx) -> Vec<Note> {
    use Wave::*;
    match sfx {
        Sfx::Tick(n) => {
            let f = 440.0 * 2f32.powf(n as f32 / 12.0);
            vec![note(0.0, 0.06, f, f, Square, 0.6)]
        }
        Sfx::Left => panned(arpeggio(&[523.0, 659.0, 523.0], 0.09, Square), -1.0),
        Sfx::Right => panned(arpeggio(&[784.0, 988.0, 784.0], 0.09, Square), 1.0),
        Sfx::Start => arpeggio(&[523.0, 659.0, 784.0, 1047.0], 0.07, Square),
        Sfx::Miss => vec![note(0.0, 0.35, 392.0, 147.0, Square, 0.8)],
        Sfx::GameOver => {
            let mut v = arpeggio(&[523.0, 392.0, 330.0, 262.0], 0.16, Triangle);
            v.push(hit(0.0, 1.2, 90.0, 30.0, Triangle, 1.0));
            v
        }
        Sfx::Shot => vec![
            hit(0.0, 0.07, 9000.0, 1500.0, Noise, 0.8),
            hit(0.0, 0.06, 180.0, 60.0, Square, 0.45),
            hit(0.0, 0.03, 1400.0, 400.0, Triangle, 0.35),
        ],
        Sfx::Blast => vec![
            hit(0.0, 0.22, 6000.0, 800.0, Noise, 1.0),
            hit(0.0, 0.18, 120.0, 40.0, Square, 0.7),
            hit(0.02, 0.3, 2000.0, 300.0, Noise, 0.4),
        ],
        Sfx::Slash => vec![note(0.0, 0.12, 2000.0, 7000.0, Noise, 0.45)],
        Sfx::Thud(crit) => {
            let mut v = vec![hit(0.0, 0.06, 1200.0, 300.0, Noise, 0.45), hit(0.0, 0.07, 220.0, 110.0, Triangle, 0.55)];
            if crit {
                v.push(hit(0.0, 0.12, 1320.0, 1320.0, Square, 0.3));
                v.push(hit(0.03, 0.12, 1760.0, 1760.0, Square, 0.25));
            }
            v
        }
        Sfx::Kill(kind) => {
            // Kids pop high; big ones go down with a heavy, wet thump.
            let f = [300.0, 520.0, 160.0][kind.min(2) as usize];
            let mut v = vec![hit(0.0, 0.16, 3000.0, 400.0, Noise, 0.65), hit(0.0, 0.14, f, f / 3.0, Triangle, 0.7)];
            if kind == 1 {
                v.push(hit(0.0, 0.03, 900.0, 900.0, Square, 0.3));
            }
            if kind == 2 {
                v.push(hit(0.02, 0.4, 90.0, 35.0, Square, 0.5));
            }
            v
        }
        Sfx::Boom { pan, gain } => placed(
            vec![
                hit(0.0, 0.8, 3000.0, 100.0, Noise, 1.0),
                Note { curve: 1.5, ..note(0.0, 0.7, 90.0, 30.0, Triangle, 1.0) },
                hit(0.05, 1.2, 800.0, 60.0, Noise, 0.5),
            ],
            pan,
            gain,
        ),
        Sfx::Throw => vec![note(0.0, 0.15, 800.0, 3000.0, Noise, 0.3), note(0.0, 0.05, 600.0, 900.0, Triangle, 0.2)],
        Sfx::Pickup(n) => {
            let f = step(n);
            vec![hit(0.0, 0.07, f, f, Square, 0.3), hit(0.02, 0.06, f * 2.0, f * 2.0, Triangle, 0.25)]
        }
        Sfx::Heal => arpeggio(&[523.0, 659.0, 784.0, 1047.0], 0.05, Triangle),
        Sfx::Hurt => vec![hit(0.0, 0.25, 300.0, 80.0, Square, 0.6), hit(0.0, 0.2, 1.0, 1.0, Noise, 0.5)],
        Sfx::Heartbeat => vec![hit(0.0, 0.12, 60.0, 40.0, Sine, 1.0), hit(0.16, 0.1, 55.0, 38.0, Sine, 0.7)],
        Sfx::LevelUp => {
            let mut v = arpeggio(&[523.0, 659.0, 784.0, 1047.0, 1319.0, 1568.0], 0.05, Square);
            v.push(note(0.0, 0.4, 400.0, 1600.0, Triangle, 0.4));
            v.push(hit(0.0, 0.5, 8000.0, 4000.0, Noise, 0.15));
            v
        }
        Sfx::Pick(rare) => {
            let mut v = arpeggio(&[659.0, 784.0, 1047.0], 0.06, Square);
            if rare {
                v.extend(arpeggio(&[1319.0, 1568.0, 2093.0], 0.06, Triangle).into_iter().map(|n| Note { delay: n.delay + 0.18, ..n }));
            }
            v
        }
        Sfx::Rise { pan, gain } => placed(vec![note(0.0, 0.35, 400.0, 150.0, Noise, 0.45), hit(0.0, 0.2, 80.0, 50.0, Triangle, 0.4)], pan, gain),
        Sfx::Groan { pan, gain } => placed(
            vec![
                note(0.0, 0.6, 95.0, 70.0, Triangle, 0.5),
                note(0.0, 0.6, 98.0, 72.0, Square, 0.15),
                note(0.05, 0.5, 300.0, 200.0, Noise, 0.12),
            ],
            pan,
            gain,
        ),
        Sfx::Caw { pan, gain } => placed(
            vec![
                hit(0.0, 0.12, 900.0, 600.0, Square, 0.35),
                hit(0.0, 0.12, 2000.0, 1500.0, Noise, 0.15),
                hit(0.18, 0.12, 850.0, 560.0, Square, 0.3),
                hit(0.18, 0.12, 2000.0, 1500.0, Noise, 0.12),
            ],
            pan,
            gain,
        ),
        Sfx::Bell => vec![hit(0.0, 1.4, 660.0, 660.0, Triangle, 0.6), hit(0.0, 1.0, 1320.0, 1320.0, Sine, 0.3), hit(0.0, 0.8, 1650.0, 1650.0, Sine, 0.2)],
        Sfx::Horde => vec![
            note(0.0, 1.3, 55.0, 50.0, Square, 0.45),
            note(0.0, 1.3, 82.0, 75.0, Square, 0.3),
            note(0.0, 1.1, 200.0, 100.0, Noise, 0.25),
        ],
        Sfx::Dawn => {
            let mut v = arpeggio(&[392.0, 523.0, 659.0, 784.0, 1047.0], 0.15, Triangle);
            v.push(note(0.75, 1.5, 1047.0, 1047.0, Triangle, 0.5));
            v.push(note(0.75, 1.5, 523.0, 523.0, Sine, 0.4));
            v
        }
    }
}

struct Voice {
    note: Note,
    t: f32,
    phase: f32,
    /// The current step of a noise voice.
    held: f32,
}

/// A low, slow chord under the game that grows louder and harsher with the dread.
struct Drone {
    level: f32,
    phases: [f32; 3],
    t: f32,
}

/// A recorded clip playing back, `rate` times as fast as it was recorded.
struct Play {
    clip: Arc<Clip>,
    pos: f32,
    rate: f32,
    gain: f32,
    pan: f32,
}

/// Stereo pan (-1 to 1) and loudness (0 to 1) for a cue the game placed in the world.
fn placement(sfx: Sfx) -> (f32, f32) {
    match sfx {
        Sfx::Boom { pan, gain } | Sfx::Rise { pan, gain } | Sfx::Groan { pan, gain } | Sfx::Caw { pan, gain } => (pan as f32 / 100.0, gain as f32 / 255.0),
        _ => (0.0, 1.0),
    }
}

struct Synth {
    rx: Receiver<Sfx>,
    voices: Vec<Voice>,
    bank: Bank,
    plays: Vec<Play>,
    volume: Arc<AtomicU32>,
    dread: Arc<AtomicU32>,
    drone: Drone,
    dt: f32,
    noise: u32,
}

impl Synth {
    fn random(&mut self) -> f32 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 17;
        self.noise ^= self.noise << 5;
        self.noise as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    fn sample(&mut self) -> [f32; 2] {
        let mut sum = [0.0; 2];
        for i in 0..self.voices.len() {
            let v = &mut self.voices[i];
            v.t += self.dt;
            if v.t < 0.0 {
                continue;
            }
            let n = v.note;
            let freq = n.f0 * (n.f1 / n.f0).powf(v.t / n.dur);
            let before = v.phase;
            v.phase = (v.phase + freq * self.dt).fract();
            let wrapped = v.phase < before || n.f0 <= 1.0;
            let wave = match n.wave {
                Wave::Square => if v.phase < 0.5 { 1.0 } else { -1.0 },
                Wave::Triangle => 4.0 * (v.phase - 0.5).abs() - 1.0,
                Wave::Sine => (v.phase * std::f32::consts::TAU).sin(),
                Wave::Noise => {
                    if wrapped {
                        let r = self.random();
                        self.voices[i].held = r;
                    }
                    self.voices[i].held
                }
            };
            let v = &self.voices[i];
            let fade = (1.0 - v.t / n.dur).max(0.0).powf(n.curve);
            let env = (v.t / ATTACK).min(1.0) * fade;
            let s = wave * env * n.gain;
            sum[0] += s * (1.0 - n.pan).min(1.0);
            sum[1] += s * (1.0 + n.pan).min(1.0);
        }
        self.voices.retain(|v| v.t < v.note.dur);
        for p in &mut self.plays {
            let f = &p.clip.frames;
            let (i, t) = (p.pos as usize, p.pos.fract());
            let (a, b) = (f[i], f[(i + 1).min(f.len() - 1)]);
            let s = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            sum[0] += s[0] * p.gain * (1.0 - p.pan).min(1.0);
            sum[1] += s[1] * p.gain * (1.0 + p.pan).min(1.0);
            p.pos += p.rate;
        }
        self.plays.retain(|p| (p.pos as usize) < p.clip.frames.len());
        let d = self.drone_sample();
        let gain = MIX * f32::from_bits(self.volume.load(Ordering::Relaxed));
        [0, 1].map(|i| ((sum[i] + d) * gain).clamp(-1.0, 1.0))
    }

    fn drone_sample(&mut self) -> f32 {
        let target = f32::from_bits(self.dread.load(Ordering::Relaxed));
        let d = &mut self.drone;
        d.level += (target - d.level) * (self.dt * 0.5).min(1.0);
        if d.level < 0.001 {
            return 0.0;
        }
        d.t += self.dt;
        // A fifth, then a minor second rubbing against the root as the dread climbs.
        let freqs = [55.0, 82.4, 58.3];
        let mut s = 0.0;
        for (i, f) in freqs.iter().enumerate() {
            d.phases[i] = (d.phases[i] + f * self.dt).fract();
            let tri = 4.0 * (d.phases[i] - 0.5).abs() - 1.0;
            let g = if i == 2 { ((d.level - 0.5) * 2.0).max(0.0) } else { 1.0 };
            s += tri * g;
        }
        let swell = 0.7 + 0.3 * (d.t * 0.4).sin();
        s * d.level * swell * DRONE_GAIN * 0.33
    }

    fn fill<T: SizedSample + cpal::FromSample<f32>>(&mut self, data: &mut [T], channels: usize) {
        while let Ok(sfx) = self.rx.try_recv() {
            let k = 1.0 + self.random() * JITTER;
            let (name, rate) = samples::cue(sfx);
            let pick = self.random() * 0.5 + 0.5;
            if let Some(clip) = self.bank.get(name, pick) {
                let (pan, gain) = placement(sfx);
                self.plays.push(Play { clip, pos: 0.0, rate: rate * k, gain, pan });
                if self.plays.len() > MAX_VOICES {
                    self.plays.remove(0);
                }
                continue;
            }
            for note in notes(sfx) {
                let note = if note.f0 > 1.0 { Note { f0: note.f0 * k, f1: note.f1 * k, ..note } } else { note };
                self.voices.push(Voice { note, t: -note.delay, phase: 0.0, held: 0.0 });
            }
            if self.voices.len() > MAX_VOICES {
                let extra = self.voices.len() - MAX_VOICES;
                self.voices.drain(..extra);
            }
        }
        for frame in data.chunks_mut(channels) {
            let [l, r] = self.sample();
            match frame {
                [left, right, rest @ ..] => {
                    *left = T::from_sample(l);
                    *right = T::from_sample(r);
                    rest.fill(T::from_sample((l + r) / 2.0));
                }
                _ => frame.fill(T::from_sample((l + r) / 2.0)),
            }
        }
    }
}

pub struct Audio {
    _stream: cpal::Stream,
    tx: Sender<Sfx>,
    volume: Arc<AtomicU32>,
    dread: Arc<AtomicU32>,
}

impl Audio {
    pub fn open() -> Result<Self, Box<dyn Error>> {
        let device = cpal::default_host().default_output_device().ok_or("no output device")?;
        let supported = device.default_output_config()?;
        eprintln!("audio: {:?} {:?}", device.name(), supported);
        let config = supported.config();
        let channels = config.channels as usize;
        let (tx, rx) = channel();
        let volume = Arc::new(AtomicU32::new(0f32.to_bits()));
        let dread = Arc::new(AtomicU32::new(0f32.to_bits()));
        let drone = Drone { level: 0.0, phases: [0.0; 3], t: 0.0 };
        let rate = config.sample_rate.0 as f32;
        let bank = samples::folder().map(|dir| Bank::load(&dir, rate)).unwrap_or_default();
        let mut synth = Synth { rx, voices: Vec::new(), bank, plays: Vec::new(), volume: volume.clone(), dread: dread.clone(), drone, dt: 1.0 / rate, noise: 0x9E37_79B9 };
        let stream = match supported.sample_format() {
            SampleFormat::F32 => device.build_output_stream(&config, move |d: &mut [f32], _: &_| synth.fill(d, channels), on_error, None)?,
            SampleFormat::I16 => device.build_output_stream(&config, move |d: &mut [i16], _: &_| synth.fill(d, channels), on_error, None)?,
            f => return Err(format!("unsupported output format {f:?}").into()),
        };
        stream.play()?;
        Ok(Self { _stream: stream, tx, volume, dread })
    }

    pub fn play(&self, sfx: Sfx) {
        self.tx.send(sfx).ok();
    }

    pub fn set_volume(&self, v: f32) {
        self.volume.store(v.to_bits(), Ordering::Relaxed);
    }

    /// How loud and harsh the drone under the game is, 0 to 1.
    pub fn set_dread(&self, d: f32) {
        self.dread.store(d.to_bits(), Ordering::Relaxed);
    }
}

fn on_error(e: cpal::StreamError) {
    eprintln!("audio: {e}");
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn synth() -> (Synth, Sender<Sfx>) {
        let (tx, rx) = channel();
        let volume = Arc::new(AtomicU32::new(1f32.to_bits()));
        let dread = Arc::new(AtomicU32::new(0f32.to_bits()));
        let drone = Drone { level: 0.0, phases: [0.0; 3], t: 0.0 };
        (Synth { rx, voices: Vec::new(), bank: Bank::default(), plays: Vec::new(), volume, dread, drone, dt: 1.0 / RATE, noise: 1 }, tx)
    }

    /// Plays `secs` of audio and returns the loudest sample.
    fn render(s: &mut Synth, secs: f32) -> f32 {
        let mut buf = vec![0.0f32; (RATE * secs) as usize * 2];
        s.fill(&mut buf, 2);
        buf.iter().fold(0.0, |m, v| m.max(v.abs()))
    }

    #[test]
    fn every_cue_sounds_ends_and_stays_under_full_scale() {
        let cues = [
            Sfx::Shot, Sfx::Blast, Sfx::Slash, Sfx::Thud(true), Sfx::Kill(0), Sfx::Kill(1), Sfx::Kill(2),
            Sfx::Boom { pan: 0, gain: 255 }, Sfx::Throw, Sfx::Pickup(20), Sfx::Heal, Sfx::Hurt, Sfx::Heartbeat,
            Sfx::LevelUp, Sfx::Pick(true), Sfx::Rise { pan: -100, gain: 255 }, Sfx::Groan { pan: 100, gain: 255 },
            Sfx::Caw { pan: 0, gain: 255 }, Sfx::Bell, Sfx::Horde, Sfx::Dawn, Sfx::GameOver,
        ];
        for cue in cues {
            let (mut s, tx) = synth();
            tx.send(cue).unwrap();
            let peak = render(&mut s, 3.0);
            assert!(peak > 0.02 && peak < 0.99, "{cue:?} peaks at {peak}");
            assert!(s.voices.is_empty(), "{cue:?} still playing");
        }
    }

    #[test]
    fn the_drone_follows_the_dread() {
        let (mut s, _tx) = synth();
        assert_eq!(render(&mut s, 0.5), 0.0);
        s.dread.store(1f32.to_bits(), Ordering::Relaxed);
        assert!(render(&mut s, 4.0) > 0.02);
    }

    #[test]
    fn a_recorded_clip_replaces_its_synth_cue() {
        let dir = std::env::temp_dir().join(format!("lh-cue-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut wav = Vec::new();
        let samples = vec![8000i16; 4800];
        let data = (samples.len() * 2) as u32;
        wav.extend(b"RIFF");
        wav.extend((36 + data).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        for v in [16u32.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), 48000u32.to_le_bytes().to_vec(), 96000u32.to_le_bytes().to_vec(), 2u16.to_le_bytes().to_vec(), 16u16.to_le_bytes().to_vec()] {
            wav.extend(v);
        }
        wav.extend(b"data");
        wav.extend(data.to_le_bytes());
        samples.iter().for_each(|v| wav.extend(v.to_le_bytes()));
        std::fs::write(dir.join("shot.wav"), wav).unwrap();
        let (mut s, tx) = synth();
        s.bank = Bank::load(&dir, RATE);
        std::fs::remove_dir_all(&dir).unwrap();
        tx.send(Sfx::Shot).unwrap();
        tx.send(Sfx::Blast).unwrap();
        render(&mut s, 0.01);
        assert_eq!(s.plays.len(), 1, "the shot plays its clip");
        assert!(!s.voices.is_empty(), "the blast keeps its synth voice");
        render(&mut s, 0.2);
        assert!(s.plays.is_empty());
    }
}
