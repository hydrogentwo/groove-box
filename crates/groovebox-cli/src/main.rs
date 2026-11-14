//! Groovebox Rust CLI — headless deterministic audio, visual, spatial, game export, and parity runner.

use std::env;
use std::fs;
use std::process;

use groovebox_dsp::{encode_wav_pcm16, generate_synth_names, render_project};
use groovebox_game::{
    classify_from_composition, export_game_package_zip, prove_software_lattice, ClassifyParams,
};
use groovebox_math::{
    compositional_xyz, debug_cross_engine_alignment, eski_fractal_pick, JsonValue, MEUM, PHI,
};
use groovebox_state::{audit_project, CompositionToggleState, ProjectState};
use groovebox_visual::{
    build_spatial_state, composition_fingerprint, encode_png_rgb, fibonacci_view,
    golden_composition_fingerprint, select_views, visual_signal_id, VideoSynthEngine,
};

fn arg_val(args: &[String], flag: &str) -> Option<String> {
    for i in 0..args.len() {
        if args[i] == flag && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
    }
    None
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn build_project_from_args(args: &[String]) -> ProjectState {
    let mut p = ProjectState::default();
    if let Some(s) = arg_val(args, "--seed") {
        p.seed_input = s;
    }
    if let Some(b) = arg_val(args, "--bpm").and_then(|v| v.parse::<f64>().ok()) {
        p.bpm = b.clamp(20.0, 480.0);
    }
    if let Some(n) = arg_val(args, "--instruments").and_then(|v| v.parse::<usize>().ok()) {
        p.n_instruments = n.clamp(2, 64);
    }
    if let Some(sr) = arg_val(args, "--sr").and_then(|v| v.parse::<u32>().ok()) {
        p.sample_rate = sr.clamp(8000, 96000);
    }
    p.toggles.goava = has_flag(args, "--goava");
    p.toggles.randomizer = has_flag(args, "--randomizer");
    p.toggles.phaselock = has_flag(args, "--phaselock");
    if has_flag(args, "--no-ot") {
        p.operator_theory = false;
    }
    p.rebuild_default_tracks();
    let names = generate_synth_names(p.tracks.len());
    for (t, nm) in p.tracks.iter_mut().zip(names.into_iter()) {
        t.name = nm;
    }
    p
}

fn usage() {
    eprintln!(
        "groovebox-cli (Rust v{})\n\n\
        Commands:\n  \
          render-wav    [--seed 42] [--bpm 120] [--bars 2] [--instruments 8] [--out out.wav]\n  \
          render-frame  [--seed 42] [--frame 0] [--width 640] [--height 360] [--out frame.png]\n  \
          classify-game [--seed 42] [--bpm 120] [--goava] [--randomizer] [--phaselock]\n  \
          export-game   [--seed 42] [--out game.zip]\n  \
          spatial       [--seed 42] [--depth 3] [--roots 5] [--goava]\n  \
          visual        [--seed 42] [--count 16]\n  \
          prove-lattice [--samples 64]\n  \
          audit         [--seed 42]\n  \
          parity        Print golden parity JSON matching the Python reference\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");

    match cmd {
        "render-wav" => {
            let p = build_project_from_args(&args);
            let bars = arg_val(&args, "--bars").and_then(|v| v.parse().ok()).unwrap_or(2);
            let out_path = arg_val(&args, "--out").unwrap_or_else(|| "groovebox_out.wav".into());
            let res = render_project(&p, bars);
            let prov = format!(
                "canonical_fp={};audio_sha256={};bpm={};seed={}",
                res.summary.canonical_fingerprint, res.summary.audio_sha256, p.bpm, p.seed_input
            );
            let wav = encode_wav_pcm16(&res.samples, res.summary.sample_rate, Some(&prov));
            if let Err(e) = fs::write(&out_path, &wav) {
                eprintln!("Failed to write {}: {}", out_path, e);
                process::exit(1);
            }
            println!(
                "Wrote {} ({} bytes, {:.2}s, rms={:.4}, peak={:.4}, sha256={})",
                out_path,
                wav.len(),
                res.summary.duration_sec,
                res.summary.rms,
                res.summary.peak,
                res.summary.audio_sha256
            );
        }
        "render-frame" => {
            let p = build_project_from_args(&args);
            let frame = arg_val(&args, "--frame").and_then(|v| v.parse().ok()).unwrap_or(0);
            let w = arg_val(&args, "--width").and_then(|v| v.parse().ok()).unwrap_or(640);
            let h = arg_val(&args, "--height").and_then(|v| v.parse().ok()).unwrap_or(360);
            let out_path = arg_val(&args, "--out").unwrap_or_else(|| "groovebox_frame.png".into());
            let mut vs = VideoSynthEngine::new(w, h);
            let fb = vs.render_frame(&p, frame, 30.0, 0.65, &[0.55; 16]);
            let png = encode_png_rgb(w as u32, h as u32, &fb.rgb);
            if let Err(e) = fs::write(&out_path, &png) {
                eprintln!("Failed to write {}: {}", out_path, e);
                process::exit(1);
            }
            println!("Wrote {} ({}x{}, {} bytes)", out_path, w, h, png.len());
        }
        "classify-game" => {
            let p = build_project_from_args(&args);
            let ident = classify_from_composition(&ClassifyParams {
                seed: p.numeric_seed(),
                bpm: p.bpm,
                seq_length: p.seq_length,
                playlist_rows: p.playlist_rows,
                goava_active: p.toggles.goava,
                randomizer_active: p.toggles.randomizer,
                phase_lock_active: p.toggles.phaselock,
                ..Default::default()
            });
            println!("{}", ident.to_json().to_pretty_string());
        }
        "export-game" => {
            let p = build_project_from_args(&args);
            let out_path = arg_val(&args, "--out").unwrap_or_else(|| "groovebox_game.zip".into());
            let ident = classify_from_composition(&ClassifyParams {
                seed: p.numeric_seed(),
                bpm: p.bpm,
                seq_length: p.seq_length,
                playlist_rows: p.playlist_rows,
                goava_active: p.toggles.goava,
                randomizer_active: p.toggles.randomizer,
                phase_lock_active: p.toggles.phaselock,
                ..Default::default()
            });
            let zip = export_game_package_zip(&ident, &p);
            if let Err(e) = fs::write(&out_path, &zip) {
                eprintln!("Failed to write {}: {}", out_path, e);
                process::exit(1);
            }
            println!(
                "Exported '{}' ({}) to {} ({} bytes, world_fp={})",
                ident.title, ident.software_kind, out_path, zip.len(), ident.world_fingerprint
            );
        }
        "spatial" => {
            let seed_raw = arg_val(&args, "--seed").unwrap_or_else(|| "42".into());
            let seed: u64 = seed_raw
                .parse::<u64>()
                .ok()
                .or_else(|| seed_raw.parse::<f64>().ok().map(|f| f.round() as i64 as u64))
                .unwrap_or_else(|| groovebox_math::seed_int_str(&seed_raw) as u64);
            let fp = arg_val(&args, "--fp").unwrap_or_else(|| "0000000000000000".into());
            let depth = arg_val(&args, "--depth").and_then(|v| v.parse().ok()).unwrap_or(3);
            let roots = arg_val(&args, "--roots").and_then(|v| v.parse().ok()).unwrap_or(5);
            let goava = has_flag(&args, "--goava");
            let snap = build_spatial_state(seed, &fp, depth, roots, goava);
            println!("{}", snap.to_json().to_pretty_string());
        }
        "visual" => {
            let seed = arg_val(&args, "--seed").and_then(|v| v.parse::<f64>().ok()).unwrap_or(42.0);
            let count = arg_val(&args, "--count").and_then(|v| v.parse().ok()).unwrap_or(16);
            let views = select_views(count.min(32), seed);
            let gfp = golden_composition_fingerprint(count, seed, &[-100.0, 0.0, 25.0, 400.0]);
            let mut m = JsonValue::obj();
            m.insert("seed".into(), seed.into());
            m.insert("count".into(), count.into());
            m.insert("golden_fingerprint".into(), gfp.into());
            m.insert("views".into(), JsonValue::Array(views.iter().map(|v| v.to_json()).collect()));
            println!("{}", JsonValue::Object(m).to_pretty_string());
        }
        "prove-lattice" => {
            let n = arg_val(&args, "--samples")
                .or_else(|| arg_val(&args, "--n-seeds"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(64);
            println!("{}", prove_software_lattice(n).to_pretty_string());
        }
        "audit" => {
            let p = build_project_from_args(&args);
            let a = audit_project(&p);
            let align = debug_cross_engine_alignment(p.numeric_seed(), 1.0, 8, p.operator_theory);
            let mut m = JsonValue::obj();
            m.insert("ok".into(), (a.ok && align.ok).into());
            m.insert("canonical_fingerprint".into(), a.fingerprint.into());
            m.insert("order_independent".into(), a.order_independent.into());
            m.insert("goava_mean_pitch".into(), a.goava_mean_pitch.into());
            m.insert("fractal_set".into(), align.fractal_set.into());
            m.insert("alignment_slots".into(), align.slots.len().into());
            m.insert("errors".into(), JsonValue::Array(a.errors.into_iter().map(JsonValue::Str).collect()));
            println!("{}", JsonValue::Object(m).to_pretty_string());
        }
        "parity" => {
            let v0 = fibonacci_view(0, 64, 42.0);
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
            let comp_fp = composition_fingerprint(&[JsonValue::Object(o1), JsonValue::Object(o2)], 42.0, "structural");
            let vsid = visual_signal_id(42.0, &comp_fp, &v0);
            let gfp = golden_composition_fingerprint(16, 918273.0, &[-100.0, 0.0, 25.0, 400.0]);
            let sp = build_spatial_state(42, "fp", 2, 3, false);
            let ts = CompositionToggleState { seed: 42, randomizer: true, goava: true, ..Default::default() };
            let ident = classify_from_composition(&ClassifyParams::default());
            let seq = [42.0, 42.0 * MEUM, 42.0 * PHI];
            let eski = eski_fractal_pick(42, &seq, 0);
            let xyz = compositional_xyz(42.0, &seq, 1.0, 0, false);

            let mut out = JsonValue::obj();
            out.insert("visual_comp_fp".into(), comp_fp.into());
            out.insert("visual_signal_id".into(), vsid.into());
            out.insert("visual_golden_fp".into(), gfp.into());
            out.insert("spatial_fp".into(), sp.fingerprint.into());
            out.insert("spatial_pt0_id".into(), sp.points[0].id.clone().into());
            out.insert("toggle_fp".into(), ts.fingerprint().into());
            out.insert("game_title".into(), ident.title.into());
            out.insert("game_comp_fp".into(), ident.composition_fingerprint.into());
            out.insert("game_world_fp".into(), ident.world_fingerprint.into());
            out.insert("game_visual_signal_id".into(), ident.visual_signal_id.into());
            out.insert("eski_pick".into(), eski.into());
            out.insert("xyz".into(), JsonValue::Array(vec![xyz.0.into(), xyz.1.into(), xyz.2.into()]));
            println!("{}", JsonValue::Object(out).to_pretty_string());
        }
        _ => usage(),
    }
}
