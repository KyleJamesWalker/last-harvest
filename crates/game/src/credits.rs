//! The Credits page: who made the game, then the art, font and sounds it ships, then every crate built into it.

use std::sync::OnceLock;

use crate::draw::{Align, Canvas, Color};
use crate::input::{Buttons, Input};
use crate::menu::{BACK, back_button, fit_label, inside, wrapped};
use crate::{DIM, Game, HOT, SCREEN_W, Sfx, WHITE};

const ASSETS: &str = include_str!("../credits/assets.txt");
const CRATES: &str = include_str!("../credits/crates.txt");

/// The buttons either side of Back that turn the page.
const PREV: [f32; 4] = [22.0, 660.0, 300.0, 76.0];
const NEXT: [f32; 4] = [702.0, 660.0, 300.0, 76.0];
const TOP: f32 = 100.0;
const LEFT: f32 = 40.0;
const ROOM: f32 = SCREEN_W - 2.0 * LEFT;
const SECTION: f32 = 36.0;
const HEAD: f32 = 30.0;
const BODY: f32 = 26.0;

const PEOPLE: [(&str, &str); 4] = [
    ("Programming", "Kyle James Walker"),
    ("Zombie Apocalypse Tileset", "Ittai Manero"),
    ("Template", "twinbit-ds"),
    ("Atkinson Hyperlegible font", "Braille Institute of America"),
];

/// One credited work from `credits/assets.txt`.
#[derive(Debug)]
struct Asset {
    kind: &'static str,
    title: &'static str,
    author: &'static str,
    licence: &'static str,
    #[cfg(test)]
    files: Vec<&'static str>,
}

fn assets() -> Vec<Asset> {
    let mut kind = "";
    let mut out = Vec::new();
    for line in ASSETS.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(k) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            kind = k;
            continue;
        }
        let mut cells = line.split('|').map(str::trim);
        let mut next = || cells.next().unwrap_or("");
        out.push(Asset {
            kind,
            title: next(),
            author: next(),
            licence: next(),
            #[cfg(test)]
            files: next().split_whitespace().collect(),
        });
    }
    out
}

/// Each licence in `credits/crates.txt` with the crates under it.
fn crates() -> Vec<(&'static str, Vec<&'static str>)> {
    CRATES
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.rsplit_once(": "))
        .map(|(licence, names)| (licence, names.split(", ").collect()))
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
enum Line {
    Section(String),
    Head(String),
    Body(String),
}

impl Line {
    fn style(&self) -> (&str, f32, Color) {
        match self {
            Line::Section(t) => (t, SECTION, HOT),
            Line::Head(t) => (t, HEAD, WHITE),
            Line::Body(t) => (t, BODY, DIM),
        }
    }

    fn height(&self) -> f32 {
        self.style().1 * 1.3
    }
}

/// Every page after the first, which names the people; each section starts a page and a heading never ends one.
fn pages() -> &'static [Vec<Line>] {
    static PAGES: OnceLock<Vec<Vec<Line>>> = OnceLock::new();
    PAGES.get_or_init(|| paginate(&flow()))
}

fn body(text: &str) -> impl Iterator<Item = Line> {
    wrapped(text, BODY, ROOM * 0.95, usize::MAX)
        .into_iter()
        .map(Line::Body)
}

fn flow() -> Vec<Line> {
    let mut out = vec![Line::Section("Art, font and sounds".into())];
    let mut kind = "";
    for a in assets() {
        if a.kind != kind {
            kind = a.kind;
            out.push(Line::Head(kind.to_string()));
        }
        let by = [a.title, a.author, a.licence]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" - ");
        out.extend(body(&by));
    }
    out.push(Line::Section("Open-source software".into()));
    out.extend(body(
        "Last Harvest is built with these Rust crates, each under the license named above it. \
         The desktop, RG DS and web downloads carry the full texts in THIRD_PARTY_LICENSES.txt.",
    ));
    for (licence, names) in crates() {
        out.push(Line::Head(licence.to_string()));
        out.extend(body(&names.join(", ")));
    }
    out
}

