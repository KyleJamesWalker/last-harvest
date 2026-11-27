//! Hints name each button by the platform's label for it, and by the game's own where the platform gives none.

use last_harvest::platform::{self, Button, NoOp, Recorder};
use last_harvest_game::{Buttons, Game, PadFamily, pad};

const FAMILIES: [PadFamily; 4] = [
    PadFamily::Nintendo,
    PadFamily::Xbox,
    PadFamily::PlayStation,
    PadFamily::Keyboard,
];

const ALL: [Buttons; 10] = [
    Buttons::A,
    Buttons::B,
    Buttons::X,
    Buttons::Y,
    Buttons::L1,
    Buttons::R1,
    Buttons::L2,
    Buttons::R2,
    Buttons::SELECT,
    Buttons::START,
];

fn assert_own_labels(game: &mut Game) {
    for family in FAMILIES {
        game.set_pad_family(family);
        for b in ALL {
            assert_eq!(&*pad::label(b), family.label(b), "{family:?} {b:?}");
            assert_eq!(&*pad::label_as(family, b), family.label(b));
        }
    }
    game.set_pad_family(PadFamily::Xbox);
    assert_eq!(
        pad::hint("{A}: pick   {Start}: menu"),
        "B: pick   Menu: menu"
    );
    game.set_pad_family(PadFamily::Nintendo);
}

#[test]
fn with_the_no_op_backend_every_label_is_the_games_own() {
    let mut game = Game::new(1);
    let mut backend = platform::backend();
    platform::serve(&mut game, backend.as_mut());
    platform::glyphs(&mut game, &NoOp);
    assert_own_labels(&mut game);
}

#[test]
fn the_recorders_labels_show_in_hints_until_it_gives_none() {
    let mut game = Game::new(1);
    let seen = Recorder::new();
    seen.set_glyph(Button::A, "Glyph A");
    seen.set_glyph(Button::Start, "Glyph Menu");
    let mut backend: Box<dyn platform::Platform> = Box::new(seen.clone());
    platform::serve(&mut game, backend.as_mut());
    game.set_pad_family(PadFamily::Xbox);
    assert_eq!(
        pad::hint("{A}: pick   {B}: back   {Start}: menu"),
        "Glyph A: pick   A: back   Glyph Menu: menu"
    );
    assert_eq!(&*pad::label(Buttons::A), "Glyph A");
    assert_eq!(&*pad::label_as(PadFamily::Keyboard, Buttons::A), "E");
    assert!(seen.calls().is_empty(), "asking for labels is no call");

    seen.set_glyph(Button::B, "Glyph B");
    platform::serve(&mut game, backend.as_mut());
    assert_eq!(&*pad::label(Buttons::B), "A", "not asked every frame");
    for _ in 0..29 {
        platform::serve(&mut game, backend.as_mut());
    }
    assert_eq!(&*pad::label(Buttons::B), "Glyph B", "asked again in time");

    platform::glyphs(&mut game, &NoOp);
    assert_own_labels(&mut game);
}
