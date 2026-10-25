//! A solid carved from three views of a sprite: seen from above, from the south and from the east. A voxel is
//! solid where all three views are, and each face takes its color from the view that looks at it. The view
//! from above is the sprite itself, unless a separate footprint gives the outline and the sprite only the colors.

/// One face of the solid. Corners are in pixels with x east, y up and z south from the north-west corner
/// on the ground; `uv` is in texture pixels, (u0, v0) at the first corner and (u1, v1) at the third.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub corners: [[f32; 3]; 4],
    pub uv: [f32; 4],
    pub normal: [f32; 3],
}

/// The views as pixel rects in a texture `stride` pixels wide. The south view is as wide as the sprite;
/// the east view is as wide as the sprite is tall, with south on its left; both are as tall as the solid.
pub struct Views {
    pub top: [u16; 4],
    /// The outline seen from above, as tall and wide as `top`, when the sprite's own outline isn't it.
    pub footprint: Option<[u16; 4]>,
    pub south: [u16; 4],
    pub east: [u16; 4],
}

pub fn carve(pixels: &[u8], stride: usize, v: &Views) -> Vec<Quad> {
    let alpha = |r: [u16; 4], x: usize, y: usize| {
        let [rx, ry, rw, rh] = r.map(usize::from);
        x < rw && y < rh && pixels[((ry + y) * stride + rx + x) * 4 + 3] > 0
    };
    let (w, d) = (v.top[2] as usize, v.top[3] as usize);
    let h = v.south[3].min(v.east[3]) as usize;
    // `up` counts from the ground; the views' rows count down from the top.
    let footprint = v.footprint.unwrap_or(v.top);
    let solid = |x: usize, z: usize, up: usize| alpha(footprint, x, z) && alpha(v.south, x, h - 1 - up) && alpha(v.east, d - 1 - z, h - 1 - up);
    let mut out = Vec::new();
    let (hf, df) = (h as f32, d as f32);
    let [tx, ty, sx, sy, ex, ey] = [v.top[0], v.top[1], v.south[0], v.south[1], v.east[0], v.east[1]].map(f32::from);

    for up in 0..h {
        let open_above = |x: usize, z: usize| up + 1 == h || !solid(x, z, up + 1);
        for [x0, z0, x1, z1] in rects(w, d, |x, z| solid(x, z, up) && open_above(x, z)) {
            let y = (up + 1) as f32;
            out.push(Quad {
                corners: [[x0, y, z0], [x1, y, z0], [x1, y, z1], [x0, y, z1]],
                uv: [tx + x0, ty + z0, tx + x1, ty + z1],
                normal: [0.0, 1.0, 0.0],
            });
        }
    }
    for z in 0..d {
        for (dz, face) in [(1i32, 1.0f32), (-1, -1.0)] {
            let open = |x: usize, up: usize| {
                let n = z as i32 + dz;
                n < 0 || n >= d as i32 || !solid(x, n as usize, up)
            };
            for [x0, r0, x1, r1] in rects(w, h, |x, r| solid(x, z, h - 1 - r) && open(x, h - 1 - r)) {
                let zf = if face > 0.0 { z as f32 + 1.0 } else { z as f32 };
                out.push(Quad {
                    corners: [[x0, hf - r0, zf], [x1, hf - r0, zf], [x1, hf - r1, zf], [x0, hf - r1, zf]],
                    uv: [sx + x0, sy + r0, sx + x1, sy + r1],
                    normal: [0.0, 0.0, face],
                });
            }
        }
    }
    for x in 0..w {
        for (dx, face) in [(1i32, 1.0f32), (-1, -1.0)] {
            let open = |z: usize, up: usize| {
                let n = x as i32 + dx;
                n < 0 || n >= w as i32 || !solid(n as usize, z, up)
            };
            // Seen from the east, south is on the left: column c is z = d - 1 - c.
            for [c0, r0, c1, r1] in rects(d, h, |c, r| solid(x, d - 1 - c, h - 1 - r) && open(d - 1 - c, h - 1 - r)) {
                let xf = if face > 0.0 { x as f32 + 1.0 } else { x as f32 };
                let (south, north) = (df - c0, df - c1);
                out.push(Quad {
                    corners: [[xf, hf - r0, south], [xf, hf - r0, north], [xf, hf - r1, north], [xf, hf - r1, south]],
                    uv: [ex + c0, ey + r0, ex + c1, ey + r1],
                    normal: [face, 0.0, 0.0],
                });
            }
        }
    }
    out
}

