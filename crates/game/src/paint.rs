//! Palette swaps: the colors of a sprite that are paint, and the coats that repaint them. Each frame comes once in
//! each coat, every frame in the first coat, then every frame in the next.
//!
//! ```text
//! paint 917662 726650 4E4637   # the colors that are paint, light to dark
//! coat AD644B 9F4831 873018    # a coat: a color for each paint color,
//! coat 4A6FA5                  # or one, its darker shades following the drawing's
//! ```
//!
//! The build reads this as a saved sprite's `paints.txt`, and a prop's text takes the same lines.

pub type Rgb = [u8; 3];

/// The most paint colors, and the most coats.
pub const MAX_COLORS: usize = 8;
pub const MAX_COATS: usize = 8;
pub const FILE: &str = "paints.txt";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Paint {
    pub colors: Vec<Rgb>,
    pub coats: Vec<Vec<Rgb>>,
}

impl Paint {
    /// How many copies each frame comes in: one per coat, or one as drawn.
    pub fn copies(&self) -> usize {
        if self.colors.is_empty() { 1 } else { self.coats.len().max(1) }
    }

    /// Coat `k`'s color for each paint color.
    pub fn shades(&self, k: usize) -> Vec<Rgb> {
        let coat = &self.coats[k];
        if coat.len() == self.colors.len() {
            return coat.clone();
        }
        self.colors.iter().map(|&c| shade(coat[0], self.colors[0], c)).collect()
    }

    /// Changes which colors are paint. A coat of one color stays so; any other keeps its shade for each color that
    /// stays and works one out for each new one.
    pub fn set_colors(&mut self, colors: Vec<Rgb>) {
        let old = std::mem::replace(&mut self.colors, colors);
        for coat in &mut self.coats {
            if coat.len() == 1 || coat.len() != old.len() {
                coat.truncate(1);
                continue;
            }
            *coat = self.colors.iter().map(|c| old.iter().position(|o| o == c).map_or_else(|| shade(coat[0], old[0], *c), |i| coat[i])).collect();
        }
    }

    /// Repaints RGBA pixels in coat `k`, or leaves them as drawn when there's no coat.
    pub fn repaint(&self, k: usize, rgba: &mut [u8]) {
        if self.colors.is_empty() || self.coats.is_empty() {
            return;
        }
        let shades = self.shades(k);
        for px in rgba.chunks_exact_mut(4).filter(|p| p[3] > 0) {
            if let Some(i) = self.colors.iter().position(|c| c[..] == px[..3]) {
                px[..3].copy_from_slice(&shades[i]);
            }
        }
    }

    /// Reads a `paint` or `coat` line's value into these, or None when `key` is neither. Errors don't name the line.
    pub fn read(&mut self, key: &str, value: &str) -> Option<Result<(), String>> {
        let list = || value.split_whitespace().map(hex).collect::<Option<Vec<Rgb>>>().filter(|l| !l.is_empty());
        Some(match (key, list()) {
            ("paint", Some(l)) if l.len() <= MAX_COLORS => {
                self.colors = l;
                Ok(())
            }
            ("paint", _) => Err(format!("paint lists 1 to {MAX_COLORS} colors such as 91765F, light to dark")),
            ("coat", Some(_)) if self.coats.len() >= MAX_COATS => Err(format!("a sprite has at most {MAX_COATS} coats")),
            ("coat", Some(l)) => {
                self.coats.push(l);
                Ok(())
            }
            ("coat", None) => Err("a coat is one color such as 4A6FA5, or one for each paint color".into()),
            _ => return None,
        })
    }

    /// Whether each coat has one color or one for each paint color.
    pub fn check(&self) -> Result<(), String> {
        if !self.coats.is_empty() && self.colors.is_empty() {
            return Err("coats need a paint line saying which colors they repaint".into());
        }
        match self.coats.iter().find(|c| c.len() != 1 && c.len() != self.colors.len()) {
            Some(_) => Err(format!("a coat has one color, or {} for the paint colors", self.colors.len())),
            None => Ok(()),
        }
    }

