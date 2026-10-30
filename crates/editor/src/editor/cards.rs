//! The editing pane's cards: importing, exporting and deleting them, and their stats pages.

use super::*;

/// The spawn mix chart under the stats: its plot, then the legend beside it.
const MIX: Rect = Rect { x: CANVAS.x + 20.0, y: CANVAS.y + 392.0, w: 370.0, h: 132.0 };
const MIX_STEPS: usize = 40;
/// Kinds' colors in the spawn mix chart, in roster order; the open card's is `HOT`.
const MIX_COLORS: [Color; 6] = [[0.55, 0.62, 0.78, 1.0], [0.5, 0.75, 0.55, 1.0], [0.78, 0.5, 0.62, 1.0], [0.62, 0.55, 0.85, 1.0], [0.45, 0.7, 0.75, 1.0], [0.75, 0.68, 0.5, 1.0]];

/// Files named and read, as Import hands them over.
pub type Files = Vec<(String, Vec<u8>)>;

/// An import waiting on the open card's unsaved changes.
pub struct HeldImport {
    pub source: String,
    pub cards: Files,
    pub sounds: Files,
}

impl Editor {
    /// Takes in a card's PNG, as Import does, and opens it.
    pub fn import_card(&mut self, file: &str, bytes: &[u8]) {
        self.import_pack(file, vec![(file.to_string(), bytes.to_vec())], Vec::new());
    }

    /// Takes in `source`'s cards and their death sounds; one that would replace the open card's unsaved changes
    /// first asks whether to save them.
    pub fn import_pack(&mut self, source: &str, cards: Files, sounds: Files) {
        let front = |(file, bytes): &(String, Vec<u8>)| crate::store::cards::identify(file, bytes).ok().and_then(|(kind, id)| sprite::card_names_of(kind, &id)).map(|n| n[0].clone());
        let open = (sprite::card_kind(&self.sprite.name), sprite::card_id(&self.sprite.name));
        let over_open = cards.iter().filter_map(front).any(|f| (sprite::card_kind(&f), sprite::card_id(&f)) == open);
        if over_open && self.unsaved() {
            self.pending = Some(self.sprite.name.clone());
            self.held_import = Some(HeldImport { source: source.to_string(), cards, sounds });
            return;
        }
        self.import_cards(source, &cards);
        self.import_sounds(&cards, sounds);
    }

    /// Takes in each card from `source`, a PNG or a mod pack, opens the last, and says which saved cards it replaced.
    fn import_cards(&mut self, source: &str, cards: &Files) {
        let open = (sprite::card_kind(&self.sprite.name), sprite::card_id(&self.sprite.name));
        let same = |front: &str| (sprite::card_kind(front), sprite::card_id(front)) == open;
        let (mut imported, mut replaced, mut failed, mut last) = (Vec::new(), Vec::new(), Vec::new(), None);
        for (file, bytes) in cards {
            let front = crate::store::cards::identify(file, bytes).ok().and_then(|(kind, id)| sprite::card_names_of(kind, &id)).map(|n| n[0].clone());
            let existed = front.is_some_and(|f| self.names.iter().any(|(n, _)| *n == f));
            match self.store.import_card(file, bytes) {
                Ok(front) => {
                    let id = sprite::card_id(&front).unwrap_or_default();
                    if existed {
                        replaced.push(id.clone());
                    }
                    imported.push((file.clone(), id));
                    last = Some(front);
                }
                Err(e) => failed.push(format!("{file}: {e}")),
            }
        }
        self.names = self.all_names();
        if let Some(front) = last {
            if same(&front) {
                self.open(&front);
            } else {
                self.switch_to(front);
            }
            self.reveal();
        }
        let message = match (&imported[..], &failed[..]) {
            ([], [only]) if cards.len() == 1 => only.clone(),
            ([(file, id)], []) => format!("Imported {file} as card {id}{}", if replaced.is_empty() { "" } else { ", replacing your saved one" }),
            _ => {
                let mut m = format!("Imported {} cards from {source}", imported.len());
                if !replaced.is_empty() {
                    m += &format!(", replacing your {}", replaced.join(", "));
                }
                if let Some(first) = failed.first() {
                    m += &format!("; {} couldn't: {first}", failed.len());
                }
                m
            }
        };
        self.say(message);
    }

