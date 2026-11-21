//! One controller's state, by a stable id, every pad's combined as one player's, and which player each pad plays as.

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

/// The most players who share one screen.
pub const MAX_PLAYERS: usize = 4;

/// Which player each pad plays as, by pad id. Player 1 is the keyboard and every pad no other player holds, so with one
/// player every pad drives them. Players 2 to 4 join with Start on a pad of their own at the title and leave the same way.
#[derive(Clone, Debug, Default)]
pub struct Seats {
    /// Player 1's own pad, the first to press or push anything; its Start never joins.
    first: Option<usize>,
    /// The pads of players 2 onward, in order.
    others: Vec<usize>,
    /// Pads whose Start just joined or left, with Start ignored until it lifts.
    swallow: Vec<usize>,
    /// Each pad's buttons last frame.
    was: Vec<(usize, Buttons)>,
}

impl Seats {
    /// Reads this frame's pads. With `joining` (at the title), Start on a pad other than player 1's joins or leaves, and
    /// a pad that has gone gives up its seat; otherwise a seat waits for its pad to come back.
    pub fn update(&mut self, pads: &[Pad], joining: bool) {
        let here = |id: usize| pads.iter().any(|p| p.id == id);
        self.swallow.retain(|&id| pads.iter().any(|p| p.id == id && p.held.contains(Buttons::START)));
        if joining {
            self.others.retain(|&id| here(id));
            self.first = self.first.filter(|&id| here(id));
        }
        for p in pads {
            let was = self.was.iter().find(|(id, _)| *id == p.id).map_or(Buttons::NONE, |&(_, b)| b);
            let start = p.held.contains(Buttons::START) && !was.contains(Buttons::START);
            let used = p.held != Buttons::NONE || p.stick != [0.0; 2] || p.look != 0.0;
            if self.first.is_none() && used && !self.others.contains(&p.id) {
                self.first = Some(p.id);
                continue;
            }
            if !joining || !start || self.first == Some(p.id) {
                continue;
            }
            match self.others.iter().position(|&id| id == p.id) {
                Some(i) => {
                    self.others.remove(i);
                }
                None if self.players() < MAX_PLAYERS => self.others.push(p.id),
                None => continue,
            }
            self.swallow.push(p.id);
        }
        self.was = pads.iter().map(|p| (p.id, p.held)).collect();
    }

    pub fn players(&self) -> usize {
        1 + self.others.len()
    }

    /// The player a pad plays as, from 0.
    pub fn player(&self, pad: usize) -> usize {
        self.others.iter().position(|&id| id == pad).map_or(0, |i| i + 1)
    }

    /// Each player's buttons and sticks, from their pads combined.
    pub fn inputs(&self, pads: &[Pad]) -> Vec<(Buttons, [f32; 2], f32)> {
        let mine = |player: usize| -> Vec<Pad> {
            let quiet = |p: &Pad| {
                let mut p = *p;
                if self.swallow.contains(&p.id) {
                    p.held.set(Buttons::START, false);
                }
                p
            };
            pads.iter().filter(|p| self.player(p.id) == player).map(quiet).collect()
        };
        (0..self.players()).map(|player| combine(&mine(player))).collect()
    }
}

/// Which pads' motors run, so each pad buzzes with the player it plays as.
#[derive(Clone, Debug, Default)]
pub struct Motors {
    on: Vec<usize>,
}

