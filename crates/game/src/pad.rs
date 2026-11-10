//! What the player's own pad or keyboard calls each game button, so hints name the button under their thumb.
//! The game's buttons are the RG DS's, and every platform maps a pad's face buttons to them by position.

use std::cell::Cell;

use crate::Buttons;

/// The labels printed on the pad that last sent input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PadFamily {
    /// A, B, X and Y as on the RG DS and a Nintendo pad; also 8BitDo pads, which print the same layout.
    #[default]
    Nintendo,
    /// Microsoft's pads and the generic XInput pads that copy them: A at the bottom, LB, RT, View and Menu.
    Xbox,
    /// Sony's pads: Cross, Circle, Square and Triangle.
    PlayStation,
    /// The desktop and browser keyboard's default keys.
    Keyboard,
}

const MICROSOFT: u16 = 0x045E;
const SONY: u16 = 0x054C;
const NINTENDO: u16 = 0x057E;
const EIGHT_BIT_DO: u16 = 0x2DC8;

const NAMES: [(&str, Buttons); 10] = [
    ("A", Buttons::A),
    ("B", Buttons::B),
    ("X", Buttons::X),
    ("Y", Buttons::Y),
    ("L1", Buttons::L1),
    ("R1", Buttons::R1),
    ("L2", Buttons::L2),
    ("R2", Buttons::R2),
    ("Select", Buttons::SELECT),
    ("Start", Buttons::START),
];

impl PadFamily {
    /// From a USB vendor id, when it names a maker whose labels are known.
    pub fn from_vendor(vendor: u16) -> Option<Self> {
        match vendor {
            MICROSOFT => Some(Self::Xbox),
            SONY => Some(Self::PlayStation),
            NINTENDO | EIGHT_BIT_DO => Some(Self::Nintendo),
            _ => None,
        }
    }

    /// From a pad's vendor id, else its name, which in a browser can carry the vendor id
    /// (`Vendor: 054c` in Chrome, a leading `054c-` in Firefox). An unknown pad is taken for an XInput pad.
    pub fn detect(vendor: Option<u16>, name: &str) -> Self {
        if let Some(f) = vendor.and_then(Self::from_vendor).or_else(|| name_vendor(name).and_then(Self::from_vendor)) {
            return f;
        }
        let name = name.to_ascii_lowercase();
        let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
        if has(&["dualshock", "dualsense", "playstation", "sony"]) {
            Self::PlayStation
        } else if has(&["nintendo", "switch", "joy-con", "8bitdo"]) {
            Self::Nintendo
        } else {
            Self::Xbox
        }
    }

    /// The label this family prints on the button that sends game button `b`, or the game's own name for one it lacks.
    pub fn label(self, b: Buttons) -> &'static str {
        use PadFamily::*;
        let i = NAMES.iter().position(|&(_, n)| n == b);
        let Some(i) = i else { return "" };
        match self {
            Nintendo => NAMES[i].0,
            Xbox => ["B", "A", "Y", "X", "LB", "RB", "LT", "RT", "View", "Menu"][i],
            PlayStation => ["Circle", "Cross", "Triangle", "Square", "L1", "R1", "L2", "R2", "Share", "Options"][i],
            Keyboard => ["X", "Z", "S", "A", "1", "W", "Q", "2", "Backspace", "Enter"][i],
        }
    }
}

fn name_vendor(name: &str) -> Option<u16> {
    let lower = name.to_ascii_lowercase();
    let hex = |s: &str| s.get(..4).filter(|h| h.bytes().all(|c| c.is_ascii_hexdigit())).and_then(|h| u16::from_str_radix(h, 16).ok());
    if let Some(at) = lower.find("vendor: ") {
        return hex(&lower[at + 8..]);
    }
    (lower.as_bytes().get(4) == Some(&b'-')).then(|| hex(&lower)).flatten()
}

thread_local! {
    static ACTIVE: Cell<PadFamily> = const { Cell::new(PadFamily::Nintendo) };
}

pub(crate) fn set_active(family: PadFamily) {
    ACTIVE.set(family);
}

pub fn active() -> PadFamily {
    ACTIVE.get()
}

/// What the active pad calls game button `b`.
pub fn label(b: Buttons) -> &'static str {
    active().label(b)
}