/// Covers the set cells of a `w` by `h` grid with rectangles, greedily: [x0, y0, x1, y1] with exclusive ends.
fn rects(w: usize, h: usize, set: impl Fn(usize, usize) -> bool) -> Vec<[f32; 4]> {
    let mut taken = vec![false; w * h];
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if taken[y * w + x] || !set(x, y) {
                continue;
            }
            let free = |x: usize, y: usize, taken: &[bool]| !taken[y * w + x] && set(x, y);
            let mut x1 = x + 1;
            while x1 < w && free(x1, y, &taken) {
                x1 += 1;
            }
            let mut y1 = y + 1;
            while y1 < h && (x..x1).all(|cx| free(cx, y1, &taken)) {
                y1 += 1;
            }
            for cy in y..y1 {
                for cx in x..x1 {
                    taken[cy * w + cx] = true;
                }
            }
            out.push([x as f32, y as f32, x1 as f32, y1 as f32]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A texture with three rects side by side: top `w`x`d`, south `w`x`h`, east `d`x`h`, all opaque unless `hole` says.
    fn texture(w: usize, d: usize, h: usize, hole: impl Fn(usize, usize, usize) -> bool) -> (Vec<u8>, usize, Views) {
        let stride = w + w + d;
        let rows = d.max(h);
        let mut px = vec![0u8; stride * rows * 4];
        let rects = [(0, w, d), (w, w, h), (2 * w, d, h)];
        for (k, &(x0, rw, rh)) in rects.iter().enumerate() {
            for y in 0..rh {
                for x in 0..rw {
                    if !hole(k, x, y) {
                        px[(y * stride + x0 + x) * 4 + 3] = 255;
                    }
                }
            }
        }
        let r = |(x0, rw, rh): (usize, usize, usize)| [x0 as u16, 0, rw as u16, rh as u16];
        (px, stride, Views { top: r(rects[0]), footprint: None, south: r(rects[1]), east: r(rects[2]) })
    }

    #[test]
    fn a_full_block_is_five_faces() {
        let (px, stride, v) = texture(6, 4, 3, |_, _, _| false);
        let q = carve(&px, stride, &v);
        assert_eq!(q.len(), 5, "top and four sides; the bottom is never seen");
        let top = q.iter().find(|q| q.normal == [0.0, 1.0, 0.0]).unwrap();
        assert_eq!(top.corners[2], [6.0, 3.0, 4.0]);
    }

    #[test]
    fn a_notch_in_the_south_view_lowers_the_roof_there() {
        // The south view's top row is clear over its left half: a hood lower than the cabin.
        let (px, stride, v) = texture(4, 2, 3, |k, x, y| k == 1 && y == 0 && x < 2);
        let q = carve(&px, stride, &v);
        let tops: Vec<f32> = q.iter().filter(|q| q.normal == [0.0, 1.0, 0.0]).map(|q| q.corners[0][1]).collect();
        assert!(tops.contains(&2.0) && tops.contains(&3.0), "{tops:?}");
    }

    #[test]
    fn the_east_view_is_read_with_south_on_its_left() {
        // Clear the east view's left column: the southern row of the solid disappears.
        let (px, stride, v) = texture(2, 3, 2, |k, x, _| k == 2 && x == 0);
        let q = carve(&px, stride, &v);
        let south = q.iter().filter(|q| q.normal == [0.0, 0.0, 1.0]).map(|q| q.corners[0][2]).fold(0.0, f32::max);
        assert_eq!(south, 2.0);
    }

    #[test]
    fn rects_cover_exactly_the_set_cells() {
        let set = |x: usize, y: usize| !(x + y).is_multiple_of(3);
        let mut covered = [false; 7 * 5];
        for [x0, y0, x1, y1] in rects(7, 5, set) {
            for (x, y) in (y0 as usize..y1 as usize).flat_map(|y| (x0 as usize..x1 as usize).map(move |x| (x, y))) {
                assert!(!covered[y * 7 + x] && set(x, y));
                covered[y * 7 + x] = true;
            }
        }
        assert!((0..5).all(|y| (0..7).all(|x| covered[y * 7 + x] == set(x, y))));
    }

    #[test]
    fn a_footprint_sets_the_outline_while_the_sprite_keeps_the_colors() {
        // The top view is fully opaque, but the footprint (the east view's rect reused) clears column 0.
        let (px, stride, mut v) = texture(3, 3, 3, |k, x, _| k == 2 && x == 0);
        v.footprint = Some(v.east);
        let q = carve(&px, stride, &v);
        let west = q.iter().filter(|q| q.normal == [0.0, 1.0, 0.0]).map(|q| q.corners[0][0]).fold(f32::MAX, f32::min);
        assert_eq!(west, 1.0, "the footprint's clear column is gone");
        let top = q.iter().find(|q| q.normal == [0.0, 1.0, 0.0]).unwrap();
        assert_eq!(top.uv[0], v.top[0] as f32 + 1.0, "colors still come from the sprite");
    }
}
