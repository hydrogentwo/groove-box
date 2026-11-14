//! Audio-immutable Fractal Spatial Engine (exact parity with `fractal_spatial_engine.py`).
//!
//! Native four-engine sculpting frame (`randomizer`, `phase_lock`, `euclidean`, `seeded`)
//! projecting into five spatial channels (`position`, `phase`, `scale`, `topology`, `complexity`).
//! GOAVA is strictly an optional post-projection adapter.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;
use groovebox_math::{
    seed_int_str, sha256_hex, sha256_u64_be, vg_cos, vg_sin, JsonValue, GOLDEN_ANGLE, MEUM,
};

pub const SPATIAL_VERSION: &str = "spatial/2026.3-native-four-engine";
pub const SPATIAL_CORE_ENGINES: [&str; 4] = ["randomizer", "phase_lock", "euclidean", "seeded"];
pub const SPATIAL_CHANNELS: [&str; 5] = ["position", "phase", "scale", "topology", "complexity"];

fn residue_u64_seed(seed: u64, label: &str) -> f64 {
    let payload = format!("{}|{}", seed, label);
    (sha256_u64_be(payload.as_bytes()) as f64) / 18446744073709551616.0
}

fn bipolar(x: f64) -> f64 {
    2.0 * x - 1.0
}

fn hash_id(payload: &JsonValue, n: usize) -> String {
    sha256_hex(payload.to_canonical_string().as_bytes())[..n].to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpatialPoint {
    pub id: String,
    pub parent: String,
    pub depth: usize,
    pub index: usize,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub phase: f64,
    pub scale: f64,
    pub topology_class: usize,
    pub complexity: f64,
    pub harmonic: usize,
}

impl SpatialPoint {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("complexity".into(), self.complexity.into());
        m.insert("depth".into(), self.depth.into());
        m.insert("harmonic".into(), self.harmonic.into());
        m.insert("id".into(), self.id.clone().into());
        m.insert("index".into(), self.index.into());
        m.insert("parent".into(), self.parent.clone().into());
        m.insert("phase".into(), self.phase.into());
        m.insert("scale".into(), self.scale.into());
        m.insert("topology_class".into(), self.topology_class.into());
        m.insert("x".into(), self.x.into());
        m.insert("y".into(), self.y.into());
        m.insert("z".into(), self.z.into());
        JsonValue::Object(m)
    }
}

#[derive(Debug, Clone)]
pub struct SpatialSnapshot {
    pub version: String,
    pub seed: u64,
    pub composition_fingerprint: String,
    pub goava_adapter: bool,
    pub goava_required: bool,
    pub points: Vec<SpatialPoint>,
    pub edges: Vec<(String, String)>,
    pub depth: usize,
    pub roots: usize,
    pub fingerprint: String,
}

impl SpatialSnapshot {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("version".into(), self.version.clone().into());
        m.insert("seed".into(), (self.seed as i64).into());
        m.insert("composition_fingerprint".into(), self.composition_fingerprint.clone().into());
        m.insert("goava_adapter".into(), self.goava_adapter.into());
        m.insert("goava_required".into(), self.goava_required.into());
        m.insert(
            "core_engines".into(),
            JsonValue::Array(SPATIAL_CORE_ENGINES.iter().map(|&s| s.into()).collect()),
        );
        m.insert(
            "channels".into(),
            JsonValue::Array(SPATIAL_CHANNELS.iter().map(|&s| s.into()).collect()),
        );
        m.insert(
            "principles".into(),
            JsonValue::Array(vec!["phase_lock".into(), "stochastic_harmonic_expansion".into()]),
        );
        m.insert("points".into(), JsonValue::Array(self.points.iter().map(|p| p.to_json()).collect()));
        m.insert(
            "edges".into(),
            JsonValue::Array(
                self.edges
                    .iter()
                    .map(|(a, b)| JsonValue::Array(vec![a.clone().into(), b.clone().into()]))
                    .collect(),
            ),
        );
        m.insert("infinite".into(), true.into());
        let mut win = JsonValue::obj();
        win.insert("depth".into(), self.depth.into());
        win.insert("roots".into(), self.roots.into());
        m.insert("window".into(), JsonValue::Object(win));
        m.insert("fingerprint".into(), self.fingerprint.clone().into());
        JsonValue::Object(m)
    }
}

#[derive(Debug, Clone)]
pub struct FractalSpatialEngine {
    pub seed: u64,
    pub composition_fingerprint: String,
    pub goava: bool,
}

