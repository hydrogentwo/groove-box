//! Canonical Meum / Golden Ratio / EQR mathematical constants and lattice utilities.

use std::f64::consts::{E, PI, TAU};

pub const MEUM: f64 = 1.1975807343385265188;
pub const MEUM_SHORT: f64 = 1.19758073433;
pub const MEUM_MINUS_1: f64 = MEUM - 1.0;
pub const MEUM_INV: f64 = 1.0 / MEUM;
pub const MEUM_SQ: f64 = MEUM * MEUM;
pub const MEUM_CUBE: f64 = MEUM_SQ * MEUM;
pub const MEUM_FOURTH: f64 = MEUM_SQ * MEUM_SQ;
pub const MEUM_NORM: f64 = MEUM_MINUS_1 * MEUM_INV; // (M - 1) / M
pub const MEUM_OVER_1_5: f64 = MEUM / 1.5;

pub const PHI: f64 = 1.618033988749895;
pub const PHI_HOST: f64 = 1.6180339887;
pub const PHI_INV: f64 = 0.6180339887498949;
pub const GOLDEN_ANGLE: f64 = TAU * (1.0 - 1.0 / PHI);

pub const E_IRR: f64 = E;
pub const PI_IRR: f64 = PI;
pub const SQRT2: f64 = std::f64::consts::SQRT_2;
pub const SQRT3: f64 = 1.7320508075688772;
pub const SILVER: f64 = 1.0 + SQRT2;

pub const EQR_FINITE_INFINITY: f64 = 134964356.0;

pub const AUDIBLE_LO_HZ: f64 = 5.2;
pub const AUDIBLE_HI_HZ: f64 = 27500.0;
pub const DEFAULT_SAMPLE_RATE: u32 = 48000;
pub const TARGET_SAMPLE_RATE: u32 = 96000;

pub const CANONICAL_MASTER_SLOTS: usize = 64;
pub const ENTROPY_FIXED_POINT: f64 = 0.5;

/// `MEUM ** ((i % 12) - 6)` for `i in 0..36`.
pub fn meum_power_36(i: usize) -> f64 {
    let exp = ((i % 12) as i32) - 6;
    MEUM.powi(exp)
}

/// `int(12 + (i * MEUM) % 24 + (i * PHI_HOST) % 12)` for `i in 0..48`.
pub fn instrument_partial_base_48(i: usize) -> usize {
    let fi = (i % 48) as f64;
    (12.0 + (fi * MEUM).rem_euclid(24.0) + (fi * PHI_HOST).rem_euclid(12.0)) as usize
}

/// `(2*pi*i)/48` for `i in 0..48`.
pub fn instrument_phase_lock_48(i: usize) -> f64 {
    (TAU * ((i % 48) as f64)) / 48.0
}

/// Pass-through frequency — no spectral clamp, no design window.
pub fn audible_hz(freq: f64) -> f64 {
    if !freq.is_finite() {
        return 0.0;
    }
    freq.abs()
}

/// Meum identity residual: `(M-1)*M + (M-1)/M - (2^M / M^2 - M)`.
pub fn meum_identity_residual() -> f64 {
    let lhs = (MEUM_MINUS_1 * MEUM) + (MEUM_MINUS_1 * MEUM_INV);
    let rhs = (2.0_f64.powf(MEUM) / MEUM_SQ) - MEUM;
    lhs - rhs
}

/// Map a live/render index onto the fixed canonical 64-slot lattice.
pub fn canonical_master_slot(index: usize, count: usize) -> usize {
    let n = count.max(1);
    let i = index.min(n - 1);
    if n <= 1 {
        return 0;
    }
    let slot = (i * (CANONICAL_MASTER_SLOTS - 1) + (n - 1) / 2) / (n - 1);
    slot.min(CANONICAL_MASTER_SLOTS - 1)
}

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// Affine bijection on Z/n whenever gcd(a, n) == 1.
pub fn cyclic_permutation(length: usize, key: u64) -> Vec<usize> {
    if length == 0 {
        return Vec::new();
    }
    if length == 1 {
        return vec![0];
    }
    let mut a = ((key % (length as u64 - 1)) as usize) + 1;
    while gcd(a, length) != 1 {
        a = (a % (length - 1)) + 1;
    }
    let b = ((key >> 16) as usize) % length;
    (0..length).map(|i| (a * i + b) % length).collect()
}

pub fn group_orbit_index(i: usize, length: usize, key: u64) -> usize {
    let perm = cyclic_permutation(length, key);
    if perm.is_empty() {
        0
    } else {
        perm[i % perm.len()]
    }
}
