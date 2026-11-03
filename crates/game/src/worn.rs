//! Where a figure wears its ready grenade: a pixel per frame, or hidden. A sprite's folder keeps its frames' in
//! `grenade.txt` as one spec, and a player card keeps one spec per facing in its `last-harvest-worn` chunk.
//!
//! A spec is `X Y` or `hide` for each frame, split by `;`; the last one carries on to any frames after it.

/// Where one frame wears the grenade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Worn {
    /// The worn sprite's middle pixel sits on this pixel of the frame.
    At([u8; 2]),
    Hidden,
}

/// The facings a player card's chunk names, in its rows' order.
pub const FACINGS: [&str; 3] = ["front", "side", "back"];

/// Reads a spec; a pixel is in texture pixels, so a detailed frame's run past 64.
pub fn parse(spec: &str) -> Result<Vec<Worn>, String> {
    spec.split(';')
        .map(|pair| {
            if pair.trim() == "hide" {
                return Ok(Worn::Hidden);
            }
            let n: Vec<u8> = pair.split_whitespace().map(|v| v.parse::<u8>().ok()).collect::<Option<_>>().unwrap_or_default();
            match n[..] {
                [x, y] => Ok(Worn::At([x, y])),
                _ => Err(format!("{pair:?} isn't a pixel: X Y, each 0 to 255")),
            }
        })
        .collect()
}

/// A spec for `frames`, as short as it reads back the same.
pub fn spec(frames: &[Worn]) -> String {
    let mut n = frames.len();
    while n > 1 && frames[n - 1] == frames[n - 2] {
        n -= 1;
    }
    frames[..n]
        .iter()
        .map(|w| match w {
            Worn::At([x, y]) => format!("{x} {y}"),
            Worn::Hidden => "hide".into(),
        })
        .collect::<Vec<_>>()
        .join(" ; ")
}

/// Frame `i` of a spec's frames, the last carrying on.
pub fn frame(frames: &[Worn], i: usize) -> Option<Worn> {
    frames.get(i.min(frames.len().saturating_sub(1))).copied()
}

/// Reads a player card's chunk: a `FACING SPEC` line for any facing it sets.
pub fn parse_card(text: &str) -> Result<[Vec<Worn>; 3], String> {
    let mut out: [Vec<Worn>; 3] = Default::default();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (facing, spec) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let at = FACINGS.iter().position(|f| *f == facing).ok_or_else(|| format!("line {}: {facing:?} isn't front, side or back", i + 1))?;
        out[at] = parse(spec).map_err(|e| format!("line {}: {e}", i + 1))?;
    }
    Ok(out)
}

/// A player card's chunk for `facings`, a line for each one set.
pub fn card_text(facings: &[Vec<Worn>; 3]) -> String {
    FACINGS.iter().zip(facings).filter(|(_, f)| !f.is_empty()).map(|(name, f)| format!("{name} {}\n", spec(f))).collect()
}

/// Where a frame `w` by `h` with alpha `alpha(x, y)` wears the grenade when nothing says: on the right hip, a pixel in
/// from the outline a little below the middle.
pub fn default_at(w: usize, h: usize, alpha: impl Fn(usize, usize) -> bool) -> [u8; 2] {
    let y = (h * 2 / 3).min(h.saturating_sub(1));
    let x = (0..w).rev().find(|&x| alpha(x, y)).map_or(w / 2, |x| x.saturating_sub(1));
    [x as u8, y as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spec_reads_a_pixel_per_frame_or_hide() {
        assert_eq!(parse("11 10").unwrap(), [Worn::At([11, 10])]);
        assert_eq!(parse(" 11 10 ; 11 11 ").unwrap(), [Worn::At([11, 10]), Worn::At([11, 11])]);
        assert_eq!(parse("hide").unwrap(), [Worn::Hidden]);
        assert_eq!(parse("1 2 ; hide").unwrap(), [Worn::At([1, 2]), Worn::Hidden]);
        for bad in ["", "11", "11 10 2", "-1 4", "256 3", "a b", "11 10 ;"] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_spec_writes_back_as_short_as_it_reads() {
        assert_eq!(spec(&[Worn::At([11, 10]); 3]), "11 10");
        assert_eq!(spec(&[Worn::At([11, 10]), Worn::At([11, 11]), Worn::At([11, 11])]), "11 10 ; 11 11");
        assert_eq!(spec(&[Worn::Hidden; 3]), "hide");
        let frames = [Worn::At([1, 2]), Worn::At([3, 4]), Worn::At([5, 6])];
        assert_eq!(parse(&spec(&frames)).unwrap(), frames);
        assert_eq!(frame(&frames[..1], 2), Some(Worn::At([1, 2])), "the last carries on");
        assert_eq!(frame(&[], 0), None);
    }

    #[test]
    fn a_card_chunk_names_its_facings() {
        let f = parse_card("front 11 10\n# hip\nback hide\n").unwrap();
        assert_eq!(f, [vec![Worn::At([11, 10])], vec![], vec![Worn::Hidden]]);
        assert_eq!(card_text(&f), "front 11 10\nback hide\n");
        assert_eq!(parse_card(&card_text(&f)).unwrap(), f);
        assert!(parse_card("up 1 1").err().unwrap().starts_with("line 1"));
        assert!(parse_card("side 1").err().unwrap().starts_with("line 1"));
    }

    #[test]
    fn every_packed_sprite_says_where_it_wears_the_grenade_in_a_spec_that_reads() {
        for spec in crate::art::WORN.iter().flatten() {
            assert!(parse(spec).is_ok_and(|w| w.len() == 1), "{spec:?}");
        }
        let farmer = [crate::art::PLAYER_FRONT[0], crate::art::PLAYER_SIDE[0], crate::art::PLAYER_BACK[0]];
        for [x, y] in farmer.iter().filter_map(|a| a.worn_at()) {
            assert!(x < 16 && y < 15, "the farmer wears it on the frame");
        }
    }

    #[test]
    fn without_a_spec_the_grenade_sits_inside_the_right_edge_below_the_middle() {
        // A 6x6 figure filling columns 1 to 4.
        assert_eq!(default_at(6, 6, |x, _| (1..=4).contains(&x)), [3, 4]);
        assert_eq!(default_at(6, 6, |_, _| false), [3, 4], "an empty frame wears it in the middle");
    }
}
