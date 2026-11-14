//! Groovebox pure-Rust Visual Determinism, Fractal Spatial Engine, VideoSynth, and PNG Encoder.

pub mod fractal_spatial;
pub mod png;
pub mod video_synth;
pub mod visual_determinism;

pub use fractal_spatial::*;
pub use png::*;
pub use video_synth::*;
pub use visual_determinism::*;

#[cfg(test)]
mod tests {
    use super::*;
    use groovebox_math::JsonValue;
    use groovebox_state::ProjectState;

    #[test]
    fn test_visual_determinism_exact_python_parity() {
        let v0 = fibonacci_view(0, 64, 42.0);
        assert!((v0.yaw_deg - (-3.0549493682611457)).abs() < 1e-10);
        assert!((v0.pitch_deg - (-9.675337363446829)).abs() < 1e-10);

        let sel = select_views(8, 42.0);
        let idxs: Vec<usize> = sel.iter().map(|v| v.index).collect();
        assert_eq!(idxs, vec![0, 30, 21, 15, 9, 19, 18, 12]);

        let mut o1 = JsonValue::obj();
        o1.insert("id".into(), 2.into());
        o1.insert("type".into(), "orb".into());
        o1.insert("x".into(), 1.0.into());
        o1.insert("y".into(), 2.0.into());
        o1.insert("z".into(), 3.0.into());
        let mut o2 = JsonValue::obj();
        o2.insert("id".into(), 1.into());
        o2.insert("type".into(), "ring".into());
        o2.insert("x".into(), 4.0.into());
        o2.insert("y".into(), 5.0.into());
        o2.insert("z".into(), 6.0.into());

        let objs_a = vec![JsonValue::Object(o1.clone()), JsonValue::Object(o2.clone())];
        let objs_b = vec![JsonValue::Object(o2), JsonValue::Object(o1)];

        let fp_a = composition_fingerprint(&objs_a, 42.0, "structural");
        let fp_b = composition_fingerprint(&objs_b, 42.0, "structural");
        assert_eq!(fp_a, fp_b);
        assert_eq!(fp_a, "5e52964bd37da571");

        let vsid = visual_signal_id(42.0, &fp_a, &v0);
        assert_eq!(vsid, "c58ab38456faa1965275695f");

        let gfp = golden_composition_fingerprint(16, 918273.0, &[-100.0, 0.0, 25.0, 400.0]);
        assert_eq!(gfp, "fa4fdcaf14d4a013");
    }

    #[test]
    fn test_fractal_spatial_exact_python_parity() {
        let sp = build_spatial_state(42, "fp", 2, 3, false);
        assert_eq!(sp.points.len(), 20);
        assert_eq!(sp.edges.len(), 18);
        assert_eq!(sp.points[0].id, "b1737edadad727fd38c78f0e");
        assert_eq!(sp.fingerprint, "63787f44f62b003a");

        // Topology is GOAVA-independent
        let sp_g = build_spatial_state(42, "fp", 2, 3, true);
        assert_eq!(sp.edges, sp_g.edges);

        // GOAVA adapter modifies phase only when enabled
        let eng = FractalSpatialEngine::new(42, "fp", true);
        let adapted = eng.apply_goava_adapter(&sp.points);
        assert_ne!(adapted, sp.points);
    }

    #[test]
    fn test_video_synth_and_png_determinism() {
        let project = ProjectState::default();
        let mut eng1 = VideoSynthEngine::new(160, 90);
        let mut eng2 = VideoSynthEngine::new(160, 90);
        let bands = vec![0.4; 16];
        let f1 = eng1.render_frame(&project, 12, 30.0, 0.6, &bands);
        let f2 = eng2.render_frame(&project, 12, 30.0, 0.6, &bands);
        assert_eq!(f1.rgb, f2.rgb);
        let png = encode_png_rgb(160, 90, &f1.rgb);
        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }
}
