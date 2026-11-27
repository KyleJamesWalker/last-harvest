//! The desktop and browser keyboard: which game button each key sends, by the key's place on the keyboard
//! (`KeyboardEvent.code` in a browser, winit's `KeyCode` names on the desktop). WASD and IJKL both walk, so either
//! hand can move while the other holds the mouse.

use crate::Buttons;

/// The game button key `code` sends, if any.
pub fn key_button(code: &str) -> Option<Buttons> {
    Some(match code {
        "ArrowUp" | "KeyW" | "KeyI" => Buttons::UP,
        "ArrowDown" | "KeyS" | "KeyK" => Buttons::DOWN,
        "ArrowLeft" | "KeyA" | "KeyJ" => Buttons::LEFT,
        "ArrowRight" | "KeyD" | "KeyL" => Buttons::RIGHT,
        "KeyQ" | "KeyU" => Buttons::Y,
        "KeyE" | "KeyO" | "KeyX" => Buttons::A,
        "KeyR" | "KeyP" | "KeyZ" | "Backspace" => Buttons::B,
        "KeyF" | "Semicolon" | "ShiftLeft" | "ShiftRight" => Buttons::X,
        "Space" => Buttons::R1,
        "KeyG" | "Quote" => Buttons::L2,
        "Tab" | "KeyM" | "Digit1" => Buttons::L1,
        "F12" | "Digit2" => Buttons::R2,
        "Enter" | "NumpadEnter" => Buttons::START,
        "F3" => Buttons::SELECT,
        "Home" => Buttons::BACK,
        "Minus" => Buttons::VOL_DOWN,
        "Equal" => Buttons::VOL_UP,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_hands_walk_and_turn_the_same_way() {
        for (left, right) in [
            ("KeyW", "KeyI"),
            ("KeyA", "KeyJ"),
            ("KeyS", "KeyK"),
            ("KeyD", "KeyL"),
            ("KeyQ", "KeyU"),
            ("KeyE", "KeyO"),
            ("KeyR", "KeyP"),
            ("KeyF", "Semicolon"),
            ("KeyG", "Quote"),
        ] {
            assert_eq!(key_button(left), key_button(right), "{left} and {right}");
            assert!(key_button(left).is_some(), "{left}");
        }
    }

    #[test]
    fn every_button_the_controls_page_moves_has_a_key() {
        for b in crate::controls::BINDABLE {
            assert!(
                [
                    "KeyE", "KeyR", "KeyF", "KeyQ", "Tab", "Space", "KeyG", "F12", "F3", "Enter"
                ]
                .iter()
                .any(|k| key_button(k) == Some(b)),
                "{b:?}"
            );
        }
    }
}