fn paginate(lines: &[Line]) -> Vec<Vec<Line>> {
    let room = BACK[1] - 12.0 - TOP;
    let mut pages: Vec<Vec<Line>> = Vec::new();
    let mut used = room;
    for (i, line) in lines.iter().enumerate() {
        let need = match line {
            Line::Body(_) => line.height(),
            _ => line.height() + lines.get(i + 1).map_or(0.0, Line::height),
        };
        if matches!(line, Line::Section(_)) || used + need > room {
            pages.push(Vec::new());
            used = 0.0;
        }
        used += line.height();
        pages
            .last_mut()
            .expect("a page was started")
            .push(line.clone());
    }
    pages
}

fn page_count() -> usize {
    1 + pages().len()
}

impl Game {
    /// Left, Right, L1, R1, Up, Down, A or the buttons beside Back turn the page; B leaves through the title's arm.
    pub(crate) fn credits_input(&mut self, input: &Input, tapped: Option<[f32; 2]>) {
        let next = input
            .pressed
            .any(Buttons::RIGHT | Buttons::R1 | Buttons::DOWN | Buttons::A)
            || tapped.is_some_and(|p| inside(p, NEXT));
        let prev = input.pressed.any(Buttons::LEFT | Buttons::L1 | Buttons::UP)
            || tapped.is_some_and(|p| inside(p, PREV));
        let by = next as i32 - prev as i32;
        if by != 0 {
            self.credits_page =
                (self.credits_page as i32 + by).rem_euclid(page_count() as i32) as usize;
            self.sounds.push(Sfx::Tick(self.credits_page as u8 * 3));
        }
    }

