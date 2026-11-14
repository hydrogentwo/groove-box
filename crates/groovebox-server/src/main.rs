//! Pure-Rust multi-threaded HTTP/1.1 server for Groovebox Web Studio & REST API.
//!
//! Binds to `0.0.0.0:PORT` (default 8080; override with `PORT` env or `--port N`).
//! Zero external crate dependencies — compiles fast and runs cleanly everywhere.

use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use groovebox_dsp::{
    encode_wav_pcm16, generate_synth_names, render_project, CommutativePairSpace, RenderSummary,
};
use groovebox_game::{
    classify_from_composition, export_game_package_zip, prove_software_lattice, ClassifyParams,
    OpenWorldSandbox,
};
use groovebox_math::{
    compositional_xyz, debug_cross_engine_alignment, domain_equation_hints, eski_fractal_pick,
    eval_expr, eval_script, parse_json, EvalContext, JsonValue, MEUM, PHI,
};
use groovebox_state::{
    audit_project, CompositionToggleState, DeterministicTriggerSculptor, GlobalPlayAlgo,
    NonRedundantEuclidean, ProjectState,
};
use groovebox_visual::{
    build_spatial_state, canonical_visual_instrument, composition_fingerprint, encode_png_rgb,
    fibonacci_view, golden_composition_fingerprint, select_views, visual_signal_id,
    VideoSynthEngine,
};

const INDEX_HTML: &str = include_str!("../assets/index.html");
const STYLE_CSS: &str = include_str!("../assets/style.css");
const APP_JS: &str = include_str!("../assets/app.js");

struct AppState {
    project: ProjectState,
    undo_stack: Vec<ProjectState>,
    last_wav: Vec<u8>,
    last_summary: Option<RenderSummary>,
    sandbox: OpenWorldSandbox,
    rand_salt: u64,
}

impl AppState {
    fn new() -> Self {
        let mut project = ProjectState::default();
        let names = generate_synth_names(project.tracks.len());
        for (t, nm) in project.tracks.iter_mut().zip(names.into_iter()) {
            t.name = nm;
        }
        let render = render_project(&project, 1);
        let prov = format!(
            "fp={};audio={}",
            render.summary.canonical_fingerprint, render.summary.audio_sha256
        );
        let wav = encode_wav_pcm16(&render.samples, render.summary.sample_rate, Some(&prov));
        let ident = classify_from_project(&project);
        let sandbox = OpenWorldSandbox::new(ident);
        Self {
            project,
            undo_stack: Vec::new(),
            last_wav: wav,
            last_summary: Some(render.summary),
            sandbox,
            rand_salt: 1,
        }
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(self.project.clone());
        if self.undo_stack.len() > 32 {
            self.undo_stack.remove(0);
        }
    }

    fn render_now(&mut self, bars: usize) -> RenderSummary {
        let out = render_project(&self.project, bars);
        let prov = format!(
            "fp={};audio={};bpm={};seed={}",
            out.summary.canonical_fingerprint,
            out.summary.audio_sha256,
            self.project.bpm,
            self.project.seed_input
        );
        self.last_wav = encode_wav_pcm16(&out.samples, out.summary.sample_rate, Some(&prov));
        self.last_summary = Some(out.summary.clone());
        out.summary
    }
}

fn classify_from_project(p: &ProjectState) -> groovebox_game::GameIdentity {
    classify_from_composition(&ClassifyParams {
        seed: p.numeric_seed(),
        bpm: p.bpm,
        seq_length: p.seq_length,
        playlist_rows: p.playlist_rows,
        goava_active: p.toggles.goava,
        randomizer_active: p.toggles.randomizer,
        phase_lock_active: p.toggles.phaselock,
        live_dj_goava: p.dj_goava_amount > 1e-4,
        live_dj_random: p.dj_random_amount > 1e-4,
        global_algo_fingerprint: if p.global_play.apply_enabled {
            Some(p.global_play.fingerprint())
        } else {
            None
        },
        step_algorithm_fingerprint: None,
        ot: false,
    })
}

