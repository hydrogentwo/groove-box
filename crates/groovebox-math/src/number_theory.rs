//! Number-theoretic lattices, GOAVA Composer sequence math, entropy draw,
//! and seed-to-pitch identity mapping (exact parity with `groovebox.py`).

use std::f64::consts::{FRAC_PI_2, PI};
use crate::constants::{audible_hz, MEUM, MEUM_INV, MEUM_NORM};
use crate::hash::safe_int_seed_host;
use crate::operator_theory::{math_scale, ot_pow};

pub fn nt_is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 || n == 3 {
        return true;
    }
    if n % 2 == 0 || n % 3 == 0 {
        return false;
    }
    let mut i = 5u64;
    while i * i <= n {
        if n % i == 0 || n % (i + 2) == 0 {
            return false;
        }
        i += 6;
    }
    true
}

pub fn nt_primes(limit: u64) -> Vec<u64> {
    (2..=limit).filter(|&n| nt_is_prime(n)).collect()
}

pub fn nt_euler_totient(mut n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut result = n;
    let mut p = 2u64;
    while p * p <= n {
        if n % p == 0 {
            while n % p == 0 {
                n /= p;
            }
            result -= result / p;
        }
        p += 1;
    }
    if n > 1 {
        result -= result / n;
    }
    result
}

pub fn nt_mobius(mut n: u64) -> i32 {
    if n == 0 {
        return 0;
    }
    if n == 1 {
        return 1;
    }
    let mut prime_factors = 0;
    let mut p = 2u64;
    while p * p <= n {
        if n % p == 0 {
            let mut count = 0;
            while n % p == 0 {
                n /= p;
                count += 1;
            }
            if count > 1 {
                return 0;
            }
            prime_factors += 1;
        }
        p += 1;
    }
    if n > 1 {
        prime_factors += 1;
    }
    if prime_factors % 2 == 0 { 1 } else { -1 }
}

/// Uniform-in-mean entropy draw on `[0, 1]` with irrational Meum offsets.
/// Exact match for `groovebox.entropy_draw_0_1`.
pub fn entropy_draw_0_1(s_abs: f64, s_frac: f64, s_int: i64, vo: usize) -> f64 {
    let v = (s_frac
        + 0.37 * (s_abs * MEUM_NORM + (vo as f64) * MEUM_INV).sin()
        + 0.29 * ((s_int as f64) * MEUM + (vo as f64)).cos())
        .rem_euclid(1.0);
    v.clamp(0.0, 1.0)
}

/// Exact match for `groovebox._seed_to_pitch_ratio`.
pub fn seed_to_pitch_ratio(seed_val: f64, op_idx: usize, step: usize, ot: bool) -> f64 {
    let s = if seed_val.is_finite() { seed_val } else { 0.0 };
    let si = safe_int_seed_host(s);
    let op = (op_idx as i64) & 0x7FFF_FFFF;
    let st = (step as i64) & 0x7FFF_FFFF;
    let mix = (si.wrapping_mul(0x9E37_79B1))
        ^ ((op + 1).wrapping_mul(0x85EB_CA6B))
        ^ ((st + 1).wrapping_mul(0xC2B2_AE35))
        ^ ((si >> 3).wrapping_mul(0x27D4_EB2D));
    let h = safe_int_seed_host(mix as f64);
    let cents = (h.rem_euclid(10800) as f64) - 5400.0;
    let exp = cents / 1200.0;
    let mut ratio = if ot {
        let mut r = ot_pow(2.0, exp);
        if cents < 0.0 && r > 0.0 {
            r = if r != 0.0 { 1.0 / r } else { 1.0 };
        }
        r
    } else {
        2.0_f64.powf(exp)
    };
    if ratio < 1.0 / 32.0 {
        ratio = 1.0 / 32.0;
    }
    if ratio > 32.0 {
        ratio = 32.0;
    }
    ratio
}

