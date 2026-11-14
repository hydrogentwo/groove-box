//! Open-World Sandbox runtime, Loom region persistence, slash-command console,
//! MusicBed, LiveSFX, and SequenceInfluence conductor.
//!
//! Open-World Contract:
//! - Player controls movement & look; idle world NEVER auto-spins or auto-walks.
//! - Releasing keys damps velocity smoothly to zero.
//! - Console routes `/sandbox`, `/region`, `/build <kind> <label>`, `/remove`, `/note <text>`, `/report`.

use std::collections::BTreeMap;
use std::f64::consts::TAU;
use groovebox_math::{safe_int_seed_vge, JsonValue, MEUM, MEUM_NORM, PHI};
use crate::chunks::WorldChunkStreamer;
use crate::identity::{meum_game_residue, GameIdentity};

#[derive(Debug, Clone)]
pub struct SequenceInfluence {
    pub seed: i64,
    pub pattern_lengths: [usize; 3],
}

impl SequenceInfluence {
    pub fn new(seed: i64, pattern_lengths: [usize; 3]) -> Self {
        Self { seed, pattern_lengths }
    }

    pub fn update(&self, t: f64) -> (usize, f64, f64) {
        let step = t.max(0.0).floor() as usize;
        let mut motion = 0.0;
        let mut vib = 0.0;
        for (idx, &len) in self.pattern_lengths.iter().enumerate() {
            let l = len.max(1);
            let pos = (step % l) as f64 / (l as f64);
            let r = meum_game_residue(self.seed, &format!("seq_cond:{}:{}", idx, step % l), false);
            motion += (0.5 + 0.5 * (TAU * (pos + r)).sin()) / 3.0;
            vib += (0.25 + 0.25 * (TAU * (t * MEUM + r)).cos()) / 3.0;
        }
        (step, motion.clamp(0.001, 1.0), vib.clamp(0.001, 0.5))
    }
}

/// Deterministic Meum harmonic MusicBed synthesizer (bounded dynamic headroom).
#[derive(Debug, Clone)]
pub struct MusicBed {
    pub seed: i64,
    pub base_hz: f64,
    pub phase: f64,
    pub time: f64,
    pub sequence_control: SequenceInfluence,
    pub sequence_vibration: f64,
}

impl MusicBed {
    pub fn new(seed: i64, base_hz: f64) -> Self {
        Self {
            seed,
            base_hz: base_hz.max(55.0),
            phase: 0.0,
            time: 0.0,
            sequence_control: SequenceInfluence::new(seed, [8, 13, 21]),
            sequence_vibration: 0.1,
        }
    }

    pub fn step(&mut self, dt: f64) -> f32 {
        self.time += dt;
        let (st, motion, vib) = self.sequence_control.update(self.time * 2.0);
        self.sequence_vibration = vib;
        let deg = meum_game_residue(self.seed, &format!("bed_deg:{}", st % 16), false);
        let ratio = 2.0_f64.powf(((deg * 12.0).floor() - 6.0) / 12.0);
        let f = self.base_hz * 0.5 * ratio * (1.0 + 0.015 * (TAU * self.time * MEUM).sin());
        self.phase = (self.phase + TAU * f * dt).rem_euclid(TAU);
        let s1 = self.phase.sin() * 0.35;
        let s2 = (self.phase * MEUM).sin() * 0.18 * motion;
        let s3 = (self.phase * 0.5).sin() * 0.22;
        ((s1 + s2 + s3) * 0.65).clamp(-0.95, 0.95) as f32
    }

    pub fn render_wav_samples(&mut self, sample_rate: u32, seconds: f64) -> Vec<f32> {
        let n = ((sample_rate as f64) * seconds).round() as usize;
        let dt = 1.0 / (sample_rate.max(8000) as f64);
        (0..n).map(|_| self.step(dt)).collect()
    }
}

/// Deterministic one-shot LiveSFX synthesizer.
#[derive(Debug, Clone)]
pub struct LiveSFX {
    pub seed: i64,
    pub active_bursts: Vec<(String, f64, usize)>,
}

impl LiveSFX {
    pub fn new(seed: i64) -> Self {
        Self { seed, active_bursts: Vec::new() }
    }

    pub fn trigger(&mut self, event: &str) {
        let r = meum_game_residue(self.seed, &format!("sfx_trig:{}", event), false);
        let freq = 220.0 + 660.0 * r;
        self.active_bursts.push((event.to_string(), freq, 0));
    }

    pub fn mix(&mut self, n_samples: usize, sample_rate: u32) -> Vec<f32> {
        let mut out = vec![0.0f32; n_samples];
        let sr = sample_rate.max(8000) as f64;
        for (_, freq, pos) in &mut self.active_bursts {
            for i in 0..n_samples {
                let t = ((*pos + i) as f64) / sr;
                if t > 0.25 {
                    break;
                }
                let env = (-t * 14.0).exp();
                let s = (TAU * (*freq) * (1.0 + 0.4 * MEUM_NORM * t) * t).sin() * env * 0.35;
                out[i] = (out[i] + s as f32).clamp(-1.0, 1.0);
            }
            *pos += n_samples;
        }
        self.active_bursts.retain(|(_, _, p)| (*p as f64) / sr < 0.25);
        out
    }
}

