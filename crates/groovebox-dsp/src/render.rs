//! Full deterministic multi-track Groovebox audio renderer.

use groovebox_math::{
    entropy_draw_0_1, goava_irrational_stream, instrument_partial_base_48, meum_power_36,
    ot_master_transform, safe_int_seed_host, seed_to_pitch_ratio, sha256_hex,
    spectral_bands_and_centroid, JsonValue, MEUM_INV, MEUM_NORM,
};
use groovebox_state::{
    compute_goava_params, DeterministicTriggerSculptor, NonRedundantEuclidean, ProjectState,
};
use crate::dj_effects::LiveDJEffects;
use crate::instruments::harmonic_spacing_ratios;
use crate::master_fx::{build_master_follow_env, EQRTensorEngine, MusicFractallizer};
use crate::oscillator::MeumModulatedOscillator;

#[derive(Debug, Clone)]
pub struct RenderSummary {
    pub sample_rate: u32,
    pub duration_sec: f64,
    pub total_samples: usize,
    pub rms: f64,
    pub peak: f64,
    pub start_sample: f32,
    pub centroid_norm: f64,
    pub audio_sha256: String,
    pub canonical_fingerprint: String,
    pub spectrum_bands: Vec<f64>,
    pub waveform_preview: Vec<f32>,
    pub eqr_last_z: f64,
}

impl RenderSummary {
    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("sample_rate".into(), self.sample_rate.into());
        m.insert("duration_sec".into(), self.duration_sec.into());
        m.insert("total_samples".into(), self.total_samples.into());
        m.insert("rms".into(), self.rms.into());
        m.insert("peak".into(), self.peak.into());
        m.insert("start_sample".into(), self.start_sample.into());
        m.insert("centroid_norm".into(), self.centroid_norm.into());
        m.insert("audio_sha256".into(), self.audio_sha256.clone().into());
        m.insert("canonical_fingerprint".into(), self.canonical_fingerprint.clone().into());
        m.insert("eqr_last_z".into(), self.eqr_last_z.into());
        m.insert(
            "spectrum_bands".into(),
            JsonValue::Array(self.spectrum_bands.iter().copied().map(JsonValue::Float).collect()),
        );
        m.insert(
            "waveform_preview".into(),
            JsonValue::Array(
                self.waveform_preview
                    .iter()
                    .map(|&v| JsonValue::Float(v as f64))
                    .collect(),
            ),
        );
        JsonValue::Object(m)
    }
}

#[derive(Debug, Clone)]
pub struct RenderOutput {
    pub samples: Vec<f32>,
    pub summary: RenderSummary,
}

