//! Deterministic 1D/2D/3D VideoSynthEngine & InstrumentVisualObject software rasterizer.
//!
//! Honors the Groovebox visual contracts:
//! - Instrument count IS the graphic count (2..64 voices -> 2..64 visual objects)
//! - N-independent identity placement via `identity_unit` & fixed 64-slot lattice
//! - Caller-authoritative alpha (no hidden 1.65x boost)
//! - Explicit deterministic `frame_index`
//! - GOAVA retains its own distinct graphical class when active

use std::f64::consts::TAU;
use groovebox_math::{
    entropy_draw_0_1, goava_get_note, identity_unit, instrument_partial_base_48, meum_power_36,
    safe_int_seed_host, seed_to_pitch_ratio, vg_cos, vg_sin, JsonValue, CANONICAL_MASTER_SLOTS,
    MEUM, MEUM_INV, MEUM_NORM, PHI, PHI_INV,
};
use groovebox_state::{InstrumentTrack, ProjectState};
use crate::visual_determinism::{fibonacci_view, instruments_handler, CameraView};

#[derive(Debug, Clone)]
pub struct CanonicalVisualSlot {
    pub index: usize,
    pub master_slot: usize,
    pub identity: f64,
    pub dimension: usize, // 1, 2, or 3
    pub base_freq: f64,
    pub ratio: f64,
    pub entropy: f64,
    pub depth: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
    pub pack: f64,
    pub verts: usize,
    pub hue: f64,
    pub life: f64,
    pub shade: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl CanonicalVisualSlot {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("index".into(), self.index.into());
        m.insert("master_slot".into(), self.master_slot.into());
        m.insert("identity".into(), self.identity.into());
        m.insert("dimension".into(), self.dimension.into());
        m.insert("base_freq".into(), self.base_freq.into());
        m.insert("ratio".into(), self.ratio.into());
        m.insert("entropy".into(), self.entropy.into());
        m.insert("depth".into(), self.depth.into());
        m.insert("yaw".into(), self.yaw.into());
        m.insert("pitch".into(), self.pitch.into());
        m.insert("roll".into(), self.roll.into());
        m.insert("pack".into(), self.pack.into());
        m.insert("verts".into(), self.verts.into());
        m.insert("hue".into(), self.hue.into());
        m.insert("life".into(), self.life.into());
        m.insert("shade".into(), self.shade.into());
        m.insert("x".into(), self.x.into());
        m.insert("y".into(), self.y.into());
        m.insert("z".into(), self.z.into());
        JsonValue::Object(m)
    }
}