    /// Keeps `bytes`, an Ogg or WAV file, as the open enemy card's own death sound, and points the card at it.
    /// Returns the sound's file name once kept, for the page to play.
    pub fn attach_sound(&mut self, file: &str, bytes: &[u8]) -> Option<String> {
        const MAX_KB: usize = 512;
        let (Some(id), Some(sprite::CardKind::Enemy)) = (sprite::card_id(&self.sprite.name), sprite::card_kind(&self.sprite.name)) else {
            self.say("Open an enemy card to give it a death sound");
            return None;
        };
        let ext = match bytes {
            [b'O', b'g', b'g', b'S', ..] => "ogg",
            [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'A', b'V', b'E', ..] => "wav",
            _ => {
                self.say(format!("{file} isn't an Ogg or WAV sound"));
                return None;
            }
        };
        if bytes.len() > MAX_KB * 1024 {
            self.say(format!("{file} is {} KB; a death sound may be at most {MAX_KB} KB", bytes.len() / 1024));
            return None;
        }
        let cue = format!("kill-{id}");
        let kept = format!("{cue}.{ext}");
        if let Err(e) = self.store.set_card_sound(&id, &kept, bytes) {
            self.say(format!("Couldn't keep {file}: {e}"));
            return None;
        }
        self.own_sound = Some(kept.clone());
        let points = self.sprite.card.as_deref().and_then(|t| last_harvest_game::card_fields(t).ok()).is_some_and(|f| f.sound == cue);
        if points {
            self.say(format!("{id} dies to {file} now; Export all takes it along"));
        } else {
            self.set_card("sound", None);
            self.say(format!("{id} dies to {file} now; Save the card, then Export all takes it along"));
        }
        Some(kept)
    }

    /// Every card's own death sound, for Export all.
    pub fn all_sounds(&self) -> Vec<(String, Vec<u8>)> {
        self.store.all_sounds()
    }

    /// Keeps the death sounds a mod pack carries for the enemy cards it brought in, `kill-<id>.ogg` or `.wav`.
    fn import_sounds(&mut self, cards: &Files, sounds: Files) {
        let ids: Vec<String> = cards.iter().filter_map(|(file, bytes)| crate::store::cards::identify(file, bytes).ok()).filter(|(k, _)| *k == sprite::CardKind::Enemy).map(|(_, id)| id).collect();
        let kept = sounds
            .into_iter()
            .filter(|(file, _)| file.rsplit_once('.').is_some_and(|(stem, ext)| matches!(ext, "ogg" | "wav") && stem.strip_prefix("kill-").is_some_and(|id| ids.iter().any(|i| i == id))))
            .filter(|(file, bytes)| self.store.set_card_sound(&file[5..file.rfind('.').unwrap_or(file.len())], file, bytes).is_ok())
            .count();
        if kept > 0
            && let Some((text, _)) = &self.status
        {
            self.say(format!("{text}, and {kept} death sound{}", if kept == 1 { "" } else { "s" }));
        }
        self.load_other_cards();
    }

    /// The open card's file name and PNG as saved, for Export; None with a word why when there's nothing to give.
    pub fn export_card(&mut self) -> Option<(String, Vec<u8>)> {
        let (Some(id), Some(kind)) = (sprite::card_id(&self.sprite.name), sprite::card_kind(&self.sprite.name)) else {
            self.say("Open a card to export it");
            return None;
        };
        if self.unsaved() {
            self.say("Save the card first: Export gives the saved copy");
            return None;
        }
        let bytes = self.store.card_bytes(&self.sprite.name)?;
        let file = format!("{id}{}", kind.extension());
        self.say(format!("Exported {file}"));
        Some((file, bytes))
    }

