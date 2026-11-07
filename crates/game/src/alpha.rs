//! How opaque a sprite draws: one number from `MIN` to 1 for every frame, or one a frame with the last carrying on, as
//! a game sprite's `alpha.txt` and a prop's `alpha` line write it, such as `0.6` or `1 0.8 0.5`.

/// The least a sprite can be; any less and it would be gone.
pub const MIN: f32 = 0.1;
/// The most frames one spec lists.
pub const MAX_FRAMES: usize = 64;

/// Reads a spec: each number from `MIN` to 1, split by spaces.
pub fn parse(spec: &str) -> Result<Vec<f32>, String> {
    let out: Vec<f32> = spec
        .split_whitespace()
        .map(|w| w.parse::<f32>().ok().filter(|v| (MIN..=1.0).contains(v)).ok_or_else(|| format!("alpha {w:?} isn't a number from {MIN} to 1")))
        .collect::<Result<_, _>>()?;
    match out.len() {
        0 => Err(format!("alpha is a number from {MIN} to 1, or one a frame, such as 1 0.8 0.5")),
        n if n > MAX_FRAMES => Err(format!("alpha lists {n} frames, but at most {MAX_FRAMES}")),
        _ => Ok(out),
    }
}

/// Frame `i`'s alpha in `spec`: the last listed carries on.
pub fn of_frame(spec: &[f32], i: usize) -> f32 {
    spec.get(i).or(spec.last()).copied().unwrap_or(1.0)
}

/// A spec as written, the shortest that reads back the same; None when every frame is opaque.
pub fn text(spec: &[f32]) -> Option<String> {
    let mut v = spec.to_vec();
    while v.len() > 1 && v[v.len() - 1] == v[v.len() - 2] {
        v.pop();
    }
    (!v.iter().all(|a| *a >= 1.0)).then(|| v.iter().map(|a| format!("{}", (a * 100.0).round() / 100.0)).collect::<Vec<_>>().join(" "))
}

/// A spec's alpha as a byte, as the texture tables keep it.
pub fn to_byte(a: f32) -> u8 {
    (a.clamp(MIN, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_alpha_reads_one_number_or_one_a_frame_and_the_last_carries_on() {
        assert_eq!(parse("0.6").unwrap(), [0.6]);
        let spec = parse(" 1 0.8\t0.5\n").unwrap();
        assert_eq!([0, 1, 2, 7].map(|i| of_frame(&spec, i)), [1.0, 0.8, 0.5, 0.5]);
        assert_eq!(of_frame(&[], 3), 1.0, "nothing said is opaque");
    }

    #[test]
    fn an_alpha_outside_its_bounds_or_not_a_number_is_refused() {
        for bad in ["", "0", "0.05", "1.5", "-1", "nan", "inf", "half", "1 0.5 x"] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        assert!(parse(&"1 ".repeat(MAX_FRAMES + 1)).is_err());
        assert!(parse(&format!("{MIN}")).is_ok());
    }

    #[test]
    fn an_alpha_writes_back_as_it_reads_and_opaque_writes_nothing() {
        assert_eq!(text(&[1.0, 1.0]), None);
        assert_eq!(text(&[0.6, 0.6, 0.6]).as_deref(), Some("0.6"));
        let t = text(&[1.0, 0.8, 0.5, 0.5]).unwrap();
        assert_eq!(t, "1 0.8 0.5");
        assert_eq!(parse(&t).unwrap(), [1.0, 0.8, 0.5]);
        assert_eq!((to_byte(1.0), to_byte(0.0)), (255, 26));
    }
}