pub fn canonical_visual_instrument(slot: usize, project: &ProjectState, seq_nums: &[f64]) -> CanonicalVisualSlot {
    let i = slot % CANONICAL_MASTER_SLOTS;
    let seedv = project.numeric_seed();
    let inst_map = instruments_handler(i, CANONICAL_MASTER_SLOTS, seedv, seq_nums);
    let fu = project.full_unison;
    let s_int = safe_int_seed_host(seedv).max(1);
    let s_abs = seedv.abs() + 1e-9;
    let s_frac = s_abs - s_abs.floor();

    let n5 = [
        project.toggles.randomizer,
        project.toggles.phaselock,
        project.toggles.apply_composition,
        seedv != 0.0,
        project.toggles.goava,
    ]
    .iter()
    .filter(|&&b| b)
    .count();
    let n_eng = if n5 > 0 { n5 } else { 6 };
    let k5 = 1.0 / (n_eng as f64);

    let tpos = ((i as f64) * MEUM * 3.0).rem_euclid(36.0);
    let tlo = (tpos as usize) % 36;
    let thi = (tlo + 1) % 36;
    let tfr = tpos - tpos.floor();
    let pow_v = meum_power_36(tlo) * (1.0 - tfr) + meum_power_36(thi) * tfr;

    let (bf, sr, ent) = if fu {
        (project.base_freq_hz, 1.0, entropy_draw_0_1(s_abs, s_frac, s_int, 0))
    } else {
        (
            project.base_freq_hz * pow_v,
            seed_to_pitch_ratio(seedv, i, i, project.operator_theory),
            entropy_draw_0_1(s_abs, s_frac, s_int, i),
        )
    };

    let ch_rnd = if project.toggles.randomizer { 0.5 * k5 } else { 0.0 };
    let ch_ph = if project.toggles.phaselock { 0.5 * k5 } else { 0.0 };
    let ch_euc = if project.toggles.apply_composition { 0.5 * k5 } else { 0.0 };
    let ch_seed = if seedv != 0.0 { 0.5 * k5 } else { 0.0 };
    let ch_goa = if project.toggles.goava { 0.5 * k5 } else { 0.0 };

    let yaw = ((i as f64) * PHI * MEUM_NORM + 0.6 * (ch_ph - ch_rnd)) % TAU;
    let pitch = 0.12 * ((i as f64) * MEUM + ch_euc * 1.3).sin();
    let roll = 0.09 * ((i as f64) * MEUM_INV + ch_ph).cos();
    let pack = 0.62 + 0.60 * pow_v * (1.0 + 0.8 * (ch_seed - 0.5 * k5));
    let max_partial = instrument_partial_base_48(i % 48);
    let verts = 4 + (max_partial % 6) + (ch_euc * 6.0) as usize;
    let hue = (360.0 * inst_map.identity + (i as f64) * 7.0 + ch_goa * 90.0).rem_euclid(360.0);

    let ax = ((i as f64) * MEUM_INV).rem_euclid(1.0);
    let conson = if fu { 1.0 } else { 0.5 + 0.5 * (ax * TAU).cos() };
    let depth = 0.96 + 0.72 * (1.0 - conson) + 0.18 * pow_v;
    let shade = (0.35 + 0.55 * (0.5 + 0.5 * (ax * TAU).sin()) * (0.5 + 0.5 * conson)).clamp(0.05, 0.98);
    let life = 0.30 + 0.70 * conson * (0.25 + 0.75 * ent);
    let rad = (2.0 - depth) * (0.30 + 0.45 * pack);

    let id_u = identity_unit(&[
        "instrument-visual",
        &format!("{:.1}", seedv),
        "master",
        &inst_map.master_slot.to_string(),
    ]);
    let dim = (1 + (id_u * 3.0) as usize).clamp(1, 3);

    CanonicalVisualSlot {
        index: i,
        master_slot: inst_map.master_slot,
        identity: id_u,
        dimension: dim,
        base_freq: bf,
        ratio: sr,
        entropy: ent,
        depth,
        yaw,
        pitch,
        roll,
        pack,
        verts,
        hue,
        life,
        shade,
        x: yaw.cos() * rad * pitch.cos(),
        y: pitch.sin() * rad * 0.72,
        z: depth * (0.6 + 0.4 * roll.sin()),
    }
}

fn hsv_to_rgb(h_deg: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let h = (h_deg.rem_euclid(360.0)) / 60.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 1.0 {
        (c, x, 0.0)
    } else if h < 2.0 {
        (x, c, 0.0)
    } else if h < 3.0 {
        (0.0, c, x)
    } else if h < 4.0 {
        (0.0, x, c)
    } else if h < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    ((r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0)
}

/// Software RGB frame buffer with caller-authoritative alpha compositing.
pub struct FrameBuffer {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

impl FrameBuffer {
    pub fn new(width: usize, height: usize, bg: (u8, u8, u8)) -> Self {
        let mut rgb = vec![0u8; width * height * 3];
        for px in rgb.chunks_exact_mut(3) {
            px[0] = bg.0;
            px[1] = bg.1;
            px[2] = bg.2;
        }
        Self { width, height, rgb }
    }

    #[inline]
    pub fn blend_px(&mut self, x: i32, y: i32, col: (f64, f64, f64), alpha: f64) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let a = alpha.clamp(0.0, 1.0);
        let idx = ((y as usize) * self.width + (x as usize)) * 3;
        let r0 = self.rgb[idx] as f64;
        let g0 = self.rgb[idx + 1] as f64;
        let b0 = self.rgb[idx + 2] as f64;
        self.rgb[idx] = (r0 * (1.0 - a) + col.0 * a).clamp(0.0, 255.0) as u8;
        self.rgb[idx + 1] = (g0 * (1.0 - a) + col.1 * a).clamp(0.0, 255.0) as u8;
        self.rgb[idx + 2] = (b0 * (1.0 - a) + col.2 * a).clamp(0.0, 255.0) as u8;
    }

    pub fn draw_dot(&mut self, cx: f64, cy: f64, col: (f64, f64, f64), alpha: f64, radius: i32) {
        let r = radius.max(1);
        let r2 = (r * r) as f64;
        let x0 = cx.round() as i32;
        let y0 = cy.round() as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let d2 = (dx * dx + dy * dy) as f64;
                if d2 <= r2 {
                    let falloff = 1.0 - (d2 / (r2 + 0.5)).sqrt() * 0.5;
                    self.blend_px(x0 + dx, y0 + dy, col, alpha * falloff);
                }
            }
        }
    }

    pub fn draw_line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, col: (f64, f64, f64), alpha: f64) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let steps = dx.abs().max(dy.abs()).ceil() as usize;
        if steps == 0 {
            self.blend_px(x0.round() as i32, y0.round() as i32, col, alpha);
            return;
        }
        let inv = 1.0 / (steps as f64);
        for s in 0..=steps {
            let f = (s as f64) * inv;
            let x = (x0 + dx * f).round() as i32;
            let y = (y0 + dy * f).round() as i32;
            self.blend_px(x, y, col, alpha);
        }
    }

    pub fn fill_tri(
        &mut self,
        p0: (f64, f64),
        p1: (f64, f64),
        p2: (f64, f64),
        col: (f64, f64, f64),
        alpha: f64,
    ) {
        let min_x = p0.0.min(p1.0).min(p2.0).floor().max(0.0) as i32;
        let max_x = p0.0.max(p1.0).max(p2.0).ceil().min((self.width - 1) as f64) as i32;
        let min_y = p0.1.min(p1.1).min(p2.1).floor().max(0.0) as i32;
        let max_y = p0.1.max(p1.1).max(p2.1).ceil().min((self.height - 1) as f64) as i32;

        let edge = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| -> f64 {
            (c.0 - a.0) * (b.1 - a.1) - (c.1 - a.1) * (b.0 - a.0)
        };
        let area = edge(p0, p1, p2);
        if area.abs() < 1e-6 {
            return;
        }
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let p = (x as f64 + 0.5, y as f64 + 0.5);
                let w0 = edge(p1, p2, p);
                let w1 = edge(p2, p0, p);
                let w2 = edge(p0, p1, p);
                if (area > 0.0 && w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0)
                    || (area < 0.0 && w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0)
                {
                    self.blend_px(x, y, col, alpha);
                }
            }
        }
    }
}