#[derive(Debug, Clone)]
pub struct SandboxObject {
    pub id: usize,
    pub kind: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl SandboxObject {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("id".into(), self.id.into());
        m.insert("kind".into(), self.kind.clone().into());
        m.insert("label".into(), self.label.clone().into());
        m.insert("x".into(), self.x.into());
        m.insert("y".into(), self.y.into());
        m.insert("z".into(), self.z.into());
        JsonValue::Object(m)
    }
}

#[derive(Debug, Clone, Default)]
pub struct LoomRegionState {
    pub objects: Vec<SandboxObject>,
    pub notes: Vec<String>,
    pub visit_count: usize,
}

impl LoomRegionState {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("objects".into(), JsonValue::Array(self.objects.iter().map(|o| o.to_json()).collect()));
        m.insert("notes".into(), JsonValue::Array(self.notes.iter().cloned().map(JsonValue::Str).collect()));
        m.insert("visit_count".into(), self.visit_count.into());
        JsonValue::Object(m)
    }
}

#[derive(Debug, Clone)]
pub struct OpenWorldSandbox {
    pub identity: GameIdentity,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub vx: f64,
    pub vy: f64,
    pub vz: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub roll_deg: f64,
    pub time: f64,
    pub next_obj_id: usize,
    pub streamer: WorldChunkStreamer,
    pub regions: BTreeMap<String, LoomRegionState>,
    pub console_log: Vec<String>,
}

impl OpenWorldSandbox {
    pub fn new(identity: GameIdentity) -> Self {
        let seed_i = safe_int_seed_vge(identity.seed);
        let mut sb = Self {
            identity,
            x: 0.0,
            y: 1.8,
            z: 0.0,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            roll_deg: 0.0,
            time: 0.0,
            next_obj_id: 1,
            streamer: WorldChunkStreamer::new(seed_i),
            regions: BTreeMap::new(),
            console_log: vec![
                "Groovebox Open-World Loom Sandbox ready. Type /help for commands.".into(),
            ],
        };
        sb.touch_current_region();
        sb
    }

    pub fn current_region_key(&self) -> String {
        let (cx, cz) = WorldChunkStreamer::world_to_chunk(self.x, self.z);
        format!("loom:{}:{}", cx, cz)
    }

    fn touch_current_region(&mut self) {
        let key = self.current_region_key();
        let reg = self.regions.entry(key).or_default();
        if reg.visit_count == 0 {
            reg.visit_count = 1;
        }
    }

    /// WASD planar movement relative to look yaw + Space/Ctrl fly (`dy`).
    pub fn perspective_move(&mut self, dx_right: f64, dy_up: f64, dz_forward: f64) {
        let speed = 8.0;
        let forward_x = self.yaw.sin();
        let forward_z = self.yaw.cos();
        let right_x = self.yaw.cos();
        let right_z = -self.yaw.sin();
        self.vx += (forward_x * dz_forward + right_x * dx_right) * speed;
        self.vz += (forward_z * dz_forward + right_z * dx_right) * speed;
        self.vy += dy_up * speed * 0.7;
    }

    pub fn mouse_look(&mut self, dyaw: f64, dpitch: f64) {
        self.yaw = (self.yaw + dyaw).rem_euclid(TAU);
        self.pitch = (self.pitch + dpitch).clamp(-1.45, 1.45);
    }

    /// Advance physics by `dt` seconds. Idle state NEVER drifts, spins, or rolls.
    pub fn tick(&mut self, dt: f64) {
        let dt = dt.clamp(0.0, 0.25);
        let prev_region = self.current_region_key();
        self.time += dt;
        self.x += self.vx * dt;
        self.y = (self.y + self.vy * dt).max(0.5);
        self.z += self.vz * dt;
        // Exponential velocity damping so releasing keys brings avatar cleanly to rest
        let damp = (-8.0 * dt).exp();
        self.vx *= damp;
        self.vy *= damp;
        self.vz *= damp;
        if self.vx.abs() < 1e-6 { self.vx = 0.0; }
        if self.vy.abs() < 1e-6 { self.vy = 0.0; }
        if self.vz.abs() < 1e-6 { self.vz = 0.0; }

        let next_region = self.current_region_key();
        if next_region != prev_region {
            self.regions.entry(next_region).or_default().visit_count += 1;
        }
    }

    pub fn sandbox_place(&mut self, kind: &str, label: &str) -> SandboxObject {
        let key = self.current_region_key();
        let obj = SandboxObject {
            id: self.next_obj_id,
            kind: kind.to_string(),
            label: label.to_string(),
            x: self.x + self.yaw.sin() * 2.5,
            y: self.y,
            z: self.z + self.yaw.cos() * 2.5,
        };
        self.next_obj_id += 1;
        self.regions.entry(key).or_default().objects.push(obj.clone());
        obj
    }

