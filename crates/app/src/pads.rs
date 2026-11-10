//! Gamepads on the desktop, through gilrs. Face buttons map by position, as the RG DS lays them out.

use gilrs::{Axis, Button, EventType, Gilrs};
use last_harvest::log;
use last_harvest_game::Buttons;

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
}

impl Pads {
    pub fn open() -> Self {
        let gilrs = Gilrs::new().inspect_err(|e| log!("gamepads off: {e}")).ok();
        for (_, pad) in gilrs.iter().flat_map(|g| g.gamepads()) {
            log!("gamepad: {}", pad.name());
        }
        Self { gilrs }
    }

    /// The buttons held, the left stick, and the right stick's sideways push, over every connected pad.
    pub fn read(&mut self) -> (Buttons, [f32; 2], f32) {
        let (mut held, mut stick, mut look) = (Buttons::NONE, [0.0f32; 2], 0.0f32);
        let Some(g) = &mut self.gilrs else { return (held, stick, look) };
        while let Some(event) = g.next_event() {
            match event.event {
                EventType::Connected => log!("gamepad: {} connected", g.gamepad(event.id).name()),
                EventType::Disconnected => log!("gamepad: {} disconnected", g.gamepad(event.id).name()),
                _ => {}
            }
        }
        for (_, pad) in g.gamepads() {
            for (b, to) in PAD {
                if pad.is_pressed(b) {
                    held |= to;
                }
            }
            // gilrs reads up as positive; the game, like SDL and the browser, reads down as positive.
            for (s, v) in stick.iter_mut().zip([pad.value(Axis::LeftStickX), -pad.value(Axis::LeftStickY)]) {
                if v.abs() >= STICK_DEAD {
                    *s = (*s + v).clamp(-1.0, 1.0);
                }
            }
            let v = pad.value(Axis::RightStickX);
            if v.abs() >= STICK_DEAD {
                look = (look + v).clamp(-1.0, 1.0);
            }
        }
        (held, stick, look)
    }
}
