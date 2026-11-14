//! Pure-Rust radix-2 Cooley-Tukey FFT/IFFT and windowing utilities.

use std::f64::consts::TAU;

#[derive(Debug, Clone, Copy, Default)]
pub struct Complex64 {
    pub re: f64,
    pub im: f64,
}

impl Complex64 {
    #[inline]
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    #[inline]
    pub fn from_polar(mag: f64, phase: f64) -> Self {
        Self {
            re: mag * phase.cos(),
            im: mag * phase.sin(),
        }
    }

    #[inline]
    pub fn mag(self) -> f64 {
        self.re.hypot(self.im)
    }

    #[inline]
    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }

    #[inline]
    pub fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.im + o.im)
    }

    #[inline]
    pub fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.im - o.im)
    }

    #[inline]
    pub fn mul(self, o: Self) -> Self {
        Self::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
}

pub fn next_pow2(n: usize) -> usize {
    if n <= 1 {
        1
    } else {
        n.next_power_of_two()
    }
}

pub fn hann_window(n: usize) -> Vec<f64> {
    if n <= 1 {
        return vec![1.0; n];
    }
    let denom = (n - 1) as f64;
    (0..n)
        .map(|i| 0.5 * (1.0 - (TAU * (i as f64) / denom).cos()))
        .collect()
}

/// In-place radix-2 Cooley-Tukey FFT (`inverse = false`) or IFFT (`inverse = true`).
/// `buf.len()` MUST be a power of two.
pub fn fft_in_place(buf: &mut [Complex64], inverse: bool) {
    let n = buf.len();
    if n <= 1 {
        return;
    }
    debug_assert!(n.is_power_of_two(), "FFT length must be power of two");

    // Bit-reversal permutation
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while (j & bit) != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            buf.swap(i, j);
        }
    }

    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2usize;
    while len <= n {
        let ang = sign * TAU / (len as f64);
        let wlen = Complex64::new(ang.cos(), ang.sin());
        let half = len >> 1;
        let mut i = 0usize;
        while i < n {
            let mut w = Complex64::new(1.0, 0.0);
            for k in 0..half {
                let u = buf[i + k];
                let v = buf[i + k + half].mul(w);
                buf[i + k] = u.add(v);
                buf[i + k + half] = u.sub(v);
                w = w.mul(wlen);
            }
            i += len;
        }
        len <<= 1;
    }

    if inverse {
        let inv_n = 1.0 / (n as f64);
        for c in buf.iter_mut() {
            c.re *= inv_n;
            c.im *= inv_n;
        }
    }
}

/// Real FFT returning `(magnitudes, phases)` for bins `0..=n_fft/2`.
pub fn rfft_mag_phase(signal: &[f64]) -> (usize, Vec<f64>, Vec<f64>) {
    let n_fft = next_pow2(signal.len().max(2));
    let mut buf = vec![Complex64::default(); n_fft];
    for (i, &v) in signal.iter().enumerate() {
        buf[i].re = v;
    }
    fft_in_place(&mut buf, false);
    let half = n_fft / 2 + 1;
    let mut mags = Vec::with_capacity(half);
    let mut phases = Vec::with_capacity(half);
    for c in &buf[..half] {
        mags.push(c.mag());
        phases.push(c.arg());
    }
    (n_fft, mags, phases)
}

/// Reconstruct real time-domain signal of length `n_fft` from half-spectrum `(mags, phases)`.
pub fn irfft_from_mag_phase(n_fft: usize, mags: &[f64], phases: &[f64]) -> Vec<f64> {
    let mut buf = vec![Complex64::default(); n_fft];
    let half = n_fft / 2;
    for k in 0..=half {
        if k < mags.len() && k < phases.len() {
            buf[k] = Complex64::from_polar(mags[k], phases[k]);
        }
    }
    for k in 1..half {
        let c = buf[k];
        buf[n_fft - k] = Complex64::new(c.re, -c.im);
    }
    fft_in_place(&mut buf, true);
    buf.into_iter().map(|c| c.re).collect()
}

/// Compute `n_bands` log-spaced spectral energy bands in `[0, 1]` and normalized centroid.
pub fn spectral_bands_and_centroid(samples: &[f32], sample_rate: u32, n_bands: usize) -> (Vec<f64>, f64) {
    let n_bands = n_bands.max(1);
    if samples.len() < 8 {
        return (vec![0.0; n_bands], 0.0);
    }
    let take = samples.len().min(2048);
    let win = hann_window(take);
    let sig: Vec<f64> = samples[samples.len() - take..]
        .iter()
        .zip(win.iter())
        .map(|(&s, &w)| (s as f64) * w)
        .collect();
    let (n_fft, mags, _) = rfft_mag_phase(&sig);
    let n_bins = mags.len();
    let mut bands = vec![0.0; n_bands];
    let max_bin = (n_bins - 1).max(1) as f64;
    for b in 0..n_bands {
        let f0 = (b as f64) / (n_bands as f64);
        let f1 = ((b + 1) as f64) / (n_bands as f64);
        let lo = ((max_bin.powf(f0) - 1.0).max(0.0) as usize).min(n_bins - 1);
        let hi = ((max_bin.powf(f1)).ceil() as usize).clamp(lo + 1, n_bins);
        let mut sum = 0.0;
        for m in &mags[lo..hi] {
            sum += *m;
        }
        let avg = sum / ((hi - lo).max(1) as f64) / ((n_fft as f64) * 0.125).max(1.0);
        bands[b] = (1.0 + avg * 8.0).ln() / (1.0 + 8.0_f64).ln();
        bands[b] = bands[b].clamp(0.0, 1.0);
    }
    let mut num = 0.0;
    let mut den = 0.0;
    let bin_hz = (sample_rate as f64) / (n_fft as f64);
    for (k, &m) in mags.iter().enumerate() {
        num += (k as f64) * bin_hz * m;
        den += m;
    }
    let centroid_hz = if den > 1e-12 { num / den } else { 0.0 };
    let nyq = (sample_rate as f64) * 0.5;
    let centroid_norm = (centroid_hz / nyq.max(1.0)).clamp(0.0, 1.0);
    (bands, centroid_norm)
}