impl FractalSpatialEngine {
    pub fn new(seed: u64, composition_fingerprint: impl Into<String>, goava: bool) -> Self {
        Self {
            seed,
            composition_fingerprint: composition_fingerprint.into(),
            goava,
        }
    }

    pub fn from_seed_str(seed: &str, composition_fingerprint: impl Into<String>, goava: bool) -> Self {
        Self::new(seed_int_str(seed), composition_fingerprint, goava)
    }

    pub fn engine_residue(&self, engine: &str, depth: usize, index: usize) -> f64 {
        let label = format!("fse/core/{}/{}/{}/{}", engine, depth, index, self.composition_fingerprint);
        residue_u64_seed(self.seed, &label)
    }

    pub fn channel(&self, depth: usize, index: usize, channel: &str) -> f64 {
        let vals = [
            self.engine_residue("randomizer", depth, index),
            self.engine_residue("phase_lock", depth, index),
            self.engine_residue("euclidean", depth, index),
            self.engine_residue("seeded", depth, index),
        ];
        let weights = match channel {
            "position" => [0.46, 0.14, 0.18, 0.22],
            "phase" => [0.12, 0.50, 0.18, 0.20],
            "scale" => [0.24, 0.18, 0.18, 0.40],
            "topology" => [0.20, 0.20, 0.44, 0.16],
            _ => [0.30, 0.18, 0.26, 0.26], // complexity
        };
        let x: f64 = vals.iter().zip(weights.iter()).map(|(v, w)| v * w).sum();
        (vg_sin(TAU * (x + MEUM * (self.seed as f64) * 1e-9)) + 1.0) * 0.5
    }

    pub fn phase_lock(&self, parent_phase: f64, depth: usize, index: usize) -> f64 {
        let r = self.channel(depth, index, "phase");
        let eu = self.engine_residue("euclidean", depth, index);
        let pl = self.engine_residue("phase_lock", depth, index);
        let mut child = parent_phase + GOLDEN_ANGLE * ((1 + index) as f64) + TAU * (eu - 0.5) * 0.5;
        child += TAU * bipolar(pl) * 0.125 + TAU * bipolar(r) * 0.0625;
        child.rem_euclid(TAU)
    }

    pub fn child_count(&self, depth: usize, index: usize) -> usize {
        let c = self.channel(depth, index, "complexity");
        let eu = self.engine_residue("euclidean", depth, index);
        let seeded = self.engine_residue("seeded", depth, index);
        let mult = if depth < 2 { 2.0 } else { 3.0 };
        1 + ((0.55 * c + 0.25 * eu + 0.20 * seeded) * mult) as usize
    }

    pub fn point(
        &self,
        depth: usize,
        index: usize,
        parent: Option<&SpatialPoint>,
        harmonic: Option<usize>,
    ) -> SpatialPoint {
        let parent_id = parent.map(|p| p.id.clone()).unwrap_or_else(|| "ROOT".to_string());
        let pp = parent
            .map(|p| p.phase)
            .unwrap_or_else(|| residue_u64_seed(self.seed, "fse/root_phase") * TAU);
        let phase = self.phase_lock(pp, depth, index);
        let pos_r = self.channel(depth, index, "position");
        let scale_r = self.channel(depth, index, "scale");
        let topo = ((self.channel(depth, index, "topology") * 8.0) as usize) % 8;
        let complexity = self.channel(depth, index, "complexity");
        let h = harmonic.unwrap_or(1 + (depth % 7));
        let radius = (0.22 + 0.78 * scale_r) * 0.72_f64.powi(depth as i32);

        let a = phase + (topo as f64) * GOLDEN_ANGLE * 0.5;
        let elev = bipolar(pos_r) * (0.22 + 0.12 * ((topo % 3) as f64));
        let (bx, by, bz) = parent.map(|p| (p.x, p.y, p.z)).unwrap_or((0.0, 0.0, 0.0));
        let mut dx = radius * vg_cos(a) * vg_cos(elev);
        let dy = radius * vg_sin(elev);
        let mut dz = radius * vg_sin(a) * vg_cos(elev);
        if (topo & 1) != 0 {
            let tmp = dx;
            dx = dz;
            dz = -tmp;
        }

        let mut id_payload = JsonValue::obj();
        id_payload.insert("s".into(), (self.seed as i64).into());
        id_payload.insert("fp".into(), self.composition_fingerprint.clone().into());
        id_payload.insert("d".into(), depth.into());
        id_payload.insert("i".into(), index.into());
        id_payload.insert("p".into(), parent_id.clone().into());
        id_payload.insert("h".into(), h.into());
        let sid = hash_id(&JsonValue::Object(id_payload), 24);

        SpatialPoint {
            id: sid,
            parent: parent_id,
            depth,
            index,
            x: bx + dx,
            y: by + dy,
            z: bz + dz,
            phase,
            scale: 0.35 + 1.65 * scale_r,
            topology_class: topo,
            complexity,
            harmonic: h,
        }
    }