pub fn seed_to_fundamental_hz(seed_val: f64, base_hz: f64, op_idx: usize, step: usize, ot: bool) -> f64 {
    let base = if base_hz.is_finite() && base_hz > 0.0 { base_hz } else { 432.0 };
    audible_hz(base * seed_to_pitch_ratio(seed_val, op_idx, step, ot))
}

/// Exact match for `groovebox.goava_get_note`.
pub fn goava_get_note(number_assigned: f64, step: f64, numbers: &[f64]) -> f64 {
    let nums: Vec<f64> = numbers.iter().copied().filter(|x| x.is_finite()).collect();
    if nums.is_empty() {
        return 0.0;
    }
    let num = number_assigned;
    let mut total = 0.0;
    for &value in &nums {
        let denom = (nums.len() as f64) + (num - value).abs();
        let phase = if value != 0.0 {
            1.0 + (FRAC_PI_2 * num).abs() + FRAC_PI_2 * ((value.abs() + num.abs()) * step)
        } else {
            1.0 + step + (FRAC_PI_2 * num).abs() + FRAC_PI_2 * ((value.abs() + num.abs()) * step)
        };
        total += (1.0 + phase.cos()) / denom;
    }
    total.abs()
}

/// DC-centered GOAVA frequency mapping (`groovebox.goava_frequency`).
pub fn goava_frequency(
    number_assigned: f64,
    step: f64,
    numbers: &[f64],
    base_frequency: f64,
) -> (f64, f64) {
    let raw = goava_get_note(number_assigned, step, numbers);
    let center = if !numbers.is_empty() {
        let sum: f64 = numbers
            .iter()
            .enumerate()
            .map(|(i, &n)| goava_get_note(n, i as f64, numbers))
            .sum();
        sum / (numbers.len() as f64)
    } else {
        0.0
    };
    let raw_c = raw - center;
    let mut ratio = 2.0_f64.powf(raw_c * 2.25);
    if !ratio.is_finite() || ratio <= 0.0 {
        ratio = 1.0;
    }
    let mut freq = base_frequency * ratio;
    if !freq.is_finite() {
        freq = base_frequency;
    }
    (freq, raw_c)
}

/// Continuous irrational GOAVA audio stream (`groovebox.goava_irrational_stream`).
pub fn goava_irrational_stream(
    t_values: &[f64],
    numbers: &[f64],
    base_frequency: f64,
    channel: usize,
    ot: bool,
) -> Vec<f32> {
    let nums: Vec<f64> = numbers.iter().copied().filter(|x| x.is_finite()).collect();
    if nums.is_empty() || t_values.is_empty() {
        return vec![0.0; t_values.len()];
    }
    let num = nums[channel % nums.len()];
    let mut scale = 0.0;
    for &value in &nums {
        scale += 1.0 / ((nums.len() as f64) + (num - value).abs());
    }
    let inv_scale = 1.0 / scale.max(1e-9);
    let gain = (1.0 + MEUM_NORM) * 0.9;
    let step_mul = base_frequency * MEUM_INV * 0.5;

    let mut out = Vec::with_capacity(t_values.len());
    for &t in t_values {
        let step = t * step_mul;
        let ref_step = step + PI;
        let mut note = 0.0;
        let mut ref_v = 0.0;
        for &value in &nums {
            let denom = (nums.len() as f64) + (num - value).abs();
            let base_ph = if value == 0.0 {
                1.0 + step + (FRAC_PI_2 * num).abs()
            } else {
                1.0 + (FRAC_PI_2 * num).abs()
            };
            let k = FRAC_PI_2 * (value.abs() + num.abs());
            note += (1.0 + (base_ph + k * step).cos()) / denom;
            ref_v += (1.0 + (base_ph + k * ref_step).cos()) / denom;
        }
        let d = (note - ref_v) * inv_scale;
        out.push(math_scale(d, gain, ot) as f32);
    }
    out
}
