//! The game's fixed 60 Hz step and the input each step sees.

use std::time::Duration;

use last_harvest_game::{Buttons, Input};

pub const HZ: u32 = 60;
pub const STEP: f32 = 1.0 / HZ as f32;
pub const MAX_STEPS: u32 = 4;

const NANOS: u128 = 1_000_000_000;

/// Owed time in nanoseconds times `HZ`, so one step is exactly `NANOS`.
#[derive(Debug, Default)]
pub struct Clock {
    owed: u128,
}

impl Clock {
    pub fn advance(&mut self, frame: Duration) -> u32 {
        self.owed += frame.as_nanos() * HZ as u128;
        let steps = self.owed / NANOS;
        self.owed %= NANOS;
        steps.min(MAX_STEPS as u128) as u32
    }

    pub fn leftover(&self) -> f32 {
        self.owed as f32 / NANOS as f32
    }
}

/// A frame's presses go to its first step only; a frame with no step keeps them for the next.
#[derive(Debug, Default)]
pub struct Ticker {
    clock: Clock,
    carried: Vec<Input>,
}

impl Ticker {
    pub fn frame(&mut self, frame: Duration, input: Input) -> Vec<Input> {
        self.frame_all(frame, vec![input])
            .into_iter()
            .map(|mut each| each.swap_remove(0))
            .collect()
    }

    /// As `frame`, for each player's input at once: one list per step, player 1's first.
    pub fn frame_all(&mut self, frame: Duration, inputs: Vec<Input>) -> Vec<Vec<Input>> {
        let carried = std::mem::take(&mut self.carried);
        let inputs: Vec<Input> = inputs
            .into_iter()
            .enumerate()
            .map(|(i, now)| match carried.get(i) {
                Some(&before) => carry(before, now),
                None => now,
            })
            .collect();
        let steps = self.clock.advance(frame);
        if steps == 0 {
            self.carried = inputs;
            return Vec::new();
        }
        let share = 1.0 / steps as f32;
        let inputs: Vec<Input> = inputs
            .into_iter()
            .map(|i| Input {
                yaw: i.yaw * share,
                pitch: i.pitch * share,
                ..i
            })
            .collect();
        let after: Vec<Input> = inputs
            .iter()
            .map(|&i| Input {
                pressed: Buttons::NONE,
                touch_started: false,
                orbit: [0.0; 2],
                wheel: 0.0,
                hover: None,
                ..i
            })
            .collect();
        std::iter::once(inputs)
            .chain(std::iter::repeat_n(after, steps as usize - 1))
            .collect()
    }

    /// How far the time owed has come toward the next step, 0 to 1, for drawing between the last two.
    pub fn alpha(&self) -> f32 {
        self.clock.leftover()
    }
}

