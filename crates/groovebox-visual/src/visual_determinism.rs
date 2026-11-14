//! Canonical deterministic visual view-space kernel (exact parity with `visual_determinism.py`).
//!
//! No mutable RNG is used. Camera directions come from an equal-area Fibonacci sphere;
//! coverage selection is deterministic greedy max-min; fingerprints match Python SHA-256 bit-for-bit.

use std::f64::consts::TAU;
use groovebox_math::{
    seed_int_f64, sha256_hex, sha256_u64_be, vg_acos, vg_asin, vg_cos, vg_sin, JsonValue,
    GOLDEN_ANGLE,
};

fn unit_u(seed: i64, label: &str) -> f64 {
    let payload = format!("{}|{}", seed, label);
    (sha256_u64_be(payload.as_bytes()) as f64) / 18446744073709551616.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraView {
    pub index: usize,
    pub count: usize,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw_deg: f64,
    pub pitch_deg: f64,
    pub roll_deg: f64,
    pub distance: f64,
    pub fov_deg: f64,
}

impl CameraView {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("index".into(), self.index.into());
        m.insert("count".into(), self.count.into());
        m.insert("x".into(), self.x.into());
        m.insert("y".into(), self.y.into());
        m.insert("z".into(), self.z.into());
        m.insert("yaw_deg".into(), self.yaw_deg.into());
        m.insert("pitch_deg".into(), self.pitch_deg.into());
        m.insert("roll_deg".into(), self.roll_deg.into());
        m.insert("distance".into(), self.distance.into());
        m.insert("fov_deg".into(), self.fov_deg.into());
        JsonValue::Object(m)
    }
}

/// Equal-area Fibonacci sphere camera view with deterministic seed rotation.
pub fn fibonacci_view(index: usize, count: usize, seed: f64) -> CameraView {
    let n = count.max(1);
    let i = index % n;
    let si = seed_int_f64(seed);
    let z = 1.0 - 2.0 * (((i as f64) + 0.5) / (n as f64));
    let r = (1.0 - z * z).max(0.0).sqrt();
    let phase = TAU * unit_u(si, "view_phase");
    let theta = (i as f64) * GOLDEN_ANGLE + phase;
    let x = r * vg_cos(theta);
    let y = r * vg_sin(theta);
    let yaw = x.atan2(z);
    let pitch = vg_asin(y.clamp(-1.0, 1.0));
    CameraView {
        index: i,
        count: n,
        x,
        y,
        z,
        yaw_deg: yaw.to_degrees(),
        pitch_deg: pitch.to_degrees(),
        roll_deg: 0.0,
        distance: 1.0,
        fov_deg: 48.0,
    }
}

/// Spherical angular distance (radians) between two canonical camera views.
pub fn camera_distance(a: &CameraView, b: &CameraView) -> f64 {
    let na = (a.x * a.x + a.y * a.y + a.z * a.z).sqrt();
    let na = if na == 0.0 { 1.0 } else { na };
    let nb = (b.x * b.x + b.y * b.y + b.z * b.z).sqrt();
    let nb = if nb == 0.0 { 1.0 } else { nb };
    let c = ((a.x * b.x + a.y * b.y + a.z * b.z) / (na * nb)).clamp(-1.0, 1.0);
    vg_acos(c)
}