    /// Every saved card by file name, for Export all; none, with a word why, while the open card has unsaved changes.
    pub fn all_cards(&mut self) -> Vec<(String, Vec<u8>)> {
        if self.is_card() && self.unsaved() {
            self.say("Save the open card first: Export all gives the saved copies");
            return Vec::new();
        }
        let cards = self.store.all_cards();
        if cards.is_empty() {
            self.say("There are no cards to export yet");
        }
        cards
    }

    /// Removes the open card from the store on a second press within a few seconds, and leaves nothing open.
    pub fn delete_card(&mut self) {
        let Some(id) = sprite::card_id(&self.sprite.name) else {
            return self.say("Open a card to delete it");
        };
        if self.delete_armed <= 0.0 {
            self.delete_armed = DELETE_CONFIRM;
            return self.say(format!("Press Delete card again to remove {id} for good"));
        }
        self.delete_armed = 0.0;
        if let Err(e) = self.store.delete_card(&self.sprite.name) {
            return self.say(format!("Couldn't delete {id}: {e}"));
        }
        self.sprite = Sprite { name: String::new(), frames: vec![Frame::blank(16, 16)], views: None, footprint: None, top: None, card: None, clear_in_3d: Vec::new() };
        (self.dirty, self.stand_touched, self.stats, self.frame) = (false, false, false, 0);
        self.undo.clear();
        self.redo.clear();
        self.names = self.all_names();
        self.changed = true;
        self.say(format!("Deleted card {id}"));
    }

    pub(super) fn is_card(&self) -> bool {
        self.sprite.card.is_some() && !self.is_prop()
    }

    /// Sets a player card's start at `difficulty`, as `set_player_start` does.
    pub(super) fn set_start(&mut self, difficulty: last_harvest_game::Difficulty, weapons: Option<&[String]>) {
        let Some(text) = &self.sprite.card else { return };
        self.sprite.card = Some(last_harvest_game::set_player_start(text, difficulty, weapons));
        self.dirty = true;
    }

    /// Sets one of the card's settings, as `set_card_field` does.
    pub(super) fn set_card(&mut self, key: &str, value: Option<&str>) {
        let Some(text) = &self.sprite.card else { return };
        self.sprite.card = Some(last_harvest_game::set_card_field(text, key, value));
        self.dirty = true;
        self.refresh_mix();
    }

    /// Reads the other cards on disk, which the spawn mix counts alongside this one.
    pub(super) fn load_other_cards(&mut self) {
        let mine = sprite::card_id(&self.sprite.name);
        let enemy_front = |n: &String| sprite::card_kind(n) == Some(sprite::CardKind::Enemy) && sprite::card_part(n).is_some_and(|(id, row)| row == 0 && Some(&id) != mine.as_ref());
        let fronts: Vec<String> = self.names.iter().map(|(n, _)| n.clone()).filter(enemy_front).collect();
        self.other_cards = fronts.iter().filter_map(|n| self.store.read(n, sprite::CARD_FILE)).filter_map(|b| String::from_utf8(b).ok()).collect();
        self.own_sound = mine.and_then(|id| self.store.card_sound(&id)).map(|(file, _)| file);
        self.refresh_mix();
    }

    /// Works out the night's spawn mix with this card as it stands now, unsaved changes and all.
    pub(super) fn refresh_mix(&mut self) {
        let enemy = sprite::card_kind(&self.sprite.name) == Some(sprite::CardKind::Enemy);
        self.mix = self.sprite.card.as_ref().filter(|_| self.stats && enemy).map(|mine| {
            let others = self.other_cards.iter().filter(|t| last_harvest_game::card_fields(t).is_ok()).map(String::as_str);
            let cards: Vec<&str> = others.chain([mine.as_str()]).collect();
            let minutes: Vec<f32> = (0..MIX_STEPS).map(|i| (i as f32 + 0.5) / MIX_STEPS as f32 * last_harvest_game::night_minutes()).collect();
            last_harvest_game::spawn_mix(&cards, &minutes)
        });
    }