/// A press or touch that lifted before any step still reaches one, held for that step.
fn carry(before: Input, now: Input) -> Input {
    let lifted = before.touch_started && now.touch.is_none();
    Input {
        held: now.held | before.pressed,
        pressed: now.pressed | before.pressed,
        touch_started: now.touch_started || before.touch_started,
        touch: if lifted { before.touch } else { now.touch },
        touches: if lifted { before.touches } else { now.touches },
        orbit: [
            before.orbit[0] + now.orbit[0],
            before.orbit[1] + now.orbit[1],
        ],
        yaw: before.yaw + now.yaw,
        pitch: before.pitch + now.pitch,
        wheel: before.wheel + now.wheel,
        hover: now.hover.or(before.hover),
        ..now
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use last_harvest_game::{InputTracker, RawInput};

    fn ms(ms: f64) -> Duration {
        Duration::from_secs_f64(ms / 1000.0)
    }

    fn steps(frames: &[f64]) -> Vec<u32> {
        let mut c = Clock::default();
        frames.iter().map(|&f| c.advance(ms(f))).collect()
    }

    #[test]
    fn a_60_hz_display_runs_one_step_a_frame() {
        let mut c = Clock::default();
        let step = Duration::from_nanos(16_666_667);
        assert!((0..600).all(|_| c.advance(step) == 1));
    }

    #[test]
    fn a_120_hz_display_runs_a_step_every_other_frame() {
        let mut c = Clock::default();
        let half = Duration::from_nanos(8_333_334);
        let run: Vec<u32> = (0..8).map(|_| c.advance(half)).collect();
        assert_eq!(run, [0, 1, 0, 1, 0, 1, 0, 1]);
    }

    #[test]
    fn a_30_hz_display_runs_two_steps_a_frame() {
        let mut c = Clock::default();
        let frame = Duration::from_nanos(33_333_334);
        assert!((0..300).all(|_| c.advance(frame) == 2));
    }

    #[test]
    fn a_second_of_frames_is_sixty_steps_whatever_their_lengths() {
        let mut c = Clock::default();
        let frames = [7.0, 13.0, 21.0, 4.0, 16.0, 30.0, 9.0];
        let mut total = 0;
        let mut spent = 0.0;
        while spent < 1000.0 {
            for f in frames {
                if spent + f <= 1000.0 {
                    total += c.advance(ms(f));
                    spent += f;
                } else {
                    total += c.advance(ms(1000.0 - spent));
                    spent = 1000.0;
                    break;
                }
            }
        }
        assert_eq!(total, 60);
    }

    #[test]
    fn leftover_time_carries_to_the_next_frame() {
        assert_eq!(steps(&[10.0, 10.0, 10.0, 10.0]), [0, 1, 0, 1]);
        let mut c = Clock::default();
        c.advance(ms(25.0));
        assert!((c.leftover() - 0.5).abs() < 1e-3, "{}", c.leftover());
    }

    #[test]
    fn a_stall_runs_at_most_max_steps_and_drops_the_rest() {
        assert_eq!(steps(&[1000.0, 16.0, 1.0]), [MAX_STEPS, 0, 1]);
        assert_eq!(steps(&[300_000.0]), [MAX_STEPS]);
        let mut c = Clock::default();
        c.advance(ms(1000.0 + 8.0));
        assert!(c.leftover() < 1.0);
        assert_eq!(steps(&[0.0, 0.0]), [0, 0]);
    }

    #[test]
    fn alpha_is_how_far_the_time_owed_has_come_toward_the_next_step() {
        let mut t = Ticker::default();
        let half = Duration::from_nanos(8_333_334);
        t.frame(half, Input::default());
        assert!((t.alpha() - 0.5).abs() < 1e-3, "{}", t.alpha());
        t.frame(half, Input::default());
        assert!(t.alpha() < 1e-3, "{}", t.alpha());
    }

    fn held(b: Buttons) -> RawInput {
        RawInput {
            held: b,
            ..Default::default()
        }
    }

    fn play(frames: &[(f64, RawInput)]) -> Vec<Input> {
        let (mut tracker, mut ticker) = (InputTracker::default(), Ticker::default());
        frames
            .iter()
            .flat_map(|&(f, raw)| ticker.frame(ms(f), tracker.frame(raw)))
            .collect()
    }

    #[test]
    fn a_press_reaches_only_the_first_of_several_steps() {
        let run = play(&[(50.0, held(Buttons::A))]);
        assert_eq!(run.len(), 3);
        assert_eq!(run[0].pressed, Buttons::A);
        assert!(
            run[1..]
                .iter()
                .all(|i| i.pressed == Buttons::NONE && i.held == Buttons::A)
        );
    }

    #[test]
    fn a_tap_in_a_frame_with_no_step_reaches_the_next_step() {
        let run = play(&[
            (17.0, held(Buttons::NONE)),
            (5.0, held(Buttons::A)),
            (5.0, held(Buttons::NONE)),
            (12.0, held(Buttons::NONE)),
        ]);
        let pressed: Vec<Buttons> = run.iter().map(|i| i.pressed).collect();
        assert_eq!(pressed, [Buttons::NONE, Buttons::A]);
        assert_eq!(
            run[1].held,
            Buttons::A,
            "the tap is held for the step it reaches"
        );
    }

    #[test]
    fn a_held_press_on_a_frame_with_no_step_fires_once() {
        let run = play(&[
            (8.0, held(Buttons::A)),
            (9.0, held(Buttons::A)),
            (8.0, held(Buttons::A)),
            (9.0, held(Buttons::A)),
        ]);
        assert_eq!(run.len(), 2);
        assert_eq!(
            run.iter().map(|i| i.pressed).collect::<Vec<_>>(),
            [Buttons::A, Buttons::NONE]
        );
        assert!(run.iter().all(|i| i.held == Buttons::A));
    }

    #[test]
    fn a_release_on_a_frame_with_no_step_is_seen_by_the_next() {
        let run = play(&[
            (17.0, held(Buttons::A)),
            (5.0, held(Buttons::NONE)),
            (12.0, held(Buttons::NONE)),
        ]);
        assert_eq!(run.len(), 2);
        assert_eq!(run[1].held, Buttons::NONE);
        assert_eq!(run[1].pressed, Buttons::NONE);
    }

    #[test]
    fn a_touch_lifted_before_any_step_still_starts_where_it_was() {
        let at = Some([10.0, 20.0]);
        let down = RawInput {
            touch: at,
            touches: [at, None, None, None, None, None, None, None, None, None],
            ..Default::default()
        };
        let run = play(&[
            (5.0, down),
            (5.0, RawInput::default()),
            (10.0, RawInput::default()),
            (17.0, RawInput::default()),
        ]);
        assert_eq!(run.len(), 2);
        assert!(run[0].touch_started);
        assert_eq!(run[0].touch, at);
        assert_eq!(run[0].touches[0], at);
        assert!(!run[1].touch_started);
        assert_eq!(run[1].touch, None);
    }

    #[test]
    fn orbit_adds_up_across_frames_with_no_step_and_goes_to_one_step() {
        let drag = RawInput {
            orbit: [3.0, 1.0],
            ..Default::default()
        };
        let run = play(&[(8.0, drag), (40.0, drag)]);
        assert_eq!(run.len(), 2);
        assert_eq!(run[0].orbit, [6.0, 2.0]);
        assert_eq!(run[1].orbit, [0.0, 0.0]);
    }
}
