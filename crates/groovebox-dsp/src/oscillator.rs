//! Stateful Meum Modulated Oscillator (exact port of `groovebox.MeumModulatedOscillator`).
//!
//! Signal order:
//!   FM -> instantaneous_frequency -> phase accumulator -> phase_shift + PM -> waveform -> AM -> output
//!
//! Contract: phase starts at 0.0 and sample 0 of an odd waveform (`isn`, `sine`, etc.) is 0.0.

use std::f64::consts::TAU;
use groovebox_math::{
    book_ics, book_isn, eqr_ics, eqr_isn, AUDIBLE_HI_HZ, MEUM_INV, MEUM_NORM,
};

#[derive(Debug, Clone)]
pub struct MeumModulatedOscillator {
    pub sample_rate: f64,
    pub frequency: f64,
    pub phase: f64,
    pub sample_index: u64,

    pub phase_shift: f64,
    pub am_depth: f64,
    pub am_rate: f64,
    pub fm_depth: f64,
    pub fm_rate: f64,
    pub pm_depth: f64,
    pub pm_rate: f64,
    pub pm_feedback: f64,
    pub meum_depth: f64,
    pub waveform: String,
}

impl MeumModulatedOscillator {
    pub fn new(sample_rate: f64, frequency: f64) -> Self {
        Self {
            sample_rate: sample_rate.max(8000.0),
            frequency,
            phase: 0.0,
            sample_index: 0,
            phase_shift: 0.0,
            am_depth: 0.0,
            am_rate: 1.0,
            fm_depth: 0.0,
            fm_rate: 1.0,
            pm_depth: 0.0,
            pm_rate: 1.0,
            pm_feedback: 0.0,
            meum_depth: 0.0,
            waveform: "isn".into(),
        }
    }

    pub fn reset_phase(&mut self) {
        self.phase = 0.0;
        self.sample_index = 0;
    }

    #[inline]
    pub fn meum_field(&self, t: f64, rate: f64) -> f64 {
        let a = (TAU * t * rate).sin();
        let b = (TAU * t * rate * MEUM_INV).sin();
        (0.5 * (a + MEUM_NORM * b)).clamp(-1.0, 1.0)
    }

    #[inline]
    pub fn amplitude_modulation(&self, t: f64) -> f64 {
        let lfo = (TAU * t * self.am_rate + self.phase_shift).sin();
        let field = self.meum_field(t, self.am_rate);
        (1.0 + self.am_depth * (lfo + self.meum_depth * field)).max(0.0)
    }

    #[inline]
    pub fn instantaneous_frequency(&self, t: f64) -> f64 {
        let lfo = (TAU * t * self.fm_rate + self.phase_shift).sin();
        let field = self.meum_field(t, self.fm_rate);
        let modulation = lfo + self.meum_depth * field;
        let freq = self.frequency * (1.0 + self.fm_depth * modulation);
        let hi = (self.sample_rate * 0.75).min(AUDIBLE_HI_HZ);
        freq.clamp(0.0, hi)
    }

    #[inline]
    pub fn phase_modulation(&self, t: f64) -> f64 {
        let lfo = (TAU * t * self.pm_rate + self.phase_shift).sin();
        let field = self.meum_field(t, self.pm_rate);
        self.pm_depth * lfo + self.pm_feedback * self.meum_depth * field
    }

    #[inline]
    pub fn waveform_sample(&self, phase: f64) -> f64 {
        let u = (phase / TAU).rem_euclid(1.0);
        match self.waveform.as_str() {
            "saw" | "sawtooth" => 2.0 * u - 1.0,
            "square" | "pulse" => if u < 0.5 { 1.0 } else { -1.0 },
            "triangle" | "tri" => 4.0 * (u - 0.5).abs() - 1.0,
            "sine" | "sin" => phase.sin(),
            "cos" | "cosine" => phase.cos(),
            "ics" => eqr_ics(phase),
            "book_isn" => book_isn(phase) * 0.5,
            "book_ics" => book_ics(phase) * 0.5,
            _ => eqr_isn(phase), // primary core: isn
        }
    }

    /// Render `n_samples` with harmonic partial summation (`partials` count, pure 1/h amplitudes).
    pub fn render(&mut self, n_samples: usize, partials: usize, fold_depth: f64) -> Vec<f32> {
        let mut out = Vec::with_capacity(n_samples);
        let partials = partials.clamp(1, 24);
        let inv_sr = 1.0 / self.sample_rate;

        for _ in 0..n_samples {
            let t = (self.sample_index as f64) * inv_sr;
            let f_inst = self.instantaneous_frequency(t);
            let pm = self.phase_modulation(t);
            let am = self.amplitude_modulation(t);
            let ph = self.phase + self.phase_shift + pm;

            // Partial series with pure 1/h amplitudes scaled by MEUM_NORM
            let mut val = 0.0;
            let mut norm = 0.0;
            for h in 1..=partials {
                let hf = h as f64;
                let delta_h = 1.0 + (fold_depth - 1.0) * 0.004 * (hf - 1.0);
                let w = MEUM_NORM / hf;
                val += w * self.waveform_sample(ph * hf * delta_h);
                norm += w;
            }
            let sample = if self.sample_index == 0 {
                // Contract: track start sample 0 is 0.0
                0.0
            } else {
                (val / norm.max(1e-9)) * am
            };
            out.push(sample as f32);

            self.phase = (self.phase + TAU * f_inst * inv_sr).rem_euclid(TAU * 1024.0);
            self.sample_index += 1;
        }
        out
    }
}