    pub(super) fn is_player_card(&self) -> bool {
        self.is_card() && sprite::card_kind(&self.sprite.name) == Some(sprite::CardKind::Player)
    }

    /// The open card's name or about as its text gives it.
    pub(super) fn card_line(&self, line: Line) -> Option<String> {
        let text = self.sprite.card.as_deref()?;
        let (name, about) = if self.is_player_card() { last_harvest_game::player_fields(text).ok().map(|f| (f.name, f.about))? } else { last_harvest_game::card_fields(text).ok().map(|f| (f.name, f.about))? };
        Some(if line == Line::Name { name } else { about })
    }

    /// The Rename and About buttons beside a card's name.
    pub(super) fn line_buttons(&self, push: &mut impl FnMut(f32, f32, f32, f32, Action, &str, bool)) {
        let typing = |line: Line| self.naming.as_ref().is_some_and(|(l, _)| *l == line);
        push(CANVAS.x + 300.0, CANVAS.y + 30.0, 116.0, 30.0, Action::EditName, "Rename", typing(Line::Name));
        push(CANVAS.x + 424.0, CANVAS.y + 30.0, 116.0, 30.0, Action::EditAbout, "About", typing(Line::About));
    }

    /// A card's name and its about under it, wrapped to two lines, with a caret on whichever is being typed.
    fn draw_lines(&self, c: &mut Canvas, name: &str, about: &str) {
        const ABOUT_ACROSS: usize = 66;
        let x = CANVAS.x + 20.0;
        let caret = if (self.clock * 2.0) as i32 % 2 == 0 { "_" } else { " " };
        let typed = |line: Line, saved: &str| match &self.naming {
            Some((l, t)) if *l == line => (format!("{t}{caret}"), HOT),
            _ => (saved.to_string(), TEXT),
        };
        let (name, ink) = typed(Line::Name, name);
        c.label(name, x, CANVAS.y + 32.0, 26.0, ink, Align::Left);
        let (about, ink) = typed(Line::About, about);
        let (about, ink) = if about.trim().is_empty() && ink == TEXT { ("No about yet: a sentence on what to expect, for the game's menus".to_string(), DIM) } else { (about, ink) };
        let split = if about.chars().count() <= ABOUT_ACROSS { about.len() } else { about[..=ABOUT_ACROSS].rfind(' ').unwrap_or(ABOUT_ACROSS) };
        for (i, part) in [&about[..split], about[split..].trim_start()].into_iter().enumerate() {
            c.label(part, x, CANVAS.y + 64.0 + i as f32 * 16.0, 14.0, ink, Align::Left);
        }
    }

    /// A player card's starting weapons at each difficulty, by id; None where it keeps the game's own.
    pub(super) fn player_starts(&self) -> [Option<Vec<String>>; 3] {
        self.sprite.card.as_deref().and_then(|t| last_harvest_game::player_fields(t).ok()).map(|f| f.start).unwrap_or_default()
    }