/// Deterministic greedy max-min angular coverage selection over Fibonacci sphere pool.
pub fn select_views(count: usize, seed: f64) -> Vec<CameraView> {
    let n = count.max(1);
    let pool_n = (n * 2).max(32);
    let pool: Vec<CameraView> = (0..pool_n).map(|i| fibonacci_view(i, pool_n, seed)).collect();
    let mut out: Vec<CameraView> = Vec::with_capacity(n);

    while out.len() < n {
        let candidates: Vec<&CameraView> = pool
            .iter()
            .filter(|v| out.iter().all(|e| e.index != v.index))
            .collect();
        if candidates.is_empty() {
            break;
        }
        let best = if out.is_empty() {
            candidates[0]
        } else {
            let mut chosen = candidates[0];
            let mut best_key = (f64::NEG_INFINITY, i64::MIN);
            for &cand in &candidates {
                let min_d = out
                    .iter()
                    .map(|e| camera_distance(cand, e))
                    .fold(f64::INFINITY, f64::min);
                let key = (min_d, -(cand.index as i64));
                if key.0 > best_key.0 || (key.0 == best_key.0 && key.1 > best_key.1) {
                    best_key = key;
                    chosen = cand;
                }
            }
            chosen
        };
        out.push(best.clone());
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstrumentTranslation {
    pub index: usize,
    pub count: usize,
    pub master_slot: usize,
    pub identity: f64,
    pub numeric_unit: f64,
    pub compensation: f64,
    pub phase: f64,
}

impl InstrumentTranslation {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("index".into(), self.index.into());
        m.insert("count".into(), self.count.into());
        m.insert("master_slot".into(), self.master_slot.into());
        m.insert("identity".into(), self.identity.into());
        m.insert("numeric_unit".into(), self.numeric_unit.into());
        m.insert("compensation".into(), self.compensation.into());
        m.insert("phase".into(), self.phase.into());
        JsonValue::Object(m)
    }
}

/// Pure numeric instrument->visual translator (exact parity with `visual_determinism.instruments_handler`).
pub fn instruments_handler(
    index: usize,
    count: usize,
    seed: f64,
    sequential_nums: &[f64],
) -> InstrumentTranslation {
    let n = count.clamp(1, 64);
    let i = index.min(n - 1);
    let master_slot = i;
    let identity = ((master_slot as f64) + 0.5) / 64.0;
    let seq: Vec<f64> = sequential_nums.iter().copied().filter(|v| v.is_finite()).collect();
    let numeric_unit = if !seq.is_empty() {
        let lo = seq.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = seq.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let v = seq[i % seq.len()];
        if hi == lo { 0.5 } else { (v - lo) / (hi - lo) }
    } else {
        unit_u(seed_int_f64(seed), &format!("instrument_numeric:{}", master_slot))
    };
    let compensation = 1.0 / (n as f64);
    let phase = TAU * identity + TAU * numeric_unit;
    InstrumentTranslation {
        index: i,
        count: n,
        master_slot,
        identity,
        numeric_unit,
        compensation,
        phase,
    }
}

pub fn instrument_population(count: usize, seed: f64, sequential_nums: &[f64]) -> Vec<InstrumentTranslation> {
    let n = count.clamp(1, 64);
    (0..n).map(|i| instruments_handler(i, n, seed, sequential_nums)).collect()
}

pub fn quantization_error(unit: f64, pixels: usize) -> f64 {
    let p = pixels.max(1) as f64;
    let q = (unit * p).round() / p;
    (q - unit).abs()
}

#[derive(Debug, Clone, PartialEq)]
pub struct PureVisualObject {
    pub master_slot: usize,
    pub identity: f64,
    pub numeric_unit: f64,
    pub phase_unit: f64,
    pub compensation: f64,
}

impl PureVisualObject {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("master_slot".into(), self.master_slot.into());
        m.insert("identity".into(), self.identity.into());
        m.insert("numeric_unit".into(), self.numeric_unit.into());
        m.insert("phase_unit".into(), self.phase_unit.into());
        m.insert("compensation".into(), self.compensation.into());
        JsonValue::Object(m)
    }
}

pub fn pure_visual_object(index: usize, count: usize, seed: f64, sequential_nums: &[f64]) -> PureVisualObject {
    let m = instruments_handler(index, count, seed, sequential_nums);
    PureVisualObject {
        master_slot: m.master_slot,
        identity: m.identity,
        numeric_unit: m.numeric_unit,
        phase_unit: (m.phase / TAU).rem_euclid(1.0),
        compensation: m.compensation,
    }
}

pub fn pure_visual_population(count: usize, seed: f64, sequential_nums: &[f64]) -> Vec<PureVisualObject> {
    let n = count.clamp(1, 64);
    (0..n).map(|i| pure_visual_object(i, n, seed, sequential_nums)).collect()
}

/// Order-independent SHA-256 composition fingerprint (16 hex chars).
/// Matches `visual_determinism.composition_fingerprint` character-for-character!
pub fn composition_fingerprint(objects: &[JsonValue], seed: f64, abstraction: &str) -> String {
    let mut rows: Vec<(String, JsonValue)> = objects
        .iter()
        .map(|o| {
            let c = o.canonicalize_round12();
            let k = c.to_canonical_string();
            (k, c)
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let sorted: Vec<JsonValue> = rows.into_iter().map(|(_, v)| v).collect();

    let mut payload = JsonValue::obj();
    payload.insert("seed".into(), seed_int_f64(seed).into());
    payload.insert("abstraction".into(), abstraction.into());
    payload.insert("objects".into(), JsonValue::Array(sorted));
    sha256_hex(JsonValue::Object(payload).to_canonical_string().as_bytes())[..16].to_string()
}

pub fn golden_composition_fingerprint(count: usize, seed: f64, sequential_nums: &[f64]) -> String {
    let objs: Vec<JsonValue> = pure_visual_population(count, seed, sequential_nums)
        .into_iter()
        .map(|o| o.to_json())
        .collect();
    composition_fingerprint(&objs, seed, "pure-instrument-visual-v1")
}

/// Cryptographic projection identity of `(seed, composition_fp, view)` (24 hex chars).
/// Matches `visual_determinism.visual_signal_id` character-for-character!
pub fn visual_signal_id(seed: f64, composition_fp: &str, view: &CameraView) -> String {
    let mut payload = JsonValue::obj();
    payload.insert("seed".into(), seed_int_f64(seed).into());
    payload.insert("composition".into(), composition_fp.into());
    payload.insert("view".into(), view.to_json().canonicalize_round12());
    sha256_hex(JsonValue::Object(payload).to_canonical_string().as_bytes())[..24].to_string()
}