pub struct VideoSynthEngine {
    pub width: usize,
    pub height: usize,
    pub camera_yaw: f64,
    pub camera_pitch: f64,
}

impl VideoSynthEngine {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width: width.clamp(64, 1920),
            height: height.clamp(64, 1080),
            camera_yaw: 0.0,
            camera_pitch: 0.0,
        }
    }

    pub fn set_camera_from_view(&mut self, view: &CameraView) {
        self.camera_yaw = view.yaw_deg.to_radians();
        self.camera_pitch = view.pitch_deg.to_radians();
    }

    fn camera_transform(&self, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
        let cy = vg_cos(self.camera_yaw);
        let sy = vg_sin(self.camera_yaw);
        let cp = vg_cos(self.camera_pitch);
        let sp = vg_sin(self.camera_pitch);
        let x1 = x * cy - z * sy;
        let z1 = x * sy + z * cy;
        let y2 = y * cp - z1 * sp;
        let z2 = y * sp + z1 * cp;
        (x1, y2, z2)
    }

    fn draw_instrument(
        &self,
        fb: &mut FrameBuffer,
        slot: &CanonicalVisualSlot,
        track: &InstrumentTrack,
        t: f64,
        energy: f64,
        bands: &[f64],
    ) {
        let w = fb.width as f64;
        let h = fb.height as f64;
        let morph = ((track.patch.morph - 0.01) / 9.99).clamp(0.0, 1.0);
        let hf = ((track.patch.harmonic_freq - 20.0) / 19980.0).clamp(0.0, 1.0);
        let chaos = track.patch.chaos.clamp(0.0, 1.0);
        let fold = ((track.patch.fold_depth - 1.0) / 15.0).clamp(0.0, 1.0);
        let lattice = track.patch.harmonic_lattice.clamp(0.0, 1.0);
        let steps_on = track.steps.iter().filter(|s| s.active).count() as f64;
        let steps_norm = (steps_on / (track.steps.len().max(1) as f64)).clamp(0.0, 1.0);
        let amp_mean = if track.steps.is_empty() {
            0.5
        } else {
            track.steps.iter().map(|s| s.amp).sum::<f64>() / (track.steps.len() as f64)
        };

        let radial = 0.25 + 0.32 * slot.identity.sqrt();
        let angle0 = TAU * slot.identity + slot.yaw;
        let phase = t * (0.5 + 0.5 * morph + 0.5 * steps_norm);
        let wobble = 0.5 * chaos * vg_sin(t * (0.5 + 2.0 * fold) + slot.identity * TAU);

        let cx = w * 0.5 + vg_cos(angle0 + 0.35 * vg_sin(t * 0.5)) * radial * w * 0.72;
        let cy = h * 0.5 + vg_sin(angle0 * MEUM_INV + 0.35 * vg_cos(t * 0.5)) * radial * h * 0.62;
        let band = if bands.is_empty() { 0.35 } else { bands[slot.master_slot % bands.len()] };
        let base_scale = (0.09 + 0.14 * lattice + 0.10 * amp_mean + 0.12 * energy) * w.min(h);
        let scale = (base_scale * (0.72 + 0.5 * band + 0.35 * vg_sin(phase).abs())).max(8.0);
        let hc = (3 + (12.0 * lattice + 4.0 * fold).round() as usize).clamp(3, 18);

        let hue = (360.0 * (slot.identity + 0.31 * hf + 0.17 * lattice + phase / TAU)).rem_euclid(360.0);
        let sat = (0.62 + 0.30 * chaos + 0.08 * lattice).clamp(0.0, 1.0);
        let val = (0.68 + 0.28 * energy + 0.04 * slot.entropy).clamp(0.0, 1.0);
        let col = hsv_to_rgb(hue, sat, val);
        let core = hsv_to_rgb((slot.identity * 360.0 + 180.0).rem_euclid(360.0), 0.20, 1.0);

        let breathe = 0.62 + 0.38 * (0.5 + 0.5 * vg_sin(phase + slot.identity * TAU));
        let alpha = (0.42 + 0.40 * breathe + 0.16 * energy).clamp(0.38, 0.98);

        match slot.dimension {
            1 => {
                // 1D harmonic filament
                let n = hc * 4;
                let mut pts = Vec::with_capacity(n);
                for k in 0..n {
                    let u = (k as f64) / ((n - 1).max(1) as f64);
                    let a = phase + TAU * (u * (1.0 + fold * 0.22));
                    let rr = scale * (0.45 + 0.85 * u) * (1.0 + 0.24 * vg_sin(a * (hc as f64) * 0.23 + t));
                    let x = cx + vg_cos(a) * rr;
                    let y = cy + vg_sin(a * MEUM_INV) * rr * (0.62 + 0.30 * morph);
                    pts.push((x, y));
                }
                for seg in pts.windows(2) {
                    fb.draw_line(seg[0].0, seg[0].1, seg[1].0, seg[1].1, col, 0.60 * alpha);
                }
                fb.draw_dot(cx, cy, core, 0.90 * alpha, 2);
            }
            2 => {
                // 2D closed harmonic surface
                let count = (hc * 4).max(16);
                let mut pts = Vec::with_capacity(count);
                for k in 0..count {
                    let a = phase + TAU * (k as f64) / (count as f64);
                    let mut harm = 0.0;
                    for j in 0..hc.min(8) {
                        harm += (1.0 / ((j + 1) as f64)) * vg_sin(a * ((j + 1) as f64) + phase * (0.2 + (j as f64) * 0.07));
                    }
                    let rr = scale * (0.82 + 0.10 * vg_sin(a * (hc as f64)) + 0.06 * harm + 0.14 * chaos * vg_sin(a * 3.0 + t));
                    pts.push((
                        cx + vg_cos(a) * rr,
                        cy + vg_sin(a * MEUM_INV) * rr * (0.72 + 0.22 * lattice),
                    ));
                }
                for k in (1..pts.len() - 1).step_by(2) {
                    fb.fill_tri(pts[0], pts[k], pts[k + 1], col, 0.14 * alpha);
                }
                for k in 0..pts.len() {
                    let a = pts[k];
                    let b = pts[(k + 1) % pts.len()];
                    fb.draw_line(a.0, a.1, b.0, b.1, col, 0.65 * alpha);
                }
                fb.draw_dot(cx, cy, core, 0.70 * alpha, 2);
            }
            _ => {
                // 3D projected shell: three rings + cross-edges
                let mut rings: Vec<Vec<(f64, f64)>> = Vec::with_capacity(3);
                let count = (hc * 3).max(18);
                for rix in 0..3 {
                    let mut pts = Vec::with_capacity(count);
                    let rr0 = scale * (0.58 + 0.22 * (rix as f64));
                    for k in 0..count {
                        let a = phase + TAU * (k as f64) / (count as f64) + (rix as f64) * 0.21;
                        let z = 0.42 * vg_sin(a * (1.0 + fold * 0.35) + t * 0.8) + wobble;
                        let x3 = rr0 * vg_cos(a);
                        let y3 = rr0 * vg_sin(a * MEUM_INV);
                        let (tx, ty, tz) = self.camera_transform(x3, y3, z * scale * 0.5);
                        let depth = (1.55 + tz / (scale.max(1.0) * 1.5)).max(0.35);
                        let sx = cx + tx / depth;
                        let sy = cy + ty / depth;
                        pts.push((sx, sy));
                    }
                    rings.push(pts);
                }
                for (ri, pts) in rings.iter().enumerate() {
                    let line_a = (0.48 + 0.12 * (ri as f64)) * alpha;
                    for k in 0..pts.len() {
                        let a = pts[k];
                        let b = pts[(k + 1) % pts.len()];
                        fb.draw_line(a.0, a.1, b.0, b.1, col, line_a);
                    }
                    if ri > 0 {
                        let prev = &rings[ri - 1];
                        let step = (pts.len() / 12).max(1);
                        for k in (0..prev.len().min(pts.len())).step_by(step) {
                            fb.draw_line(prev[k].0, prev[k].1, pts[k].0, pts[k].1, col, 0.30 * alpha);
                        }
                    }
                }
                fb.draw_dot(cx, cy, core, 0.80 * alpha, 2);
            }
        }

        // Harmonic spectrum halo
        for j in 0..hc.min(8) {
            let a = phase + TAU * (j as f64) / (hc.max(1) as f64);
            let rr = scale * (1.25 + 0.14 * (j as f64)) * (0.72 + 0.28 * lattice);
            fb.draw_dot(
                cx + vg_cos(a) * rr,
                cy + vg_sin(a * MEUM_INV) * rr * 0.70,
                col,
                0.28 * alpha,
                1 + (2.0 * lattice) as i32,
            );
        }
    }

    /// Render one deterministic RGB frame at explicit `frame_index` and time `t_sec`.
    pub fn render_frame(
        &mut self,
        project: &ProjectState,
        frame_index: usize,
        fps: f64,
        energy: f64,
        bands: &[f64],
    ) -> FrameBuffer {
        let t = (frame_index as f64) / fps.max(1.0);
        let seedv = project.numeric_seed();
        let seq_nums = project.sequential_seed_values(t);
        let view = fibonacci_view(frame_index % 64, 64, seedv);
        self.set_camera_from_view(&view);

        // Deep cosmic dark background subtly tinted by seed
        let bg_h = (seedv.abs() * 37.0 * MEUM).rem_euclid(360.0);
        let (br, bg_g, bb) = hsv_to_rgb(bg_h, 0.35, 0.08);
        let mut fb = FrameBuffer::new(self.width, self.height, (br as u8, bg_g as u8, bb as u8));

        // Subtle deterministic background harmonic grid
        let grid_col = hsv_to_rgb((bg_h + 150.0) % 360.0, 0.45, 0.30);
        let gx_n = 8;
        let gy_n = 6;
        for gx in 1..gx_n {
            let x = (gx as f64) * (self.width as f64) / (gx_n as f64);
            fb.draw_line(x, 0.0, x, (self.height - 1) as f64, grid_col, 0.10);
        }
        for gy in 1..gy_n {
            let y = (gy as f64) * (self.height as f64) / (gy_n as f64);
            fb.draw_line(0.0, y, (self.width - 1) as f64, y, grid_col, 0.10);
        }

        // Draw exactly N canonical visual objects (instrument count == graphic count)
        for (i, track) in project.tracks.iter().enumerate() {
            let slot = canonical_visual_instrument(i, project, &seq_nums);
            self.draw_instrument(&mut fb, &slot, track, t, energy, bands);
        }

        // GOAVA second graphical class: irrational numeric orbital stream
        if project.toggles.goava {
            let nums = if seq_nums.is_empty() { vec![seedv] } else { seq_nums };
            let w = self.width as f64;
            let h = self.height as f64;
            let mut prev: Option<(f64, f64)> = None;
            for k in 0..nums.len().clamp(1, 16) * 8 {
                let nval = nums[k % nums.len()];
                let gn = goava_get_note(nval, t * 2.0 + (k as f64) * 0.25, &nums);
                let ang = (k as f64) * PHI_INV * TAU + t * MEUM;
                let r = (0.12 + 0.32 * (gn.rem_euclid(1.0))) * w.min(h);
                let x = w * 0.5 + vg_cos(ang) * r;
                let y = h * 0.43 + vg_sin(ang) * r * 0.78;
                let gcol = hsv_to_rgb((185.0 + gn * 140.0).rem_euclid(360.0), 0.85, 0.98);
                if let Some((px, py)) = prev {
                    fb.draw_line(px, py, x, y, gcol, 0.45);
                }
                fb.draw_dot(x, y, gcol, 0.85, 2);
                prev = Some((x, y));
            }
        }

        fb
    }
}