    /// Makes card `id` of `card` kind in the store, from the template for New or from the open card for Copy, and
    /// opens its front.
    pub(super) fn make_card(&mut self, kind: Ask, card: sprite::CardKind, id: &str) {
        let Some(names) = sprite::card_names_of(card, id) else {
            return self.say("A card's id uses a-z, 0-9 and _");
        };
        if self.names.iter().any(|(n, _)| *n == names[0]) {
            return self.say(format!("Card {id} already exists"));
        }
        let parts: Vec<Sprite> = match kind {
            Ask::Copy => {
                let (Some((from, _)), Some(was)) = (sprite::card_part(&self.sprite.name), sprite::card_kind(&self.sprite.name)) else {
                    return self.say("Copy a card to make a card, or start one with New");
                };
                if was != card {
                    return self.say("Copy a card to a card of its own kind");
                }
                let text = self.sprite.card.clone().unwrap_or_default().replacen(&format!("[{from}]"), &format!("[{id}]"), 1);
                let source = sprite::card_names_of(card, &from).unwrap_or_default();
                let mut parts = Vec::new();
                for (src, name) in source.iter().zip(&names) {
                    let frames = if *src == self.sprite.name { self.sprite.frames.clone() } else { self.store.load(src).map(|s| s.frames).unwrap_or_default() };
                    parts.push(Sprite { name: name.clone(), frames, views: None, footprint: None, top: None, card: Some(text.clone()), clear_in_3d: Vec::new() });
                }
                parts
            }
            Ask::New => {
                let text = format!("[{id}]\nname        {}\n", id[..1].to_ascii_uppercase() + &id[1..]);
                let frames = sprite::card_template();
                names.iter().zip(frames.chunks(sprite::CARD_PART_FRAMES)).map(|(name, f)| Sprite { name: name.clone(), frames: f.to_vec(), views: None, footprint: None, top: None, card: Some(text.clone()), clear_in_3d: Vec::new() }).collect()
            }
        };
        if let Err(e) = parts.iter().try_for_each(|p| self.store.save(p)) {
            return self.say(format!("Couldn't make the card: {e}"));
        }
        self.ask = None;
        self.dirty = false;
        self.stand_touched = false;
        self.open(&names[0]);
        self.reveal();
        self.say(format!("Made card {id}: its parts are {}", sprite::CARD_PARTS.join(", ")));
    }

    /// How many frames the open sprite may have, when the game holds it to a number or a range.
    pub(super) fn frame_limits(&self) -> Option<(usize, usize)> {
        let player = sprite::card_kind(&self.sprite.name) == Some(sprite::CardKind::Player);
        match sprite::card_part(&self.sprite.name) {
            Some((_, row)) if row < 3 && !player => Some((1, last_harvest_game::MAX_WALK)),
            Some(_) => Some((sprite::CARD_PART_FRAMES, sprite::CARD_PART_FRAMES)),
            None => last_harvest_game::frame_limits(&self.sprite.name),
        }
    }

    /// Whether adding, deleting or moving a frame would take the sprite outside its limits, saying why if so.
    pub(super) fn frames_refused(&mut self, a: Action) -> bool {
        let Some((lo, hi)) = self.frame_limits() else { return false };
        let n = self.sprite.frames.len();
        let stills = self.sprite.name.ends_with("_FLASH") || self.sprite.name.ends_with("_HURT");
        let fixed = || if stills { format!("{} keeps {hi} frames, one for each facing", self.sprite.name) } else { format!("{} keeps {hi} frames: the game walks it with that many", self.sprite.name) };
        let why = match a {
            Action::AddFrame if n >= hi => Some(if lo == hi { fixed() } else { format!("A walk has at most {hi} frames") }),
            Action::DeleteFrame if n <= lo => Some(if lo == hi { fixed() } else { "A walk keeps at least one frame".into() }),
            Action::MoveFrame(_) if stills => Some(format!("{}'s frames go front, side, back", self.sprite.name)),
            _ => None,
        };
        why.map(|w| self.say(w)).is_some()
    }

    /// A player card's stats page: its name and about, and the weapons it starts with at each difficulty.
    pub(super) fn draw_player_stats(&self, c: &mut Canvas) {
        c.rect(CANVAS.x, CANVAS.y, CANVAS.w, CANVAS.h, PANEL);
        let x = CANVAS.x + 20.0;
        let fields = match last_harvest_game::player_fields(self.sprite.card.as_deref().unwrap_or("")) {
            Ok(f) => f,
            Err(e) => {
                c.label("This card's text doesn't read:", x, CANVAS.y + 20.0, 20.0, HOT, Align::Left);
                c.label(e, x, CANVAS.y + 52.0, 16.0, TEXT, Align::Left);
                return;
            }
        };
        c.label(format!("Player card [{}]", fields.id), x, CANVAS.y + 8.0, 17.0, DIM, Align::Left);
        self.draw_lines(c, &fields.name, &fields.about);
        for (d, difficulty) in last_harvest_game::Difficulty::ALL.into_iter().enumerate() {
            let top = START_TOP + d as f32 * START_BLOCK;
            let how = if fields.start[d].is_some() { "starts with" } else { "starts as the game does, with" };
            c.label(format!("{} {how}", difficulty.name()), x, top + 4.0, 16.0, DIM, Align::Left);
        }
        let t = last_harvest_game::tuning::tuning();
        for (i, (label, own)) in [("Health", t.player_hp), ("Speed", t.player_speed), ("Magnet", t.player_magnet)].into_iter().enumerate() {
            let (v, ink) = fields.base[i].map_or((own, DIM), |v| (v, TEXT));
            c.label(format!("{label} {v}"), x + i as f32 * 180.0, BASE_Y + 5.0, 16.0, ink, Align::Left);
        }
        let note = format!("Toggle up to {} weapons a run starts with; Default keeps the game's own", last_harvest_game::weapon_slots());
        c.label(note, x, CANVAS.y + CANVAS.h - 22.0, 13.0, DIM, Align::Left);
    }