struct HttpRequest {
    method: String,
    path: String,
    query: HashMap<String, String>,
    body: Vec<u8>,
}

struct HttpResponse {
    status: u16,
    status_text: &'static str,
    content_type: &'static str,
    extra_headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn ok_bytes(content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            status_text: "OK",
            content_type,
            extra_headers: Vec::new(),
            body,
        }
    }

    fn ok_json(v: &JsonValue) -> Self {
        Self::ok_bytes("application/json; charset=utf-8", v.to_canonical_string().into_bytes())
    }

    fn err(status: u16, msg: &str) -> Self {
        let mut m = JsonValue::obj();
        m.insert("ok".into(), false.into());
        m.insert("error".into(), msg.into());
        let status_text = match status {
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            _ => "Internal Server Error",
        };
        Self {
            status,
            status_text,
            content_type: "application/json; charset=utf-8",
            extra_headers: Vec::new(),
            body: JsonValue::Object(m).to_canonical_string().into_bytes(),
        }
    }

    fn with_header(mut self, k: &'static str, v: impl Into<String>) -> Self {
        self.extra_headers.push((k, v.into()));
        self
    }
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                    if let Ok(val) = u8::from_str_radix(hex, 16) {
                        out.push(val);
                        i += 3;
                        continue;
                    }
                }
                out.push(bytes[i]);
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut it = pair.splitn(2, '=');
        let k = url_decode(it.next().unwrap_or(""));
        let v = url_decode(it.next().unwrap_or(""));
        map.insert(k, v);
    }
    map
}

fn read_request(stream: &TcpStream) -> Option<HttpRequest> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).ok()? == 0 {
        return None;
    }
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let method = parts[0].to_string();
    let raw_target = parts[1];
    let (path, query) = match raw_target.split_once('?') {
        Some((p, q)) => (p.to_string(), parse_query(q)),
        None => (raw_target.to_string(), HashMap::new()),
    };

    let mut content_len = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                content_len = v.trim().parse::<usize>().unwrap_or(0).min(8 * 1024 * 1024);
            }
        }
    }

    let mut body = vec![0u8; content_len];
    if content_len > 0 {
        reader.read_exact(&mut body).ok()?;
    }

    Some(HttpRequest { method, path, query, body })
}

fn write_response(mut stream: &TcpStream, resp: HttpResponse) {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n",
        resp.status,
        resp.status_text,
        resp.content_type,
        resp.body.len()
    );
    for (k, v) in resp.extra_headers {
        head.push_str(&format!("{}: {}\r\n", k, v));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&resp.body);
    let _ = stream.flush();
}

fn state_payload(st: &AppState) -> JsonValue {
    let mut m = JsonValue::obj();
    m.insert("ok".into(), true.into());
    m.insert("project".into(), st.project.to_json());
    m.insert("undo_depth".into(), st.undo_stack.len().into());
    if let Some(ref s) = st.last_summary {
        m.insert("render".into(), s.to_json());
    }
    let ps = CommutativePairSpace::new(48);
    let pd = ps.descriptor(st.project.dj_pair.0, st.project.dj_pair.1);
    let mut dj = JsonValue::obj();
    dj.insert("a".into(), pd.a.into());
    dj.insert("b".into(), pd.b.into());
    dj.insert("index".into(), pd.index.into());
    dj.insert("signature".into(), format!("{:016x}", pd.signature).into());
    dj.insert("phase".into(), pd.phase.into());
    dj.insert("ratio".into(), pd.ratio.into());
    dj.insert("spread".into(), pd.spread.into());
    m.insert("dj_pair_descriptor".into(), JsonValue::Object(dj));
    JsonValue::Object(m)
}

