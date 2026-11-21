//! The game's float functions, the same bits on every platform: std's `sin`, `cos` and the like call the platform's
//! own libm, which rounds differently, so replays and online clients drift. `crates/game/clippy.toml` bans std's.

#[inline]
pub fn sin(x: f32) -> f32 {
    libm::sinf(x)
}

#[inline]
pub fn cos(x: f32) -> f32 {
    libm::cosf(x)
}

#[inline]
pub fn sin_cos(x: f32) -> (f32, f32) {
    libm::sincosf(x)
}

#[inline]
pub fn tan(x: f32) -> f32 {
    libm::tanf(x)
}

/// The angle of (`x`, `y`) in radians, in -π to π, called as `y.atan2(x)` reads.
#[inline]
pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}

#[inline]
pub fn exp(x: f32) -> f32 {
    libm::expf(x)
}

#[inline]
pub fn hypot(x: f32, y: f32) -> f32 {
    libm::hypotf(x, y)
}

/// `x` to the `n`, multiplied in the order LLVM and compiler-rt use for `powi`, which documents no order of its own.
#[inline]
pub fn powi(x: f32, n: i32) -> f32 {
    let (mut r, mut sq, mut e) = (1.0f32, x, n.unsigned_abs());
    while e > 0 {
        if e & 1 == 1 {
            r *= sq;
        }
        e >>= 1;
        if e > 0 {
            sq *= sq;
        }
    }
    if n < 0 { 1.0 / r } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_to_std() {
        for i in -20_000..20_000 {
            let x = i as f32 * 0.000_37 * std::f32::consts::PI;
            let near = |a: f32, b: f32| (a - b).abs() <= 2.0 * f32::EPSILON * b.abs().max(1.0);
            #[allow(clippy::disallowed_methods)]
            let want = [x.sin(), x.cos(), x.tan(), x.atan2(0.7), (x * 0.01).exp(), x.hypot(3.0)];
            let got = [sin(x), cos(x), tan(x), atan2(x, 0.7), exp(x * 0.01), hypot(x, 3.0)];
            for (k, (g, w)) in got.into_iter().zip(want).enumerate() {
                assert!(near(g, w) || k == 2 && w.abs() > 1e4, "fn {k} at {x}: {g} against std's {w}");
            }
            assert_eq!(sin_cos(x), (sin(x), cos(x)), "{x}");
        }
    }

    #[test]
    fn powi_multiplies_as_llvm_does() {
        for x in [0.3f32, 1.01, 7.5, -2.25, 1e-3] {
            assert_eq!(powi(x, 0), 1.0);
            assert_eq!(powi(x, 2).to_bits(), (x * x).to_bits());
            assert_eq!(powi(x, 3).to_bits(), (x * (x * x)).to_bits());
            assert_eq!(powi(x, 4).to_bits(), ((x * x) * (x * x)).to_bits());
            assert_eq!(powi(x, -2).to_bits(), (1.0 / (x * x)).to_bits());
        }
    }
}