    /// A card's stats page: its name, about, numbers, death sound and flags, as the game will read them.
    pub(super) fn draw_stats(&self, c: &mut Canvas) {
        if self.is_player_card() {
            return self.draw_player_stats(c);
        }
        c.rect(CANVAS.x, CANVAS.y, CANVAS.w, CANVAS.h, PANEL);
        let text = self.sprite.card.as_deref().unwrap_or("");
        let x = CANVAS.x + 20.0;
        let f = match last_harvest_game::card_fields(text) {
            Ok(f) => f,
            Err(e) => {
                c.label("This card's text doesn't read:", x, CANVAS.y + 20.0, 20.0, HOT, Align::Left);
                c.label(e, x, CANVAS.y + 52.0, 16.0, TEXT, Align::Left);
                return;
            }
        };
        let replaces = self.mix.as_ref().and_then(|m| m.as_ref().ok()).filter(|m| m.replaced.contains(&f.id)).map(|_| "  replaces the game's own");
        c.label(format!("Enemy card [{}]{}", f.id, replaces.unwrap_or("")), x, CANVAS.y + 8.0, 17.0, if replaces.is_some() { HOT } else { DIM }, Align::Left);
        self.draw_lines(c, &f.name, &f.about);
        for (i, ((_, v), (label, _))) in f.numbers.iter().zip(STAT_ROWS).enumerate() {
            let y = STAT_TOP + i as f32 * STAT_ROW;
            c.label(label, x, y + 3.0, 16.0, DIM, Align::Left);
            c.label(format!("{}", (v * 1000.0).round() / 1000.0), CANVAS.x + 380.0, y + 3.0, 16.0, TEXT, Align::Right);
        }
        let y = STAT_TOP + STAT_ROWS.len() as f32 * STAT_ROW;
        c.label("Death sound", x, y + 3.0, 16.0, DIM, Align::Left);
        let own = self.own_sound.as_deref().filter(|file| file.starts_with(&format!("{}.", f.sound)));
        c.label(own.map_or(f.sound.clone(), |file| format!("{file}, its own")), CANVAS.x + 380.0, y + 3.0, 16.0, if own.is_some() { HOT } else { TEXT }, Align::Right);
        self.draw_mix(c, &f.id);
        c.label("Shift steps ten times as far.  T: back to the pixels", x, CANVAS.y + CANVAS.h - 22.0, 13.0, DIM, Align::Left);
    }

