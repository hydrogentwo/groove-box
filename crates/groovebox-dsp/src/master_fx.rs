//! Master DSP stages: PKP isn envelope, MusicFractallizer (contextual & spectral FFT),
//! HarmonicLattice, and EQRTensorEngine (50% max wet doctrine).

use std::f64::consts::{PI, TAU};
use groovebox_math::{
    book_isn, eqr_tensor_audio, hann_window, irfft_from_mag_phase, math_mul, rfft_mag_phase,
};

/// Exact port of `groovebox.book_isn_envelope_shape(u, decay)`.
pub fn book_isn_envelope_shape(u: &[f64], decay: f64) -> Vec<f32> {
    if u.is_empty() {
        return Vec::new();
    }
    let d = decay.clamp(0.0, 1.0);
    let isn_cycle = |uu: f64| -> f64 { book_isn(uu * TAU) * 0.5 };
    let two_step = |uu: f64| -> f64 {
        let p = uu * 2.0;
        if p <= 0.5 {
            book_isn((p / 0.5) * PI) * 0.5
        } else if p < 1.5 {
            1.0
        } else {
            book_isn(PI - ((p - 1.5) / 0.5) * PI) * 0.5
        }
    };
    let cycle_sustain = |uu: f64| -> f64 {
        let cf = 0.35;
        if uu <= cf {
            isn_cycle(uu / cf)
        } else {
            1.0
        }
    };

    let mut shape: Vec<f64> = u
        .iter()
        .map(|&raw| {
            let uu = raw.clamp(0.0, 1.0);
            if d <= 1e-9 {
                isn_cycle(uu)
            } else if d >= 1.0 - 1e-9 {
                cycle_sustain(uu)
            } else if d <= 0.5 {
                let a = d / 0.5;
                (1.0 - a) * isn_cycle(uu) + a * two_step(uu)
            } else {
                let a = (d - 0.5) / 0.5;
                (1.0 - a) * two_step(uu) + a * cycle_sustain(uu)
            }
        })
        .collect();

    let peak = shape.iter().copied().fold(0.0_f64, f64::max);
    if peak > 1e-12 {
        for v in &mut shape {
            *v /= peak;
        }
    }
    let mean = shape.iter().sum::<f64>() / (shape.len() as f64);
    if mean > 1e-12 {
        let max_s = shape.iter().copied().fold(0.0_f64, f64::max).max(1e-12);
        let bal = (0.50 / mean).min(1.0 / max_s);
        for v in &mut shape {
            *v *= bal;
        }
    }
    shape.into_iter().map(|v| v as f32).collect()
}

/// Exact port of `groovebox.build_master_follow_env`.
pub fn build_master_follow_env(
    n_samples: usize,
    bpm: f64,
    sample_rate: u32,
    pkp_decay: f64,
    row_length_samples: Option<usize>,
    step_length_samples: Option<usize>,
) -> Vec<f32> {
    if n_samples == 0 {
        return vec![1.0];
    }
    let decay = if pkp_decay > 1.0 {
        (pkp_decay / 1000.0).clamp(0.0, 1.0)
    } else {
        pkp_decay.clamp(0.0, 1.0)
    };
    let sr = (sample_rate.max(1)) as f64;
    let beat_samples = sr * (60.0 / bpm.max(1.0));
    let default_step = (beat_samples / 4.0).round().max(1.0) as usize;
    let step = step_length_samples.unwrap_or(default_step).max(1);
    let row = row_length_samples.unwrap_or(default_step * 16).max(step);

    let mut tile = ((step as f64) + decay * ((row - step) as f64)).round() as usize;
    tile = tile.clamp(step, n_samples);
    if tile > 1 && n_samples >= tile {
        let n_tiles = ((n_samples as f64) / (tile as f64)).round().max(1.0) as usize;
        let snapped = ((n_samples as f64) / (n_tiles as f64)).round().max(step as f64) as usize;
        if snapped.abs_diff(tile) <= (tile / 16).max(1) {
            tile = snapped;
        }
    }

    let mut env = vec![0.0f32; n_samples];
    let mut pos = 0;
    while pos < n_samples {
        let hi = (pos + tile).min(n_samples);
        let seg_n = hi - pos;
        let u: Vec<f64> = (0..seg_n).map(|i| (i as f64) / (seg_n as f64)).collect();
        let seg = book_isn_envelope_shape(&u, decay);
        env[pos..hi].copy_from_slice(&seg);
        pos = hi;
    }
    env
}

/// Sliding boxcar moving average (same-length output).
fn moving_average_abs(x: &[f64], win: usize) -> Vec<f64> {
    let n = x.len();
    if n == 0 {
        return Vec::new();
    }
    let w = win.max(1).min(n);
    let half = w / 2;
    let mut prefix = vec![0.0; n + 1];
    for i in 0..n {
        prefix[i + 1] = prefix[i] + x[i].abs();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let lo = i.saturating_sub(half);
        let hi = (lo + w).min(n);
        out.push((prefix[hi] - prefix[lo]) / ((hi - lo).max(1) as f64));
    }
    out
}

