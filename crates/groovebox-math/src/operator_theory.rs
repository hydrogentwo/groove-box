//! Operator Theory arithmetic, equivalence kernels, isn/ics trig, and
//! Equation of Reality (EQR) reality-tensor evaluation (book p.38-50, p.78-79).

use std::f64::consts::FRAC_PI_2;
use crate::constants::{EQR_FINITE_INFINITY, MEUM, MEUM_NORM};

// =============================================================================
// Book Operator Theory arithmetic (explicit symbolic rules, p.49-50)
// =============================================================================

#[inline]
pub fn ot_band(x: f64) -> f64 {
    let ax = x.abs();
    if ax <= 1.0 {
        1.0
    } else if ax <= 2.0 {
        2.0
    } else if ax <= 3.0 {
        3.0
    } else {
        1.0
    }
}

#[inline]
pub fn ot_add(n: f64, v: f64) -> f64 {
    let s = n + v;
    let b = if v.abs() >= n.abs() {
        ot_band(v)
    } else {
        ot_band(n)
    };
    if s >= 0.0 {
        s + b * 0.5
    } else {
        s - b * 0.5
    }
}

#[inline]
pub fn ot_sub(n: f64, v: f64) -> f64 {
    if n < 0.0 && v > 0.0 {
        return n + v;
    }
    ot_add(n, -v)
}

#[inline]
pub fn ot_prod(a: f64, b: f64) -> f64 {
    if a == 0.0 && b == 0.0 {
        return 1.0;
    }
    if a == 0.0 || b == 0.0 {
        return 0.0;
    }
    let mag = (a * b).abs();
    let an = a < 0.0;
    let bn = b < 0.0;
    if an && bn {
        -mag
    } else if !(an || bn) {
        mag
    } else {
        -mag
    }
}

#[inline]
pub fn ot_pow(b: f64, e: f64) -> f64 {
    let r = b.abs().powf(e.abs());
    let same_hand = (b >= 0.0) == (e >= 0.0);
    if same_hand { r } else { -r }
}

#[inline]
pub fn ot_div(a: f64, b: f64) -> f64 {
    if a == 0.0 && b == 0.0 {
        return 1.0;
    }
    let ab = b.abs();
    if ab == 0.0 {
        return (1.0_f64.copysign(a)) * 1e9;
    }
    if a == 0.0 {
        return 0.0;
    }
    let mag = a.abs() / ab;
    let sign = if a < 0.0 { -1.0 } else { 1.0 };
    sign * mag
}

#[inline]
pub fn ot_i_phase(x: f64, k: i64) -> f64 {
    if k.rem_euclid(2) == 0 { -x } else { x }
}

// =============================================================================
// Dual math dispatch (OT ON -> book arithmetic; OT OFF -> IEEE arithmetic)
// =============================================================================

#[inline]
pub fn math_add(a: f64, b: f64, ot: bool) -> f64 {
    if ot { ot_add(a, b) } else { a + b }
}

#[inline]
pub fn math_sub(a: f64, b: f64, ot: bool) -> f64 {
    if ot { ot_sub(a, b) } else { a - b }
}

#[inline]
pub fn math_mul(a: f64, b: f64, ot: bool) -> f64 {
    if ot { ot_prod(a, b) } else { a * b }
}

#[inline]
pub fn math_div(a: f64, b: f64, ot: bool) -> f64 {
    if ot {
        ot_div(a, b)
    } else if b == 0.0 {
        0.0
    } else {
        a / b
    }
}

#[inline]
pub fn math_pow(b: f64, e: f64, ot: bool) -> f64 {
    if ot { ot_pow(b, e) } else { b.powf(e) }
}

#[inline]
pub fn math_scale(x: f64, gain: f64, ot: bool) -> f64 {
    if ot { ot_prod(x, gain) } else { x * gain }
}

// =============================================================================
// Book isn / ics & EQR Meum-normalized sinusoids (OT-equivalence preserving)
// =============================================================================

#[inline]
pub fn book_isn(x: f64) -> f64 {
    2.0 * (0.5 * x).sin()
}

#[inline]
pub fn book_ics(x: f64) -> f64 {
    2.0 * (0.5 * x).cos()
}

#[inline]
pub fn book_isn_inv(y: f64) -> f64 {
    2.0 * (0.5 * y).clamp(-1.0, 1.0).asin()
}

#[inline]
pub fn book_ics_inv(y: f64) -> f64 {
    2.0 * (0.5 * y).clamp(-1.0, 1.0).acos()
}

#[inline]
pub fn vg_sin(x: f64) -> f64 {
    0.5 * book_isn(2.0 * x)
}

#[inline]
pub fn vg_cos(x: f64) -> f64 {
    0.5 * book_ics(2.0 * x)
}

#[inline]
pub fn vg_asin(y: f64) -> f64 {
    0.5 * book_isn_inv(2.0 * y.clamp(-1.0, 1.0))
}

#[inline]
pub fn vg_acos(y: f64) -> f64 {
    0.5 * book_ics_inv(2.0 * y.clamp(-1.0, 1.0))
}

#[inline]
pub fn vg_tan(x: f64) -> f64 {
    let c = vg_cos(x);
    if c.abs() < 1e-12 {
        1e12_f64.copysign(vg_sin(x))
    } else {
        vg_sin(x) / c
    }
}

