//! Where each local player's view goes when several share one window.

/// A rectangle in window pixels: x and y from the top-left, then width and height.
pub type Rect = [u32; 4];

/// The views of `players` (1 to 4, clamped) in a window `size` pixels across and down, one per player in order. Two
/// sit side by side in a wide or square window and stacked in a tall one; three or four share a 2x2 grid, and three
/// leave the bottom-right cell empty. The views tile the window: no gaps and no overlap.
pub fn split(players: usize, [w, h]: [u32; 2]) -> Vec<Rect> {
    let (half_w, half_h) = (w / 2, h / 2);
    match players.clamp(1, 4) {
        1 => vec![[0, 0, w, h]],
        2 if w >= h => vec![[0, 0, half_w, h], [half_w, 0, w - half_w, h]],
        2 => vec![[0, 0, w, half_h], [0, half_h, w, h - half_h]],
        n => [
            [0, 0, half_w, half_h],
            [half_w, 0, w - half_w, half_h],
            [0, half_h, half_w, h - half_h],
            [half_w, half_h, w - half_w, h - half_h],
        ][..n]
            .to_vec(),
    }
}

/// A canvas `size` units across and down in `view` (x, y, w, h), as large as fits and centred.
pub fn fit([x, y, w, h]: [i32; 4], [cw, ch]: [f32; 2]) -> [i32; 4] {
    let k = (w as f32 / cw).min(h as f32 / ch);
    let (fw, fh) = ((cw * k).round() as i32, (ch * k).round() as i32);
    [x + (w - fw) / 2, y + (h - fh) / 2, fw.min(w), fh.min(h)]
}

/// How much to scale a projection's clip x and y so a scene made for a canvas of aspect `canvas` fills a view of
/// aspect `view`: a wider view sees more to the sides, a taller one more above and below, and nothing stretches.
pub fn widen(view: f32, canvas: f32) -> [f32; 2] {
    if view >= canvas {
        [canvas / view, 1.0]
    } else {
        [1.0, view / canvas]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: [[u32; 2]; 9] = [
        [640, 480],
        [720, 720],
        [1280, 720],
        [1024, 768],
        [1080, 2400],
        [390, 844],
        [641, 481],
        [1, 1],
        [1279, 719],
    ];

    fn tiles(rects: &[Rect], [w, h]: [u32; 2]) -> bool {
        let area: u64 = rects.iter().map(|r| r[2] as u64 * r[3] as u64).sum();
        let inside = rects
            .iter()
            .all(|&[x, y, rw, rh]| x + rw <= w && y + rh <= h);
        let apart = rects.iter().enumerate().all(|(i, a)| {
            rects[i + 1..].iter().all(|b| {
                a[0] + a[2] <= b[0]
                    || b[0] + b[2] <= a[0]
                    || a[1] + a[3] <= b[1]
                    || b[1] + b[3] <= a[1]
            })
        });
        inside && apart && area == w as u64 * h as u64
    }

    #[test]
    fn one_player_fills_the_window() {
        for size in SIZES {
            assert_eq!(split(1, size), vec![[0, 0, size[0], size[1]]]);
        }
    }

    #[test]
    fn two_players_sit_side_by_side_when_wide_and_stacked_when_tall() {
        assert_eq!(
            split(2, [1280, 720]),
            vec![[0, 0, 640, 720], [640, 0, 640, 720]]
        );
        assert_eq!(
            split(2, [640, 480]),
            vec![[0, 0, 320, 480], [320, 0, 320, 480]]
        );
        assert_eq!(
            split(2, [720, 720]),
            vec![[0, 0, 360, 720], [360, 0, 360, 720]],
            "square counts as wide"
        );
        assert_eq!(
            split(2, [1080, 2400]),
            vec![[0, 0, 1080, 1200], [0, 1200, 1080, 1200]]
        );
        assert_eq!(
            split(2, [390, 844]),
            vec![[0, 0, 390, 422], [0, 422, 390, 422]]
        );
    }

    #[test]
    fn two_players_tile_every_window_without_gaps_or_overlap() {
        for size in SIZES {
            assert!(tiles(&split(2, size), size), "{size:?}");
        }
        assert_eq!(
            split(2, [641, 481]),
            vec![[0, 0, 320, 481], [320, 0, 321, 481]],
            "the odd pixel goes right"
        );
        assert_eq!(
            split(2, [481, 641]),
            vec![[0, 0, 481, 320], [0, 320, 481, 321]],
            "or down"
        );
    }

    #[test]
    fn four_players_share_a_grid_that_tiles_the_window() {
        assert_eq!(
            split(4, [1280, 720]),
            vec![
                [0, 0, 640, 360],
                [640, 0, 640, 360],
                [0, 360, 640, 360],
                [640, 360, 640, 360]
            ]
        );
        assert_eq!(
            split(4, [641, 481]),
            vec![
                [0, 0, 320, 240],
                [320, 0, 321, 240],
                [0, 240, 320, 241],
                [320, 240, 321, 241]
            ]
        );
        for size in SIZES {
            assert!(tiles(&split(4, size), size), "{size:?}");
        }
    }

    #[test]
    fn three_players_leave_the_bottom_right_cell_empty() {
        for size in SIZES {
            let three = split(3, size);
            assert_eq!(three[..], split(4, size)[..3], "{size:?}");
        }
        assert_eq!(
            split(3, [720, 720]),
            vec![[0, 0, 360, 360], [360, 0, 360, 360], [0, 360, 360, 360]]
        );
    }

    #[test]
    fn counts_outside_one_to_four_are_clamped() {
        assert_eq!(split(0, [640, 480]), split(1, [640, 480]));
        assert_eq!(split(9, [640, 480]), split(4, [640, 480]));
    }

    #[test]
    fn a_canvas_fits_its_view_centred() {
        assert_eq!(fit([0, 0, 1024, 768], [1024.0, 768.0]), [0, 0, 1024, 768]);
        assert_eq!(
            fit([640, 0, 640, 720], [1024.0, 768.0]),
            [640, 120, 640, 480],
            "a narrow view bars above and below"
        );
        assert_eq!(
            fit([0, 0, 1280, 360], [1024.0, 768.0]),
            [400, 0, 480, 360],
            "a wide one at the sides"
        );
        assert_eq!(fit([0, 0, 0, 0], [1024.0, 768.0]), [0, 0, 0, 0]);
    }

    #[test]
    fn a_scene_widens_the_way_its_view_is_longer_and_lines_up_with_the_fitted_canvas() {
        let canvas = 4.0 / 3.0;
        assert_eq!(widen(canvas, canvas), [1.0, 1.0]);
        assert_eq!(widen(32.0 / 9.0, canvas), [0.375, 1.0]);
        assert_eq!(widen(1.0, 2.0), [1.0, 0.5]);
        for view in [[640, 720], [1280, 360], [360, 360], [1080, 1200]] {
            let fitted = fit([0, 0, view[0], view[1]], [1024.0, 768.0]);
            let [sx, sy] = widen(view[0] as f32 / view[1] as f32, canvas);
            assert!(
                (sx * view[0] as f32 - fitted[2] as f32).abs() <= 1.0,
                "{view:?}: the canvas's edge is where the scene's was"
            );
            assert!(
                (sy * view[1] as f32 - fitted[3] as f32).abs() <= 1.0,
                "{view:?}"
            );
        }
    }
}
