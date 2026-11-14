//! 48-instrument Groovebox topology catalog, elemental naming, and
//! N-independent harmonic spacing ratios.

use groovebox_math::{MEUM, MEUM_INV, MEUM_NORM, MEUM_SQ, PHI, PHI_INV};

pub const DEFAULT_INSTRUMENT_LIST: [&str; 48] = [
    // Family 1: Topological Wave-Folding & Non-Linear Curvature (1-8)
    "Meum Phase-Fold Oscillator", "Z-Pinch Waveguide Synth", "Hyperbolic Attractor Generator", "Non-Linear Polynomial Folder",
    "Strange Attractor Chaos Engine", "Topological Torus Synthesizer", "Klein Bottle Surface Generator", "Crystalline Wavefolder Matrix",
    // Family 2: Multivectorial & Phase-Space Dynamics (9-16)
    "Quaternion Cl(0,3) Space Synth", "Clifford Multivector Rotor", "Phase-Space Trajectory Synth", "Spinor Standing Wave Generator",
    "Tensor Curvature Field Lead", "Vector Field Flow Synthesizer", "Eigenstate Harmonic Matrix", "Wavepacket Localization Engine",
    // Family 3: Quantum, Soliton & Field-Coupling (17-24)
    "Quantum Tunneling Oscillator", "Soliton Pulse Engine", "Bose-Einstein Condensate Pad", "Zero-Point Energy Oscillator",
    "Casimir Force Resonator", "Photon-Coupling Synth", "Neutrino Flux Modulator", "Quark Confinement Bass",
    // Family 4: Stochastic, Thermodynamic & Entropic Noise (25-32)
    "Stochastic Noise Chamber", "Entropy Decay Engine", "Doppler Shift Emulator", "Brownian Motion Synth",
    "Fractional Brownian Filter", "Thermal Noise Generator", "Microstate Combinatoric Pad", "Dissipative Structure Synth",
    // Family 5: Input-Dependent Spatial & Spectral Effects (33-40)
    "Topological Phase Shifter", "Non-Linear Spectral Fold-Back Effect", "Curvature Convolution Matrix", "Gravitational Time-Dilation Delay",
    "Wave-Number Dispersion Filter", "Vortex Phase Modulator", "Anisotropic Spatial Diffusion", "Tensor Field Reverb Processor",
    // Family 6: Input-Dependent Dynamic Waveform Resonators (41-48)
    "Spectral Centroid Dynamic Shifter", "Soliton Envelope Shaper", "Wavepacket Granulator Effect", "Non-Linear Diode Clipper Effect",
    "Plasma Ionization Gate", "Magnetostrictive Resonator Effect", "Crystalline Lattice Damper", "Event Horizon Limiter",
];

const ADJECTIVES: [&str; 20] = [
    "Deep", "Bright", "Hollow", "Resonant", "Sub", "Airy", "Fractal",
    "Crystal", "Ion", "Phase", "Quantum", "Stochastic", "Hyper", "Nano",
    "Meum", "Soliton", "Tensor", "Drift", "Fold", "Pulse",
];