/// Contextual sub/superharmonic + log-frequency FFT Fractallizer (max 50% wet).
#[derive(Debug, Clone)]
pub struct MusicFractallizer {
    pub sample_rate: u32,
}

impl MusicFractallizer {
    pub fn new(sample_rate: u32) -> Self {
        Self { sample_rate: sample_rate.max(8000) }
    }

    /// Time-variant contextual sub/superharmonic companion (exact port of `MusicFractallizer._contextual_sub_super`).
    pub fn contextual_sub_super(&self, signal: &[f32], strength: f64) -> Vec<f32> {
        let n = signal.len();
        if n < 16 {
            return vec![0.0; n];
        }
        let x: Vec<f64> = signal.iter().map(|&v| v as f64).collect();
        let sr = self.sample_rate as f64;
        let win = (n / 64).clamp(8, 256);
        let local_env = moving_average_abs(&x, win);

        // Estimate local fundamental via zero-crossing period per block
        let block = (n / 32).clamp(32, 512);
        let nb = (n / block).max(1);
        let mut f0_ctrl = Vec::with_capacity(nb);
        for b in 0..nb {
            let lo = b * block;
            let hi = (lo + block).min(n);
            let mut zc = Vec::new();
            for i in lo..hi.saturating_sub(1) {
                if (x[i] < 0.0) != (x[i + 1] < 0.0) {
                    zc.push(i);
                }
            }
            let f0 = if zc.len() >= 2 {
                let mut diffs: Vec<f64> = zc.windows(2).map(|w| (w[1] - w[0]) as f64).collect();
                diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let med = diffs[diffs.len() / 2];
                sr / (med * 2.0).max(2.0)
            } else {
                110.0
            };
            f0_ctrl.push(f0.clamp(40.0, sr * 0.45));
        }

        let ratios = [
            0.5, 1.0 / 3.0, 0.25, 0.2, 1.0 / 6.0,
            2.0, 3.0, 4.0, 5.0, 6.0,
        ];
        let weights = [
            0.14, 0.11, 0.09, 0.07, 0.05,
            0.14, 0.11, 0.09, 0.07, 0.05,
        ];

        // Integrate instantaneous phase
        let mut phase = vec![0.0f64; n];
        let mut acc = 0.0;
        for i in 0..n {
            let pos = if nb <= 1 {
                0.0
            } else {
                (i as f64) * ((nb - 1) as f64) / ((n - 1).max(1) as f64)
            };
            let lo = (pos.floor() as usize).min(nb - 1);
            let hi = (lo + 1).min(nb - 1);
            let frac = pos - (lo as f64);
            let f0 = f0_ctrl[lo] * (1.0 - frac) + f0_ctrl[hi] * frac;
            acc += TAU * f0 / sr;
            phase[i] = acc;
        }

        let mut companion = vec![0.0f64; n];
        for (&r, &w) in ratios.iter().zip(weights.iter()) {
            let mut sum = 0.0;
            let mut vals = Vec::with_capacity(n);
            for &ph in &phase {
                let p = book_isn((ph * r).rem_euclid(TAU));
                sum += p;
                vals.push(p);
            }
            let mean = sum / (n as f64);
            for i in 0..n {
                companion[i] += w * (vals[i] - mean);
            }
        }
        (0..n)
            .map(|i| (companion[i] * local_env[i] * strength) as f32)
            .collect()
    }