    pub fn generate(&self, depth: usize, roots: usize) -> Vec<SpatialPoint> {
        let roots = roots.max(1);
        let mut out = Vec::new();
        let mut frontier: Vec<SpatialPoint> = (0..roots)
            .map(|i| self.point(0, i, None, Some(1)))
            .collect();
        out.extend(frontier.iter().cloned());
        for d in 1..=depth {
            let mut nxt = Vec::new();
            for parent in &frontier {
                let n = self.child_count(d, parent.index);
                for j in 0..n {
                    nxt.push(self.point(d, j, Some(parent), Some(1 + d + j)));
                }
            }
            out.extend(nxt.iter().cloned());
            frontier = nxt;
        }
        out
    }

    pub fn topology_edges(&self, points: &[SpatialPoint]) -> Vec<(String, String)> {
        let ids: BTreeSet<&str> = points.iter().map(|p| p.id.as_str()).collect();
        let mut edges: Vec<(String, String)> = points
            .iter()
            .filter(|p| ids.contains(p.parent.as_str()))
            .map(|p| (p.parent.clone(), p.id.clone()))
            .collect();

        let mut by_parent: BTreeMap<String, Vec<&SpatialPoint>> = BTreeMap::new();
        for p in points {
            by_parent.entry(p.parent.clone()).or_default().push(p);
        }
        // Preserve insertion order for exact Python edge ordering when points are generated natively
        let mut seen_parents = Vec::new();
        let mut seen_set = BTreeSet::new();
        for p in points {
            if seen_set.insert(p.parent.clone()) {
                seen_parents.push(p.parent.clone());
            }
        }
        for parent in seen_parents {
            if let Some(siblings) = by_parent.get_mut(&parent) {
                siblings.sort_by_key(|p| p.index);
                for w in siblings.windows(2) {
                    if w[0].topology_class % 3 == 0 {
                        edges.push((w[0].id.clone(), w[1].id.clone()));
                    }
                }
            }
        }
        edges
    }

    pub fn apply_goava_adapter(&self, points: &[SpatialPoint]) -> Vec<SpatialPoint> {
        if !self.goava {
            return points.to_vec();
        }
        points
            .iter()
            .map(|p| {
                let g = residue_u64_seed(
                    self.seed,
                    &format!("fse/goava-adapter/{}/{}/{}", p.depth, p.index, p.id),
                );
                let mut np = p.clone();
                np.phase = (p.phase + TAU * 0.125 * bipolar(g)).rem_euclid(TAU);
                np
            })
            .collect()
    }

    pub fn snapshot(&self, depth: usize, roots: usize) -> SpatialSnapshot {
        let points = self.generate(depth, roots);
        let edges = self.topology_edges(&points);
        let mut fp_payload = JsonValue::obj();
        fp_payload.insert("seed".into(), (self.seed as i64).into());
        fp_payload.insert("fp".into(), self.composition_fingerprint.clone().into());
        fp_payload.insert("depth".into(), depth.into());
        fp_payload.insert("roots".into(), roots.into());
        fp_payload.insert(
            "points".into(),
            JsonValue::Array(points.iter().map(|p| p.to_json()).collect()),
        );
        fp_payload.insert(
            "edges".into(),
            JsonValue::Array(
                edges
                    .iter()
                    .map(|(a, b)| JsonValue::Array(vec![a.clone().into(), b.clone().into()]))
                    .collect(),
            ),
        );
        let fingerprint = hash_id(&JsonValue::Object(fp_payload), 16);

        SpatialSnapshot {
            version: SPATIAL_VERSION.to_string(),
            seed: self.seed,
            composition_fingerprint: self.composition_fingerprint.clone(),
            goava_adapter: self.goava,
            goava_required: false,
            points,
            edges,
            depth,
            roots,
            fingerprint,
        }
    }
}

pub fn build_spatial_state(
    seed: u64,
    composition_fingerprint: &str,
    depth: usize,
    roots: usize,
    goava: bool,
) -> SpatialSnapshot {
    FractalSpatialEngine::new(seed, composition_fingerprint, goava).snapshot(depth, roots)
}
