//! Core deterministic mathematical foundation for Groovebox.

pub mod constants;
pub mod eski;
pub mod expr;
pub mod fft;
pub mod hash;
pub mod json;
pub mod number_theory;
pub mod operator_theory;

pub use constants::*;
pub use eski::*;
pub use expr::*;
pub use fft::*;
pub use hash::*;
pub use json::*;
pub use number_theory::*;
pub use operator_theory::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_blake2b_python_parity() {
        assert_eq!(
            blake2b_hex(b"eqr-pair|48|0|1", 16),
            "138407c5e86f8e2dfa976d3438de5e84"
        );
        assert_eq!(
            blake2b_hex(b"instrument-visual|42.0|master|0", 8),
            "265dd530d595f370"
        );
    }

    #[test]
    fn test_meum_identity_and_operator_theory() {
        assert!(meum_identity_residual().abs() < 1e-6);
        assert_eq!(ot_prod(0.0, 0.0), 1.0);
        assert_eq!(ot_div(0.0, 0.0), 1.0);
        assert_eq!(ot_prod(-2.0, -3.0), -6.0);
        assert!((vg_sin(0.7) - 0.7_f64.sin()).abs() < 1e-12);
        assert!((vg_cos(0.7) - 0.7_f64.cos()).abs() < 1e-12);
        let y = eqr_isn(0.6);
        let x = eqr_isn_inv(y);
        assert!((x - 0.6).abs() < 1e-6);
    }

    #[test]
    fn test_eski_python_parity() {
        let seed = 42.0;
        let seq = [seed, seed * MEUM, seed * PHI];
        assert_eq!(eski_fractal_pick(42, &seq, 0), "starburst");
        let xyz = compositional_xyz(seed, &seq, 1.0, 0, false);
        assert!((xyz.0 - 0.0807044591848225).abs() < 1e-10);
        assert!((xyz.1 - (-0.987000000000009)).abs() < 1e-10);
        assert!((xyz.2 - (-0.24640817563645356)).abs() < 1e-10);

        let (zs, n_done, escaped) = eski_fractal_iterate_z("starburst", xyz.0, xyz.1, xyz.2, 0.3, 12, false);
        assert_eq!(n_done, 12);
        assert!(!escaped);
        assert!((zs[0] - (-0.7381744002196161)).abs() < 1e-9);
        assert!((zs[11] - (-1.332926495663114)).abs() < 1e-9);

        let rep = debug_cross_engine_alignment(seed, 1.0, 8, false);
        assert!(rep.ok);
        assert_eq!(rep.slots.len(), 8);
    }

    #[test]
    fn test_fft_roundtrip() {
        let sig: Vec<f64> = (0..64).map(|i| (i as f64 * 0.2).sin() + 0.5 * (i as f64 * 0.7).cos()).collect();
        let (n_fft, mags, phases) = rfft_mag_phase(&sig);
        let recon = irfft_from_mag_phase(n_fft, &mags, &phases);
        for (a, b) in sig.iter().zip(recon.iter()) {
            assert!((a - b).abs() < 1e-10);
        }
    }

    #[test]
    fn test_json_roundtrip_and_canonical() {
        let mut m = JsonValue::obj();
        m.insert("b".into(), JsonValue::Float(1.0));
        m.insert("a".into(), JsonValue::Int(42));
        let j = JsonValue::Object(m);
        let s = j.to_canonical_string();
        assert_eq!(s, r#"{"a":42,"b":1.0}"#);
        let parsed = parse_json(&s).unwrap();
        assert_eq!(parsed.get("a").unwrap().as_i64(), Some(42));
        assert_eq!(parsed.get("b").unwrap().as_f64(), Some(1.0));
    }

    #[test]
    fn test_script_evaluator() {
        let ctx = EvalContext::new(1.5, 42.0, 2, 120.0, true);
        let v = eval_expr("sin(t * MEUM) * 0.35 + cos(t * PHI) * 0.65", &ctx).unwrap();
        let expected = (1.5 * MEUM).sin() * 0.35 + (1.5 * PHI).cos() * 0.65;
        assert!((v - expected).abs() < 1e-12);

        let script = "# Global script algo\ndef global_script(t, name, i):\n    v = isn(t * MEUM) * 0.4 + ics(t * PHI) * 0.3\n    return v * 0.35\n";
        let sv = eval_script(script, &ctx).unwrap();
        let exp_s = (eqr_isn(1.5 * MEUM) * 0.4 + eqr_ics(1.5 * PHI) * 0.3) * 0.35;
        assert!((sv - exp_s).abs() < 1e-12);
    }
}
