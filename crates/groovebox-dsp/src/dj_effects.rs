//! Low-latency live-DJ effects and commutative unordered pair space.
//! Exact pure-Rust port of `dj_effects.py` (including BLAKE2b 16-byte pair signatures).

use std::f64::consts::TAU;
use groovebox_math::blake2b_16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairDescriptor {
    pub a: usize,
    pub b: usize,
    pub index: usize,
    pub signature: u64,
    pub phase: f64,
    pub ratio: f64,
    pub spread: f64,
}

#[derive(Debug, Clone)]
pub struct CommutativePairSpace {
    pub size: usize,
    pub count: usize,
}

impl CommutativePairSpace {
    pub fn new(size: usize) -> Self {
        let size = size.max(2);
        let count = size * (size - 1) / 2;
        Self { size, count }
    }

    pub fn normalize(&self, a: usize, b: usize) -> (usize, usize) {
        let a = a % self.size;
        let mut b = b % self.size;
        if a == b {
            b = (b + 1) % self.size;
        }
        if a < b { (a, b) } else { (b, a) }
    }

    pub fn index(&self, a: usize, b: usize) -> usize {
        let (a, b) = self.normalize(a, b);
        a * (2 * self.size - a - 1) / 2 + (b - a - 1)
    }

    pub fn descriptor(&self, a: usize, b: usize) -> PairDescriptor {
        let (a, b) = self.normalize(a, b);
        let idx = self.index(a, b);
        let raw = blake2b_16(format!("eqr-pair|{}|{}|{}", self.size, a, b).as_bytes());
        let s0 = u64::from_be_bytes([raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7]]);
        let s1 = u64::from_be_bytes([raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15]]);
        let u0 = (s0 as f64) / 18446744073709551616.0;
        let u1 = (s1 as f64) / 18446744073709551616.0;
        PairDescriptor {
            a,
            b,
            index: idx,
            signature: s0,
            phase: TAU * u0,
            ratio: 0.5 + 1.5 * u1,
            spread: 0.15 + 0.85 * ((idx + 1) as f64 / (self.count.max(1) as f64)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LiveDJEffects {
    pub sample_rate: u32,
    pub seed: u32,
    pub amount_goava: f64,
    pub amount_random: f64,
    pub pair_space: CommutativePairSpace,
    pub pair: PairDescriptor,
    pub boost_interval: usize,
    pub boost_phase: f64,
    pub boost_amount: f64,
}

impl LiveDJEffects {
    pub fn new(sample_rate: u32) -> Self {
        let pair_space = CommutativePairSpace::new(48);
        let pair = pair_space.descriptor(0, 1);
        Self {
            sample_rate: sample_rate.max(8000),
            seed: 0,
            amount_goava: 0.0,
            amount_random: 0.0,
            pair_space,
            pair,
            boost_interval: 0,
            boost_phase: 0.0,
            boost_amount: 0.0,
        }
    }

    pub fn set_context(&mut self, seed: f64, pair: (usize, usize), sample_rate: Option<u32>) {
        self.seed = ((seed.abs() * 1_000_003.0) as u64 & 0xFFFF_FFFF) as u32;
        if let Some(sr) = sample_rate {
            self.sample_rate = sr.max(8000);
        }
        self.pair = self.pair_space.descriptor(pair.0, pair.1);
    }

    pub fn set_boost(&mut self, interval: usize, phase: f64, amount: f64) {
        self.boost_interval = interval;
        self.boost_phase = phase;
        self.boost_amount = amount.clamp(0.0, 1.0);
    }

    pub fn goava_pair_morph(
        &self,
        x: &[f32],
        start_sample: usize,
        goava_scalar: f64,
        bpm: f64,
        amount_override: Option<f64>,
    ) -> Vec<f32> {
        let amt = amount_override.unwrap_or(self.amount_goava);
        if amt <= 1e-6 || x.is_empty() {
            return x.to_vec();
        }
        let d = self.pair;
        let sr = self.sample_rate as f64;
        let mod_hz = goava_scalar.abs() * (bpm / 60.0) * (0.5 + d.spread);
        let drive = 1.0 + 2.8 * (0.25 + 0.75 * d.ratio) * amt;
        let mix_a = (0.55 * amt).clamp(0.0, 1.0);

        let mut out = Vec::with_capacity(x.len());
        for (i, &xs) in x.iter().enumerate() {
            let t = ((start_sample + i) as f64) / sr;
            let m = (TAU * mod_hz * t + d.phase).sin();
            let xf = xs as f64;
            let wet = (xf * drive) * (0.78 + 0.22 * m) + xf * (0.16 * amt) * m;
            out.push((xf * (1.0 - mix_a) + wet * mix_a) as f32);
        }
        out
    }

    pub fn random_parametric(
        &self,
        x: &[f32],
        start_sample: usize,
        bpm: f64,
        amount_override: Option<f64>,
    ) -> Vec<f32> {
        let amt = amount_override.unwrap_or(self.amount_random);
        if amt <= 1e-6 || x.is_empty() {
            return x.to_vec();
        }
        let d = self.pair;
        let sr = self.sample_rate as f64;
        let seed_phase = (((self.seed & 0xFFFF) as f64) / 65536.0) * TAU;
        let beat = bpm / 60.0;
        let mix_a = (0.68 * amt).clamp(0.0, 1.0);

        let mut out = Vec::with_capacity(x.len());
        for (i, &xs) in x.iter().enumerate() {
            let t = ((start_sample + i) as f64) / sr;
            let lfo1 = (TAU * beat * (0.5 + d.spread) * t + d.phase + seed_phase).sin();
            let lfo2 = (TAU * beat * (1.0 + d.ratio) * t + seed_phase * 0.37 + d.phase * 1.7).sin();
            let drive = 1.0 + amt * (1.5 + 2.5 * (0.5 + 0.5 * lfo1));
            let trem = 0.72 + 0.28 * (0.5 + 0.5 * lfo2);
            let xf = xs as f64;
            let wet = (xf * drive) * trem + xf * (0.035 * amt) * (lfo1 + lfo2 + d.phase).sin();
            out.push((xf * (1.0 - mix_a) + wet * mix_a) as f32);
        }
        out
    }

    pub fn process(&self, x: &[f32], start_sample: usize, goava_scalar: f64, bpm: f64) -> Vec<f32> {
        let mut y = x.to_vec();
        if self.amount_goava > 1e-6 {
            y = self.goava_pair_morph(&y, start_sample, goava_scalar, bpm, None);
        }
        if self.amount_random > 1e-6 {
            y = self.random_parametric(&y, start_sample, bpm, None);
        }
        if self.boost_interval > 0 && self.boost_amount > 1e-6 && !y.is_empty() {
            let iv = self.boost_interval as f64;
            let width = (iv * 0.06).max(1.0);
            for (i, s) in y.iter_mut().enumerate() {
                let t = (start_sample + i) as f64;
                let pos = (t - self.boost_phase).rem_euclid(iv);
                let g = 1.0 + self.boost_amount * (-0.5 * (pos / width).powi(2)).exp();
                *s = ((*s as f64) * g) as f32;
            }
        }
        y
    }
}