pub fn generate_synth_names(count: usize) -> Vec<String> {
    let n = count.clamp(2, 64);
    let elemental: Option<&[&str]> = match n {
        2 => Some(&["Ice", "Fire"]),
        3 => Some(&["Earth", "Wind", "Water"]),
        4 => Some(&["Earth", "Air", "Fire", "Water"]),
        5 => Some(&["Earth", "Air", "Fire", "Water", "Aether"]),
        6 => Some(&["Earth", "Air", "Fire", "Water", "Aether", "Void"]),
        7 => Some(&["Sol", "Luna", "Mars", "Mercury", "Jupiter", "Venus", "Saturn"]),
        8 => Some(&["North", "South", "East", "West", "Zenith", "Nadir", "Dawn", "Dusk"]),
        _ => None,
    };
    if let Some(el) = elemental {
        return el.iter().map(|s| (*s).to_string()).collect();
    }
    if n <= DEFAULT_INSTRUMENT_LIST.len() {
        if n == DEFAULT_INSTRUMENT_LIST.len() {
            return DEFAULT_INSTRUMENT_LIST.iter().map(|s| (*s).to_string()).collect();
        }
        let mut seen = std::collections::HashMap::<String, usize>::new();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let idx = ((i as f64) * ((DEFAULT_INSTRUMENT_LIST.len() - 1) as f64) / ((n - 1).max(1) as f64)).round() as usize;
            let name = DEFAULT_INSTRUMENT_LIST[idx.min(DEFAULT_INSTRUMENT_LIST.len() - 1)];
            let entry = seen.entry(name.to_string()).or_insert(0);
            if *entry == 0 {
                out.push(name.to_string());
            } else {
                let adj = ADJECTIVES[*entry % ADJECTIVES.len()];
                out.push(format!("{} {}", adj, name));
            }
            *entry += 1;
        }
        return out;
    }
    let mut out: Vec<String> = DEFAULT_INSTRUMENT_LIST.iter().map(|s| (*s).to_string()).collect();
    let extra = n - out.len();
    for k in 0..extra {
        let root = DEFAULT_INSTRUMENT_LIST[k % DEFAULT_INSTRUMENT_LIST.len()];
        let adj = ADJECTIVES[k % ADJECTIVES.len()];
        out.push(format!("{} {}", adj, root));
    }
    out
}

/// Stable per-index voice ratios anchored to an absolute 48-slot six-octave lattice.
/// Adding/removing voices never retunes existing voices.
pub fn harmonic_spacing_ratios(count: usize) -> Vec<f64> {
    let n = count.max(1);
    let span_octaves = 6.0;
    let center = 23.5;
    let denom = 47.0;
    (0..n)
        .map(|i| {
            let t = ((i as f64) - center) / denom;
            let warped = t * MEUM_NORM + (1.0 - MEUM_NORM) * t;
            2.0_f64.powf(span_octaves * warped)
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub struct CanonicalMacroDefaults {
    pub tuning: f64,
    pub filter: f64,
    pub drive: f64,
    pub amplitude: f64,
    pub duration: f64,
    pub phase_shift: f64,
    pub am_depth: f64,
    pub am_rate: f64,
    pub fm_depth: f64,
    pub fm_rate: f64,
    pub pm_depth: f64,
    pub pm_rate: f64,
    pub pm_feedback: f64,
    pub meum_depth: f64,
}

pub fn canonical_macro_defaults(ctx: f64) -> CanonicalMacroDefaults {
    CanonicalMacroDefaults {
        tuning: MEUM_INV + (PHI - 1.0) * ctx,
        filter: MEUM_NORM + MEUM_INV * ctx,
        drive: MEUM_NORM * PHI_INV + MEUM_NORM * ctx,
        amplitude: MEUM_INV * PHI_INV + MEUM_NORM * ctx,
        duration: MEUM_NORM + MEUM_INV * (1.0 - ctx),
        phase_shift: 0.0,
        am_depth: MEUM_NORM * PHI_INV + MEUM_NORM * ctx * PHI_INV,
        am_rate: MEUM_NORM + MEUM * ctx,
        fm_depth: MEUM_NORM * PHI_INV + MEUM_NORM * ctx * PHI_INV,
        fm_rate: MEUM_SQ * ctx + MEUM_INV,
        pm_depth: MEUM_NORM * PHI_INV + MEUM_NORM * ctx * PHI_INV,
        pm_rate: MEUM_SQ * ctx + MEUM_INV,
        pm_feedback: MEUM_NORM * PHI_INV + MEUM_NORM * ctx * PHI_INV,
        meum_depth: MEUM_NORM * PHI_INV + MEUM_NORM * ctx * PHI_INV,
    }
}