    /// The share of each kind among a night's spawns from dusk to dawn, stacked, with this card's in `HOT`, and a
    /// legend giving each kind's share over the night and flagging any that never come.
    pub(super) fn draw_mix(&self, c: &mut Canvas, mine: &str) {
        let m = MIX;
        c.label("Spawn mix through the night", m.x, m.y - 4.0, 15.0, DIM, Align::Left);
        let (plot_y, plot_h) = (m.y + 18.0, m.h - 36.0);
        c.rect(m.x, plot_y, m.w, plot_h, [0.12, 0.12, 0.14, 1.0]);
        let mix = match &self.mix {
            Some(Ok(mix)) => mix,
            Some(Err(e)) => return c.label(e.clone(), m.x + 8.0, plot_y + 8.0, 14.0, HOT, Align::Left),
            None => return,
        };
        let color = |i: usize| if mix.kinds[i].0 == mine { HOT } else { MIX_COLORS[i % MIX_COLORS.len()] };
        let col = m.w / mix.shares.len() as f32;
        for (s, shares) in mix.shares.iter().enumerate() {
            let mut y = plot_y + plot_h;
            for (i, share) in shares.iter().enumerate() {
                let h = share * plot_h;
                if h > 0.0 {
                    c.rect(m.x + s as f32 * col, y - h, col + 0.5, h, color(i));
                }
                y -= h;
            }
        }
        let night = last_harvest_game::night_minutes();
        for minute in [0.0, night / 2.0, night] {
            let align = if minute == 0.0 { Align::Left } else if minute == night { Align::Right } else { Align::Center };
            c.label(format!("{minute} min"), m.x + minute / night * m.w, plot_y + plot_h + 2.0, 13.0, DIM, align);
        }
        let lx = m.x + m.w + 12.0;
        for (i, (id, name)) in mix.kinds.iter().enumerate() {
            let y = plot_y + i as f32 * 18.0;
            let total = mix.shares.iter().map(|s| s[i]).sum::<f32>() / mix.shares.len() as f32;
            c.rect(lx, y + 3.0, 10.0, 10.0, color(i));
            let label: String = name.chars().take(10).collect();
            c.label(label, lx + 14.0, y, 14.0, if id == mine { HOT } else { TEXT }, Align::Left);
            let (share, ink) = if total <= 0.0 { ("never".to_string(), [1.0, 0.45, 0.4, 1.0]) } else { (format!("{:.0}%", total * 100.0), DIM) };
            c.label(share, CANVAS.x + CANVAS.w - 12.0, y, 14.0, ink, Align::Right);
        }
    }
}

/// Every weapon by id, with a label short enough for a toggle: its id, capitalized.
pub(super) fn weapon_labels() -> Vec<(String, String)> {
    last_harvest_game::weapons().into_iter().map(|(id, _)| (id.clone(), id[..1].to_ascii_uppercase() + &id[1..])).collect()
}

/// Frames in a card's figure: each facing's first walking frame, then its flash, hurt and dead frames.
pub(super) const FIGURE_FRAMES: usize = 12;

fn blank(f: &Frame) -> bool {
    f.px.iter().all(|p| p[3] < 128)
}

/// A still frame for `row` (flash, hurt or dead) drawn from a standing frame, as the game draws a blank one.
fn derive(row: usize, stand: &Frame) -> Frame {
    let paint = if row == 3 { last_harvest_game::flash_pixel } else { last_harvest_game::hurt_pixel };
    Frame { w: stand.w, h: stand.h, px: stand.px.iter().map(|&p| paint(p)).collect() }
}

impl Editor {
    /// Reads the open card's parts as saved, or none when a card isn't open.
    pub(super) fn load_card_parts(&mut self) {
        let names = sprite::card_part(&self.sprite.name).and_then(|(id, _)| sprite::card_names_of(sprite::card_kind(&self.sprite.name)?, &id));
        let parts = names.and_then(|names| names.iter().map(|n| self.store.load(n).map(|s| s.frames)).collect::<Option<Vec<_>>>());
        self.card_parts = parts.unwrap_or_default();
    }

    /// The open card's parts, with the open part's unsaved frames.
    fn card_rows(&self) -> Option<Vec<Vec<Frame>>> {
        let (_, row) = sprite::card_part(&self.sprite.name)?;
        let mut rows = self.card_parts.clone();
        *rows.get_mut(row)? = self.sprite.frames.clone();
        Some(rows)
    }