impl Motors {
    /// The pads whose motor should start (true) or stop (false) this frame, by pad id, from whether each pad's player
    /// `buzzing`, by seat from 0; seats before `from` are left alone.
    pub fn changes(&mut self, seats: &Seats, pads: &[Pad], from: usize, buzzing: impl Fn(usize) -> bool) -> Vec<(usize, bool)> {
        self.on.retain(|id| pads.iter().any(|p| p.id == *id));
        let mut out = Vec::new();
        for p in pads {
            let seat = seats.player(p.id);
            let want = seat >= from && buzzing(seat);
            if want != self.on.contains(&p.id) {
                out.push((p.id, want));
                if want {
                    self.on.push(p.id);
                } else {
                    self.on.retain(|&id| id != p.id);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_pad_buzzes_with_its_own_player_once_per_change() {
        let mut seats = Seats::default();
        frame(&mut seats, &[pad(0, Buttons::A), pad(1, Buttons::NONE)], true);
        tap_start(&mut seats, &[0, 1], 1, true);
        let pads = [pad(0, Buttons::NONE), pad(1, Buttons::NONE)];
        let mut m = Motors::default();
        assert_eq!(m.changes(&seats, &pads, 0, |seat| seat == 1), [(1, true)], "player 2's pad only");
        assert!(m.changes(&seats, &pads, 0, |seat| seat == 1).is_empty(), "and only when it changes");
        assert_eq!(m.changes(&seats, &pads, 0, |seat| seat == 0), [(0, true), (1, false)]);
        assert_eq!(m.changes(&seats, &pads, 1, |_| true), [(0, false), (1, true)], "seats before `from` stay still");
    }

    #[test]
    fn with_one_player_every_pad_buzzes_for_them() {
        let mut seats = Seats::default();
        let pads = [pad(0, Buttons::A), pad(5, Buttons::NONE)];
        seats.update(&pads, true);
        assert_eq!(Motors::default().changes(&seats, &pads, 0, |seat| seat == 0), [(0, true), (5, true)]);
    }

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

    fn pad(id: usize, held: Buttons) -> Pad {
        Pad { id, held, ..Default::default() }
    }

    /// Runs one frame with these pads and returns each player's buttons.
    fn frame(seats: &mut Seats, pads: &[Pad], joining: bool) -> Vec<Buttons> {
        seats.update(pads, joining);
        seats.inputs(pads).into_iter().map(|(held, ..)| held).collect()
    }

    /// Presses Start on pad `id` and lets it go, the other pads idle.
    fn tap_start(seats: &mut Seats, ids: &[usize], id: usize, joining: bool) {
        let with = |start: bool| ids.iter().map(|&i| pad(i, if start && i == id { Buttons::START } else { Buttons::NONE })).collect::<Vec<_>>();
        frame(seats, &with(true), joining);
        frame(seats, &with(false), joining);
    }

    #[test]
    fn with_one_player_every_pad_drives_player_one() {
        let mut seats = Seats::default();
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::A), pad(5, Buttons::B)], true), [Buttons::A | Buttons::B]);
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::NONE), pad(5, Buttons::X)], false), [Buttons::X]);
        assert_eq!(seats.players(), 1);
    }

    #[test]
    fn without_joining_start_on_a_second_pad_is_player_ones() {
        let mut seats = Seats::default();
        frame(&mut seats, &[pad(0, Buttons::A), pad(1, Buttons::NONE)], false);
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::NONE), pad(1, Buttons::START)], false), [Buttons::START]);
        assert_eq!(seats.players(), 1);
    }

    #[test]
    fn the_first_pad_used_is_player_ones_and_its_start_never_joins() {
        let mut seats = Seats::default();
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::NONE), pad(3, Buttons::START)], true), [Buttons::START], "the first press claims player 1");
        frame(&mut seats, &[pad(0, Buttons::NONE), pad(3, Buttons::NONE)], true);
        tap_start(&mut seats, &[0, 3], 3, true);
        assert_eq!(seats.players(), 1);
    }

    #[test]
    fn start_on_another_pad_at_the_title_joins_and_again_leaves() {
        let mut seats = Seats::default();
        frame(&mut seats, &[pad(0, Buttons::A), pad(1, Buttons::NONE)], true);
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::NONE), pad(1, Buttons::START)], true), [Buttons::NONE, Buttons::NONE], "the joining Start reaches no one");
        assert_eq!(seats.players(), 2);
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::NONE), pad(1, Buttons::START)], true), [Buttons::NONE; 2], "nor while it's held");
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::A), pad(1, Buttons::B)], true), [Buttons::A, Buttons::B], "each pad its own player");
        frame(&mut seats, &[pad(0, Buttons::NONE), pad(1, Buttons::NONE)], true);
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::NONE), pad(1, Buttons::START)], true), [Buttons::NONE], "leaving");
        assert_eq!(seats.players(), 1);
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::NONE), pad(1, Buttons::START | Buttons::A)], true), [Buttons::A], "the leaving Start stays swallowed while held");
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::NONE), pad(1, Buttons::B)], true), [Buttons::B], "and the pad is player 1's again");
    }

    #[test]
    fn up_to_four_players_join_in_order() {
        let mut seats = Seats::default();
        let ids = [7, 2, 9, 4, 5];
        frame(&mut seats, &ids.map(|i| pad(i, if i == 7 { Buttons::A } else { Buttons::NONE })), true);
        for id in [9, 2, 4, 5] {
            tap_start(&mut seats, &ids, id, true);
        }
        assert_eq!(seats.players(), MAX_PLAYERS, "a fifth pad stays player 1's");
        assert_eq!([7, 9, 2, 4, 5].map(|id| seats.player(id)), [0, 1, 2, 3, 0]);
        tap_start(&mut seats, &ids, 9, true);
        assert_eq!([2, 4].map(|id| seats.player(id)), [1, 2], "the players after one who leaves move up");
    }

    #[test]
    fn seats_follow_pad_ids_not_the_order_pads_are_listed() {
        let mut seats = Seats::default();
        frame(&mut seats, &[pad(0, Buttons::A), pad(1, Buttons::NONE)], true);
        tap_start(&mut seats, &[0, 1], 1, true);
        assert_eq!(frame(&mut seats, &[pad(1, Buttons::B), pad(0, Buttons::A)], false), [Buttons::A, Buttons::B]);
    }

    #[test]
    fn during_a_run_an_unplugged_pad_keeps_its_seat_until_it_comes_back() {
        let mut seats = Seats::default();
        frame(&mut seats, &[pad(0, Buttons::A), pad(1, Buttons::NONE)], true);
        tap_start(&mut seats, &[0, 1], 1, true);
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::A)], false), [Buttons::A, Buttons::NONE], "unplugged mid-run");
        assert_eq!(seats.players(), 2);
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::NONE), pad(1, Buttons::X)], false), [Buttons::NONE, Buttons::X], "plugged back in");
    }

    #[test]
    fn at_the_title_an_unplugged_pad_gives_up_its_seat() {
        let mut seats = Seats::default();
        frame(&mut seats, &[pad(0, Buttons::A), pad(1, Buttons::NONE)], true);
        tap_start(&mut seats, &[0, 1], 1, true);
        frame(&mut seats, &[pad(0, Buttons::NONE)], true);
        assert_eq!(seats.players(), 1);
        assert_eq!(frame(&mut seats, &[pad(0, Buttons::NONE), pad(1, Buttons::B)], true), [Buttons::B], "back as player 1's");
    }

    #[test]
    fn when_player_ones_pad_goes_at_the_title_the_next_pad_used_takes_its_place() {
        let mut seats = Seats::default();
        frame(&mut seats, &[pad(0, Buttons::A), pad(1, Buttons::NONE), pad(2, Buttons::NONE)], true);
        tap_start(&mut seats, &[0, 1, 2], 2, true);
        frame(&mut seats, &[pad(1, Buttons::NONE), pad(2, Buttons::NONE)], true);
        tap_start(&mut seats, &[1, 2], 1, true);
        assert_eq!(seats.players(), 2, "pad 1's Start claimed player 1 rather than joining");
        assert_eq!([1, 2].map(|id| seats.player(id)), [0, 1]);
    }
}
