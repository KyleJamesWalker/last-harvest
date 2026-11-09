//! Requests from a local agent to read and redraw the open sprite while its artist watches. Each redraw is one
//! undo step and nothing saves: the artist keeps or drops it.

use super::{Action, CARVE_EXTRAS, Editor, FOOTPRINT, Key, NORTH, ROOF, TOP, WEST, optional, optional_mut};
use crate::sprite::{self, Frame};

/// A reply: status, content type and body.
pub type Reply = (u16, &'static str, Vec<u8>);

fn text(status: u16, body: impl Into<String>) -> Reply {
    (status, "text/plain; charset=utf-8", body.into().into_bytes())
}

/// A picture of the open sprite: one of its frames, or a carving view.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Surface {
    Frame(usize),
    View(usize),
}

const VIEWS: [(&str, usize); 7] = [("south", 0), ("east", 1), ("footprint", FOOTPRINT), ("top", TOP), ("roof", ROOF), ("north", NORTH), ("west", WEST)];

fn surface(name: &str) -> Option<Surface> {
    let name = name.strip_suffix(".png").unwrap_or(name);
    name.parse().ok().map(Surface::Frame).or_else(|| VIEWS.iter().find(|(n, _)| *n == name).map(|&(_, v)| Surface::View(v)))
}

/// A key by the name the remote door gives it: a single character, or space, slash, enter, escape, backspace, left, right, up,
/// down, undo, redo, save, copy, cut or paste.
fn key_named(name: &str) -> Option<Key> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Some(Key::Char(c));
    }
    Some(match name {
        "space" => Key::Char(' '),
        "slash" => Key::Char('/'),
        "enter" => Key::Enter,
        "escape" => Key::Escape,
        "backspace" | "delete" => Key::Backspace,
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "undo" => Key::Undo,
        "redo" => Key::Redo,
        "save" => Key::Save,
        "copy" => Key::Copy,
        "cut" => Key::Cut,
        "paste" => Key::Paste,
        _ => return None,
    })
}

fn surface_name(s: Surface) -> String {
    match s {
        Surface::Frame(i) => i.to_string(),
        Surface::View(v) => VIEWS.iter().find(|(_, x)| *x == v).map_or("?", |(n, _)| n).to_string(),
    }
}

/// A number the request's query gives `name`, as `?x=3` does.
fn path_query_num(path: &str, name: &str) -> Option<f32> {
    path.split_once('?')?.1.split('&').find_map(|p| p.strip_prefix(name)?.strip_prefix('=')?.parse().ok())
}

/// Whether the request's query names `flag`, as `?shift` does.
fn path_query_has(path: &str, flag: &str) -> bool {
    path.split_once('?').is_some_and(|(_, q)| q.split('&').any(|p| p == flag))
}

impl Editor {
    fn surface(&self, s: Surface) -> Option<&Frame> {
        match s {
            Surface::Frame(i) => self.sprite.frames.get(i),
            Surface::View(v) if v < 2 => self.sprite.views.as_ref().map(|views| &views[v]),
            Surface::View(v) => optional(&self.sprite, v).as_ref(),
        }
    }

    fn surface_mut(&mut self, s: Surface) -> Option<&mut Frame> {
        match s {
            Surface::Frame(i) => self.sprite.frames.get_mut(i),
            Surface::View(v) if v < 2 => self.sprite.views.as_mut().map(|views| &mut views[v]),
            Surface::View(v) => optional_mut(&mut self.sprite, v).as_mut(),
        }
    }

    /// Every picture the open sprite has, frames first.
    fn surfaces_listed(&self) -> Vec<Surface> {
        let frames = (0..self.sprite.frames.len()).map(Surface::Frame);
        frames.chain(VIEWS.iter().map(|&(_, v)| Surface::View(v))).filter(|&s| self.surface(s).is_some()).collect()
    }

    fn showing(&self) -> Surface {
        match self.view {
            Some(v) if self.surface(Surface::View(v)).is_some() => Surface::View(v),
            _ => Surface::Frame(self.frame),
        }
    }