/// Meum-normalized odd sinusoid: `sin(x)*MEUM_NORM + sin(x*MEUM)*(1 - MEUM_NORM)`.
#[inline]
pub fn eqr_isn(x: f64) -> f64 {
    x.sin() * MEUM_NORM + (x * MEUM).sin() * (1.0 - MEUM_NORM)
}

/// Meum-normalized even cosinusoid: `cos(x)*MEUM_NORM + cos(x*MEUM)*(1 - MEUM_NORM)`.
#[inline]
pub fn eqr_ics(x: f64) -> f64 {
    x.cos() * MEUM_NORM + (x * MEUM).cos() * (1.0 - MEUM_NORM)
}

/// Bisection inverse of `eqr_isn`.
pub fn eqr_isn_inv(mut y: f64) -> f64 {
    let limit = eqr_isn(FRAC_PI_2);
    if y.abs() > limit {
        y = limit.copysign(y);
    }
    let mut a = -FRAC_PI_2;
    let mut b = FRAC_PI_2;
    let mut fa = eqr_isn(a) - y;
    let mut fb = eqr_isn(b) - y;
    if fa * fb > 0.0 {
        for _ in 0..8 {
            a *= 1.25;
            b *= 1.25;
            fa = eqr_isn(a) - y;
            fb = eqr_isn(b) - y;
            if fa * fb <= 0.0 {
                break;
            }
        }
    }
    if fa * fb > 0.0 {
        return 0.0;
    }
    let mut x = (a + b) * 0.5;
    for _ in 0..24 {
        let fx = eqr_isn(x) - y;
        if (fa < 0.0) != (fx < 0.0) {
            b = x;
        } else {
            a = x;
            fa = fx;
        }
        x = (a + b) * 0.5;
    }
    x
}

// =============================================================================
// EQR Reality Tensor (book p.78 LaTeX)
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqrTensorResult {
    pub p: f64,
    pub e: f64,
    pub d: f64,
    pub z: f64,
}

/// Exact discrete sum reality tensor (book p.78).
pub fn eqr_tensor_step(sample: f64, neighbours: &[f64], t: f64) -> EqrTensorResult {
    let fallback = [sample];
    let pts = if neighbours.is_empty() { &fallback[..] } else { neighbours };
    let k = (pts.len().saturating_sub(1)).max(1) as f64;
    let mut sum_p = 0.0;
    let mut sum_e = 0.0;
    for &v in pts {
        let dn = (v - sample).abs() + 1e-9;
        sum_p += book_isn_inv((book_isn(dn) + book_isn(t)) * 0.5);
        sum_e += book_isn(v) / dn;
    }
    let p = sum_p / k;
    let e = sum_e / k;
    let mut d = 0.0;
    if p.abs() > 1e-12 {
        let mut acc = 0.0;
        for &v in pts {
            acc += book_isn_inv(book_isn(v) * e / (EQR_FINITE_INFINITY * p));
        }
        d = acc / k;
    }
    let z = p * e + d;
    EqrTensorResult { p, e, d, z }
}

/// Closed-form audio reduction of the reality tensor (O(1) per sample).
pub fn eqr_tensor_audio(_sample: f64, d_char: f64, theta_char: f64, t: f64) -> EqrTensorResult {
    let d_abs = d_char.abs() + 1e-9;
    let p = book_isn_inv((book_isn(d_abs) + book_isn(t)) * 0.5);
    let e = book_isn(theta_char) / d_abs;
    let mut d = 0.0;
    if p.abs() > 1e-12 {
        d = book_isn_inv(book_isn(theta_char) * e / (EQR_FINITE_INFINITY * p));
    }
    let z = p * e + d;
    EqrTensorResult { p, e, d, z }
}

/// Operator-theory re-encoding of the master bus (rules e, f, 1).
pub fn ot_master_transform(x: &[f32]) -> Vec<f32> {
    let n = x.len();
    if n == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(n);
    for &v in x {
        let xf = v as f64;
        let band = ot_band(xf);
        let sign = if xf > 0.0 {
            1.0
        } else if xf < 0.0 {
            -1.0
        } else {
            0.0
        };
        let mut y = xf + 0.5 * sign * band;
        y *= 1.0 + MEUM_NORM * 0.15 / (1.0 + y.abs());
        out.push(y);
    }
    let mut res = Vec::with_capacity(n);
    for i in 0..n {
        let prev = if i == 0 { out[0] } else { out[i - 1] };
        let cur = out[i];
        let v = if cur < 0.0 && prev < 0.0 {
            -cur.abs()
        } else {
            cur
        };
        res.push(v as f32);
    }
    res
}

/// Game-lattice OT helpers (parity with `videogame_engine.py`).
pub fn ot_game_residue(r: f64) -> f64 {
    if r == 0.0 {
        return 0.0;
    }
    let b = ot_band(r * 2.0);
    let sgn = 1.0_f64.copysign(r);
    ((r + b * 0.5 * sgn).rem_euclid(1.0)).abs().min(0.9999999)
}

pub fn ot_game_angle(ang: f64) -> f64 {
    let a = ang.rem_euclid(1.0);
    if a <= 0.0 {
        return a;
    }
    let banded = a + ot_band(a) * 0.05;
    (banded / (banded.abs() * 0.4 + 1.0)).rem_euclid(1.0)
}