    /// Frequency-domain spectral magnitude fractal resonator (README stages 1-4).
    /// Preserves input phase exactly at every FFT bin.
    pub fn process_spectral(&self, dry: &[f32], activation: f64, gamma: f64, pkp_env: Option<&[f32]>) -> Vec<f32> {
        let n = dry.len();
        let act = activation.clamp(0.0, 1.0);
        if n < 32 || act < 1e-6 {
            return dry.to_vec();
        }
        let g = gamma.max(1.05);
        let win = hann_window(n);
        let windowed: Vec<f64> = dry.iter().zip(win.iter()).map(|(&s, &w)| (s as f64) * w).collect();
        let peak_in = dry.iter().map(|v| v.abs() as f64).fold(0.0_f64, f64::max);
        if peak_in < 1e-9 {
            return dry.to_vec();
        }

        let (n_fft, mags, phases) = rfft_mag_phase(&windowed);
        let k_max = (mags.len() - 1) as f64;
        let scales = [1.0 / g, 1.0, g, g * g];
        let weights = [0.20, 0.35, 0.30, 0.15];

        let interp_mag = |pos: f64| -> f64 {
            let p = pos.clamp(1.0, k_max);
            let lo = (p.floor() as usize).min(mags.len() - 1);
            let hi = (lo + 1).min(mags.len() - 1);
            let f = p - (lo as f64);
            mags[lo] * (1.0 - f) + mags[hi] * f
        };

        // Stage 1: log-frequency magnitude self-convolution
        let mut new_m = vec![0.0; mags.len()];
        for k in 0..mags.len() {
            let safe = (k as f64).max(1.0);
            let mut sum = 0.0;
            for (&s, &w) in scales.iter().zip(weights.iter()) {
                sum += w * interp_mag(safe * s);
            }
            new_m[k] = sum;
        }

        // Stage 2: smooth spectral detail
        let logmag: Vec<f64> = mags.iter().map(|&m| (1.0 + m).ln()).collect();
        let detail_amt = 0.10 * act;
        for k in 1..mags.len() {
            let lo = k.saturating_sub(4);
            let hi = (k + 5).min(mags.len());
            let smooth = logmag[lo..hi].iter().sum::<f64>() / ((hi - lo) as f64);
            let residual = logmag[k] - smooth;
            let taper = ((k as f64) / k_max.max(1.0)).sqrt();
            let detail = (smooth + detail_amt * residual * taper).clamp(-20.0, 20.0).exp() - 1.0;
            new_m[k] = 0.88 * new_m[k] + 0.12 * detail.max(0.0);
        }
        new_m[0] = mags[0]; // DC anchored

        // Stage 3: phase-preserving inverse FFT + peak norm undo
        let recon = irfft_from_mag_phase(n_fft, &new_m, &phases);
        let y_slice = &recon[..n];
        let peak_out = y_slice.iter().map(|v| v.abs()).fold(0.0_f64, f64::max);
        let scale = if peak_out > 1e-12 { peak_in / peak_out } else { 1.0 };

        // Stage 4: tempo-locked PKP envelope and 50% wet cap
        let wet_mix = 0.5 * act;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let d = dry[i] as f64;
            let frac_sig = y_slice[i] * scale;
            let env = pkp_env.map(|e| (e[i % e.len()] as f64).clamp(0.0, 1.5)).unwrap_or(1.0);
            let detail = (frac_sig - d) * env;
            out.push((d + wet_mix * detail) as f32);
        }
        out
    }

    /// Combined contextual Fractallizer pass (50% max wet).
    pub fn process(&self, dry: &[f32], activation: f64, gamma: f64, pkp_env: Option<&[f32]>) -> Vec<f32> {
        let act = activation.clamp(0.0, 1.0);
        if dry.is_empty() || act < 1e-6 {
            return dry.to_vec();
        }
        let strength = 0.5 + 0.5 * gamma / 4.0;
        let companion = self.contextual_sub_super(dry, strength);
        let wet_mix = 0.5 * act;
        let mut out = Vec::with_capacity(dry.len());
        for i in 0..dry.len() {
            let env = pkp_env.map(|e| e[i % e.len()]).unwrap_or(1.0);
            out.push(dry[i] + (wet_mix as f32) * companion[i] * env);
        }
        out
    }
}

/// Equation of Reality (EQR) audio processor (book p.78, closed-form O(1) audio reduction, 50% max wet).
#[derive(Debug, Clone, Default)]
pub struct EQRTensorEngine {
    pub last_z: f64,
}

impl EQRTensorEngine {
    pub fn process(&mut self, dry: &[f32], activation: f64, pkp_env: Option<&[f32]>, ot: bool) -> Vec<f32> {
        let n = dry.len();
        let act = activation.clamp(0.0, 1.0);
        let wet_mix = 0.5 * act;
        if n == 0 || wet_mix < 1e-6 {
            return dry.to_vec();
        }
        let x: Vec<f64> = dry.iter().map(|&v| v as f64).collect();
        let win = (n / 64).clamp(8, 256);
        let d_char = moving_average_abs(&x, win);

        let ctrl_n = (n / 128).clamp(4, 128);
        let mut idxs = Vec::with_capacity(ctrl_n);
        let mut z_ctrl = Vec::with_capacity(ctrl_n);
        for i in 0..ctrl_n {
            let ix = if ctrl_n <= 1 {
                0
            } else {
                (i * (n - 1)) / (ctrl_n - 1)
            };
            let t = (ix as f64) / ((n - 1).max(1) as f64);
            let res = eqr_tensor_audio(x[ix], d_char[ix], x[ix], t);
            idxs.push(ix);
            z_ctrl.push(res.z.clamp(-4.0, 4.0));
        }
        if let Some(&last) = z_ctrl.last() {
            self.last_z = last;
        }

        // Interpolate Z across samples and mix up to 50% wet
        let mut out = Vec::with_capacity(n);
        let mut seg = 0usize;
        for i in 0..n {
            while seg + 1 < ctrl_n && idxs[seg + 1] < i {
                seg += 1;
            }
            let zi = if seg + 1 < ctrl_n && idxs[seg + 1] > idxs[seg] {
                let frac = ((i - idxs[seg]) as f64) / ((idxs[seg + 1] - idxs[seg]) as f64);
                z_ctrl[seg] * (1.0 - frac) + z_ctrl[seg + 1] * frac
            } else {
                z_ctrl[seg]
            };
            let env = pkp_env.map(|e| e[i % e.len()] as f64).unwrap_or(1.0);
            let contrib = math_mul(zi, x[i], ot) * env;
            let val = (1.0 - wet_mix) * x[i] + wet_mix * contrib;
            out.push(val as f32);
        }
        out
    }
}