    /// The `paint` and `coat` lines, or nothing when there's no paint.
    pub fn text(&self) -> String {
        let line = |key: &str, l: &[Rgb]| format!("{key} {}\n", l.iter().map(|c| format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])).collect::<Vec<_>>().join(" "));
        if self.colors.is_empty() {
            return String::new();
        }
        std::iter::once(line("paint", &self.colors)).chain(self.coats.iter().map(|c| line("coat", c))).collect()
    }
}

/// The shade of `base` that `c` is of `first`, channel by channel.
fn shade(base: Rgb, first: Rgb, c: Rgb) -> Rgb {
    std::array::from_fn(|i| if first[i] == 0 { c[i] } else { (base[i] as u32 * c[i] as u32 / first[i] as u32).min(255) as u8 })
}

/// Reads a `paints.txt`; errors name the line.
pub fn parse(text: &str) -> Result<Paint, String> {
    let mut p = Paint::default();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
        match p.read(key, value) {
            Some(r) => r.map_err(|e| format!("{}: {e}", i + 1))?,
            None => return Err(format!("{}: {key} isn't paint or coat", i + 1)),
        }
    }
    p.check()?;
    Ok(p)
}

/// A color from `5A6E3C`.
pub fn hex(t: &str) -> Option<Rgb> {
    let byte = |i: usize| t.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok());
    (t.len() == 6).then_some(())?;
    Some([byte(0)?, byte(2)?, byte(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    const WRECK: &str = "paint 91765F 726650 4E4637\ncoat AD644B 9F4831 873018\ncoat 4A6FA5\n";

    #[test]
    fn coats_repaint_only_the_paint() {
        let p = parse(WRECK).unwrap();
        assert_eq!((p.copies(), parse(&p.text()).unwrap()), (2, p.clone()));
        let mut px = [0x91, 0x76, 0x5F, 255, 0x87, 0x30, 0x18, 255, 0x4E, 0x46, 0x37, 0];
        p.repaint(0, &mut px);
        assert_eq!(px, [0xAD, 0x64, 0x4B, 255, 0x87, 0x30, 0x18, 255, 0x4E, 0x46, 0x37, 0], "taillights and clear pixels stay");
        assert_eq!(p.shades(1)[0], [0x4A, 0x6F, 0xA5]);
        assert!(p.shades(1)[2].iter().zip([0x4A, 0x6F, 0xA5]).all(|(d, b)| *d < b), "darker shades follow the drawing's");
        assert_eq!(Paint::default().copies(), 1);
    }

    #[test]
    fn coats_keep_their_shades_as_paint_colors_change() {
        let mut p = parse(WRECK).unwrap();
        p.set_colors(vec![[0x91, 0x76, 0x5F], [0x4E, 0x46, 0x37], [0x30, 0x30, 0x30]]);
        assert_eq!(p.coats[0][..2], [[0xAD, 0x64, 0x4B], [0x87, 0x30, 0x18]], "shades of colors that stay stay");
        assert_eq!((p.coats[0].len(), p.coats[1].len()), (3, 1), "a new color gets a shade, and a one-color coat stays one");
        p.check().unwrap();
    }

    #[test]
    fn bad_paint_lines_are_refused() {
        assert!(parse("coat 4A6FA5").unwrap_err().contains("paint line"));
        assert!(parse("paint 91765F 726650\ncoat 4A6FA5 000000 111111").unwrap_err().contains("one color, or 2"));
        assert!(parse("paint #91765F").unwrap_err().starts_with("1: paint lists"));
        assert!(parse("paint 91765F\nshine 1").unwrap_err().starts_with("2: shine"));
        assert!(parse(&format!("paint 91765F\n{}", "coat 000000\n".repeat(MAX_COATS + 1))).unwrap_err().contains("at most"));
    }
}
