//! Atomic Canonical Composition State (Rust port of `composition_state.py` + Global Play).
//!
//! Guarantees:
//! 1. Deterministic: same seed + `CompositionToggleState` => identical fingerprint & derived state.
//! 2. Order-independent: toggle activation order never mutates canonical memory or derived output.
//! 3. Non-destructive: canonical user composition remains the carrier.
//! 4. Exact parity with Python `composition_state.py` residues and fingerprints.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;
use groovebox_math::{
    eval_expr, eval_script, parse_seed_numeric_list, sha256_hex, EvalContext, JsonValue,
    MEUM_SHORT,
};

pub const TOGGLE_NAMES: [&str; 9] = [
    "randomizer",
    "phaselock",
    "live_randomizer",
    "live_phaselock",
    "goava",
    "pkp_boost",
    "rand_param",
    "apply_algorithm",
    "apply_composition",
];

// =============================================================================
// CompositionToggleState (exact SHA-256 parity with composition_state.py)
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositionToggleState {
    pub seed: i64,
    pub randomizer: bool,
    pub phaselock: bool,
    pub live_randomizer: bool,
    pub live_phaselock: bool,
    pub goava: bool,
    pub pkp_boost: bool,
    pub rand_param: bool,
    pub apply_algorithm: bool,
    pub apply_composition: bool,
    pub version: i32,
}

impl Default for CompositionToggleState {
    fn default() -> Self {
        Self {
            seed: 0,
            randomizer: false,
            phaselock: false,
            live_randomizer: false,
            live_phaselock: false,
            goava: false,
            pkp_boost: false,
            rand_param: false,
            apply_algorithm: false,
            apply_composition: false,
            version: 400,
        }
    }
}

impl CompositionToggleState {
    pub fn get(&self, name: &str) -> Option<bool> {
        match name {
            "randomizer" => Some(self.randomizer),
            "phaselock" => Some(self.phaselock),
            "live_randomizer" => Some(self.live_randomizer),
            "live_phaselock" => Some(self.live_phaselock),
            "goava" => Some(self.goava),
            "pkp_boost" => Some(self.pkp_boost),
            "rand_param" => Some(self.rand_param),
            "apply_algorithm" => Some(self.apply_algorithm),
            "apply_composition" => Some(self.apply_composition),
            _ => None,
        }
    }

    pub fn with_toggle(mut self, name: &str, enabled: bool) -> Result<Self, String> {
        match name {
            "randomizer" => self.randomizer = enabled,
            "phaselock" => self.phaselock = enabled,
            "live_randomizer" => self.live_randomizer = enabled,
            "live_phaselock" => self.live_phaselock = enabled,
            "goava" => self.goava = enabled,
            "pkp_boost" => self.pkp_boost = enabled,
            "rand_param" => self.rand_param = enabled,
            "apply_algorithm" => self.apply_algorithm = enabled,
            "apply_composition" => self.apply_composition = enabled,
            _ => return Err(format!("Unknown composition toggle: {}", name)),
        }
        Ok(self)
    }

    pub fn active_toggles(&self) -> BTreeSet<&'static str> {
        TOGGLE_NAMES
            .iter()
            .copied()
            .filter(|&name| self.get(name).unwrap_or(false))
            .collect()
    }

    /// Exact parity with Python `CompositionToggleState.fingerprint()`.
    pub fn fingerprint(&self) -> String {
        let mut parts: Vec<String> = TOGGLE_NAMES
            .iter()
            .map(|&name| format!("{}={}", name, if self.get(name).unwrap_or(false) { 1 } else { 0 }))
            .collect();
        parts.push(format!("seed={}", self.seed));
        parts.push(format!("version={}", self.version));
        let payload = parts.join("|");
        sha256_hex(payload.as_bytes())[..16].to_string()
    }

    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("seed".into(), self.seed.into());
        for &name in &TOGGLE_NAMES {
            m.insert(name.into(), self.get(name).unwrap_or(false).into());
        }
        m.insert("version".into(), self.version.into());
        m.insert("fingerprint".into(), self.fingerprint().into());
        JsonValue::Object(m)
    }

    pub fn from_json(v: &JsonValue) -> Self {
        let mut s = Self::default();
        if let Some(seed) = v.get("seed").and_then(|x| x.as_i64()) {
            s.seed = seed;
        }
        for &name in &TOGGLE_NAMES {
            if let Some(b) = v.get(name).and_then(|x| x.as_bool()) {
                if let Ok(next) = s.with_toggle(name, b) {
                    s = next;
                }
            }
        }
        if let Some(ver) = v.get("version").and_then(|x| x.as_i64()) {
            s.version = ver as i32;
        }
        s
    }
}

// =============================================================================
// Meum Effect Residue Calculus (exact parity with composition_state.py)
// =============================================================================

/// Exact match for `composition_state.meum_effect_residue(seed, label)`.
pub fn meum_effect_residue(seed: i64, label: &str) -> f64 {
    let h = sha256_hex(format!("{}::{}", seed, label).as_bytes());
    let i = u64::from_str_radix(&h[..13], 16).unwrap_or(0);
    ((i % 10_000_000) as f64) / 10_000_000.0
}

pub fn meum_effect_bank(seed: i64, count: usize, label_prefix: &str) -> Vec<f64> {
    (0..count)
        .map(|k| meum_effect_residue(seed, &format!("{}:{}", label_prefix, k)))
        .collect()
}

#[inline]
pub fn residue_to_bipolar(r: f64) -> f64 {
    r * 2.0 - 1.0
}

#[inline]
pub fn residue_to_sym_drive(r: f64, half_range: f64) -> f64 {
    1.0 + residue_to_bipolar(r) * half_range
}

// =============================================================================
// ToggleLayer & CompositionEngine (order-independent derived rebuild)
// =============================================================================

#[derive(Debug, Clone)]
pub struct ToggleLayer {
    pub name: String,
    pub writes: BTreeMap<String, JsonValue>,
    pub events: Vec<String>,
}