/// Render a deterministic Groovebox mixdown for `bars` bars (typically 1..4 bars for live loop / export).
/// Guaranteed:
/// - Identical `ProjectState` => bit-identical PCM sample buffer (`audio_sha256`)
/// - Sample 0 is 0.0 (phase 0 start contract)
pub fn render_project(project: &ProjectState, bars: usize) -> RenderOutput {
    let sr = project.sample_rate.clamp(8000, 96000);
    let bpm = project.bpm.clamp(20.0, 480.0);
    let bars = bars.clamp(1, 16);
    let steps = project.seq_length.clamp(4, 64);

    let step_sec = 60.0 / bpm / 4.0; // 16th note
    let total_steps = steps * bars;
    let step_samples = ((step_sec * (sr as f64)).round() as usize).max(64);
    let total_samples = step_samples * total_steps;

    let seed_val = project.numeric_seed();
    let s_int = safe_int_seed_host(seed_val);
    let s_abs = seed_val.abs() + 1e-9;
    let s_frac = s_abs - s_abs.floor();
    let ot = project.operator_theory;
    let seq_nums = {
        let v = project.sequential_seed_values(0.0);
        if v.is_empty() { vec![seed_val] } else { v }
    };

    let n_tracks = project.tracks.len().max(1);
    let ratios = harmonic_spacing_ratios(n_tracks);
    let inst_names: Vec<String> = project.tracks.iter().map(|t| t.name.clone()).collect();
    let sculptor = DeterministicTriggerSculptor::new(s_int, &inst_names, steps);

    let mut mix = vec![0.0f64; total_samples];
    let inv_norm = 1.0 / (n_tracks as f64).sqrt().max(1.0);

    for (ti, track) in project.tracks.iter().enumerate() {
        if track.patch.muted || track.patch.gain <= 1e-6 {
            continue;
        }
        let ent = entropy_draw_0_1(s_abs, s_frac, s_int, if project.full_unison { 0 } else { ti });
        let base_partials = (instrument_partial_base_48(ti) / 3).clamp(3, 14);
        let euclid_mask = if project.toggles.live_phaselock || project.toggles.apply_composition {
            let pulses = (2 + (ti * 3) % steps.min(12).max(2)).min(steps);
            Some(NonRedundantEuclidean::generate(steps, pulses, -1, s_int, &track.name))
        } else {
            None
        };

        let mut osc = MeumModulatedOscillator::new(sr as f64, track.patch.harmonic_freq);
        osc.waveform = track.patch.waveform.clone();
        osc.am_depth = track.patch.am_depth;
        osc.am_rate = track.patch.am_rate;
        osc.fm_depth = track.patch.fm_depth;
        osc.fm_rate = track.patch.fm_rate;
        osc.pm_depth = track.patch.pm_depth;
        osc.pm_rate = track.patch.pm_rate;
        osc.pm_feedback = track.patch.pm_feedback;
        osc.meum_depth = track.patch.meum_depth;
        osc.phase = 0.0; // Start phase 0 contract

        let attack = (step_samples / 20).clamp(8, 256);
        let release = (step_samples / 5).clamp(16, 1024);

        for global_step in 0..total_steps {
            let st = global_step % steps;
            let cell = track.steps.get(st);
            let mut active = cell.map(|c| c.active).unwrap_or(false);

            if project.toggles.randomizer || project.toggles.live_randomizer {
                if sculptor.should_trigger(&track.name, st) {
                    active = true;
                }
            }
            if let Some(ref em) = euclid_mask {
                if em[st % em.len()] {
                    active = true;
                }
            }
            if !active {
                osc.sample_index += step_samples as u64;
                continue;
            }

            let step_amp = cell.map(|c| c.amp).unwrap_or(0.8);
            let step_pitch = cell.map(|c| c.pitch_ratio).unwrap_or(1.0);
            let t_step = (global_step as f64) * step_sec;

            // Pitch calculation: canonical base + seed weight + unison + toggles
            let seed_ratio = if seed_val != 0.0 {
                let raw_r = seed_to_pitch_ratio(seed_val, ti, st, ot);
                let w = if project.fullweight_seed {
                    project.seed_weight
                } else {
                    project.seed_weight * 0.5
                };
                // Blend gently in octave domain toward musical range
                let semi_clamped = raw_r.log2().clamp(-2.0, 2.0);
                2.0_f64.powf(semi_clamped * w * 0.45)
            } else {
                1.0
            };

            let unison_target = project.base_freq_hz * step_pitch;
            let voice_base = if project.full_unison {
                unison_target * (1.0 - project.unison_blend)
                    + (project.base_freq_hz * meum_power_36(ti % 36)) * project.unison_blend
            } else {
                track.patch.harmonic_freq * ratios[ti].powf(0.25) * step_pitch * seed_ratio
            };

            let mut freq = voice_base;
            let mut drive = 1.0;
            let mut dc_corr = 0.0;

            if project.toggles.phaselock {
                // Lock onto harmonic multiples of base_freq_hz / 4
                let q = (freq / (project.base_freq_hz * 0.25)).round().max(1.0);
                freq = q * project.base_freq_hz * 0.25;
            }

            if project.toggles.goava {
                let gv = compute_goava_params(s_int, &track.name, t_step, bpm);
                freq *= 2.0_f64.powf(gv.pitch_shift_semitones / 12.0);
                drive *= gv.drive.clamp(0.4, 1.8);
                dc_corr = gv.dc_offset;
            }

            if project.global_play.apply_enabled {
                let gp_mod = project.global_play.evaluate_modulation(t_step, seed_val, ti, bpm, ot);
                freq *= 2.0_f64.powf((gp_mod * 0.35).clamp(-1.0, 1.0));
            }

            osc.frequency = freq.clamp(24.0, (sr as f64) * 0.42);
            let partials = (base_partials + (ent * 4.0) as usize).clamp(2, 16);
            let seg = osc.render(step_samples, partials, track.patch.fold_depth);

            let start = global_step * step_samples;
            let voice_gain = track.patch.gain * step_amp * drive * inv_norm * 0.55;
            for j in 0..step_samples {
                // Smooth step envelope (click-free attack + release)
                let env = if j < attack {
                    (j as f64) / (attack as f64)
                } else if j + release > step_samples {
                    ((step_samples - j) as f64) / (release as f64)
                } else {
                    1.0
                };
                // Subtle phase-fold saturation from chaos parameter
                let raw = (seg[j] as f64) - dc_corr;
                let shaped = if track.patch.chaos > 0.01 {
                    raw * (1.0 - 0.25 * track.patch.chaos)
                        + (raw * (1.0 + track.patch.fold_depth * 0.35 * track.patch.chaos)).sin()
                            * (0.25 * track.patch.chaos)
                } else {
                    raw
                };
                mix[start + j] += shaped * env * voice_gain;
            }
        }
    }

    // Optional GOAVA continuous irrational stream overlay
    if project.toggles.goava && seed_val != 0.0 {
        let t_vals: Vec<f64> = (0..total_samples).map(|i| (i as f64) / (sr as f64)).collect();
        let g_stream = goava_irrational_stream(&t_vals, &seq_nums, project.base_freq_hz, 0, ot);
        for i in 0..total_samples {
            mix[i] += (g_stream[i] as f64) * 0.14;
        }
    }

    // Global Play master overlay modulation
    if project.global_play.apply_enabled {
        let mix_w = project.global_play.params.mix.clamp(0.0, 1.0);
        // Evaluate on a 64-sample control grid for speed & smoothness
        let ctrl_step = 64usize;
        let mut last_m = 0.0;
        for i in 0..total_samples {
            if i % ctrl_step == 0 {
                let t = (i as f64) / (sr as f64);
                last_m = project.global_play.evaluate_modulation(t, seed_val, 0, bpm, ot);
            }
            let gain = 1.0 + 0.25 * mix_w * last_m;
            mix[i] *= gain;
        }
    }

    let mut bus: Vec<f32> = mix.iter().map(|&v| v as f32).collect();

    // Shared PKP isn follow envelope
    let pkp_env = build_master_follow_env(
        total_samples,
        bpm,
        sr,
        project.pkp_decay,
        Some(step_samples * steps),
        Some(step_samples),
    );

    // Master Fractallizer (up to 50% wet, spectral + contextual blend)
    if project.fractallizer_amount > 1e-4 {
        let frac = MusicFractallizer::new(sr);
        let ctx_out = frac.process(&bus, project.fractallizer_amount * 0.65, project.fractallizer_gamma, Some(&pkp_env));
        bus = frac.process_spectral(&ctx_out, project.fractallizer_amount * 0.35, project.fractallizer_gamma, Some(&pkp_env));
    }

    // Master EQR reality tensor (up to 50% wet)
    let mut eqr = EQRTensorEngine::default();
    if project.eqr_amount > 1e-4 {
        bus = eqr.process(&bus, project.eqr_amount, Some(&pkp_env), ot);
    }

    // Live DJ Effects (GOAVA pair morph, Random parametric, Boost hit)
    if project.dj_goava_amount > 1e-4 || project.dj_random_amount > 1e-4 || project.dj_boost_amount > 1e-4 {
        let mut dj = LiveDJEffects::new(sr);
        dj.set_context(seed_val, project.dj_pair, Some(sr));
        dj.amount_goava = project.dj_goava_amount;
        dj.amount_random = project.dj_random_amount;
        if project.dj_boost_amount > 1e-4 {
            dj.set_boost(step_samples * 4, (MEUM_NORM * (step_samples as f64)).floor(), project.dj_boost_amount);
        }
        let goava_scalar = if seed_val != 0.0 {
            (seed_val * MEUM_INV).rem_euclid(4.0) + 0.5
        } else {
            1.0
        };
        bus = dj.process(&bus, 0, goava_scalar, bpm);
    }

    // Optional Operator Theory master re-encoding when apply_algorithm + OT are both ON
    if ot && project.toggles.apply_algorithm && project.toggles.pkp_boost {
        let ot_bus = ot_master_transform(&bus);
        for i in 0..bus.len() {
            bus[i] = bus[i] * 0.75 + ot_bus[i] * 0.25;
        }
    }

    // Master Volume & safe headroom scaling (preserve relative dynamics, prevent digital wrap)
    let vol = project.master_volume as f32;
    let mut max_abs = 0.0f32;
    for s in &mut bus {
        if !s.is_finite() {
            *s = 0.0;
        }
        *s *= vol;
        let a = s.abs();
        if a > max_abs {
            max_abs = a;
        }
    }
    if max_abs > 0.96 {
        let scale = 0.96 / max_abs;
        for s in &mut bus {
            *s *= scale;
        }
    }
    // Enforce start-of-track silence contract: sample 0 == 0.0
    if !bus.is_empty() {
        bus[0] = 0.0;
    }

    // Compute audio fingerprint, RMS, peak, FFT spectrum bands, and waveform preview
    let mut sum_sq = 0.0f64;
    let mut peak = 0.0f64;
    let mut byte_buf = Vec::with_capacity(bus.len() * 4);
    for &s in &bus {
        let sf = s as f64;
        sum_sq += sf * sf;
        peak = peak.max(sf.abs());
        byte_buf.extend_from_slice(&s.to_le_bytes());
    }
    let rms = (sum_sq / (bus.len().max(1) as f64)).sqrt();
    let audio_sha256 = sha256_hex(&byte_buf)[..24].to_string();
    let (spectrum_bands, centroid_norm) = spectral_bands_and_centroid(&bus, sr, 32);

    // 256-point waveform preview for live oscilloscope
    let preview_n = 256usize;
    let waveform_preview: Vec<f32> = if bus.is_empty() {
        vec![0.0; preview_n]
    } else {
        (0..preview_n)
            .map(|i| {
                let idx = (i * bus.len()) / preview_n;
                bus[idx.min(bus.len() - 1)]
            })
            .collect()
    };

    let summary = RenderSummary {
        sample_rate: sr,
        duration_sec: (total_samples as f64) / (sr as f64),
        total_samples,
        rms,
        peak,
        start_sample: bus.first().copied().unwrap_or(0.0),
        centroid_norm,
        audio_sha256,
        canonical_fingerprint: project.canonical_fingerprint(),
        spectrum_bands,
        waveform_preview,
        eqr_last_z: eqr.last_z,
    };

    RenderOutput { samples: bus, summary }
}
