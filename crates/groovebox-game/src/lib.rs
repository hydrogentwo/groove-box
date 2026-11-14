//! Groovebox pure-Rust Deterministic Videogame / Software Generator & Open-World Sandbox.

pub mod chunks;
pub mod identity;
pub mod sandbox;
pub mod zip_export;

pub use chunks::*;
pub use identity::*;
pub use sandbox::*;
pub use zip_export::*;

#[cfg(test)]
mod tests {
    use super::*;
    use groovebox_state::ProjectState;

    #[test]
    fn test_classify_exact_python_parity() {
        let p = ClassifyParams {
            seed: 42.0,
            bpm: 120.0,
            seq_length: 16,
            playlist_rows: 32,
            ..Default::default()
        };
        let ident = classify_from_composition(&p);
        assert_eq!(ident.title, "Glass Pilgrimage of Thread [35a52c]");
        assert_eq!(ident.genre, "open_world");
        assert_eq!(ident.camera, "first_person");
        assert_eq!(ident.topology, "open_world");
        assert_eq!(ident.social, "local_coop");
        assert_eq!(ident.mood, "industrial");
        assert_eq!(ident.software_kind, "videogame");
        assert_eq!(ident.objective, "siege");
        assert_eq!(ident.difficulty, "meum_insane");
        assert_eq!(ident.level_type, "coral_grid");
        assert_eq!(ident.composition_fingerprint, "35a52c497fb87ec5");
        assert_eq!(ident.world_fingerprint, "000891775d470a34");
        assert_eq!(ident.visual_signal_id, "e1f0f176a7e0d17f10cc05fa");
        assert_eq!(ident.host_port, 33436);
        assert_eq!(ident.ui_palette.get("bg").map(String::as_str), Some("#b73238"));
        assert_eq!(ident.ui_palette.get("accent").map(String::as_str), Some("#c666b2"));
    }

    #[test]
    fn test_open_world_sandbox_contract() {
        let ident = classify_from_composition(&ClassifyParams::default());
        let mut sb = OpenWorldSandbox::new(ident);
        let (x0, z0, y0) = (sb.x, sb.z, sb.yaw);
        for _ in 0..120 {
            sb.tick(1.0 / 30.0);
        }
        // Free-roam idle stability: world never moves or rolls without player input
        assert!((sb.x - x0).abs() < 1e-12);
        assert!((sb.z - z0).abs() < 1e-12);
        assert!((sb.yaw - y0).abs() < 1e-12);
        assert!(sb.roll_deg.abs() < 1e-12);

        // Player-authored movement + damping
        sb.perspective_move(0.0, 0.0, 1.0);
        for _ in 0..10 {
            sb.tick(1.0 / 30.0);
        }
        assert!((sb.z - z0).abs() > 0.1);

        // Region sandbox commands
        let r1 = sb.handle_console_command("/build crate Test Box");
        assert!(r1.contains("Test Box"));
        let r2 = sb.handle_console_command("/note hello rust sandbox");
        assert!(r2.contains("hello rust sandbox"));
        let r3 = sb.handle_console_command("/remove");
        assert!(r3.contains("Removed"));
    }

    #[test]
    fn test_zip_export_deterministic() {
        let project = ProjectState::default();
        let ident = classify_from_composition(&ClassifyParams::default());
        let z1 = export_game_package_zip(&ident, &project);
        let z2 = export_game_package_zip(&ident, &project);
        assert_eq!(z1, z2);
        assert_eq!(&z1[0..4], b"PK\x03\x04");
        assert!(z1.len() > 20_000);
    }
}