    /// The card's figure for the previews, `FIGURE_FRAMES` long, with blank still frames drawn as the game draws
    /// them, and whether it has dead frames of its own.
    pub(super) fn figure_frames(&self) -> Option<(Vec<Frame>, bool)> {
        let rows = self.card_rows()?;
        let stand: Vec<Frame> = rows[..3].iter().map(|r| r.first().cloned()).collect::<Option<_>>()?;
        let mut out = stand.clone();
        for (row, frames) in rows.iter().enumerate().skip(3) {
            for (facing, s) in stand.iter().enumerate() {
                let f = frames.get(facing).filter(|f| !blank(f) && (f.w, f.h) == (s.w, s.h)).cloned();
                out.push(f.unwrap_or_else(|| derive(row, s)));
            }
        }
        let dead = rows[5].iter().any(|f| !blank(f));
        Some((out, dead))
    }

    /// The row of the open card part when it is a still one: flash, hurt or dead.
    pub(super) fn still_row(&self) -> Option<usize> {
        sprite::card_part(&self.sprite.name).map(|(_, row)| row).filter(|&r| r >= 3)
    }

    /// The row a game sprite's name gives it when it is a figure's flash, hurt or dead frames beside walking
    /// ones, such as `SKINNY_DEAD` beside `SKINNY_FRONT`.
    pub(super) fn built_in_still(&self) -> Option<usize> {
        let (prefix, part) = self.sprite.name.rsplit_once('_')?;
        let row = ["FLASH", "HURT", "DEAD"].iter().position(|p| *p == part)? + 3;
        let walks = ["FRONT", "SIDE", "BACK"].iter().all(|w| self.names.iter().any(|(n, _)| *n == format!("{prefix}_{w}")));
        (!self.is_card() && walks).then_some(row)
    }

    /// Each facing's first walking frame, for drawing the open still sprite from.
    fn stand_frames(&self) -> Option<Vec<Frame>> {
        if self.is_card() {
            return self.card_rows()?[..3].iter().map(|r| r.first().cloned()).collect();
        }
        let (prefix, _) = self.sprite.name.rsplit_once('_')?;
        ["FRONT", "SIDE", "BACK"].iter().map(|w| self.store.load(&format!("{prefix}_{w}"))?.frames.into_iter().next()).collect()
    }

    /// Whether the open part is a card's dead frames, which the previews show lying down.
    pub fn dead_part(&self) -> bool {
        self.still_row() == Some(5)
    }

    /// The figure to lay down in the previews while dead frames are open, a card's or the game's, showing the
    /// frame being edited the way the game tips a corpse over.
    pub fn lying_figure(&self) -> Option<last_harvest_game::Figure> {
        let i = self.frame;
        if self.dead_part() {
            let mut f = self.figure()?;
            match &mut f.dead {
                Some(d) => *d = [d[i.min(2)]; 3],
                None => f.hurt = [f.hurt[i.min(2)]; 3],
            }
            return Some(f);
        }
        let a = *self.live.get(i).filter(|_| self.built_in_still() == Some(5) && i < self.sprite.frames.len())?;
        Some(last_harvest_game::Figure { stand: [a; 3], flash: [a; 3], hurt: [a; 3], dead: Some([a; 3]), gibs: 0 })
    }

    /// Draws the open still sprite's frames, one per facing, as the game draws a card's blank ones, to edit from.
    pub(super) fn generate_part(&mut self) {
        let Some(row) = self.still_row().or_else(|| self.built_in_still()) else { return };
        let Some(stand) = self.stand_frames() else {
            return self.say("Generate draws from each facing's first walking frame, and one is missing");
        };
        self.remember();
        self.sprite.frames = stand.iter().map(|s| derive(row, s)).collect();
        self.frame = self.frame.min(self.sprite.frames.len() - 1);
        self.say("Drew each facing from its first walking frame; edit them, or Undo to go back");
    }

    /// Blanks the open still part, so the game draws it from the walk.
    pub(super) fn clear_part(&mut self) {
        if self.still_row().is_none() {
            return;
        }
        self.remember();
        for f in &mut self.sprite.frames {
            *f = Frame::blank(f.w, f.h);
        }
        self.say("Cleared: the game draws these from each facing's first walking frame");
    }
}
