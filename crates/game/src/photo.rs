//! Cutting a photo mode capture out of what the app read back: the top screen, then its middle square.

/// A rect of whole pixels: x, y, width, height.
pub type Px = [u32; 4];

/// The largest square in the middle of `rect`.
pub fn square([x, y, w, h]: Px) -> Px {
    let side = w.min(h);
    [x + (w - side) / 2, y + (h - side) / 2, side, side]
}

/// The RGBA pixels of `rect` out of an image `width` pixels across, kept inside the image.
pub fn crop(rgba: &[u8], width: u32, [x, y, w, h]: Px) -> (Px, Vec<u8>) {
    let height = rgba.len() as u32 / 4 / width.max(1);
    let (x, y) = (x.min(width), y.min(height));
    let (w, h) = (w.min(width - x), h.min(height - y));
    let row = width as usize * 4;
    let out = rgba.chunks_exact(row).skip(y as usize).take(h as usize).flat_map(|r| &r[x as usize * 4..(x + w) as usize * 4]).copied().collect();
    ([x, y, w, h], out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_square_is_the_middle_of_the_picture() {
        assert_eq!(square([0, 0, 1024, 768]), [128, 0, 768, 768]);
        assert_eq!(square([0, 0, 600, 800]), [0, 100, 600, 600]);
        assert_eq!(square([40, 10, 400, 300]), [90, 10, 300, 300], "inside a phone's window, offset by the picture");
        assert_eq!(square([0, 0, 5, 2]), [1, 0, 2, 2], "an odd margin rounds down");
    }

    #[test]
    fn a_crop_takes_those_pixels_and_stays_inside_the_image() {
        // A 4x3 image whose every pixel holds its own index.
        let img: Vec<u8> = (0..12u8).flat_map(|i| [i; 4]).collect();
        let (rect, out) = crop(&img, 4, [1, 1, 2, 2]);
        assert_eq!(rect, [1, 1, 2, 2]);
        assert_eq!(out.chunks(4).map(|p| p[0]).collect::<Vec<_>>(), [5, 6, 9, 10]);
        let (rect, out) = crop(&img, 4, [3, 2, 9, 9]);
        assert_eq!((rect, out.len()), ([3, 2, 1, 1], 4));
    }
}