    /// Why the canvas can't take a redraw now, if it can't.
    fn remote_busy(&self) -> Option<&'static str> {
        if self.none_open() || self.map.is_some() || self.data.is_some() {
            Some("no sprite is open on the canvas")
        } else if self.browse.is_some() {
            Some("the artist is browsing history")
        } else if self.drawing.is_some() || self.stroke.is_some() {
            Some("the artist is mid-stroke; try again")
        } else if self.selection.as_ref().is_some_and(|s| s.float.is_some()) {
            Some("a selection is floating; the artist places or drops it first")
        } else if self.stats || self.coats_page || self.pending.is_some() || self.ask.is_some() {
            Some("a page or question is over the canvas")
        } else {
            None
        }
    }

    fn remote_state(&self) -> String {
        let list = self.surfaces_listed().into_iter().map(|s| {
            let f = self.surface(s).expect("listed");
            format!("{}={}x{}", surface_name(s), f.w, f.h)
        });
        format!(
            "name {}\nunsaved {}\ndetail {}\nshowing {}\nsurfaces {}\nbusy {}\nstatus {}\ntext {}\n",
            self.sprite.name,
            self.unsaved(),
            self.sprite.detail,
            surface_name(self.showing()),
            list.collect::<Vec<_>>().join(" "),
            self.remote_busy().unwrap_or("no"),
            self.status.as_ref().map_or("", |(t, _)| t.as_str()),
            if self.wants_text() { self.text().unwrap_or("") } else { "-" },
        )
    }

    fn show(&mut self, s: Surface) {
        match s {
            Surface::Frame(i) => (self.view, self.frame) = (None, i),
            Surface::View(v) => self.view = Some(v),
        }
    }

    fn remote_put(&mut self, s: Surface, body: &[u8]) -> Reply {
        let new = match sprite::decode_png(body) {
            Ok(f) => f,
            Err(e) => return text(400, format!("not a PNG: {e}")),
        };
        let Some(old) = self.surface(s) else { return text(404, "the open sprite has no such picture") };
        if (old.w, old.h) != (new.w, new.h) {
            return text(400, format!("{} is {}x{}, not {}x{}", surface_name(s), old.w, old.h, new.w, new.h));
        }
        let changed = old.px.iter().zip(&new.px).filter(|(a, b)| a != b).count();
        if changed > 0 {
            self.remember();
            *self.surface_mut(s).expect("checked") = new;
            self.say(format!("An agent redrew {} ({changed} px); Cmd+Z takes it back", surface_name(s)));
        }
        self.show(s);
        text(200, format!("{changed} px changed"))
    }

    /// Every button on show, one a line: its index, label, action and whether it's lit.
    fn remote_buttons(&self) -> String {
        self.buttons().iter().enumerate().map(|(i, b)| format!("{i}\t{}\t{:?}\t{}\n", b.label, b.action, b.on as u8)).collect()
    }

    /// Presses the button named by its index, its action as `GET /buttons` writes it, or its label when only one has it.
    fn remote_press(&mut self, which: &str, shift: bool) -> Reply {
        let buttons = self.buttons();
        let which = which.trim();
        let found: Vec<_> = match which.parse::<usize>() {
            Ok(i) => buttons.get(i).into_iter().collect(),
            Err(_) => {
                let by_action: Vec<_> = buttons.iter().filter(|b| format!("{:?}", b.action) == which).collect();
                if by_action.is_empty() { buttons.iter().filter(|b| b.label.eq_ignore_ascii_case(which)).collect() } else { by_action }
            }
        };
        match found.as_slice() {
            [b] => {
                let label = b.label.clone();
                self.act(b.action, shift);
                text(200, format!("pressed {label}\n{}", self.remote_state()))
            }
            [] => text(404, "no such button on show; GET /buttons lists them"),
            many => text(409, format!("{} buttons match; name one by index or action:\n{}", many.len(), many.iter().map(|b| format!("{:?}", b.action)).collect::<Vec<_>>().join("\n"))),
        }
    }

    /// The window point at the middle of canvas pixel (x, y) of the picture on show.
    fn canvas_point(&self, x: f32, y: f32) -> [f32; 2] {
        let (o, zoom) = self.grid();
        [o[0] + (x + 0.5) * zoom, o[1] + (y + 0.5) * zoom]
    }

    /// Clicks canvas pixel (x, y) with the tool in hand, or drags from it to (x1, y1), as the mouse would.
    fn remote_pointer(&mut self, path: &str, drag: bool) -> Reply {
        if self.none_open() || self.map.is_some() || self.data.is_some() {
            return text(409, "clicks land on a sprite's canvas; open a sprite first");
        }
        let (Some(x), Some(y)) = (path_query_num(path, "x"), path_query_num(path, "y")) else { return text(400, "give x and y in canvas pixels") };
        let (right, shift) = (path_query_has(path, "right"), path_query_has(path, "shift"));
        self.mouse_down(self.canvas_point(x, y), right, shift);
        if drag {
            let (Some(x1), Some(y1)) = (path_query_num(path, "x1"), path_query_num(path, "y1")) else {
                self.mouse_up();
                return text(400, "a drag gives x1 and y1 too");
            };
            let steps = ((x1 - x).abs().max((y1 - y).abs()) as usize).max(1);
            for i in 1..=steps {
                let f = i as f32 / steps as f32;
                self.drag(self.canvas_point(x + (x1 - x) * f, y + (y1 - y) * f));
            }
        }
        self.mouse_up();
        text(200, self.remote_state())
    }

    /// Answers one request from the remote door: read the open sprite, redraw one of its pictures, show one, or
    /// open another sprite when nothing is unsaved.
    pub fn remote(&mut self, method: &str, path: &str, body: &[u8]) -> Reply {
        let path_full = path;
        let path = path.split(['?', '#']).next().unwrap_or("");
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        match (method, parts.as_slice()) {
            ("GET", ["state"]) => text(200, self.remote_state()),
            ("GET", ["surface", s]) => match surface(s).and_then(|s| self.surface(s)) {
                Some(f) => (200, "image/png", sprite::encode_png(f)),
                None => text(404, "the open sprite has no such picture"),
            },
            ("PUT", ["surface", s]) => match (self.remote_busy(), surface(s)) {
                (Some(why), _) => text(409, why),
                (None, Some(s)) => self.remote_put(s, body),
                (None, None) => text(404, "a picture is a frame number, or south, east, footprint, top, roof, north or west"),
            },
            ("POST", ["show", s]) => match (self.remote_busy(), surface(s).filter(|&s| self.surface(s).is_some())) {
                (Some(why), _) => text(409, why),
                (None, Some(s)) => {
                    self.show(s);
                    text(200, "showing")
                }
                (None, None) => text(404, "the open sprite has no such picture"),
            },
            ("POST", ["add", s]) => match (self.remote_busy(), surface(s)) {
                (Some(why), _) => text(409, why),
                (None, Some(Surface::View(v))) if CARVE_EXTRAS.contains(&v) && self.sprite.views.is_some() => {
                    if self.surface(Surface::View(v)).is_none() {
                        self.act(Action::AddView(v), false);
                    }
                    text(200, self.remote_state())
                }
                (None, _) => text(404, "a carved sprite adds footprint, roof, north or west"),
            },
            ("GET", ["buttons"]) => text(200, self.remote_buttons()),
            ("POST", ["press"]) => match (self.drawing.is_some() || self.stroke.is_some(), std::str::from_utf8(body)) {
                (true, _) => text(409, "the artist is mid-stroke; try again"),
                (false, Ok(which)) => self.remote_press(which, path_query_has(path_full, "shift")),
                (false, Err(_)) => text(400, "name a button as text"),
            },
            ("POST", ["key", name]) => match (self.drawing.is_some() || self.stroke.is_some(), key_named(name)) {
                (true, _) => text(409, "the artist is mid-stroke; try again"),
                (false, Some(k)) => {
                    self.nudge = if path_query_has(path_full, "shift") { 8 } else { 1 };
                    self.key(k);
                    text(200, self.remote_state())
                }
                (false, None) => text(404, "a key is one character, or space, slash, enter, escape, backspace, left, right, up, down, undo, redo, save, copy, cut or paste"),
            },
            ("POST", ["click"]) => self.remote_pointer(path_full, false),
            ("POST", ["drag"]) => self.remote_pointer(path_full, true),
            ("POST", ["text"]) => match std::str::from_utf8(body) {
                Ok(t) if self.wants_text() => {
                    self.set_text(t);
                    text(200, self.remote_state())
                }
                Ok(_) => text(409, "nothing is waiting for text; press the button that asks for it first"),
                Err(_) => text(400, "not text"),
            },
            ("POST", ["open", name]) if sprite::valid_name(name) => {
                if let Some(why) = self.remote_busy().filter(|_| !self.none_open() && self.map.is_none() && self.data.is_none()) {
                    return text(409, why);
                }
                if self.unsaved() {
                    return text(409, "the open sprite has unsaved changes; the artist saves or drops them first");
                }
                self.switch_to(name.to_string());
                if self.sprite.name == *name { text(200, self.remote_state()) } else { text(404, "no such sprite") }
            }
            _ => text(404, "no such call"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::editor;
    use super::*;

    fn png_of(f: &Frame) -> Vec<u8> {
        sprite::encode_png(f)
    }

    #[test]
    fn a_remote_redraw_is_one_undo_step_and_never_saves() {
        let mut e = editor();
        let before = e.sprite.frames[0].clone();
        let mut f = before.clone();
        f.px[0] = [0xE8, 0xD2, 0x7A, 255];
        let (status, _, body) = e.remote("PUT", "/surface/0.png", &png_of(&f));
        assert_eq!((status, String::from_utf8(body).unwrap()), (200, "1 px changed".into()));
        assert_eq!(e.sprite.frames[0], f);
        assert!(e.unsaved());
        e.key(crate::editor::Key::Undo);
        assert_eq!(e.sprite.frames[0], before, "Cmd+Z takes the agent's redraw back");
    }

    #[test]
    fn a_remote_redraw_keeps_the_pictures_size_and_waits_for_the_artist() {
        let mut e = editor();
        let f = e.sprite.frames[0].clone();
        let wrong = Frame { w: f.w + 1, h: f.h, px: vec![[0; 4]; (f.w + 1) * f.h] };
        assert_eq!(e.remote("PUT", "/surface/0", &png_of(&wrong)).0, 400);
        assert_eq!(e.remote("PUT", "/surface/south", &png_of(&f)).0, 404, "BARREL isn't carved");
        assert_eq!(e.remote("PUT", "/surface/0", b"nope").0, 400);
        e.selection = Some(super::super::Selection { x: 0, y: 0, w: f.w, h: f.h, float: Some(f.clone()) });
        assert_eq!(e.remote("PUT", "/surface/0", &png_of(&f)).0, 409, "a floating selection is the artist's");
    }

    #[test]
    fn the_state_lists_every_picture_and_reads_back_as_png() {
        let mut e = editor();
        let state = String::from_utf8(e.remote("GET", "/state", &[]).2).unwrap();
        let f = &e.sprite.frames[0];
        assert!(state.contains(&format!("surfaces 0={}x{}", f.w, f.h)) && state.contains("showing 0"), "{state}");
        let (status, kind, png) = e.remote("GET", "/surface/0.png", &[]);
        assert_eq!((status, kind), (200, "image/png"));
        assert_eq!(&sprite::decode_png(&png).unwrap(), &e.sprite.frames[0]);
        assert_eq!(e.remote("GET", "/surface/9", &[]).0, 404);
        assert_eq!(e.remote("DELETE", "/state", &[]).0, 404);
    }

    #[test]
    fn a_carved_sprite_adds_its_optional_views_as_the_tiles_do() {
        let mut e = editor();
        assert_eq!(e.remote("POST", "/add/north", &[]).0, 404, "BARREL isn't carved");
        let f = e.sprite.frames[0].clone();
        e.sprite.views = Some(sprite::block_views(&f, 6));
        let state = String::from_utf8(e.remote("POST", "/add/north", &[]).2).unwrap();
        let south = e.sprite.views.as_ref().unwrap()[0].clone();
        assert_eq!(e.sprite.north.as_ref(), Some(&south.mirrored()), "it starts as the south view seen from behind");
        assert!(state.contains("north=") && state.contains("showing north"), "{state}");
        e.key(crate::editor::Key::Undo);
        assert_eq!(e.sprite.north, None, "one undo step");
        assert_eq!(e.remote("POST", "/add/3", &[]).0, 404);
    }

    #[test]
    fn every_button_on_show_can_be_listed_and_pressed() {
        let mut e = editor();
        let list = String::from_utf8(e.remote("GET", "/buttons", &[]).2).unwrap();
        assert!(list.lines().any(|l| l.contains("\tPlay\t")), "{list}");
        let playing = e.playing;
        assert_eq!(e.remote("POST", "/press", b"Play").0, 200);
        assert_ne!(e.playing, playing, "pressing Play by its label plays");
        let index = list.lines().find(|l| l.contains("\tPlay\t")).unwrap().split('\t').next().unwrap().to_string();
        assert_eq!(e.remote("POST", "/press", index.as_bytes()).0, 200);
        assert_eq!(e.playing, playing, "and by its index");
        assert_eq!(e.remote("POST", "/press", b"No Such Button").0, 404);
        assert_eq!(e.remote("POST", "/press", b"-").0, 409, "a label more than one button has is named by action instead");
    }

    #[test]
    fn keys_and_text_reach_the_editor_as_the_window_sends_them() {
        let mut e = editor();
        let before = e.sprite.frames[0].clone();
        let mut f = before.clone();
        f.px[0] = [0, 0, 0, 255];
        e.remote("PUT", "/surface/0", &png_of(&f));
        assert_eq!(e.remote("POST", "/key/undo", &[]).0, 200);
        assert_eq!(e.sprite.frames[0], before);
        assert_eq!(e.remote("POST", "/key/nope", &[]).0, 404);
        assert_eq!(e.remote("POST", "/text", b"GRASS").0, 409, "nothing asked for text yet");
        assert_eq!(e.remote("POST", "/key/slash", &[]).0, 200);
        let state = String::from_utf8(e.remote("POST", "/text", b"GRASS").2).unwrap();
        assert!(state.contains("text GRASS"), "/ starts the list filter, and text fills it: {state}");
    }

    #[test]
    fn clicks_and_drags_land_on_canvas_pixels_with_the_tool_in_hand() {
        let mut e = editor();
        e.color = [0xE8, 0xD2, 0x7A, 255];
        e.tool = super::super::Tool::Pencil;
        assert_eq!(e.remote("POST", "/click?x=1&y=2", &[]).0, 200);
        let f = &e.sprite.frames[0];
        assert_eq!(f.px[2 * f.w + 1], [0xE8, 0xD2, 0x7A, 255], "the pencil drew where the click landed");
        assert_eq!(e.remote("POST", "/drag?x=0&y=0&x1=3&y1=0", &[]).0, 200);
        let f = &e.sprite.frames[0];
        assert!((0..4).all(|x| f.px[x] == [0xE8, 0xD2, 0x7A, 255]), "a drag paints the stroke");
        assert_eq!(e.remote("POST", "/click?x=1", &[]).0, 400);
    }

    #[test]
    fn opening_another_sprite_never_drops_the_artists_changes() {
        let mut e = editor();
        let mut f = e.sprite.frames[0].clone();
        f.px[0] = [0, 0, 0, 255];
        e.remote("PUT", "/surface/0", &png_of(&f));
        assert_eq!(e.remote("POST", "/open/GRASS", &[]).0, 409);
        assert_eq!(e.sprite.name, "BARREL");
        assert_eq!(e.remote("POST", "/open/../x", &[]).0, 404);
    }
}
