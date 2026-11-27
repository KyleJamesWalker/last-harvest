//! What the player's own pad or keyboard calls each game button, so hints name the button under their thumb.
//! The game's buttons are the RG DS's, and every platform maps a pad's face buttons to them by position.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

use crate::Buttons;
use crate::controls::Controls;

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
        if let Some(f) = vendor
            .and_then(Self::from_vendor)
            .or_else(|| name_vendor(name).and_then(Self::from_vendor))
        {
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

    /// What the Controls page calls the family.
    pub fn name(self) -> &'static str {
        match self {
            Self::Nintendo => "Pad",
            Self::Xbox => "Xbox",
            Self::PlayStation => "PlayStation",
            Self::Keyboard => "Keys",
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
            PlayStation => [
                "Circle", "Cross", "Triangle", "Square", "L1", "R1", "L2", "R2", "Share", "Options",
            ][i],
            Keyboard => [
                "E", "R", "F", "Q", "Tab", "Space", "G", "F12", "F3", "Enter",
            ][i],
        }
    }
}

fn name_vendor(name: &str) -> Option<u16> {
    let lower = name.to_ascii_lowercase();
    let hex = |s: &str| {
        s.get(..4)
            .filter(|h| h.bytes().all(|c| c.is_ascii_hexdigit()))
            .and_then(|h| u16::from_str_radix(h, 16).ok())
    };
    if let Some(at) = lower.find("vendor: ") {
        return hex(&lower[at + 8..]);
    }
    (lower.as_bytes().get(4) == Some(&b'-'))
        .then(|| hex(&lower))
        .flatten()
}

thread_local! {
    static ACTIVE: Cell<PadFamily> = const { Cell::new(PadFamily::Nintendo) };
    static PAD: Cell<PadFamily> = const { Cell::new(PadFamily::Nintendo) };
    static CONTROLS: Cell<Controls> = Cell::new(Controls::default());
    static BY_NAME: Cell<bool> = const { Cell::new(false) };
    static MOUSE: Cell<bool> = const { Cell::new(false) };
    static PLATFORM: RefCell<[Option<Rc<str>>; 10]> = const { RefCell::new([const { None }; 10]) };
}

/// A button's label: the game's own, or the one the platform's input layer gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Label {
    Own(&'static str),
    Platform(Rc<str>),
}

impl Deref for Label {
    type Target = str;

    fn deref(&self) -> &str {
        match self {
            Self::Own(s) => s,
            Self::Platform(s) => s,
        }
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self)
    }
}

impl PartialEq<&str> for Label {
    fn eq(&self, other: &&str) -> bool {
        **self == **other
    }
}

/// Replaces the platform's labels, each keyed by the pad button it names; an empty one keeps the game's own.
/// Returns whether any changed.
pub(crate) fn set_platform_labels<'a>(
    labels: impl IntoIterator<Item = (Buttons, &'a str)>,
) -> bool {
    let mut new: [Option<&str>; 10] = [None; 10];
    for (b, text) in labels {
        if let Some(i) = NAMES.iter().position(|&(_, n)| n == b)
            && !text.is_empty()
        {
            new[i] = Some(text);
        }
    }
    PLATFORM.with_borrow_mut(|held| {
        if held.iter().map(Option::as_deref).eq(new) {
            return false;
        }
        *held = new.map(|l| l.map(Rc::from));
        true
    })
}

fn platform(b: Buttons) -> Option<Rc<str>> {
    let i = NAMES.iter().position(|&(_, n)| n == b)?;
    PLATFORM.with_borrow(|held| held[i].clone())
}

/// The platform maps a pad's face buttons by the names printed on them rather than by position, as Android does.
pub(crate) fn set_by_name(on: bool) {
    BY_NAME.set(on);
}

/// The platform's label for pad button `b`, else what `family` prints on it. The platform never relabels keys.
fn shown(family: PadFamily, b: Buttons) -> Label {
    if family != PadFamily::Keyboard
        && let Some(l) = platform(b)
    {
        return Label::Platform(l);
    }
    Label::Own(printed(family, b))
}