fn handle_request(req: HttpRequest, state: &Arc<Mutex<AppState>>) -> HttpResponse {
    if req.method == "OPTIONS" {
        return HttpResponse::ok_bytes("text/plain", Vec::new());
    }
    let is_head = req.method == "HEAD";
    let method_str = if is_head { "GET" } else { req.method.as_str() };

    let mut resp = match (method_str, req.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => {
            HttpResponse::ok_bytes("text/html; charset=utf-8", INDEX_HTML.as_bytes().to_vec())
        }
        ("GET", "/style.css") => {
            HttpResponse::ok_bytes("text/css; charset=utf-8", STYLE_CSS.as_bytes().to_vec())
        }
        ("GET", "/app.js") => {
            HttpResponse::ok_bytes("application/javascript; charset=utf-8", APP_JS.as_bytes().to_vec())
        }
        ("GET", "/api/health") => {
            let mut m = JsonValue::obj();
            m.insert("ok".into(), true.into());
            m.insert("engine".into(), "groovebox-rust".into());
            m.insert("version".into(), env!("CARGO_PKG_VERSION").into());
            HttpResponse::ok_json(&JsonValue::Object(m))
        }
        ("GET", "/api/state") => {
            let st = state.lock().unwrap();
            HttpResponse::ok_json(&state_payload(&st))
        }
        ("POST", "/api/state") => {
            let body_str = String::from_utf8_lossy(&req.body);
            let parsed = match parse_json(&body_str) {
                Ok(v) => v,
                Err(e) => return HttpResponse::err(400, &format!("Invalid JSON: {}", e)),
            };
            let mut st = state.lock().unwrap();
            st.push_undo();
            let rebuild_tracks = parsed.get("rebuild_tracks").and_then(|v| v.as_bool()).unwrap_or(false);
            let proj_val = parsed.get("project").unwrap_or(&parsed);
            let prev_n = st.project.tracks.len();
            let prev_steps = st.project.seq_length;
            let mut next = ProjectState::from_json(proj_val);

            // Resize tracks cleanly if n_instruments or seq_length changed without full rebuild
            if rebuild_tracks {
                next.rebuild_default_tracks();
            } else {
                if next.tracks.len() != next.n_instruments {
                    let target_n = next.n_instruments.clamp(2, 64);
                    if target_n < next.tracks.len() {
                        next.tracks.truncate(target_n);
                    } else {
                        let mut template = next.clone();
                        template.n_instruments = target_n;
                        template.rebuild_default_tracks();
                        for idx in next.tracks.len()..target_n {
                            if let Some(t) = template.tracks.get(idx) {
                                next.tracks.push(t.clone());
                            }
                        }
                    }
                }
                if next.seq_length != prev_steps {
                    let sl = next.seq_length.clamp(4, 64);
                    for t in &mut next.tracks {
                        while t.steps.len() < sl {
                            let copy_idx = t.steps.len() % prev_steps.max(1);
                            let c = t.steps.get(copy_idx).cloned().unwrap_or_default();
                            t.steps.push(c);
                        }
                        t.steps.truncate(sl);
                    }
                }
            }
            if next.tracks.len() != prev_n || rebuild_tracks {
                let names = generate_synth_names(next.tracks.len());
                for (t, nm) in next.tracks.iter_mut().zip(names.into_iter()) {
                    t.name = nm;
                }
            }
            st.project = next;
            let auto_render = parsed.get("render").and_then(|v| v.as_bool()).unwrap_or(false);
            if auto_render {
                let bars = parsed.get("bars").and_then(|v| v.as_i64()).unwrap_or(1) as usize;
                st.render_now(bars);
            }
            HttpResponse::ok_json(&state_payload(&st))
        }
        ("POST", "/api/undo") => {
            let mut st = state.lock().unwrap();
            if let Some(prev) = st.undo_stack.pop() {
                st.project = prev;
                st.render_now(1);
            }
            HttpResponse::ok_json(&state_payload(&st))
        }
        ("POST", "/api/tracks/euclidean") => {
            let body_str = String::from_utf8_lossy(&req.body);
            let parsed = parse_json(&body_str).unwrap_or(JsonValue::Null);
            let mut st = state.lock().unwrap();
            st.push_undo();
            let track_idx = parsed.get("track").and_then(|v| v.as_i64());
            let pulses_arg = parsed.get("pulses").and_then(|v| v.as_i64());
            let steps = st.project.seq_length.clamp(4, 64);
            let seed_i = st.project.numeric_seed() as i64;
            for (i, tr) in st.project.tracks.iter_mut().enumerate() {
                if let Some(ti) = track_idx {
                    if ti as usize != i {
                        continue;
                    }
                }
                let pulses = pulses_arg
                    .map(|p| (p as usize).clamp(1, steps))
                    .unwrap_or_else(|| (2 + (i * 3 + 1) % steps.min(11).max(2)).min(steps));
                let pat = NonRedundantEuclidean::generate(steps, pulses, -1, seed_i, &tr.name);
                for (s_idx, cell) in tr.steps.iter_mut().enumerate() {
                    cell.active = pat.get(s_idx).copied().unwrap_or(false);
                }
            }
            st.render_now(1);
            HttpResponse::ok_json(&state_payload(&st))
        }
        ("POST", "/api/tracks/sculpt") => {
            let mut st = state.lock().unwrap();
            st.push_undo();
            let steps = st.project.seq_length.clamp(4, 64);
            let seed_i = st.project.numeric_seed() as i64;
            let names: Vec<String> = st.project.tracks.iter().map(|t| t.name.clone()).collect();
            let sculptor = DeterministicTriggerSculptor::new(seed_i, &names, steps);
            for tr in &mut st.project.tracks {
                for (s_idx, cell) in tr.steps.iter_mut().enumerate() {
                    cell.active = sculptor.should_trigger(&tr.name, s_idx);
                }
            }
            st.render_now(1);
            HttpResponse::ok_json(&state_payload(&st))
        }
        ("POST", "/api/global-play/randomize") => {
            let mut st = state.lock().unwrap();
            st.push_undo();
            st.rand_salt = st.rand_salt.wrapping_add(1);
            let seed_i = st.project.numeric_seed() as i64;
            // Authoring-only: apply_enabled stays false per spec
            st.project.global_play = GlobalPlayAlgo::randomized_from_seed(seed_i, st.rand_salt);
            HttpResponse::ok_json(&state_payload(&st))
        }
        ("POST", "/api/global-play/eval") => {
            let body_str = String::from_utf8_lossy(&req.body);
            let parsed = parse_json(&body_str).unwrap_or(JsonValue::Null);
            let st = state.lock().unwrap();
            let script = parsed
                .get("script")
                .and_then(|v| v.as_str())
                .unwrap_or(&st.project.global_play.script);
            let domain = parsed
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or(&st.project.global_play.domain);
            let seed = st.project.numeric_seed();
            let bpm = st.project.bpm;
            let ot = st.project.operator_theory;

            let mut script_vals = Vec::with_capacity(64);
            let mut domain_vals = Vec::with_capacity(64);
            let mut script_err: Option<String> = None;
            let mut domain_err: Option<String> = None;

            for k in 0..64 {
                let t = (k as f64) * 0.125;
                let ctx = EvalContext::new(t, seed, 0, bpm, ot);
                match eval_script(script, &ctx) {
                    Ok(v) => script_vals.push(JsonValue::Float(v)),
                    Err(e) => {
                        if script_err.is_none() { script_err = Some(e); }
                        script_vals.push(JsonValue::Float(0.0));
                    }
                }
                match eval_expr(domain, &ctx) {
                    Ok(v) => domain_vals.push(JsonValue::Float(v)),
                    Err(e) => {
                        if domain_err.is_none() { domain_err = Some(e); }
                        domain_vals.push(JsonValue::Float(0.0));
                    }
                }
            }
            let hints = domain_equation_hints(domain);
            let mut out = JsonValue::obj();
            out.insert("ok".into(), true.into());
            out.insert("script_curve".into(), JsonValue::Array(script_vals));
            out.insert("domain_curve".into(), JsonValue::Array(domain_vals));
            out.insert("hints".into(), JsonValue::Array(hints.into_iter().map(|s| s.into()).collect()));
            if let Some(e) = script_err { out.insert("script_error".into(), e.into()); }
            if let Some(e) = domain_err { out.insert("domain_error".into(), e.into()); }
            HttpResponse::ok_json(&JsonValue::Object(out))
        }
        ("POST", "/api/render") => {
            let body_str = String::from_utf8_lossy(&req.body);
            let parsed = parse_json(&body_str).unwrap_or(JsonValue::Null);
            let bars = parsed.get("bars").and_then(|v| v.as_i64()).unwrap_or(1).clamp(1, 8) as usize;
            let mut st = state.lock().unwrap();
            let summary = st.render_now(bars);
            let mut out = JsonValue::obj();
            out.insert("ok".into(), true.into());
            out.insert("render".into(), summary.to_json());
            HttpResponse::ok_json(&JsonValue::Object(out))
        }
        ("GET", "/api/audio.wav") => {
            let st = state.lock().unwrap();
            let fp = st.project.canonical_fingerprint();
            HttpResponse::ok_bytes("audio/wav", st.last_wav.clone())
                .with_header("Content-Disposition", format!("inline; filename=\"groovebox_{}.wav\"", fp))
        }
        ("GET", "/api/visual/slots") => {
            let st = state.lock().unwrap();
            let seed = st.project.numeric_seed();
            let seq = st.project.sequential_seed_values(0.0);
            let slots: Vec<JsonValue> = (0..st.project.tracks.len())
                .map(|i| canonical_visual_instrument(i, &st.project, &seq).to_json())
                .collect();
            let views: Vec<JsonValue> = select_views(16, seed).into_iter().map(|v| v.to_json()).collect();
            let view0 = fibonacci_view(0, 64, seed);
            let fp = st.project.canonical_fingerprint();
            let vsid = visual_signal_id(seed, &fp, &view0);
            let align = debug_cross_engine_alignment(seed, 1.0, st.project.tracks.len().min(16), st.project.operator_theory);

            let mut out = JsonValue::obj();
            out.insert("ok".into(), true.into());
            out.insert("count".into(), st.project.tracks.len().into());
            out.insert("seed".into(), seed.into());
            out.insert("canonical_fingerprint".into(), fp.into());
            out.insert("visual_signal_id".into(), vsid.into());
            out.insert("eski_fractal_set".into(), align.fractal_set.into());
            out.insert("view".into(), view0.to_json());
            out.insert("views".into(), JsonValue::Array(views));
            out.insert("slots".into(), JsonValue::Array(slots));
            HttpResponse::ok_json(&JsonValue::Object(out))
        }
        ("GET", "/api/visual/frame.png") => {
            let frame = req.query.get("frame").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let w = req.query.get("w").and_then(|v| v.parse::<usize>().ok()).unwrap_or(480).clamp(120, 1280);
            let h = req.query.get("h").and_then(|v| v.parse::<usize>().ok()).unwrap_or(270).clamp(90, 720);
            let st = state.lock().unwrap();
            let energy = st.last_summary.as_ref().map(|s| (s.rms * 3.0).clamp(0.25, 1.0)).unwrap_or(0.55);
            let bands = st
                .last_summary
                .as_ref()
                .map(|s| s.spectrum_bands.clone())
                .unwrap_or_else(|| vec![0.45; 16]);
            let mut vs = VideoSynthEngine::new(w, h);
            let fb = vs.render_frame(&st.project, frame, 30.0, energy, &bands);
            let png = encode_png_rgb(w as u32, h as u32, &fb.rgb);
            HttpResponse::ok_bytes("image/png", png)
        }
        ("GET", "/api/spatial") => {
            let st = state.lock().unwrap();
            let depth = req.query.get("depth").and_then(|v| v.parse::<usize>().ok()).unwrap_or(3).clamp(1, 4);
            let roots = req.query.get("roots").and_then(|v| v.parse::<usize>().ok()).unwrap_or(6).clamp(2, 10);
            let goava = req
                .query
                .get("goava")
                .map(|v| v == "1" || v == "true")
                .unwrap_or(st.project.toggles.goava);
            let seed = st.project.numeric_seed().abs() as u64;
            let fp = st.project.canonical_fingerprint();
            let snap = build_spatial_state(seed, &fp, depth, roots, goava);
            HttpResponse::ok_json(&snap.to_json())
        }
        ("GET", "/api/game/classify") => {
            let st = state.lock().unwrap();
            let ident = classify_from_project(&st.project);
            let proof = prove_software_lattice(64);
            let mut out = JsonValue::obj();
            out.insert("ok".into(), true.into());
            out.insert("identity".into(), ident.to_json());
            out.insert("lattice_proof".into(), proof);
            HttpResponse::ok_json(&JsonValue::Object(out))
        }
        ("GET", "/api/game/export.zip") => {
            let st = state.lock().unwrap();
            let ident = classify_from_project(&st.project);
            let zip = export_game_package_zip(&ident, &st.project);
            HttpResponse::ok_bytes("application/zip", zip).with_header(
                "Content-Disposition",
                format!("attachment; filename=\"groovebox_game_{}.zip\"", ident.world_fingerprint),
            )
        }
        ("GET", "/api/sandbox/state") => {
            let mut st = state.lock().unwrap();
            let snap = st.sandbox.snapshot_json();
            HttpResponse::ok_json(&snap)
        }
        ("POST", "/api/sandbox/reset") => {
            let mut st = state.lock().unwrap();
            let ident = classify_from_project(&st.project);
            st.sandbox = OpenWorldSandbox::new(ident);
            let snap = st.sandbox.snapshot_json();
            HttpResponse::ok_json(&snap)
        }
        ("POST", "/api/sandbox/input") => {
            let body_str = String::from_utf8_lossy(&req.body);
            let parsed = parse_json(&body_str).unwrap_or(JsonValue::Null);
            let dx = parsed.get("dx").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let dy = parsed.get("dy").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let dz = parsed.get("dz").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let dyaw = parsed.get("dyaw").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let dpitch = parsed.get("dpitch").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let dt = parsed.get("dt").and_then(|v| v.as_f64()).unwrap_or(0.1).clamp(0.01, 0.25);

            let mut st = state.lock().unwrap();
            if dyaw.abs() > 1e-9 || dpitch.abs() > 1e-9 {
                st.sandbox.mouse_look(dyaw, dpitch);
            }
            if dx.abs() > 1e-9 || dy.abs() > 1e-9 || dz.abs() > 1e-9 {
                st.sandbox.perspective_move(dx, dy, dz);
            }
            st.sandbox.tick(dt);
            let snap = st.sandbox.snapshot_json();
            HttpResponse::ok_json(&snap)
        }
        ("POST", "/api/sandbox/command") => {
            let body_str = String::from_utf8_lossy(&req.body);
            let parsed = parse_json(&body_str).unwrap_or(JsonValue::Null);
            let cmd = parsed.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let mut st = state.lock().unwrap();
            let reply = st.sandbox.handle_console_command(cmd);
            let mut snap = match st.sandbox.snapshot_json() {
                JsonValue::Object(m) => m,
                _ => JsonValue::obj(),
            };
            snap.insert("reply".into(), reply.into());
            HttpResponse::ok_json(&JsonValue::Object(snap))
        }
        ("GET", "/api/audit") => {
            let st = state.lock().unwrap();
            let a = audit_project(&st.project);
            let r1 = render_project(&st.project, 1);
            let r2 = render_project(&st.project, 1);
            let bit_identical = r1.summary.audio_sha256 == r2.summary.audio_sha256;
            let start_silence = r1.summary.start_sample == 0.0;

            // Golden Python-parity verification vectors
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
            let align = debug_cross_engine_alignment(st.project.numeric_seed(), 1.0, 8, st.project.operator_theory);

            let mut parity = JsonValue::obj();
            parity.insert("visual_comp_fp".into(), comp_fp.clone().into());
            parity.insert("visual_comp_fp_match".into(), (comp_fp == "5e52964bd37da571").into());
            parity.insert("visual_signal_id".into(), vsid.clone().into());
            parity.insert("visual_signal_id_match".into(), (vsid == "c58ab38456faa1965275695f").into());
            parity.insert("golden_fp".into(), gfp.clone().into());
            parity.insert("golden_fp_match".into(), (gfp == "fa4fdcaf14d4a013").into());
            parity.insert("spatial_fp".into(), sp.fingerprint.clone().into());
            parity.insert("spatial_fp_match".into(), (sp.fingerprint == "63787f44f62b003a").into());
            parity.insert("toggle_fp".into(), ts.fingerprint().into());
            parity.insert("toggle_fp_match".into(), (ts.fingerprint() == "b72e9ecb6fb10f37").into());
            parity.insert("game_title".into(), ident.title.clone().into());
            parity.insert("game_title_match".into(), (ident.title == "Glass Pilgrimage of Thread [35a52c]").into());
            parity.insert("game_comp_fp".into(), ident.composition_fingerprint.clone().into());
            parity.insert("game_comp_fp_match".into(), (ident.composition_fingerprint == "35a52c497fb87ec5").into());
            parity.insert("game_world_fp".into(), ident.world_fingerprint.clone().into());
            parity.insert("game_world_fp_match".into(), (ident.world_fingerprint == "000891775d470a34").into());
            parity.insert("eski_pick".into(), eski.into());
            parity.insert("eski_pick_match".into(), (eski == "starburst").into());
            parity.insert("xyz".into(), JsonValue::Array(vec![xyz.0.into(), xyz.1.into(), xyz.2.into()]));

            let all_parity = comp_fp == "5e52964bd37da571"
                && vsid == "c58ab38456faa1965275695f"
                && gfp == "fa4fdcaf14d4a013"
                && sp.fingerprint == "63787f44f62b003a"
                && ts.fingerprint() == "b72e9ecb6fb10f37"
                && ident.composition_fingerprint == "35a52c497fb87ec5"
                && ident.world_fingerprint == "000891775d470a34";

            let mut out = JsonValue::obj();
            out.insert("ok".into(), (a.ok && align.ok && bit_identical && start_silence && all_parity).into());
            out.insert("canonical_fingerprint".into(), a.fingerprint.into());
            out.insert("audio_sha256".into(), r1.summary.audio_sha256.into());
            out.insert("audio_bit_identical".into(), bit_identical.into());
            out.insert("start_sample_zero".into(), start_silence.into());
            out.insert("order_independent".into(), a.order_independent.into());
            out.insert("goava_mean_pitch".into(), a.goava_mean_pitch.into());
            out.insert("cross_engine_ok".into(), align.ok.into());
            out.insert("cross_engine_fractal_set".into(), align.fractal_set.into());
            out.insert("all_python_parity_match".into(), all_parity.into());
            out.insert("python_parity".into(), JsonValue::Object(parity));
            out.insert("errors".into(), JsonValue::Array(a.errors.into_iter().map(JsonValue::Str).collect()));
            HttpResponse::ok_json(&JsonValue::Object(out))
        }
        _ => HttpResponse::err(404, "Endpoint not found"),
    };
    if is_head {
        resp.body.clear();
    }
    resp
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut port: u16 = env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8080);
    for i in 1..args.len() {
        if args[i] == "--port" && i + 1 < args.len() {
            if let Ok(p) = args[i + 1].parse::<u16>() {
                port = p;
            }
        }
    }

    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).expect("Failed to bind TCP listener");
    let state = Arc::new(Mutex::new(AppState::new()));
    println!("Groovebox Rust Studio listening on http://{}", addr);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let st = Arc::clone(&state);
                thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
                    if let Some(req) = read_request(&stream) {
                        let resp = handle_request(req, &st);
                        write_response(&stream, resp);
                    }
                });
            }
            Err(e) => eprintln!("Accept error: {}", e),
        }
    }
}
