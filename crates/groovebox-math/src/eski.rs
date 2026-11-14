//! Eski Book Fractal Sets (Scientific Theories and Inventions, p.26),
//! shared `compositional_xyz`, bounded z-iteration (max 12), and
//! `instrument_geometry_mode` cross-engine alignment.

use std::f64::consts::{PI, TAU};
use crate::constants::{MEUM, MEUM_INV, PHI, PHI_INV};
use crate::operator_theory::{ot_add, ot_pow, ot_prod};

pub const ESKI_FRACTAL_SET_NAMES: [&str; 6] = [
    "divergent_space", // Y = x*c + c
    "wormhole",        // Y = x^c + x
    "wormhill",        // Y = x + c
    "worms",           // Y = c * root(x)
    "star",            // Y = c^x
    "starburst",       // Y = root(c) * x
];

pub const ESKI_FRACTAL_MAX_ITER: usize = 12;

fn fractal_add(a: f64, b: f64, ot: bool) -> f64 {
    if ot { ot_add(a, b) } else { a + b }
}

fn fractal_mul(a: f64, b: f64, ot: bool) -> f64 {
    if ot { ot_prod(a, b) } else { a * b }
}

fn fractal_pow(base: f64, exp: f64, ot: bool) -> f64 {
    if ot {
        return ot_pow(base, exp);
    }
    if base < 0.0 {
        if exp.fract() == 0.0 {
            let ei = exp as i64;
            let mag = base.abs().powf(exp);
            return if ei.rem_euclid(2) != 0 { -mag } else { mag };
        }
        return f64::NAN;
    }
    base.powf(exp)
}

fn fractal_root(x: f64, n: f64, ot: bool) -> f64 {
    let n = if n == 0.0 { 2.0 } else { n };
    if ot {
        return ot_pow(if x < 0.0 { 0.0 } else { x }, 1.0 / n);
    }
    if x < 0.0 && (n - 2.0).abs() < 1e-12 {
        return f64::NAN;
    }
    (if x < 0.0 { 0.0 } else { x }).powf(1.0 / n)
}

/// Evaluate one Eski book fractal set at `(x, c)`.
pub fn eski_fractal_eval(set_name: &str, x: f64, c: f64, ot: bool) -> f64 {
    let name = set_name.trim().to_lowercase().replace(' ', "_");
    let mut y = match name.as_str() {
        "divergent_space" | "divergent" | "divergent_space_set" => {
            fractal_add(fractal_mul(x, c, ot), c, ot)
        }
        "wormhole" | "wormhole_set" => fractal_add(fractal_pow(x, c, ot), x, ot),
        "wormhill" | "wormhill_set" => fractal_add(x, c, ot),
        "worms" | "worms_set" => fractal_mul(c, fractal_root(x.max(0.0), 2.0, ot), ot),
        "star" | "star_set" => fractal_pow(c, x, ot),
        "starburst" | "starburst_set" => fractal_mul(fractal_root(c.max(0.0), 2.0, ot), x, ot),
        _ => fractal_add(x, c, ot),
    };
    if !y.is_finite() {
        y = 0.0;
    }
    if y.abs() > 1e6 {
        y = 1e6_f64.copysign(y);
    }
    y
}

/// Pick the prevalent Eski book fractal set from seed / sequential numerics / playlist hash.
pub fn eski_fractal_pick(seed_key: i64, sequential_nums: &[f64], playlist_hash: i64) -> &'static str {
    let fallback = [seed_key as f64];
    let seq = if sequential_nums.is_empty() {
        &fallback[..]
    } else {
        sequential_nums
    };
    let mean = seq.iter().map(|v| v.abs()).sum::<f64>() / (seq.len().max(1) as f64);
    let fold = (seq[0] * MEUM).abs().rem_euclid(1.0);
    // Exact parity with Python operator precedence: `int(seed_key) ^ (int(playlist_hash) & 0x7FFFFFFF)`
    let mut h = seed_key ^ (playlist_hash & 0x7FFF_FFFF);
    h ^= ((fold * 1000.0) as i64) & 0xFFFF;
    h ^= ((mean * 100.0) as i64) & 0xFFFF;
    let idx = h.rem_euclid(ESKI_FRACTAL_SET_NAMES.len() as i64) as usize;
    ESKI_FRACTAL_SET_NAMES[idx]
}

pub fn eski_fractal_field_sample(set_name: &str, u: f64, v: f64, c: f64, t: f64, ot: bool) -> f64 {
    let x = u + MEUM * v * (t * MEUM_INV + c).sin();
    eski_fractal_eval(set_name, x, c, ot)
}

/// Shared N-independent compositional `(x, y, z)` for music, video, and game engines.
/// Uses `PHI_HOST` / `PHI_INV` consistent with Python `groovebox.py` / `videogame_engine.py`.
pub fn compositional_xyz(
    seed: f64,
    sequential_nums: &[f64],
    t: f64,
    slot: usize,
    ot: bool,
) -> (f64, f64, f64) {
    let s = if seed.is_finite() { seed } else { 0.0 };
    let i = (slot & 0x7FFF) as f64;
    let s0 = sequential_nums.first().copied().unwrap_or(s);
    let s1 = sequential_nums.get(1).copied().unwrap_or(s * MEUM);
    let s2 = sequential_nums.get(2).copied().unwrap_or(s * PHI);

    let mut x = (s0 * MEUM_INV + i * PHI_INV + t * 0.01).rem_euclid(2.0) - 1.0;
    let mut y = (s1 * MEUM_INV + i * MEUM + t * 0.013).rem_euclid(2.0) - 1.0;
    let mut z = (s2 * MEUM_INV + i * 0.07 + t * 0.008).rem_euclid(2.0) - 1.0;

    if ot {
        x = fractal_add(x, fractal_mul(y, 0.05, true), true);
        y = fractal_add(y, fractal_mul(z, 0.05, true), true);
        z = fractal_add(z, fractal_mul(x, 0.03, true), true);
    }
    (x, y, z)
}