    pub(crate) fn draw_credits(&self, c: &mut Canvas) {
        let page = self.credits_page.min(page_count() - 1);
        c.label("Credits", SCREEN_W / 2.0, 24.0, 48.0, WHITE, Align::Center);
        c.label(
            format!("{} / {}", page + 1, page_count()),
            SCREEN_W - 24.0,
            36.0,
            26.0,
            DIM,
            Align::Right,
        );
        match page.checked_sub(1) {
            None => {
                for (i, (role, name)) in PEOPLE.iter().enumerate() {
                    let y = 116.0 + i as f32 * 132.0;
                    c.label(*role, SCREEN_W / 2.0, y, 30.0, DIM, Align::Center);
                    c.label(*name, SCREEN_W / 2.0, y + 40.0, 46.0, WHITE, Align::Center);
                }
            }
            Some(i) => {
                let mut y = TOP;
                for line in &pages()[i] {
                    let (text, size, color) = line.style();
                    fit_label(c, text, LEFT, y, size, ROOM, color, Align::Left);
                    y += line.height();
                }
            }
        }
        back_button(c);
        for ([x, y, w, h], text) in [(PREV, "< Previous"), (NEXT, "Next >")] {
            c.rect(x, y, w, h, [1.0, 1.0, 1.0, 0.08]);
            c.label(
                text,
                x + w / 2.0,
                y + h / 2.0 - 18.0,
                32.0,
                WHITE,
                Align::Center,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    }

    #[test]
    fn every_credit_has_a_title_and_author_and_every_crate_a_licence() {
        let assets = assets();
        assert!(assets.iter().any(|a| a.title == "Atkinson Hyperlegible"));
        for a in &assets {
            assert!(
                !a.kind.is_empty() && !a.title.is_empty() && !a.author.is_empty(),
                "{a:?}"
            );
        }
        let crates = crates();
        assert!(crates.len() > 5, "{crates:?}");
        for (licence, names) in &crates {
            assert!(!licence.is_empty() && names.iter().all(|n| !n.is_empty()));
        }
    }

    #[test]
    fn the_font_credit_matches_its_licence_file() {
        let ofl = std::fs::read_to_string(root().join("assets/fonts/OFL.txt")).unwrap();
        let font = assets().into_iter().find(|a| a.kind == "Font").unwrap();
        assert!(ofl.contains(font.author), "{}", font.author);
        assert!(ofl.contains("Open Font License, Version 1.1"));
        assert_eq!(font.licence, "SIL Open Font License 1.1");
    }

    /// Each row of the private README's Sources table whose source names an author must be credited under that
    /// author and licence, and every credited sound must have a row.
    #[test]
    fn the_sound_credits_match_the_recorded_sources() {
        let Ok(readme) = std::fs::read_to_string(root().join("assets/private/README.md")) else {
            eprintln!("no assets/private: the sound credits go unchecked");
            return;
        };
        let assets = assets();
        let sounds: Vec<_> = assets.iter().filter(|a| a.kind == "Sounds").collect();
        let mut recorded = BTreeSet::new();
        for row in readme.lines().filter(|l| l.starts_with("| `sounds/")) {
            let cells: Vec<&str> = row.split('|').map(str::trim).collect();
            let (files, source, licence) = (cells[1], cells[2], cells[3]);
            let licence = licence
                .strip_prefix('[')
                .and_then(|l| l.split_once(']'))
                .map_or(licence, |(name, _)| name);
            let author = source
                .split_once(" by ")
                .map(|(_, rest)| rest.split(',').next().unwrap().trim());
            for file in files.split('`').filter_map(|f| f.strip_prefix("sounds/")) {
                let name = file.rsplit_once('.').map_or(file, |(n, _)| n);
                recorded.insert(name);
                let Some(author) = author else { continue };
                let credit = sounds.iter().find(|a| a.files.contains(&name));
                assert!(
                    credit.is_some_and(|a| a.author == author && a.licence == licence),
                    "{name}: recorded as by {author} under {licence}, credited as {credit:?}"
                );
            }
        }
        for a in &sounds {
            for f in &a.files {
                assert!(
                    recorded.contains(f),
                    "{f} is credited but has no recorded source"
                );
            }
        }
    }

    #[test]
    fn every_crate_and_credit_is_on_some_page_and_fits_above_back() {
        let shown: String = pages()
            .iter()
            .flatten()
            .map(|l| l.style().0.to_string() + " ")
            .collect();
        for (licence, names) in crates() {
            assert!(shown.contains(licence), "{licence}");
            for n in names {
                assert!(shown.split([' ', ',']).any(|w| w == n), "{n} is on no page");
            }
        }
        for a in assets() {
            assert!(shown.contains(a.title) && shown.contains(a.author), "{a:?}");
        }
        for page in pages() {
            let h: f32 = page.iter().map(Line::height).sum();
            assert!(TOP + h <= BACK[1], "{page:?}");
            assert!(!matches!(
                page.last(),
                Some(Line::Head(_) | Line::Section(_))
            ));
        }
    }

    #[test]
    fn the_pad_and_a_tap_turn_the_pages_round_and_b_leaves() {
        let mut game = Game::new(1);
        game.page = crate::menu::Page::Credits;
        let n = page_count();
        assert!(n > 2);
        let step = |game: &mut Game, input: Input| game.update(&input, 0.016);
        for _ in 0..n {
            step(
                &mut game,
                Input {
                    pressed: Buttons::RIGHT,
                    ..Default::default()
                },
            );
            game.draw();
        }
        assert_eq!(game.credits_page, 0, "a full turn comes back to the people");
        step(
            &mut game,
            Input {
                pressed: Buttons::L1,
                ..Default::default()
            },
        );
        assert_eq!(game.credits_page, n - 1, "back from the first is the last");
        let tap = |p: [f32; 4]| Input {
            touch: Some([p[0] + 10.0, p[1] + 10.0]),
            touch_started: true,
            ..Default::default()
        };
        step(&mut game, tap(NEXT));
        assert_eq!(game.credits_page, 0);
        step(&mut game, tap(NEXT));
        step(&mut game, tap(PREV));
        assert_eq!(game.credits_page, 0);
        step(&mut game, tap(NEXT));
        step(&mut game, tap(BACK));
        assert_eq!(game.page, crate::menu::Page::Main);
        assert_eq!(game.credits_page, 0, "it opens on the first page next time");
    }
}