/// A hint with each `{A}`, `{L1}`, `{Start}` and so on replaced by the active pad's label for that game button.
pub fn hint(text: &str) -> String {
    fill(active(), text)
}

fn fill(family: PadFamily, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}').and_then(|close| NAMES.iter().find(|(n, _)| *n == &after[..close]).map(|&(_, b)| (close, b))) {
            Some((close, b)) => {
                out.push_str(family.label(b));
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `hint(text)` and a size that keeps it as wide as the RG DS's names make it at `size`.
pub fn hint_sized(text: &str, size: f32) -> (String, f32) {
    let (shown, own) = (hint(text), fill(PadFamily::Nintendo, text));
    let k = own.chars().count() as f32 / shown.chars().count().max(1) as f32;
    (shown, size * k.min(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_family_names_the_button_where_the_rg_ds_has_it() {
        let faces = [Buttons::A, Buttons::B, Buttons::X, Buttons::Y];
        let names = |f: PadFamily| faces.map(|b| f.label(b));
        assert_eq!(names(PadFamily::Nintendo), ["A", "B", "X", "Y"]);
        assert_eq!(names(PadFamily::Xbox), ["B", "A", "Y", "X"], "right, bottom, top, left");
        assert_eq!(names(PadFamily::PlayStation), ["Circle", "Cross", "Triangle", "Square"]);
        assert_eq!(names(PadFamily::Keyboard), ["X", "Z", "S", "A"]);
        let rest = [Buttons::L1, Buttons::R1, Buttons::L2, Buttons::R2, Buttons::SELECT, Buttons::START];
        assert_eq!(rest.map(|b| PadFamily::Xbox.label(b)), ["LB", "RB", "LT", "RT", "View", "Menu"]);
        assert_eq!(rest.map(|b| PadFamily::Nintendo.label(b)), ["L1", "R1", "L2", "R2", "Select", "Start"]);
        assert_eq!(rest.map(|b| PadFamily::PlayStation.label(b)), ["L1", "R1", "L2", "R2", "Share", "Options"]);
    }

    #[test]
    fn a_pad_is_known_by_its_vendor_or_its_name() {
        assert_eq!(PadFamily::detect(Some(0x045E), ""), PadFamily::Xbox);
        assert_eq!(PadFamily::detect(Some(0x054C), "Wireless Controller"), PadFamily::PlayStation);
        assert_eq!(PadFamily::detect(Some(0x057E), "Pro Controller"), PadFamily::Nintendo);
        assert_eq!(PadFamily::detect(Some(0x2DC8), "8BitDo Pro 2"), PadFamily::Nintendo);
        assert_eq!(PadFamily::detect(None, "Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 09cc)"), PadFamily::PlayStation);
        assert_eq!(PadFamily::detect(None, "057e-2009-Pro Controller"), PadFamily::Nintendo);
        assert_eq!(PadFamily::detect(None, "Xbox 360 Controller (XInput STANDARD GAMEPAD)"), PadFamily::Xbox);
        assert_eq!(PadFamily::detect(None, "DualSense Wireless Controller"), PadFamily::PlayStation);
        assert_eq!(PadFamily::detect(Some(0x1234), "Some Pad"), PadFamily::Xbox, "an unknown pad is taken for XInput");
    }

    #[test]
    fn a_hint_names_the_active_pads_buttons() {
        let text = "Up / Down and {X}: choose.   {L1}: banish.   {Start} or {B}: resume.   {Fog} stays";
        assert_eq!(hint(text), "Up / Down and X: choose.   L1: banish.   Start or B: resume.   {Fog} stays");
        set_active(PadFamily::Xbox);
        assert_eq!(hint(text), "Up / Down and Y: choose.   LB: banish.   Menu or A: resume.   {Fog} stays");
        set_active(PadFamily::PlayStation);
        assert_eq!(label(Buttons::X), "Triangle");
        let (text, size) = hint_sized("{X}: aim", 24.0);
        assert_eq!(text, "Triangle: aim");
        assert!((size - 24.0 * 6.0 / 13.0).abs() < 1e-4, "as wide as X: aim at 24: {size}");
        assert_eq!(hint_sized("{L1}: map", 24.0).1, 24.0, "never grows");
        set_active(PadFamily::Nintendo);
    }
}
