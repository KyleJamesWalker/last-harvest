//! Gamepads on the desktop, through gilrs. Face buttons map by position, as the RG DS lays them out.

use gilrs::ff::{BaseEffect, BaseEffectType, Effect, EffectBuilder, Replay, Ticks};
use gilrs::{Axis, Button, EventType, Gilrs};
use last_harvest::controllers::{Motors, Pad, Seats};
use last_harvest::log;
use last_harvest_game::{Buttons, PadFamily};

const STICK_DEAD: f32 = 0.15;

const PAD: [(Button, Buttons); 14] = [
    (Button::East, Buttons::A),
    (Button::South, Buttons::B),
    (Button::North, Buttons::X),
    (Button::West, Buttons::Y),
    (Button::LeftTrigger, Buttons::L1),
    (Button::RightTrigger, Buttons::R1),
    (Button::LeftTrigger2, Buttons::L2),
    (Button::RightTrigger2, Buttons::R2),
    (Button::Select, Buttons::SELECT),
    (Button::Start, Buttons::START),
    (Button::DPadUp, Buttons::UP),
    (Button::DPadDown, Buttons::DOWN),
    (Button::DPadLeft, Buttons::LEFT),
    (Button::DPadRight, Buttons::RIGHT),
];

pub struct Pads {
    gilrs: Option<Gilrs>,
    /// The pads as `read` last found them.
    last: Vec<Pad>,
    motors: Motors,
    /// Each force-feedback pad's buzz, by gilrs id, made the first time it buzzes.
    effects: Vec<(usize, Effect)>,
}

impl Pads {
    pub fn open() -> Self {
        let gilrs = Gilrs::new().inspect_err(|e| log!("gamepads off: {e}")).ok();
        for (_, pad) in gilrs.iter().flat_map(|g| g.gamepads()) {
            log!("gamepad: {} ({:?})", pad.name(), family(&pad));
        }
        Self {
            gilrs,
            last: Vec::new(),
            motors: Motors::default(),
            effects: Vec::new(),
        }
    }

    /// Runs each pad's motor, where it has one, while the player it plays as is `buzzing`, by seat from 0.
    pub fn rumble(&mut self, seats: &Seats, buzzing: impl Fn(usize) -> bool) {
        for (id, on) in self.motors.changes(seats, &self.last, 0, buzzing) {
            self.motor(id, on);
        }
    }

    fn motor(&mut self, id: usize, on: bool) {
        let Some(g) = &mut self.gilrs else { return };
        if on && !self.effects.iter().any(|(e, _)| *e == id) {
            let Some(pad) = g
                .gamepads()
                .find(|(p, pad)| usize::from(*p) == id && pad.is_ff_supported())
                .map(|(p, _)| p)
            else {
                return;
            };
            let strong = BaseEffect {
                kind: BaseEffectType::Strong { magnitude: 48_000 },
                scheduling: Replay {
                    play_for: Ticks::from_ms(1000),
                    ..Default::default()
                },
                envelope: Default::default(),
            };
            match EffectBuilder::new()
                .add_effect(strong)
                .gamepads(&[pad])
                .finish(g)
            {
                Ok(effect) => self.effects.push((id, effect)),
                Err(e) => return log!("rumble: {e}"),
            }
        }
        let Some((_, effect)) = self.effects.iter().find(|(e, _)| *e == id) else {
            return;
        };
        if let Err(e) = if on { effect.play() } else { effect.stop() } {
            log!("rumble: {e}");
        }
    }

    /// Each connected pad by its gilrs id, and the labels of the pad that pressed a button or pushed a stick this
    /// frame.
    pub fn read(&mut self) -> (Vec<Pad>, Option<PadFamily>) {
        let mut used = None;
        let Some(g) = &mut self.gilrs else {
            return (Vec::new(), used);
        };
        while let Some(event) = g.next_event() {
            match event.event {
                EventType::Connected => log!(
                    "gamepad: {} connected ({:?})",
                    g.gamepad(event.id).name(),
                    family(&g.gamepad(event.id))
                ),
                EventType::Disconnected => {
                    log!("gamepad: {} disconnected", g.gamepad(event.id).name());
                    self.effects.retain(|(id, _)| *id != usize::from(event.id));
                }
                EventType::ButtonPressed(..) => used = Some(family(&g.gamepad(event.id))),
                EventType::AxisChanged(_, v, _) if v.abs() >= STICK_DEAD => {
                    used = Some(family(&g.gamepad(event.id)))
                }
                _ => {}
            }
        }
        let live = |v: f32| if v.abs() >= STICK_DEAD { v } else { 0.0 };
        let pads = g
            .gamepads()
            .map(|(id, pad)| {
                let held = PAD
                    .iter()
                    .filter(|(b, _)| pad.is_pressed(*b))
                    .fold(Buttons::NONE, |held, &(_, to)| held | to);
                // gilrs reads up as positive; the game, like SDL and the browser, reads down as positive.
                let stick = [
                    live(pad.value(Axis::LeftStickX)),
                    live(-pad.value(Axis::LeftStickY)),
                ];
                Pad {
                    id: usize::from(id),
                    held,
                    stick,
                    look: live(pad.value(Axis::RightStickX)),
                }
            })
            .collect::<Vec<Pad>>();
        self.last.clone_from(&pads);
        (pads, used)
    }
}

fn family(pad: &gilrs::Gamepad) -> PadFamily {
    PadFamily::detect(pad.vendor_id(), pad.name())
}