    pub fn sandbox_remove_last(&mut self) -> Option<SandboxObject> {
        let key = self.current_region_key();
        self.regions.get_mut(&key).and_then(|r| r.objects.pop())
    }

    pub fn handle_console_command(&mut self, input: &str) -> String {
        let line = input.trim();
        if line.is_empty() {
            return String::new();
        }
        let reply = if !line.starts_with('/') {
            format!("[chat] {}", line)
        } else {
            let mut parts = line.splitn(3, ' ');
            let cmd = parts.next().unwrap_or("");
            let arg1 = parts.next().unwrap_or("");
            let arg2 = parts.next().unwrap_or("");
            match cmd {
                "/sandbox" | "/region" => {
                    let key = self.current_region_key();
                    let reg = self.regions.get(&key);
                    let n_obj = reg.map(|r| r.objects.len()).unwrap_or(0);
                    let n_notes = reg.map(|r| r.notes.len()).unwrap_or(0);
                    let visits = reg.map(|r| r.visit_count).unwrap_or(1);
                    format!("Region {} | visits={} | objects={} | notes={}", key, visits, n_obj, n_notes)
                }
                "/build" => {
                    let kind = if arg1.is_empty() { "crate" } else { arg1 };
                    let label = if arg2.is_empty() { kind } else { arg2 };
                    let obj = self.sandbox_place(kind, label);
                    format!("Placed #{} [{}] '{}' at ({:.1}, {:.1})", obj.id, obj.kind, obj.label, obj.x, obj.z)
                }
                "/remove" => match self.sandbox_remove_last() {
                    Some(o) => format!("Removed #{} [{}] '{}'", o.id, o.kind, o.label),
                    None => "No placed objects in current region.".into(),
                },
                "/note" => {
                    let note = format!("{} {}", arg1, arg2).trim().to_string();
                    if note.is_empty() {
                        "Usage: /note <text>".into()
                    } else {
                        let key = self.current_region_key();
                        self.regions.entry(key.clone()).or_default().notes.push(note.clone());
                        format!("Attached note to {}: \"{}\"", key, note)
                    }
                }
                "/tp" | "/teleport" => {
                    let nx = arg1.parse::<f64>().unwrap_or(0.0);
                    let nz = arg2.parse::<f64>().unwrap_or(0.0);
                    self.x = nx;
                    self.z = nz;
                    self.vx = 0.0;
                    self.vz = 0.0;
                    self.touch_current_region();
                    format!("Teleported to ({:.1}, {:.1}) [{}]", self.x, self.z, self.current_region_key())
                }
                "/report" => {
                    format!(
                        "{} | kind={} | genre={} | topo={} | fp={} | pos=({:.1},{:.1},{:.1}) | chunks={}",
                        self.identity.title,
                        self.identity.software_kind,
                        self.identity.genre,
                        self.identity.topology,
                        self.identity.composition_fingerprint,
                        self.x, self.y, self.z,
                        self.streamer.cached_count()
                    )
                }
                "/help" => "Commands: /sandbox, /region, /build <kind> <label>, /remove, /note <text>, /tp <x> <z>, /report".into(),
                _ => format!("Unknown command '{}'. Type /help.", cmd),
            }
        };
        self.console_log.push(format!("> {}", line));
        self.console_log.push(reply.clone());
        if self.console_log.len() > 80 {
            let drop_n = self.console_log.len() - 80;
            self.console_log.drain(0..drop_n);
        }
        reply
    }

    pub fn snapshot_json(&mut self) -> JsonValue {
        let chunks = self.streamer.chunks_around(self.x, self.z, 3);
        let region_key = self.current_region_key();
        let region = self.regions.get(&region_key).cloned().unwrap_or_default();

        let mut player = JsonValue::obj();
        player.insert("x".into(), self.x.into());
        player.insert("y".into(), self.y.into());
        player.insert("z".into(), self.z.into());
        player.insert("yaw".into(), self.yaw.into());
        player.insert("pitch".into(), self.pitch.into());
        player.insert("roll_deg".into(), self.roll_deg.into());
        player.insert("region_key".into(), region_key.into());

        let mut all_regions = JsonValue::obj();
        for (k, v) in &self.regions {
            all_regions.insert(k.clone(), v.to_json());
        }

        let mut m = JsonValue::obj();
        m.insert("identity".into(), self.identity.to_json());
        m.insert("player".into(), JsonValue::Object(player));
        m.insert("current_region".into(), region.to_json());
        m.insert("regions".into(), JsonValue::Object(all_regions));
        m.insert("chunks".into(), JsonValue::Array(chunks.iter().map(|c| c.to_json()).collect()));
        m.insert("console_log".into(), JsonValue::Array(self.console_log.iter().cloned().map(JsonValue::Str).collect()));
        m.insert("time".into(), self.time.into());
        m.insert("phi_clock".into(), (self.time * PHI).rem_euclid(TAU).into());
        JsonValue::Object(m)
    }
}