/// Iterate fractal z-values up to `max_iter` (hard-capped at 12).
pub fn eski_fractal_iterate_z(
    set_name: &str,
    x: f64,
    y: f64,
    z0: f64,
    c: f64,
    max_iter: usize,
    ot: bool,
) -> (Vec<f64>, usize, bool) {
    let cap = max_iter.clamp(1, ESKI_FRACTAL_MAX_ITER);
    let mut z = z0;
    let mut zs = Vec::with_capacity(cap);
    let mut escaped = false;
    for _ in 0..cap {
        let xi = fractal_add(fractal_add(x, fractal_mul(MEUM, y, ot), ot), z, ot);
        z = eski_fractal_eval(set_name, xi, c, ot);
        zs.push(z);
        if z.abs() > 8.0 {
            escaped = true;
            break;
        }
    }
    let n_done = zs.len();
    (zs, n_done, escaped)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EngineFlags {
    pub randomizer: bool,
    pub phase_lock: bool,
    pub idealize_rhythm: bool,
    pub seeded: bool,
    pub goava: bool,
}

#[derive(Debug, Clone)]
pub struct GeometryMode {
    pub lattice: f64,
    pub book_set: f64,
    pub phase_lock: f64,
    pub scatter: f64,
    pub goava: f64,
    pub snap: f64,
    pub near_phase_point: bool,
    pub fractal_set: String,
    pub slot: usize,
}

pub fn instrument_geometry_mode(
    slot: usize,
    phase: f64,
    xyz: (f64, f64, f64),
    flags: EngineFlags,
    fractal_set: &str,
) -> GeometryMode {
    let (x, y, z) = xyz;
    let ph = phase.rem_euclid(TAU);
    let d0 = ph.abs().min((ph - TAU).abs());
    let dpi = (ph - PI).abs();
    let near = d0.min(dpi) / PI;
    let snap = 1.0 - near;
    let geo = ((x.abs() + y.abs() + z.abs()) / 3.0).clamp(0.0, 1.0);

    let mut lattice = 0.45 + 0.25 * (1.0 - geo);
    let mut book_set = 0.20 + 0.20 * geo;
    let phase_lock = 0.08 * if flags.phase_lock { 1.5 } else { 0.4 };
    let scatter = 0.08 * if flags.randomizer { 1.5 } else { 0.4 };
    let goava = 0.08 * if flags.goava { 1.6 } else { 0.3 };

    if snap > 0.65 {
        if slot % 2 == 0 {
            lattice += 0.20 * snap;
        } else {
            book_set += 0.20 * snap;
        }
    } else {
        lattice = lattice * (1.0 - 0.3 * near) + book_set * 0.15 * near;
    }

    let sum = lattice + book_set + phase_lock + scatter + goava;
    let denom = if sum == 0.0 { 1.0 } else { sum };

    GeometryMode {
        lattice: lattice / denom,
        book_set: book_set / denom,
        phase_lock: phase_lock / denom,
        scatter: scatter / denom,
        goava: goava / denom,
        snap,
        near_phase_point: snap > 0.65,
        fractal_set: fractal_set.to_string(),
        slot,
    }
}

#[derive(Debug, Clone)]
pub struct CrossEngineAlignmentSlot {
    pub slot: usize,
    pub xyz: (f64, f64, f64),
    pub weights_sum: f64,
    pub z_iters: usize,
    pub escaped: bool,
}

#[derive(Debug, Clone)]
pub struct CrossEngineAlignmentReport {
    pub ok: bool,
    pub fractal_set: String,
    pub slots: Vec<CrossEngineAlignmentSlot>,
    pub errors: Vec<String>,
}

pub fn debug_cross_engine_alignment(
    seed: f64,
    t: f64,
    n_slots: usize,
    ot: bool,
) -> CrossEngineAlignmentReport {
    let seq = [seed, seed * MEUM, seed * PHI];
    let set_name = eski_fractal_pick((seed as i64) & 0x7FFF_FFFF, &seq, 0);
    let mut slots = Vec::with_capacity(n_slots);
    let mut errors = Vec::new();
    for slot in 0..n_slots {
        let xyz = compositional_xyz(seed, &seq, t, slot, ot);
        if !(xyz.0.is_finite() && xyz.1.is_finite() && xyz.2.is_finite()) {
            errors.push(format!("slot {} non-finite xyz", slot));
        }
        let mode = instrument_geometry_mode(slot, t, xyz, EngineFlags::default(), set_name);
        let wsum = mode.lattice + mode.book_set + mode.phase_lock + mode.scatter + mode.goava;
        if (wsum - 1.0).abs() > 1e-3 {
            errors.push(format!("slot {} weights sum {:.6}", slot, wsum));
        }
        let (_, z_iters, escaped) = eski_fractal_iterate_z(
            set_name,
            xyz.0,
            xyz.1,
            xyz.2,
            0.3,
            ESKI_FRACTAL_MAX_ITER,
            ot,
        );
        if z_iters > ESKI_FRACTAL_MAX_ITER {
            errors.push(format!("slot {} z_iters {} > 12", slot, z_iters));
        }
        slots.push(CrossEngineAlignmentSlot {
            slot,
            xyz,
            weights_sum: wsum,
            z_iters,
            escaped,
        });
    }
    CrossEngineAlignmentReport {
        ok: errors.is_empty(),
        fractal_set: set_name.to_string(),
        slots,
        errors,
    }
}
