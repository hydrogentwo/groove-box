//! Deterministic WorldChunkStreamer with near/mid/far LOD ladders and bounded cache.

use std::collections::HashMap;
use groovebox_math::{blake2b_8, JsonValue};

pub const CHUNK_SIZE: f64 = 16.0;
pub const NEAR_R: i32 = 2;
pub const MID_R: i32 = 5;
pub const FAR_R: i32 = 8;
pub const MAX_CACHED: usize = 220;

#[derive(Debug, Clone)]
pub struct ChunkProp {
    pub x: f64,
    pub z: f64,
    pub kind: String,
    pub scale: f64,
    pub hue: f64,
}

impl ChunkProp {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("x".into(), self.x.into());
        m.insert("z".into(), self.z.into());
        m.insert("kind".into(), self.kind.clone().into());
        m.insert("scale".into(), self.scale.into());
        m.insert("hue".into(), self.hue.into());
        JsonValue::Object(m)
    }
}

#[derive(Debug, Clone)]
pub struct WorldChunk {
    pub cx: i32,
    pub cz: i32,
    pub lod: u8,
    pub base_x: f64,
    pub base_z: f64,
    pub elev: f64,
    pub biome: f64,
    pub hue: f64,
    pub props: Vec<ChunkProp>,
}

impl WorldChunk {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("cx".into(), (self.cx as i64).into());
        m.insert("cz".into(), (self.cz as i64).into());
        m.insert("lod".into(), (self.lod as i64).into());
        m.insert("base_x".into(), self.base_x.into());
        m.insert("base_z".into(), self.base_z.into());
        m.insert("elev".into(), self.elev.into());
        m.insert("biome".into(), self.biome.into());
        m.insert("hue".into(), self.hue.into());
        m.insert("props".into(), JsonValue::Array(self.props.iter().map(|p| p.to_json()).collect()));
        JsonValue::Object(m)
    }
}

#[derive(Debug, Clone)]
pub struct WorldChunkStreamer {
    pub seed: i64,
    cache: HashMap<(i32, i32, u8), WorldChunk>,
}

impl WorldChunkStreamer {
    pub fn new(seed: i64) -> Self {
        Self {
            seed: seed & 0x7FFF_FFFF,
            cache: HashMap::new(),
        }
    }

    pub fn world_to_chunk(x: f64, z: f64) -> (i32, i32) {
        ((x / CHUNK_SIZE).floor() as i32, (z / CHUNK_SIZE).floor() as i32)
    }

    fn residue(&self, cx: i32, cz: i32, tag: &str) -> f64 {
        let raw = blake2b_8(format!("{}|{}|{}|{}", self.seed, cx, cz, tag).as_bytes());
        (u64::from_be_bytes(raw) as f64) / 18446744073709551616.0
    }

    pub fn build_chunk(&self, cx: i32, cz: i32, lod: u8) -> WorldChunk {
        let base_x = (cx as f64 + 0.5) * CHUNK_SIZE;
        let base_z = (cz as f64 + 0.5) * CHUNK_SIZE;
        let elev = self.residue(cx, cz, "elev");
        let biome = self.residue(cx, cz, "biome");
        let hue = (self.residue(cx, cz, "hue") * 360.0).rem_euclid(360.0);
        let mut props = Vec::new();
        let kinds = ["tree", "rock", "ruin", "crystal", "beacon"];

        if lod == 0 {
            let n = 3 + (self.residue(cx, cz, "nprop") * 6.0) as usize;
            for i in 0..n {
                let px = base_x + (self.residue(cx, cz, &format!("px{}", i)) - 0.5) * CHUNK_SIZE * 0.9;
                let pz = base_z + (self.residue(cx, cz, &format!("pz{}", i)) - 0.5) * CHUNK_SIZE * 0.9;
                let k = kinds[((self.residue(cx, cz, &format!("pk{}", i)) * 5.0) as usize) % 5];
                props.push(ChunkProp {
                    x: px,
                    z: pz,
                    kind: k.into(),
                    scale: 0.4 + self.residue(cx, cz, &format!("ps{}", i)) * 1.6,
                    hue: (hue + self.residue(cx, cz, &format!("ph{}", i)) * 40.0).rem_euclid(360.0),
                });
            }
        } else if lod == 1 {
            let n = 1 + (self.residue(cx, cz, "midn") * 2.0) as usize;
            for i in 0..n {
                props.push(ChunkProp {
                    x: base_x + (self.residue(cx, cz, &format!("mx{}", i)) - 0.5) * CHUNK_SIZE * 0.6,
                    z: base_z + (self.residue(cx, cz, &format!("mz{}", i)) - 0.5) * CHUNK_SIZE * 0.6,
                    kind: "silhouette".into(),
                    scale: 0.8 + self.residue(cx, cz, &format!("ms{}", i)),
                    hue,
                });
            }
        }
        WorldChunk { cx, cz, lod, base_x, base_z, elev, biome, hue, props }
    }

    fn lod_for(cx: i32, cz: i32, pcx: i32, pcz: i32) -> u8 {
        let d = (cx - pcx).abs().max((cz - pcz).abs());
        if d <= NEAR_R {
            0
        } else if d <= MID_R {
            1
        } else {
            2
        }
    }

    pub fn chunks_around(&mut self, player_x: f64, player_z: f64, radius: i32) -> Vec<WorldChunk> {
        let r = radius.clamp(1, FAR_R);
        let (pcx, pcz) = Self::world_to_chunk(player_x, player_z);
        let mut out = Vec::new();
        for dx in -r..=r {
            for dz in -r..=r {
                let cx = pcx + dx;
                let cz = pcz + dz;
                let lod = Self::lod_for(cx, cz, pcx, pcz);
                let key = (cx, cz, lod);
                let chunk = if let Some(existing) = self.cache.get(&key) {
                    existing.clone()
                } else {
                    let built = self.build_chunk(cx, cz, lod);
                    self.cache.insert(key, built.clone());
                    built
                };
                out.push(chunk);
            }
        }
        if self.cache.len() > MAX_CACHED {
            let mut keys: Vec<(i32, i32, u8)> = self.cache.keys().copied().collect();
            keys.sort_by_key(|&(cx, cz, _)| -((cx - pcx).abs().max((cz - pcz).abs())));
            for k in keys.into_iter().take(self.cache.len() - MAX_CACHED) {
                self.cache.remove(&k);
            }
        }
        out
    }

    pub fn cached_count(&self) -> usize {
        self.cache.len()
    }
}