/// What `family` prints on the button that sends game button `b`, by position or, with `set_by_name`, by name.
fn printed(family: PadFamily, b: Buttons) -> &'static str {
    if family == PadFamily::Keyboard && MOUSE.get() {
        match b {
            Buttons::R1 => return "Click",
            Buttons::L2 => return "Right-click",
            _ => {}
        }
    }
    if !BY_NAME.get() {
        return family.label(b);
    }
    let Some(i) = NAMES.iter().position(|&(_, n)| n == b) else {
        return "";
    };
    match family {
        PadFamily::Xbox => ["A", "B", "X", "Y", "LB", "RB", "LT", "RT", "View", "Menu"][i],
        PadFamily::PlayStation => [
            "Cross", "Circle", "Square", "Triangle", "L1", "R1", "L2", "R2", "Share", "Options",
        ][i],
        PadFamily::Nintendo | PadFamily::Keyboard => family.label(b),
    }
}

/// A mouse button has been used, so keyboard hints name the mouse for the quick throw and grenade aim.
pub(crate) fn set_mouse(on: bool) {
    MOUSE.set(on);
}

pub(crate) fn set_active(family: PadFamily) {
    ACTIVE.set(family);
    if family != PadFamily::Keyboard {
        PAD.set(family);
    }
}

pub(crate) fn set_controls(controls: Controls) {
    CONTROLS.set(controls);
}

pub fn active() -> PadFamily {
    ACTIVE.get()
}

/// The pad family last used, whatever keys were pressed since.
pub fn last_pad() -> PadFamily {
    PAD.get()
}

/// What the active pad calls the button that sends game button `b`.
pub fn label(b: Buttons) -> Label {
    label_as(active(), b)
}

/// What `family` calls the button that sends game button `b`.
pub fn label_as(family: PadFamily, b: Buttons) -> Label {
    shown(family, CONTROLS.get().physical(b))
}

/// A hint with each `{A}`, `{L1}`, `{Start}` and so on replaced by the active pad's label for that game button.
pub fn hint(text: &str) -> String {
    fill(text, |b| shown(active(), CONTROLS.get().physical(b)))
}

