//! One controller's state, by a stable id, and every pad's combined as one player's.

use last_harvest_game::Buttons;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pad {
    pub id: usize,
    pub held: Buttons,
    pub stick: [f32; 2],
    pub look: f32,
}

/// Every pad's buttons, left stick and right stick's sideways push, as one.
pub fn combine(pads: &[Pad]) -> (Buttons, [f32; 2], f32) {
    let add = |a: f32, b: f32| (a + b).clamp(-1.0, 1.0);
    pads.iter().fold((Buttons::NONE, [0.0; 2], 0.0), |(held, [x, y], look), p| (held | p.held, [add(x, p.stick[0]), add(y, p.stick[1])], add(look, p.look)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_pads_is_nothing_held() {
        assert_eq!(combine(&[]), (Buttons::NONE, [0.0; 2], 0.0));
    }

    #[test]
    fn one_pad_is_itself() {
        let p = Pad { id: 3, held: Buttons::A | Buttons::UP, stick: [0.5, -0.25], look: 0.75 };
        assert_eq!(combine(&[p]), (p.held, p.stick, p.look));
    }

    #[test]
    fn buttons_or_together() {
        let a = Pad { id: 0, held: Buttons::A, ..Default::default() };
        let b = Pad { id: 1, held: Buttons::B | Buttons::A, ..Default::default() };
        assert_eq!(combine(&[a, b]).0, Buttons::A | Buttons::B);
    }

    #[test]
    fn sticks_add_and_clamp() {
        let a = Pad { id: 0, stick: [0.75, -0.5], look: -0.75, ..Default::default() };
        let b = Pad { id: 1, stick: [0.75, 0.25], look: -0.75, ..Default::default() };
        assert_eq!(combine(&[a, b]), (Buttons::NONE, [1.0, -0.25], -1.0));
    }

    #[test]
    fn opposite_sticks_cancel() {
        let a = Pad { id: 0, stick: [1.0, 0.0], ..Default::default() };
        let b = Pad { id: 1, stick: [-1.0, 0.0], ..Default::default() };
        assert_eq!(combine(&[a, b]).1, [0.0, 0.0]);
    }

    #[test]
    fn the_sum_clamps_as_it_goes() {
        let pads = [1.0, 1.0, -1.0].map(|x| Pad { stick: [x, 0.0], ..Default::default() });
        assert_eq!(combine(&pads).1, [0.0, 0.0]);
    }
}