impl ToggleLayer {
    pub fn new(name: impl Into<String>, writes: BTreeMap<String, JsonValue>, events: Vec<String>) -> Self {
        Self {
            name: name.into(),
            writes,
            events,
        }
    }

    pub fn fingerprint(&self) -> String {
        let mut m = JsonValue::obj();
        m.insert("name".into(), self.name.clone().into());
        m.insert("writes".into(), JsonValue::Object(self.writes.clone()));
        m.insert(
            "events".into(),
            JsonValue::Array(self.events.iter().cloned().map(JsonValue::Str).collect()),
        );
        sha256_hex(JsonValue::Object(m).to_canonical_string().as_bytes())[..16].to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct CompositionEngine {
    pub canonical_memory: BTreeMap<String, JsonValue>,
    pub layers: BTreeMap<String, ToggleLayer>,
}

impl CompositionEngine {
    pub fn new(canonical_memory: BTreeMap<String, JsonValue>) -> Self {
        Self {
            canonical_memory,
            layers: BTreeMap::new(),
        }
    }

    pub fn set_layer(&mut self, name: impl Into<String>, writes: BTreeMap<String, JsonValue>, events: Vec<String>) {
        let n = name.into();
        self.layers.insert(n.clone(), ToggleLayer::new(n, writes, events));
    }

    pub fn remove_layer(&mut self, name: &str) {
        self.layers.remove(name);
    }

    pub fn clear_layers(&mut self) {
        self.layers.clear();
    }

    /// Deterministically rebuild derived memory in sorted layer-name order.
    /// Canonical memory is never mutated or deleted from.
    pub fn build(&self) -> BTreeMap<String, JsonValue> {
        let mut result = self.canonical_memory.clone();
        for layer in self.layers.values() {
            for (k, v) in &layer.writes {
                result.insert(k.clone(), v.clone());
            }
        }
        result
    }

    pub fn fingerprint(&self) -> String {
        let derived = self.build();
        let mut m = JsonValue::obj();
        m.insert("derived".into(), JsonValue::Object(derived));
        let layer_fps: Vec<JsonValue> = self
            .layers
            .iter()
            .map(|(name, l)| {
                let mut pair = JsonValue::obj();
                pair.insert("name".into(), name.clone().into());
                pair.insert("fp".into(), l.fingerprint().into());
                JsonValue::Object(pair)
            })
            .collect();
        m.insert("layers".into(), JsonValue::Array(layer_fps));
        sha256_hex(JsonValue::Object(m).to_canonical_string().as_bytes())[..16].to_string()
    }
}

// =============================================================================
// CompositionMemoryLedger & ToggleUndoStack
// =============================================================================

#[derive(Debug, Clone)]
pub struct ToggleLedgerEntry {
    pub toggle: String,
    pub owned_keys: BTreeSet<String>,
    pub overwritten: BTreeMap<String, Option<JsonValue>>,
    pub playlist_events_owned: BTreeSet<(usize, usize)>,
    pub fingerprint_at_apply: String,
}

#[derive(Debug, Clone, Default)]
pub struct CompositionMemoryLedger {
    pub entries: BTreeMap<String, ToggleLedgerEntry>,
}

impl CompositionMemoryLedger {
    pub fn record_apply(&mut self, entry: ToggleLedgerEntry) {
        self.entries.insert(entry.toggle.clone(), entry);
    }

    pub fn record_remove(&mut self, toggle: &str) -> Option<ToggleLedgerEntry> {
        self.entries.remove(toggle)
    }

    pub fn all_owned_keys(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for e in self.entries.values() {
            out.extend(e.owned_keys.iter().cloned());
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct ToggleUndoStack {
    stack: Vec<(CompositionToggleState, BTreeMap<String, JsonValue>)>,
    max_depth: usize,
}

impl Default for ToggleUndoStack {
    fn default() -> Self {
        Self::new(64)
    }
}

impl ToggleUndoStack {
    pub fn new(max_depth: usize) -> Self {
        Self {
            stack: Vec::new(),
            max_depth: max_depth.max(1),
        }
    }

    pub fn push(&mut self, state: CompositionToggleState, snapshot: BTreeMap<String, JsonValue>) {
        self.stack.push((state, snapshot));
        if self.stack.len() > self.max_depth {
            self.stack.remove(0);
        }
    }

    pub fn pop(&mut self) -> Option<(CompositionToggleState, BTreeMap<String, JsonValue>)> {
        self.stack.pop()
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}

// =============================================================================
// DeterministicTriggerSculptor, GoavaVoice, MeumEffectChain, NonRedundantEuclidean
// =============================================================================

#[derive(Debug, Clone)]
pub struct DeterministicTriggerSculptor {
    pub seed: i64,
    pub steps: usize,
    pub masks: BTreeMap<String, Vec<bool>>,
}

impl DeterministicTriggerSculptor {
    pub fn new(seed: i64, instrument_ids: &[String], steps: usize) -> Self {
        let steps = steps.max(1);
        let mut masks = BTreeMap::new();
        for inst in instrument_ids {
            let density = 0.15 + 0.70 * meum_effect_residue(seed, &format!("trig_density::{}", inst));
            let phase = meum_effect_residue(seed, &format!("trig_phase::{}", inst));
            let mut mask = Vec::with_capacity(steps);
            for t in 0..steps {
                let r = meum_effect_residue(seed, &format!("trig::{}::{}", inst, t));
                let prob = density + 0.2 * (TAU * ((t as f64) / (steps as f64) + phase)).sin();
                mask.push(r < prob);
            }
            masks.insert(inst.clone(), mask);
        }
        Self { seed, steps, masks }
    }

    pub fn should_trigger(&self, instrument_id: &str, step: usize) -> bool {
        match self.masks.get(instrument_id) {
            Some(m) if !m.is_empty() => m[step % m.len()],
            _ => true,
        }
    }

    pub fn as_events(&self) -> BTreeMap<String, Vec<usize>> {
        self.masks
            .iter()
            .map(|(inst, m)| {
                let idxs = m.iter().enumerate().filter_map(|(i, &v)| v.then_some(i)).collect();
                (inst.clone(), idxs)
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoavaVoice {
    pub drive: f64,
    pub pitch_shift_semitones: f64,
    pub dc_offset: f64,
}

/// Exact match for `composition_state.compute_goava_params`.
pub fn compute_goava_params(seed: i64, instrument_id: &str, t: f64, _bpm: f64) -> GoavaVoice {
    let t_key = (t * 100.0) as i64;
    let r_drive = meum_effect_residue(seed, &format!("goava_drive::{}::{}", instrument_id, t_key));
    let r_pitch = meum_effect_residue(seed, &format!("goava_pitch::{}::{}", instrument_id, t_key));
    let r_dc = meum_effect_residue(seed, &format!("goava_dc::{}", instrument_id));

    let drive = residue_to_sym_drive(r_drive, 1.4).clamp(0.2, 2.4);
    let pitch = residue_to_bipolar(r_pitch) * 2.0;
    let dc = residue_to_bipolar(r_dc) * 0.02;
    let mod_term = (TAU * t * MEUM_SHORT / 4.0).sin() * 0.15 * residue_to_bipolar(r_pitch);

    GoavaVoice {
        drive: drive + mod_term,
        pitch_shift_semitones: pitch,
        dc_offset: dc,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MeumEffectChain {
    pub seed: i64,
}

impl MeumEffectChain {
    pub fn new(seed: i64) -> Self {
        Self { seed }
    }

    pub fn effect(&self, label: &str, t: f64, base: f64) -> f64 {
        let r = meum_effect_residue(self.seed, label);
        base * (1.0 + residue_to_bipolar(r) * 0.5 * (MEUM_SHORT * t).sin())
    }
}

pub struct NonRedundantEuclidean;

impl NonRedundantEuclidean {
    /// Exact match for `composition_state.NonRedundantEuclidean.generate`.
    pub fn generate(steps: usize, pulses: usize, rotation: i32, seed: i64, label: &str) -> Vec<bool> {
        if steps == 0 {
            return Vec::new();
        }
        let r = meum_effect_residue(seed, &format!("euclid_rot::{}", label));
        let rot = if rotation < 0 {
            (r * (steps as f64)) as usize
        } else {
            rotation as usize
        } % steps;
        let pattern: Vec<bool> = (0..steps).map(|i| (i * pulses) % steps < pulses).collect();
        if rot == 0 {
            return pattern;
        }
        let split = steps - rot;
        let mut out = Vec::with_capacity(steps);
        out.extend_from_slice(&pattern[split..]);
        out.extend_from_slice(&pattern[..split]);
        out
    }
}

// =============================================================================
// Global Play Panel State (Script / Domain / Wire / Params)
// =============================================================================

pub const GLOBAL_DETECTORS: [&str; 8] = [
    "phase", "energy", "spectrum", "goava", "euclidean", "seed", "bpm", "pair",
];

pub const GLOBAL_TARGETS: [&str; 8] = [
    "master_mix", "fractallizer", "eqr", "pkp", "ensemble", "scenograph", "domain", "unison",
];

#[derive(Debug, Clone)]
pub struct GlobalWire {
    pub source: String,
    pub target: String,
    pub amount: f64,
}

#[derive(Debug, Clone)]
pub struct GlobalAlgoParams {
    pub mix: f64,
    pub enable_script: bool,
    pub enable_domain: bool,
    pub enable_wire: bool,
    pub script_amount: f64,
    pub domain_amount: f64,
    pub wire_amount: f64,
}

impl Default for GlobalAlgoParams {
    fn default() -> Self {
        Self {
            mix: 0.35,
            enable_script: true,
            enable_domain: true,
            enable_wire: true,
            script_amount: 0.75,
            domain_amount: 0.75,
            wire_amount: 0.60,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GlobalPlayAlgo {
    pub script: String,
    pub domain: String,
    pub wires: Vec<GlobalWire>,
    pub params: GlobalAlgoParams,
    pub apply_enabled: bool,
}

impl Default for GlobalPlayAlgo {
    fn default() -> Self {
        Self {
            script: "# Global script algo\ndef global_script(t, name, i):\n    v = isn(t * MEUM) * 0.4 + ics(t * PHI) * 0.3\n    return v * 0.35\n".into(),
            domain: "sin(t * MEUM) * 0.35 + cos(t * PHI) * 0.65".into(),
            wires: vec![
                GlobalWire { source: "phase".into(), target: "master_mix".into(), amount: 0.45 },
                GlobalWire { source: "energy".into(), target: "scenograph".into(), amount: 0.65 },
            ],
            params: GlobalAlgoParams::default(),
            apply_enabled: false,
        }
    }
}

impl GlobalPlayAlgo {
    /// Deterministic authoring-only randomizer seeded from `(seed, salt)`.
    /// Always leaves `apply_enabled = false` per README contract!
    pub fn randomized_from_seed(seed: i64, salt: u64) -> Self {
        let s = seed.wrapping_add((salt as i64).wrapping_mul(7919));
        let a = (0.5 + 7.5 * meum_effect_residue(s, "gp:a")).clamp(0.5, 8.0);
        let a = (a * 1000.0).round() / 1000.0;
        let b = ((0.25 + 3.75 * meum_effect_residue(s, "gp:b")) * 1000.0).round() / 1000.0;
        let mix = ((0.20 + 0.65 * meum_effect_residue(s, "gp:mix")) * 1000.0).round() / 1000.0;
        let comp = ((1.0 - mix) * 1000.0).round() / 1000.0;
        let sa = ((0.40 + 0.60 * meum_effect_residue(s, "gp:sa")) * 1000.0).round() / 1000.0;
        let da = ((0.40 + 0.60 * meum_effect_residue(s, "gp:da")) * 1000.0).round() / 1000.0;
        let wa = ((0.25 + 0.75 * meum_effect_residue(s, "gp:wa")) * 1000.0).round() / 1000.0;

        let script_pool = [
            format!("# Global script algo\ndef global_script(t, name, i):\n    return sin(t * MEUM * {a}) * {mix} + cos(t * PHI) * {comp}\n"),
            format!("# Global script algo\ndef global_script(t, name, i):\n    return (MEUM_NORM * sin(t * {a}) + (1 - MEUM_NORM) * cos(t * {b})) * {mix}\n"),
            format!("# Global script algo\ndef global_script(t, name, i):\n    v = isn(t * MEUM) * {a} + ics(t * PHI) * {b}\n    return v * {mix}\n"),
            format!("# Global script algo\ndef global_script(t, name, i):\n    if sin(t * MEUM) >= 0:\n        return {mix} * cos(t * {a})\n    return {comp} * sin(t * {b})\n"),
            format!("# Global script algo\ndef global_script(t, name, i):\n    return isn(sin(t * MEUM * {a}) * cos(t * PHI * {b})) * {mix}\n"),
        ];
        let domain_pool = [
            format!("sin(t * MEUM) * {mix} + cos(t * PHI) * {comp}"),
            format!("MEUM_NORM * sin(t * {a}) + (1 - MEUM_NORM) * cos(t * {b})"),
            format!("isn(sin(t * MEUM * {a})) * cos(t * PHI)"),
            format!("log2(abs(sin(t * MEUM)) + 1) * {a} + sqrt(abs(cos(t))) * {b}"),
            format!("sin(t * MEUM * {a}) * cos(t * {b}) + MEUM_INV * sin(t * PHI)"),
        ];

        let si = ((meum_effect_residue(s, "gp:sidx") * script_pool.len() as f64) as usize) % script_pool.len();
        let di = ((meum_effect_residue(s, "gp:didx") * domain_pool.len() as f64) as usize) % domain_pool.len();
        let n_wires = 1 + ((meum_effect_residue(s, "gp:nw") * 4.0) as usize).min(3);
        let mut wires = Vec::new();
        let mut used = BTreeSet::new();
        for w in 0..n_wires {
            let src_i = ((meum_effect_residue(s, &format!("gp:wsrc:{w}")) * GLOBAL_DETECTORS.len() as f64) as usize) % GLOBAL_DETECTORS.len();
            let dst_i = ((meum_effect_residue(s, &format!("gp:wdst:{w}")) * GLOBAL_TARGETS.len() as f64) as usize) % GLOBAL_TARGETS.len();
            if used.insert((src_i, dst_i)) {
                let amt = ((0.15 + 0.80 * meum_effect_residue(s, &format!("gp:wamt:{w}"))) * 1000.0).round() / 1000.0;
                wires.push(GlobalWire {
                    source: GLOBAL_DETECTORS[src_i].into(),
                    target: GLOBAL_TARGETS[dst_i].into(),
                    amount: amt,
                });
            }
        }

        Self {
            script: script_pool[si].clone(),
            domain: domain_pool[di].clone(),
            wires,
            params: GlobalAlgoParams {
                mix,
                enable_script: true,
                enable_domain: true,
                enable_wire: true,
                script_amount: sa,
                domain_amount: da,
                wire_amount: wa,
            },
            apply_enabled: false, // Authoring-only per spec
        }
    }

    /// Evaluate combined scalar modulation at `(t, seed, index, bpm)` when `apply_enabled`.
    pub fn evaluate_modulation(&self, t: f64, seed: f64, index: usize, bpm: f64, ot: bool) -> f64 {
        if !self.apply_enabled {
            return 0.0;
        }
        let ctx = EvalContext::new(t, seed, index, bpm, ot);
        let mut acc = 0.0;
        if self.params.enable_script && !self.script.trim().is_empty() {
            if let Ok(v) = eval_script(&self.script, &ctx) {
                acc += v * self.params.script_amount;
            }
        }
        if self.params.enable_domain && !self.domain.trim().is_empty() {
            if let Ok(v) = eval_expr(&self.domain, &ctx) {
                acc += v * self.params.domain_amount;
            }
        }
        if self.params.enable_wire {
            let wire_sum: f64 = self
                .wires
                .iter()
                .filter(|w| w.target == "master_mix" || w.target == "ensemble")
                .map(|w| w.amount * 0.25)
                .sum();
            acc += wire_sum * self.params.wire_amount * (t * std::f64::consts::TAU).sin();
        }
        (acc * self.params.mix).clamp(-2.0, 2.0)
    }

    pub fn fingerprint(&self) -> String {
        sha256_hex(self.to_json().to_canonical_string().as_bytes())[..16].to_string()
    }

    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("script".into(), self.script.clone().into());
        m.insert("domain".into(), self.domain.clone().into());
        m.insert("apply_enabled".into(), self.apply_enabled.into());
        let wires: Vec<JsonValue> = self
            .wires
            .iter()
            .map(|w| {
                let mut wm = JsonValue::obj();
                wm.insert("source".into(), w.source.clone().into());
                wm.insert("target".into(), w.target.clone().into());
                wm.insert("amount".into(), w.amount.into());
                JsonValue::Object(wm)
            })
            .collect();
        m.insert("wire".into(), JsonValue::Array(wires));
        let mut pm = JsonValue::obj();
        pm.insert("mix".into(), self.params.mix.into());
        pm.insert("enable_script".into(), self.params.enable_script.into());
        pm.insert("enable_domain".into(), self.params.enable_domain.into());
        pm.insert("enable_wire".into(), self.params.enable_wire.into());
        pm.insert("script_amount".into(), self.params.script_amount.into());
        pm.insert("domain_amount".into(), self.params.domain_amount.into());
        pm.insert("wire_amount".into(), self.params.wire_amount.into());
        m.insert("params".into(), JsonValue::Object(pm));
        JsonValue::Object(m)
    }

    pub fn from_json(v: &JsonValue) -> Self {
        let mut g = Self::default();
        if let Some(s) = v.get("script").and_then(|x| x.as_str()) {
            g.script = s.to_string();
        }
        if let Some(d) = v.get("domain").and_then(|x| x.as_str()) {
            g.domain = d.to_string();
        }
        if let Some(a) = v.get("apply_enabled").and_then(|x| x.as_bool()) {
            g.apply_enabled = a;
        }
        if let Some(arr) = v.get("wire").and_then(|x| x.as_array()) {
            g.wires = arr
                .iter()
                .filter_map(|w| {
                    let source = w.get("source")?.as_str()?.to_string();
                    let target = w.get("target")?.as_str()?.to_string();
                    let amount = w.get("amount").and_then(|x| x.as_f64()).unwrap_or(0.5);
                    Some(GlobalWire { source, target, amount })
                })
                .collect();
        }
        if let Some(p) = v.get("params") {
            if let Some(x) = p.get("mix").and_then(|x| x.as_f64()) { g.params.mix = x; }
            if let Some(x) = p.get("enable_script").and_then(|x| x.as_bool()) { g.params.enable_script = x; }
            if let Some(x) = p.get("enable_domain").and_then(|x| x.as_bool()) { g.params.enable_domain = x; }
            if let Some(x) = p.get("enable_wire").and_then(|x| x.as_bool()) { g.params.enable_wire = x; }
            if let Some(x) = p.get("script_amount").and_then(|x| x.as_f64()) { g.params.script_amount = x; }
            if let Some(x) = p.get("domain_amount").and_then(|x| x.as_f64()) { g.params.domain_amount = x; }
            if let Some(x) = p.get("wire_amount").and_then(|x| x.as_f64()) { g.params.wire_amount = x; }
        }
        g
    }
}

// =============================================================================
// Full Project Document (Save / Load / Canonical Audio-Visual-Game Source)
// =============================================================================

/// Five per-instrument synth patch parameters + AM/FM/PM/Meum modulation.
#[derive(Debug, Clone)]
pub struct InstrumentPatch {
    pub morph: f64,            // 0.01..10.0
    pub harmonic_freq: f64,    // Hz
    pub chaos: f64,            // 0..1
    pub fold_depth: f64,       // 1..16
    pub harmonic_lattice: f64, // 0..1
    pub waveform: String,      // isn, ics, sine, saw, square, triangle
    pub am_depth: f64,
    pub am_rate: f64,
    pub fm_depth: f64,
    pub fm_rate: f64,
    pub pm_depth: f64,
    pub pm_rate: f64,
    pub pm_feedback: f64,
    pub meum_depth: f64,
    pub gain: f64,
    pub muted: bool,
}

impl Default for InstrumentPatch {
    fn default() -> Self {
        Self {
            morph: 1.2,
            harmonic_freq: 432.0,
            chaos: 0.35,
            fold_depth: 4.0,
            harmonic_lattice: 0.33,
            waveform: "isn".into(),
            am_depth: 0.15,
            am_rate: 1.5,
            fm_depth: 0.12,
            fm_rate: 2.0,
            pm_depth: 0.18,
            pm_rate: 1.2,
            pm_feedback: 0.08,
            meum_depth: 0.45,
            gain: 0.85,
            muted: false,
        }
    }
}

impl InstrumentPatch {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("morph".into(), self.morph.into());
        m.insert("harmonic_freq".into(), self.harmonic_freq.into());
        m.insert("chaos".into(), self.chaos.into());
        m.insert("fold_depth".into(), self.fold_depth.into());
        m.insert("harmonic_lattice".into(), self.harmonic_lattice.into());
        m.insert("waveform".into(), self.waveform.clone().into());
        m.insert("am_depth".into(), self.am_depth.into());
        m.insert("am_rate".into(), self.am_rate.into());
        m.insert("fm_depth".into(), self.fm_depth.into());
        m.insert("fm_rate".into(), self.fm_rate.into());
        m.insert("pm_depth".into(), self.pm_depth.into());
        m.insert("pm_rate".into(), self.pm_rate.into());
        m.insert("pm_feedback".into(), self.pm_feedback.into());
        m.insert("meum_depth".into(), self.meum_depth.into());
        m.insert("gain".into(), self.gain.into());
        m.insert("muted".into(), self.muted.into());
        JsonValue::Object(m)
    }

    pub fn from_json(v: &JsonValue) -> Self {
        let mut p = Self::default();
        if let Some(x) = v.get("morph").and_then(|x| x.as_f64()) { p.morph = x; }
        if let Some(x) = v.get("harmonic_freq").and_then(|x| x.as_f64()) { p.harmonic_freq = x; }
        if let Some(x) = v.get("chaos").and_then(|x| x.as_f64()) { p.chaos = x; }
        if let Some(x) = v.get("fold_depth").and_then(|x| x.as_f64()) { p.fold_depth = x; }
        if let Some(x) = v.get("harmonic_lattice").and_then(|x| x.as_f64()) { p.harmonic_lattice = x; }
        if let Some(x) = v.get("waveform").and_then(|x| x.as_str()) { p.waveform = x.to_string(); }
        if let Some(x) = v.get("am_depth").and_then(|x| x.as_f64()) { p.am_depth = x; }
        if let Some(x) = v.get("am_rate").and_then(|x| x.as_f64()) { p.am_rate = x; }
        if let Some(x) = v.get("fm_depth").and_then(|x| x.as_f64()) { p.fm_depth = x; }
        if let Some(x) = v.get("fm_rate").and_then(|x| x.as_f64()) { p.fm_rate = x; }
        if let Some(x) = v.get("pm_depth").and_then(|x| x.as_f64()) { p.pm_depth = x; }
        if let Some(x) = v.get("pm_rate").and_then(|x| x.as_f64()) { p.pm_rate = x; }
        if let Some(x) = v.get("pm_feedback").and_then(|x| x.as_f64()) { p.pm_feedback = x; }
        if let Some(x) = v.get("meum_depth").and_then(|x| x.as_f64()) { p.meum_depth = x; }
        if let Some(x) = v.get("gain").and_then(|x| x.as_f64()) { p.gain = x; }
        if let Some(x) = v.get("muted").and_then(|x| x.as_bool()) { p.muted = x; }
        p
    }
}

#[derive(Debug, Clone)]
pub struct StepCell {
    pub active: bool,
    pub amp: f64,
    pub pitch_ratio: f64,
}

impl Default for StepCell {
    fn default() -> Self {
        Self {
            active: false,
            amp: 0.85,
            pitch_ratio: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstrumentTrack {
    pub index: usize,
    pub name: String,
    pub patch: InstrumentPatch,
    pub steps: Vec<StepCell>,
}

impl InstrumentTrack {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("index".into(), self.index.into());
        m.insert("name".into(), self.name.clone().into());
        m.insert("patch".into(), self.patch.to_json());
        let steps: Vec<JsonValue> = self
            .steps
            .iter()
            .map(|s| {
                let mut sm = JsonValue::obj();
                sm.insert("active".into(), s.active.into());
                sm.insert("amp".into(), s.amp.into());
                sm.insert("pitch_ratio".into(), s.pitch_ratio.into());
                JsonValue::Object(sm)
            })
            .collect();
        m.insert("steps".into(), JsonValue::Array(steps));
        JsonValue::Object(m)
    }

    pub fn from_json(v: &JsonValue, fallback_index: usize) -> Self {
        let index = v.get("index").and_then(|x| x.as_i64()).unwrap_or(fallback_index as i64) as usize;
        let name = v
            .get("name")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Instrument {}", index + 1));
        let patch = v.get("patch").map(InstrumentPatch::from_json).unwrap_or_default();
        let steps = v
            .get("steps")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|s| StepCell {
                        active: s.get("active").and_then(|x| x.as_bool()).unwrap_or(false),
                        amp: s.get("amp").and_then(|x| x.as_f64()).unwrap_or(0.85),
                        pitch_ratio: s.get("pitch_ratio").and_then(|x| x.as_f64()).unwrap_or(1.0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { index, name, patch, steps }
    }
}

/// Complete Groovebox composition state shared by audio, visuals, video, and game engines.
#[derive(Debug, Clone)]
pub struct ProjectState {
    pub bpm: f64,
    pub base_freq_hz: f64,
    pub sample_rate: u32,
    pub seq_length: usize,
    pub playlist_rows: usize,
    pub n_instruments: usize,
    pub seed_input: String,
    pub seed_weight: f64,
    pub fullweight_seed: bool,
    pub full_unison: bool,
    pub unison_blend: f64,
    pub master_volume: f64,
    pub operator_theory: bool,
    pub fractallizer_amount: f64,
    pub fractallizer_gamma: f64,
    pub eqr_amount: f64,
    pub pkp_decay: f64,
    pub dj_goava_amount: f64,
    pub dj_random_amount: f64,
    pub dj_pair: (usize, usize),
    pub dj_boost_amount: f64,
    pub toggles: CompositionToggleState,
    pub global_play: GlobalPlayAlgo,
    pub tracks: Vec<InstrumentTrack>,
}

impl Default for ProjectState {
    fn default() -> Self {
        let mut p = Self {
            bpm: 120.0,
            base_freq_hz: 432.0,
            sample_rate: 48000,
            seq_length: 16,
            playlist_rows: 32,
            n_instruments: 8,
            seed_input: "42".into(),
            seed_weight: 0.72,
            fullweight_seed: true,
            full_unison: false,
            unison_blend: 0.55,
            master_volume: 0.80,
            operator_theory: true,
            fractallizer_amount: 0.33,
            fractallizer_gamma: 1.83,
            eqr_amount: 0.25,
            pkp_decay: 0.35,
            dj_goava_amount: 0.0,
            dj_random_amount: 0.0,
            dj_pair: (0, 1),
            dj_boost_amount: 0.0,
            toggles: CompositionToggleState {
                seed: 42,
                ..Default::default()
            },
            global_play: GlobalPlayAlgo::default(),
            tracks: Vec::new(),
        };
        p.rebuild_default_tracks();
        p
    }
}

impl ProjectState {
    /// Parse primary numeric seed (`0.0` when blank or zero = no-seed mode).
    pub fn numeric_seed(&self) -> f64 {
        let vals = self.sequential_seed_values(0.0);
        vals.first().copied().unwrap_or(0.0)
    }

    pub fn sequential_seed_values(&self, t: f64) -> Vec<f64> {
        parse_seed_numeric_list(&self.seed_input, t, self.operator_theory)
    }

    pub fn rebuild_default_tracks(&mut self) {
        let n = self.n_instruments.clamp(2, 64);
        let steps = self.seq_length.clamp(4, 64);
        let seed_i = self.numeric_seed() as i64;
        self.toggles.seed = seed_i;

        let waveforms = ["isn", "ics", "sine", "triangle", "saw", "square"];
        let mut tracks = Vec::with_capacity(n);
        for i in 0..n {
            let pulses = 2 + ((i * 3 + 1) % (steps.min(9).max(3)));
            let pattern = NonRedundantEuclidean::generate(steps, pulses, -1, seed_i, &format!("tr:{}", i));
            let base_ratio = 2.0_f64.powf(((i as f64 - (n as f64 - 1.0) * 0.5) / (n.max(2) as f64)) * 2.5);
            let mut patch = InstrumentPatch::default();
            patch.harmonic_freq = (self.base_freq_hz * base_ratio).clamp(32.0, 4800.0);
            patch.morph = 0.6 + 2.4 * meum_effect_residue(seed_i, &format!("morph:{}", i));
            patch.chaos = 0.15 + 0.65 * meum_effect_residue(seed_i, &format!("chaos:{}", i));
            patch.fold_depth = 1.5 + 8.5 * meum_effect_residue(seed_i, &format!("fold:{}", i));
            patch.harmonic_lattice = 0.2 + 0.7 * meum_effect_residue(seed_i, &format!("lat:{}", i));
            patch.waveform = waveforms[i % waveforms.len()].to_string();

            let cells = (0..steps)
                .map(|st| {
                    let r = meum_effect_residue(seed_i, &format!("step:{}:{}", i, st));
                    let semi = ((r * 12.0).floor() - 5.0) / 12.0;
                    StepCell {
                        active: pattern.get(st).copied().unwrap_or(false),
                        amp: 0.65 + 0.30 * r,
                        pitch_ratio: 2.0_f64.powf(semi),
                    }
                })
                .collect();

            tracks.push(InstrumentTrack {
                index: i,
                name: format!("Voice {}", i + 1),
                patch,
                steps: cells,
            });
        }
        self.tracks = tracks;
    }

    pub fn canonical_fingerprint(&self) -> String {
        let mut m = JsonValue::obj();
        m.insert("bpm".into(), self.bpm.into());
        m.insert("base_freq_hz".into(), self.base_freq_hz.into());
        m.insert("seq_length".into(), self.seq_length.into());
        m.insert("playlist_rows".into(), self.playlist_rows.into());
        m.insert("seed_input".into(), self.seed_input.clone().into());
        m.insert("seed_weight".into(), self.seed_weight.into());
        m.insert("fullweight_seed".into(), self.fullweight_seed.into());
        m.insert("full_unison".into(), self.full_unison.into());
        m.insert("operator_theory".into(), self.operator_theory.into());
        m.insert("toggles_fp".into(), self.toggles.fingerprint().into());
        m.insert("global_play_fp".into(), self.global_play.fingerprint().into());
        sha256_hex(JsonValue::Object(m).to_canonical_string().as_bytes())[..16].to_string()
    }

    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("bpm".into(), self.bpm.into());
        m.insert("base_freq_hz".into(), self.base_freq_hz.into());
        m.insert("sample_rate".into(), self.sample_rate.into());
        m.insert("seq_length".into(), self.seq_length.into());
        m.insert("playlist_rows".into(), self.playlist_rows.into());
        m.insert("n_instruments".into(), self.n_instruments.into());
        m.insert("seed_input".into(), self.seed_input.clone().into());
        m.insert("seed_numeric".into(), self.numeric_seed().into());
        m.insert("seed_weight".into(), self.seed_weight.into());
        m.insert("fullweight_seed".into(), self.fullweight_seed.into());
        m.insert("full_unison".into(), self.full_unison.into());
        m.insert("unison_blend".into(), self.unison_blend.into());
        m.insert("master_volume".into(), self.master_volume.into());
        m.insert("operator_theory".into(), self.operator_theory.into());
        m.insert("fractallizer_amount".into(), self.fractallizer_amount.into());
        m.insert("fractallizer_gamma".into(), self.fractallizer_gamma.into());
        m.insert("eqr_amount".into(), self.eqr_amount.into());
        m.insert("pkp_decay".into(), self.pkp_decay.into());
        m.insert("dj_goava_amount".into(), self.dj_goava_amount.into());
        m.insert("dj_random_amount".into(), self.dj_random_amount.into());
        m.insert("dj_pair".into(), JsonValue::Array(vec![self.dj_pair.0.into(), self.dj_pair.1.into()]));
        m.insert("dj_boost_amount".into(), self.dj_boost_amount.into());
        m.insert("toggles".into(), self.toggles.to_json());
        m.insert("global_play".into(), self.global_play.to_json());
        m.insert("tracks".into(), JsonValue::Array(self.tracks.iter().map(|t| t.to_json()).collect()));
        m.insert("canonical_fingerprint".into(), self.canonical_fingerprint().into());
        JsonValue::Object(m)
    }

    pub fn from_json(v: &JsonValue) -> Self {
        let mut p = Self::default();
        if let Some(x) = v.get("bpm").and_then(|x| x.as_f64()) { p.bpm = x.clamp(20.0, 480.0); }
        if let Some(x) = v.get("base_freq_hz").and_then(|x| x.as_f64()) { p.base_freq_hz = x.clamp(16.0, 12000.0); }
        if let Some(x) = v.get("sample_rate").and_then(|x| x.as_i64()) { p.sample_rate = (x as u32).clamp(8000, 96000); }
        if let Some(x) = v.get("seq_length").and_then(|x| x.as_i64()) { p.seq_length = (x as usize).clamp(4, 64); }
        if let Some(x) = v.get("playlist_rows").and_then(|x| x.as_i64()) { p.playlist_rows = (x as usize).clamp(4, 256); }
        if let Some(x) = v.get("n_instruments").and_then(|x| x.as_i64()) { p.n_instruments = (x as usize).clamp(2, 64); }
        if let Some(x) = v.get("seed_input").and_then(|x| x.as_str()) { p.seed_input = x.to_string(); }
        if let Some(x) = v.get("seed_weight").and_then(|x| x.as_f64()) { p.seed_weight = x.clamp(0.0, 1.0); }
        if let Some(x) = v.get("fullweight_seed").and_then(|x| x.as_bool()) { p.fullweight_seed = x; }
        if let Some(x) = v.get("full_unison").and_then(|x| x.as_bool()) { p.full_unison = x; }
        if let Some(x) = v.get("unison_blend").and_then(|x| x.as_f64()) { p.unison_blend = x.clamp(0.0, 1.0); }
        if let Some(x) = v.get("master_volume").and_then(|x| x.as_f64()) { p.master_volume = x.clamp(0.0, 2.0); }
        if let Some(x) = v.get("operator_theory").and_then(|x| x.as_bool()) { p.operator_theory = x; }
        if let Some(x) = v.get("fractallizer_amount").and_then(|x| x.as_f64()) { p.fractallizer_amount = x.clamp(0.0, 1.0); }
        if let Some(x) = v.get("fractallizer_gamma").and_then(|x| x.as_f64()) { p.fractallizer_gamma = x.clamp(1.05, 4.0); }
        if let Some(x) = v.get("eqr_amount").and_then(|x| x.as_f64()) { p.eqr_amount = x.clamp(0.0, 1.0); }
        if let Some(x) = v.get("pkp_decay").and_then(|x| x.as_f64()) { p.pkp_decay = x.clamp(0.0, 1.0); }
        if let Some(x) = v.get("dj_goava_amount").and_then(|x| x.as_f64()) { p.dj_goava_amount = x.clamp(0.0, 1.0); }
        if let Some(x) = v.get("dj_random_amount").and_then(|x| x.as_f64()) { p.dj_random_amount = x.clamp(0.0, 1.0); }
        if let Some(arr) = v.get("dj_pair").and_then(|x| x.as_array()) {
            if arr.len() >= 2 {
                let a = arr[0].as_i64().unwrap_or(0).max(0) as usize;
                let b = arr[1].as_i64().unwrap_or(1).max(0) as usize;
                p.dj_pair = (a, b);
            }
        }
        if let Some(x) = v.get("dj_boost_amount").and_then(|x| x.as_f64()) { p.dj_boost_amount = x.clamp(0.0, 1.0); }
        if let Some(t) = v.get("toggles") { p.toggles = CompositionToggleState::from_json(t); }
        p.toggles.seed = p.numeric_seed() as i64;
        if let Some(gp) = v.get("global_play") { p.global_play = GlobalPlayAlgo::from_json(gp); }
        if let Some(arr) = v.get("tracks").and_then(|x| x.as_array()) {
            if !arr.is_empty() {
                p.tracks = arr
                    .iter()
                    .enumerate()
                    .map(|(idx, tv)| InstrumentTrack::from_json(tv, idx))
                    .collect();
                p.n_instruments = p.tracks.len().clamp(2, 64);
            } else {
                p.rebuild_default_tracks();
            }
        } else {
            p.rebuild_default_tracks();
        }
        p
    }
}

// =============================================================================
// CompositionAudit
// =============================================================================

#[derive(Debug, Clone)]
pub struct AuditResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub fingerprint: String,
    pub goava_mean_pitch: f64,
    pub order_independent: bool,
}

pub fn audit_project(project: &ProjectState) -> AuditResult {
    let mut errors = Vec::new();
    let seed = project.toggles.seed;
    let inst_ids: Vec<String> = project.tracks.iter().map(|t| t.name.clone()).collect();

    // 1. Order-independence check on CompositionEngine
    let mut e1 = CompositionEngine::default();
    let mut e2 = CompositionEngine::default();
    let mut wa = BTreeMap::new();
    wa.insert("k_rnd".into(), JsonValue::Float(meum_effect_residue(seed, "rnd")));
    let mut wb = BTreeMap::new();
    wb.insert("k_pl".into(), JsonValue::Float(meum_effect_residue(seed, "pl")));
    e1.set_layer("randomizer", wa.clone(), vec![]);
    e1.set_layer("phaselock", wb.clone(), vec![]);
    e2.set_layer("phaselock", wb, vec![]);
    e2.set_layer("randomizer", wa, vec![]);
    let order_ok = e1.fingerprint() == e2.fingerprint();
    if !order_ok {
        errors.push("CompositionEngine layer ordering is not commutative".into());
    }

    // 2. GOAVA mean pitch bias check
    let mut goava_mean_pitch = 0.0;
    if !inst_ids.is_empty() {
        let pitches: Vec<f64> = inst_ids
            .iter()
            .map(|id| compute_goava_params(seed, id, 0.0, project.bpm).pitch_shift_semitones)
            .collect();
        goava_mean_pitch = pitches.iter().sum::<f64>() / (pitches.len() as f64);
        if project.toggles.goava && inst_ids.len() >= 8 && goava_mean_pitch.abs() > 0.85 {
            errors.push(format!("GOAVA mean pitch bias {:.3} exceeds bound", goava_mean_pitch));
        }
    }

    // 3. Determinism check on trigger sculptor
    let sc_a = DeterministicTriggerSculptor::new(seed, &inst_ids, project.seq_length);
    let sc_b = DeterministicTriggerSculptor::new(seed, &inst_ids, project.seq_length);
    if sc_a.masks != sc_b.masks {
        errors.push("DeterministicTriggerSculptor drifted across identical calls".into());
    }

    AuditResult {
        ok: errors.is_empty(),
        errors,
        fingerprint: project.canonical_fingerprint(),
        goava_mean_pitch,
        order_independent: order_ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toggle_fingerprint_python_parity() {
        let ts = CompositionToggleState {
            seed: 42,
            randomizer: true,
            goava: true,
            ..Default::default()
        };
        assert_eq!(ts.fingerprint(), "b72e9ecb6fb10f37");
    }

    #[test]
    fn test_residue_and_goava_python_parity() {
        assert!((meum_effect_residue(42, "fx:0") - 0.9832998).abs() < 1e-12);
        let gv = compute_goava_params(42, "inst0", 0.5, 120.0);
        assert!((gv.drive - 1.736011137066482).abs() < 1e-12);
        assert!((gv.pitch_shift_semitones - (-0.8889488)).abs() < 1e-12);
        assert!((gv.dc_offset - (-0.006451204)).abs() < 1e-12);
    }

    #[test]
    fn test_euclidean_python_parity() {
        let pat = NonRedundantEuclidean::generate(16, 5, -1, 42, "k");
        let expected = vec![
            false, false, true, false, false, true, false, false,
            false, true, false, false, true, false, false, true,
        ];
        assert_eq!(pat, expected);
    }

    #[test]
    fn test_composition_engine_order_independence() {
        let mut canon = BTreeMap::new();
        canon.insert("user_note".into(), JsonValue::Float(432.0));
        let mut a = CompositionEngine::new(canon.clone());
        let mut b = CompositionEngine::new(canon);
        let mut l1 = BTreeMap::new();
        l1.insert("x".into(), JsonValue::Int(1));
        let mut l2 = BTreeMap::new();
        l2.insert("y".into(), JsonValue::Int(2));
        a.set_layer("randomizer", l1.clone(), vec![]);
        a.set_layer("goava", l2.clone(), vec![]);
        b.set_layer("goava", l2, vec![]);
        b.set_layer("randomizer", l1, vec![]);
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_eq!(a.build(), b.build());
    }

    #[test]
    fn test_global_play_authoring_only() {
        let gp = GlobalPlayAlgo::randomized_from_seed(42, 1);
        assert!(!gp.apply_enabled);
        assert_eq!(gp.evaluate_modulation(1.0, 42.0, 0, 120.0, true), 0.0);
    }
}