fn fill(text: &str, name: impl Fn(Buttons) -> Label) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}').and_then(|close| {
            NAMES
                .iter()
                .find(|(n, _)| *n == &after[..close])
                .map(|&(_, b)| (close, b))
        }) {
            Some((close, b)) => {
                out.push_str(&name(b));
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
    let (shown, own) = (
        hint(text),
        fill(text, |b| Label::Own(PadFamily::Nintendo.label(b))),
    );
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
        assert_eq!(
            names(PadFamily::Xbox),
            ["B", "A", "Y", "X"],
            "right, bottom, top, left"
        );
        assert_eq!(
            names(PadFamily::PlayStation),
            ["Circle", "Cross", "Triangle", "Square"]
        );
        assert_eq!(names(PadFamily::Keyboard), ["E", "R", "F", "Q"]);
        let rest = [
            Buttons::L1,
            Buttons::R1,
            Buttons::L2,
            Buttons::R2,
            Buttons::SELECT,
            Buttons::START,
        ];
        assert_eq!(
            rest.map(|b| PadFamily::Xbox.label(b)),
            ["LB", "RB", "LT", "RT", "View", "Menu"]
        );
        assert_eq!(
            rest.map(|b| PadFamily::Nintendo.label(b)),
            ["L1", "R1", "L2", "R2", "Select", "Start"]
        );
        assert_eq!(
            rest.map(|b| PadFamily::PlayStation.label(b)),
            ["L1", "R1", "L2", "R2", "Share", "Options"]
        );
    }

    #[test]
    fn by_name_each_family_names_the_button_printed_with_the_game_buttons_name() {
        set_by_name(true);
        let all = [
            Buttons::A,
            Buttons::B,
            Buttons::X,
            Buttons::Y,
            Buttons::L1,
            Buttons::SELECT,
            Buttons::START,
        ];
        assert_eq!(
            all.map(|b| label_as(PadFamily::Xbox, b)),
            ["A", "B", "X", "Y", "LB", "View", "Menu"]
        );
        assert_eq!(
            all.map(|b| label_as(PadFamily::PlayStation, b)),
            [
                "Cross", "Circle", "Square", "Triangle", "L1", "Share", "Options"
            ]
        );
        assert_eq!(
            all.map(|b| label_as(PadFamily::Nintendo, b)),
            ["A", "B", "X", "Y", "L1", "Select", "Start"]
        );
        set_active(PadFamily::PlayStation);
        assert_eq!(hint("{A}: pick"), "Cross: pick");
        set_by_name(false);
        assert_eq!(hint("{A}: pick"), "Circle: pick");
    }

    #[test]
    fn a_pad_is_known_by_its_vendor_or_its_name() {
        assert_eq!(PadFamily::detect(Some(0x045E), ""), PadFamily::Xbox);
        assert_eq!(
            PadFamily::detect(Some(0x054C), "Wireless Controller"),
            PadFamily::PlayStation
        );
        assert_eq!(
            PadFamily::detect(Some(0x057E), "Pro Controller"),
            PadFamily::Nintendo
        );
        assert_eq!(
            PadFamily::detect(Some(0x2DC8), "8BitDo Pro 2"),
            PadFamily::Nintendo
        );
        assert_eq!(
            PadFamily::detect(
                None,
                "Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 09cc)"
            ),
            PadFamily::PlayStation
        );
        assert_eq!(
            PadFamily::detect(None, "057e-2009-Pro Controller"),
            PadFamily::Nintendo
        );
        assert_eq!(
            PadFamily::detect(None, "Xbox 360 Controller (XInput STANDARD GAMEPAD)"),
            PadFamily::Xbox
        );
        assert_eq!(
            PadFamily::detect(None, "DualSense Wireless Controller"),
            PadFamily::PlayStation
        );
        assert_eq!(
            PadFamily::detect(Some(0x1234), "Some Pad"),
            PadFamily::Xbox,
            "an unknown pad is taken for XInput"
        );
    }

    #[test]
    fn a_hint_names_the_active_pads_buttons() {
        let text =
            "Up / Down and {X}: choose.   {L1}: banish.   {Start} or {B}: resume.   {Fog} stays";
        assert_eq!(
            hint(text),
            "Up / Down and X: choose.   L1: banish.   Start or B: resume.   {Fog} stays"
        );
        set_active(PadFamily::Xbox);
        assert_eq!(
            hint(text),
            "Up / Down and Y: choose.   LB: banish.   Menu or A: resume.   {Fog} stays"
        );
        set_active(PadFamily::PlayStation);
        assert_eq!(label(Buttons::X), "Triangle");
        let (text, size) = hint_sized("{X}: aim", 24.0);
        assert_eq!(text, "Triangle: aim");
        assert!(
            (size - 24.0 * 6.0 / 13.0).abs() < 1e-4,
            "as wide as X: aim at 24: {size}"
        );
        assert_eq!(hint_sized("{L1}: map", 24.0).1, 24.0, "never grows");
        set_active(PadFamily::Nintendo);
    }

    #[test]
    fn the_platforms_labels_name_the_pad_buttons_but_never_the_keys() {
        set_active(PadFamily::Xbox);
        assert!(set_platform_labels([
            (Buttons::A, "Glyph A"),
            (Buttons::L1, "")
        ]));
        assert!(!set_platform_labels([(Buttons::A, "Glyph A")]), "unchanged");
        assert_eq!(label(Buttons::A), "Glyph A");
        assert!(matches!(label(Buttons::A), Label::Platform(_)));
        assert_eq!(
            label(Buttons::L1),
            "LB",
            "an empty label keeps the family's"
        );
        set_by_name(true);
        assert_eq!(label_as(PadFamily::PlayStation, Buttons::A), "Glyph A");
        set_by_name(false);
        assert_eq!(label_as(PadFamily::Keyboard, Buttons::A), "E");
        set_mouse(true);
        assert_eq!(label_as(PadFamily::Keyboard, Buttons::R1), "Click");
        set_mouse(false);
        assert_eq!(hint("{A}: pick   {B}: back"), "Glyph A: pick   A: back");

        let mut swapped = Controls::default();
        swapped.bind(Buttons::B, Buttons::A);
        set_controls(swapped);
        assert_eq!(
            label(Buttons::B),
            "Glyph A",
            "keyed by the pad button, after the remap"
        );
        set_controls(Controls::default());

        let (text, size) = hint_sized("{A}: aim", 24.0);
        assert_eq!(text, "Glyph A: aim");
        assert!(
            (size - 24.0 * 6.0 / 12.0).abs() < 1e-4,
            "as wide as A: aim at 24: {size}"
        );

        assert!(set_platform_labels([]));
        assert_eq!(label(Buttons::A), "B", "cleared");
        set_active(PadFamily::Nintendo);
    }
}
