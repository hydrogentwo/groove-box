//! Groovebox pure-Rust DSP, synth, FX, and deterministic audio renderer.

pub mod dj_effects;
pub mod instruments;
pub mod master_fx;
pub mod oscillator;
pub mod render;
pub mod wav;

pub use dj_effects::*;
pub use instruments::*;
pub use master_fx::*;
pub use oscillator::*;
pub use render::*;
pub use wav::*;

#[cfg(test)]
mod tests {
    use super::*;
    use groovebox_state::ProjectState;

    #[test]
    fn test_commutative_pair_space_python_parity() {
        let ps = CommutativePairSpace::new(48);
        let d_ab = ps.descriptor(0, 1);
        let d_ba = ps.descriptor(1, 0);
        assert_eq!(d_ab, d_ba);
        assert_eq!(format!("{:016x}", d_ab.signature), "138407c5e86f8e2d");
    }

    #[test]
    fn test_harmonic_spacing_n_independent() {
        let r8 = harmonic_spacing_ratios(8);
        let r48 = harmonic_spacing_ratios(48);
        for i in 0..8 {
            assert!((r8[i] - r48[i]).abs() < 1e-12);
        }
    }

    #[test]
    fn test_render_determinism_and_start_silence() {
        let mut p = ProjectState::default();
        p.sample_rate = 22050;
        let r1 = render_project(&p, 1);
        let r2 = render_project(&p, 1);
        assert_eq!(r1.summary.audio_sha256, r2.summary.audio_sha256);
        assert_eq!(r1.samples[0], 0.0);
        assert!(r1.summary.rms > 0.005);
        assert!(r1.summary.peak <= 1.0);
    }

    #[test]
    fn test_wav_roundtrip_provenance() {
        let samples = vec![0.0f32, 0.25, -0.5, 0.75, 0.0];
        let wav = encode_wav_pcm16(&samples, 48000, Some("fp=abcdef1234567890"));
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(extract_wav_provenance(&wav).as_deref(), Some("fp=abcdef1234567890"));
    }
}
