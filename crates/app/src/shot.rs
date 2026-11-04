//! Reading back what a frame drew, for a screenshot or a photo, and encoding it as PNG.

use glow::HasContext;

pub struct Shot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub top: [u32; 4],
}

const SHOT_GAP: u32 = 16;

/// The current surface's pixels, `width` by `height`, as RGBA rows from the top.
pub fn read_back(gl: &glow::Context, width: u32, height: u32) -> Shot {
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    unsafe {
        gl.read_pixels(0, 0, width as i32, height as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut rgba)));
    }
    // GL rows run bottom to top, and the window's alpha channel is not meaningful in a screenshot.
    let row = (width * 4) as usize;
    let mut flipped: Vec<u8> = rgba.chunks_exact(row).rev().flatten().copied().collect();
    flipped.chunks_exact_mut(4).for_each(|px| px[3] = 255);
    Shot { width, height, rgba: flipped, top: [0, 0, width, height] }
}

/// `shots` one above the other, with a gap between.
pub fn stack(shots: Vec<Shot>) -> Shot {
    let width = shots.iter().map(|s| s.width).max().unwrap_or(0);
    let height = shots.iter().map(|s| s.height).sum::<u32>() + SHOT_GAP;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for (i, s) in shots.iter().enumerate() {
        if i > 0 {
            rgba.extend(std::iter::repeat_n([40, 40, 48, 255], (width * SHOT_GAP) as usize).flatten());
        }
        for row in s.rgba.chunks_exact((s.width * 4) as usize) {
            rgba.extend_from_slice(row);
            rgba.extend(std::iter::repeat_n(0, ((width - s.width) * 4) as usize));
        }
    }
    Shot { width, height, rgba, top: [0; 4] }
}

/// RGBA rows `width` pixels across as a PNG file's bytes.
pub fn png(width: u32, rgba: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, width, rgba.len() as u32 / 4 / width.max(1));
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgba)?;
    Ok(out)
}
