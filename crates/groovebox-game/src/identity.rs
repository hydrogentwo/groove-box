//! Deterministic Game & Software Lattice Classifier (exact parity with `videogame_engine.py`).

use std::collections::BTreeMap;
use std::f64::consts::TAU;
use groovebox_math::{
    format_python_f64, ot_band, ot_game_residue, safe_int_seed_vge, sha256_hex, sha256_u64_be,
    JsonValue, MEUM, PHI,
};
use groovebox_visual::{
    build_spatial_state, fibonacci_view, visual_signal_id, CameraView, SpatialSnapshot,
};

pub const GENRES: [&str; 15] = [
    "open_world", "fps", "rpg", "sandbox", "survival", "arena",
    "storytelling", "gta_like", "platformer", "strategy", "racing", "puzzle",
    "adventure", "dating_sim", "metroidvania",
];

pub fn genre_hook(genre: &str) -> Option<&'static str> {
    match genre {
        "open_world" | "sandbox" => Some("hook_world_free_roam"),
        "fps" => Some("hook_aim_sight_click"),
        "rpg" => Some("hook_quest_chains"),
        "storytelling" => Some("hook_story_arc_log"),
        "gta_like" => Some("hook_world_vehicles_free_roam"),
        "metroidvania" => Some("hook_power_gates_backtrack"),
        "survival" => Some("hook_craft_resources"),
        "strategy" => Some("hook_orders_queue"),
        "racing" => Some("hook_time_trial"),
        "arena" => Some("hook_encounter_wave"),
        "platformer" => Some("hook_platform_physics"),
        "puzzle" => Some("hook_sigil_logic"),
        "adventure" => Some("hook_dungeon_rumble"),
        "dating_sim" => Some("hook_affinity_dialogue"),
        _ => None,
    }
}

pub const CAMERAS: [&str; 5] = ["first_person", "second_person", "third_person", "top_down", "isometric"];
pub const TOPOLOGIES: [&str; 5] = ["linear", "open_world", "hub_spoke", "arena_loop", "roguelike_deck"];
pub const SOCIAL: [&str; 4] = ["singleplayer", "local_coop", "online_multiplayer", "asynchronous"];
pub const MOODS: [&str; 6] = ["neon_noir", "pastoral", "cosmic", "industrial", "mythic", "glitch"];
pub const OBJECTIVES: [&str; 6] = ["harvest", "escort", "survey", "siege", "nexus", "pilgrimage"];
pub const DIFFICULTIES: [&str; 4] = ["tutorial", "standard", "master", "meum_insane"];
pub const LEVEL_TYPES: [&str; 5] = ["heightfield", "boss_rush", "dungeon", "sky_islands", "coral_grid"];

pub const SOFTWARE_KINDS: [&str; 15] = [
    "videogame", "network_tool", "utility", "simulator", "media_player",
    "radio_toolkit", "data_viz", "chat_server", "protocol_lab", "instrument_lab",
    "office_suite", "file_manager", "terminal_lab", "browser_shell", "ide_lite",
];

pub const TEXTURE_FAMILIES: [&str; 10] = [
    "noise_meum", "grid_phi", "radial_harm", "crystal", "circuit",
    "organic", "moire", "barcode", "constellation", "wavefront",
];

pub const MODEL_PRIMITIVES: [&str; 10] = [
    "filament", "ring", "spiral", "panel", "prism",
    "polytope", "heightfield", "lattice_cage", "orb", "beam",
];

const TITLE_BANK_A: [&str; 32] = [
    "Hollow", "Radiant", "Silent", "Feral", "Ashen", "Gilded", "Errant",
    "Fractured", "Luminous", "Withered", "Untethered", "Crowned", "Drowned",
    "Static", "Recursive", "Nameless", "Borrowed", "Unmoored", "Spiral",
    "Threadbare", "Ossified", "Flickering", "Molten", "Quiet", "Errant",
    "Tessellated", "Unwritten", "Marrow", "Glass", "Ember", "Paper", "Salt",
];

const TITLE_BANK_B: [&str; 32] = [
    "Engine", "Choir", "Garden", "Reactor", "Ledger", "Orbit", "Threshold",
    "Cartography", "Lattice", "Season", "Harvest", "Skyline", "Vault",
    "Wager", "Ritual", "Migration", "Signal", "Cathedral", "Frontier",
    "Undertow", "Compass", "Archive", "Pilgrimage", "Static", "Refrain",
    "Foundry", "Meridian", "Aperture", "Reckoning", "Bloom", "Circuit", "Tide",
];

const TITLE_BANK_C: [&str; 24] = [
    "of Glass", "of Iron", "of Echoes", "of Dust", "of Tomorrow",
    "of the Deep", "of Rust", "of Salt", "of the Fold", "of the Interval",
    "of Static", "of the Long Night", "of Thread", "of the Nine",
    "of the Hollow", "of Amber", "of the Loop", "of the Tide",
    "of the Unwritten", "of the Meridian", "Reborn", "Unbound", "Interrupted",
    "Recompiled",
];

const TITLE_BANK_D: [&str; 20] = [
    "//", "—", "::", "∞", "◦", "○", "·", "‡", "†", "*", "~", "^", "»", "◊",
    "△", "▽", "◆", "☍", "❖", "✦",
];

pub fn objective_brief(obj: &str, free_play: bool) -> &'static str {
    if free_play {
        return "Free roam / sandbox: no forced objective. Explore loom regions, place objects, take notes. Ambient pickups exist but nothing is a win condition — you are not playing against a sigil ring.";
    }
    match obj {
        "harvest" => "Harvest: gather resource nodes scattered on the orbit. Sigils are not the goal; empty the resource field.",
        "escort" => "Escort: follow the waypoint trail in order. Movement and timing matter more than pickups.",
        "survey" => "Survey: visit every waypoint checkpoint. The world is a map to cover, not a ring to clear.",
        "siege" => "Siege: engage PvE threats and survive hazard rings. Combat density is the objective, not collectibles.",
        "nexus" => "Nexus: this is the rare case where sigils *are* the goal. Collect the sigil ring to complete the objective.",
        _ => "Pilgrimage: walk the waypoint path; slower score, larger level jumps. The journey is the content.",
    }
}

pub fn software_input_schema(kind: &str) -> JsonValue {
    let mut m = JsonValue::obj();
    let (req, opt, safety): (&[&str], &[&str], Option<&str>) = match kind {
        "network_tool" => (&["seed", "time", "peer_events"], &["host_port", "player_input"], None),
        "utility" => (&["seed", "command"], &["numeric_value", "text"], None),
        "simulator" => (&["seed", "time", "initial_state"], &["parameters", "player_input"], None),
        "media_player" => (&["seed", "time", "media_index"], &["audio_rms", "visual_phase"], None),
        "radio_toolkit" => (&["seed", "frequency", "mode"], &["morse_text", "waterfall_time"], Some("simulation_only_no_tx")),
        "data_viz" => (&["seed", "dataset"], &["x_field", "y_field", "time"], None),
        "chat_server" => (&["seed", "peer_events", "message"], &["host_port"], None),
        "protocol_lab" => (&["seed", "state", "event"], &["trace"], None),
        "instrument_lab" => (&["seed", "instrument_index"], &["patch_state", "sequence_state"], None),
        "office_suite" => (&["seed", "document_action"], &["text", "table"], None),
        "file_manager" => (&["seed", "virtual_path", "action"], &["selection"], None),
        "terminal_lab" => (&["seed", "command_text"], &["arguments"], Some("simulation_only_no_shell_exec")),
        "browser_shell" => (&["seed", "virtual_page"], &["navigation"], None),
        "ide_lite" => (&["seed", "source_buffer", "run_simulation"], &["language", "test_input"], Some("simulation_only_no_arbitrary_exec")),
        _ => (&["seed", "time", "player_input"], &["audio_rms", "phase", "composition_fingerprint"], None),
    };
    m.insert("required".into(), JsonValue::Array(req.iter().map(|&s| s.into()).collect()));
    m.insert("optional".into(), JsonValue::Array(opt.iter().map(|&s| s.into()).collect()));
    if let Some(s) = safety {
        m.insert("safety".into(), s.into());
    }
    JsonValue::Object(m)
}

pub fn mix_u64(seed: i64, label: &str) -> u64 {
    let blob = format!("{}|{}|{:.12}", seed, label, MEUM);
    sha256_u64_be(blob.as_bytes())
}

pub fn meum_game_residue(seed: i64, label: &str, ot: bool) -> f64 {
    let i = mix_u64(seed, label);
    let r = ((i % 10_000_000) as f64) / 10_000_000.0;
    if ot { ot_game_residue(r) } else { r }
}

pub fn meum_angle(k: f64, ot: bool) -> f64 {
    let mut base = (k * MEUM).rem_euclid(1.0);
    if ot {
        let banded = base + ot_band(base) * 0.05;
        base = (banded / (banded.abs() * 0.4 + 1.0)).rem_euclid(1.0);
    }
    TAU * base
}

fn res_idx(seed: i64, label: &str, modulo: usize, ot: bool) -> usize {
    ((meum_game_residue(seed, label, ot) * (modulo as f64)) as usize) % modulo.max(1)
}

pub fn generate_title(seed: i64, genre: &str, camera: &str, topology: &str, mood: &str, fp: &str) -> String {
    let a = TITLE_BANK_A[(mix_u64(seed, &format!("title_a|{}", genre)) as usize) % TITLE_BANK_A.len()];
    let b = TITLE_BANK_B[(mix_u64(seed, &format!("title_b|{}", topology)) as usize) % TITLE_BANK_B.len()];
    let c = TITLE_BANK_C[(mix_u64(seed, &format!("title_c|{}", camera)) as usize) % TITLE_BANK_C.len()];
    let use_flourish = (mix_u64(seed, "title_flourish") % 3) == 0;
    let d = TITLE_BANK_D[(mix_u64(seed, &format!("title_d|{}", mood)) as usize) % TITLE_BANK_D.len()];
    let base = format!("{} {} {}", a, b, c);
    let base = if use_flourish {
        format!("{} {} {}", d, base, d)
    } else {
        base
    };
    let short_fp = if fp.len() >= 6 { &fp[..6] } else { fp };
    format!("{} [{}]", base, short_fp)
}

#[derive(Debug, Clone)]
pub struct GoavaSeedRandomizer {
    pub seed: i64,
    pub ot: bool,
}

impl GoavaSeedRandomizer {
    pub fn new(seed: f64, ot: bool) -> Self {
        Self {
            seed: safe_int_seed_vge(seed),
            ot,
        }
    }

    pub fn residue(&self, namespace: &str, key: &str, op: &str) -> f64 {
        let label = format!("{}|{}|{}", namespace, key, op);
        meum_game_residue(self.seed, &label, self.ot)
    }

    pub fn value(&self, namespace: &str, key: &str, lo: f64, hi: f64) -> f64 {
        let (a, b) = if hi < lo { (hi, lo) } else { (lo, hi) };
        a + (b - a) * self.residue(namespace, key, "value")
    }

    pub fn index(&self, namespace: &str, key: &str, size: usize) -> usize {
        if size <= 1 {
            0
        } else {
            ((self.residue(namespace, key, "index") * (size as f64)) as usize).min(size - 1)
        }
    }

    pub fn signed(&self, namespace: &str, key: &str, magnitude: f64) -> f64 {
        (self.residue(namespace, key, "signed") * 2.0 - 1.0) * magnitude.abs()
    }
}

#[derive(Debug, Clone)]
pub struct ClassifyParams {
    pub seed: f64,
    pub bpm: f64,
    pub seq_length: usize,
    pub playlist_rows: usize,
    pub goava_active: bool,
    pub randomizer_active: bool,
    pub phase_lock_active: bool,
    pub live_dj_goava: bool,
    pub live_dj_random: bool,
    pub global_algo_fingerprint: Option<String>,
    pub step_algorithm_fingerprint: Option<String>,
    pub ot: bool,
}

impl Default for ClassifyParams {
    fn default() -> Self {
        Self {
            seed: 42.0,
            bpm: 120.0,
            seq_length: 16,
            playlist_rows: 32,
            goava_active: false,
            randomizer_active: false,
            phase_lock_active: false,
            live_dj_goava: false,
            live_dj_random: false,
            global_algo_fingerprint: None,
            step_algorithm_fingerprint: None,
            ot: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GameIdentity {
    pub seed: f64,
    pub title: String,
    pub genre: String,
    pub camera: String,
    pub topology: String,
    pub social: String,
    pub mood: String,
    pub online: bool,
    pub host_port: u16,
    pub model_sets_1d: Vec<String>,
    pub model_sets_2d: Vec<String>,
    pub model_sets_3d: Vec<String>,
    pub ui_palette: BTreeMap<String, String>,
    pub gameplay_hooks: Vec<String>,
    pub music_variation: String,
    pub composition_fingerprint: String,
    pub splash_bars: usize,
    pub objective: String,
    pub difficulty: String,
    pub level_type: String,
    pub sigil_count: usize,
    pub world_fingerprint: String,
    pub software_kind: String,
    pub texture_family: String,
    pub material_spec: JsonValue,
    pub sfx_bank: Vec<String>,
    pub asset_manifest: JsonValue,
    pub goava_active: bool,
    pub randomizer_active: bool,
    pub phase_lock_active: bool,
    pub input_schema: JsonValue,
    pub visual_view: CameraView,
    pub visual_signal_id: String,
    pub spatial_state: SpatialSnapshot,
}

impl GameIdentity {
    pub fn is_free_play(&self) -> bool {
        self.genre == "open_world" || self.genre == "sandbox" || self.topology == "open_world"
    }

    pub fn to_json(&self) -> JsonValue {
        let mut m = JsonValue::obj();
        m.insert("seed".into(), self.seed.into());
        m.insert("title".into(), self.title.clone().into());
        m.insert("genre".into(), self.genre.clone().into());
        m.insert("camera".into(), self.camera.clone().into());
        m.insert("topology".into(), self.topology.clone().into());
        m.insert("social".into(), self.social.clone().into());
        m.insert("mood".into(), self.mood.clone().into());
        m.insert("online".into(), self.online.into());
        m.insert("host_port".into(), (self.host_port as i64).into());
        m.insert("free_play".into(), self.is_free_play().into());
        m.insert("objective_brief".into(), objective_brief(&self.objective, self.is_free_play()).into());
        m.insert("model_sets_1d".into(), JsonValue::Array(self.model_sets_1d.iter().cloned().map(JsonValue::Str).collect()));
        m.insert("model_sets_2d".into(), JsonValue::Array(self.model_sets_2d.iter().cloned().map(JsonValue::Str).collect()));
        m.insert("model_sets_3d".into(), JsonValue::Array(self.model_sets_3d.iter().cloned().map(JsonValue::Str).collect()));
        let mut pal = JsonValue::obj();
        for (k, v) in &self.ui_palette {
            pal.insert(k.clone(), v.clone().into());
        }
        m.insert("ui_palette".into(), JsonValue::Object(pal));
        m.insert("gameplay_hooks".into(), JsonValue::Array(self.gameplay_hooks.iter().cloned().map(JsonValue::Str).collect()));
        m.insert("music_variation".into(), self.music_variation.clone().into());
        m.insert("composition_fingerprint".into(), self.composition_fingerprint.clone().into());
        m.insert("splash_bars".into(), self.splash_bars.into());
        m.insert("objective".into(), self.objective.clone().into());
        m.insert("difficulty".into(), self.difficulty.clone().into());
        m.insert("level_type".into(), self.level_type.clone().into());
        m.insert("sigil_count".into(), self.sigil_count.into());
        m.insert("world_fingerprint".into(), self.world_fingerprint.clone().into());
        m.insert("software_kind".into(), self.software_kind.clone().into());
        m.insert("texture_family".into(), self.texture_family.clone().into());
        m.insert("material_spec".into(), self.material_spec.clone());
        m.insert("sfx_bank".into(), JsonValue::Array(self.sfx_bank.iter().cloned().map(JsonValue::Str).collect()));
        m.insert("asset_manifest".into(), self.asset_manifest.clone());
        m.insert("goava_active".into(), self.goava_active.into());
        m.insert("randomizer_active".into(), self.randomizer_active.into());
        m.insert("phase_lock_active".into(), self.phase_lock_active.into());
        m.insert("input_schema".into(), self.input_schema.clone());
        m.insert("visual_view_state".into(), self.visual_view.to_json());
        m.insert("visual_signal_id".into(), self.visual_signal_id.clone().into());
        m.insert("spatial_fingerprint".into(), self.spatial_state.fingerprint.clone().into());
        JsonValue::Object(m)
    }
}

pub fn instrument_material_from_seed(seed: i64, fp: &str, mood: &str, texture_family: &str, ot: bool) -> JsonValue {
    let ent = meum_game_residue(seed, &format!("mat_ent|{}", fp), ot);
    let conson = meum_game_residue(seed, &format!("mat_con|{}", fp), ot);
    let metal = 0.15 + 0.75 * meum_game_residue(seed, &format!("mat_metal|{}", mood), ot);
    let rough = 0.2 + 0.7 * ent;
    let emissive = 0.05 + 0.55 * (1.0 - conson) * meum_game_residue(seed, &format!("mat_em|{}", fp), ot);
    let hue = meum_game_residue(seed, &format!("mat_hue|{}", fp), ot);
    let sat = 0.35 + 0.55 * meum_game_residue(seed, &format!("mat_sat|{}", mood), ot);

    let r5 = |v: f64| (v * 1e5).round() / 1e5;
    let mut m = JsonValue::obj();
    m.insert("texture_family".into(), texture_family.into());
    m.insert("entropy".into(), r5(ent).into());
    m.insert("consonance".into(), r5(conson).into());
    m.insert("metallic".into(), r5(metal).into());
    m.insert("roughness".into(), r5(rough).into());
    m.insert("emissive".into(), r5(emissive).into());
    m.insert("hue".into(), r5(hue).into());
    m.insert("saturation".into(), r5(sat).into());
    m.insert("normal_strength".into(), (((0.2 + 1.4 * ent) * 1e4).round() / 1e4).into());
    JsonValue::Object(m)
}

pub fn instrument_sfx_bank(seed: i64, n_instruments: usize, ot: bool) -> Vec<String> {
    let verbs = ["blip", "thud", "chime", "sweep", "click", "whoosh", "spark", "drone"];
    let n = n_instruments.clamp(4, 16);
    (0..n)
        .map(|i| {
            let v = verbs[((meum_game_residue(seed, &format!("sfx_v:{}", i), ot) * verbs.len() as f64) as usize) % verbs.len()];
            let id = (meum_game_residue(seed, &format!("sfx_id:{}", i), ot) * 999.0) as usize;
            format!("{}_{}_{:03}", v, i, id)
        })
        .collect()
}

pub fn build_asset_manifest(
    seed: i64,
    m1: &[String],
    m2: &[String],
    m3: &[String],
    texture_family: &str,
    material_spec: &JsonValue,
    sfx_bank: &[String],
    software_kind: &str,
    ot: bool,
) -> JsonValue {
    let prim_count = m2.len().clamp(3, 12);
    let prims: Vec<JsonValue> = (0..prim_count)
        .map(|i| {
            let idx = ((meum_game_residue(seed, &format!("prim:{}", i), ot) * MODEL_PRIMITIVES.len() as f64) as usize) % MODEL_PRIMITIVES.len();
            MODEL_PRIMITIVES[idx].into()
        })
        .collect();
    let palette: Vec<JsonValue> = (0..6)
        .map(|i| {
            let mut pm = JsonValue::obj();
            pm.insert("h".into(), (((meum_game_residue(seed, &format!("pal/h:{}", i), ot)) * 1e4).round() / 1e4).into());
            pm.insert("s".into(), (((0.35 + 0.55 * meum_game_residue(seed, &format!("pal/s:{}", i), ot)) * 1e4).round() / 1e4).into());
            pm.insert("v".into(), (((0.45 + 0.50 * meum_game_residue(seed, &format!("pal/v:{}", i), ot)) * 1e4).round() / 1e4).into());
            JsonValue::Object(pm)
        })
        .collect();
    let lod_levels = 2 + (3.0 * meum_game_residue(seed, "lod_n", ot)) as usize;
    let decor_n = 3 + (5.0 * meum_game_residue(seed, "decor_n", ot)) as usize;
    let decor: Vec<JsonValue> = (0..decor_n)
        .map(|i| {
            let v = ((meum_game_residue(seed, &format!("decor:{}", i), ot) * 1000.0) as usize) % 1000;
            format!("decor_{}", v).into()
        })
        .collect();

    let mut models = JsonValue::obj();
    models.insert("1d".into(), JsonValue::Array(m1.iter().cloned().map(JsonValue::Str).collect()));
    models.insert("2d".into(), JsonValue::Array(m2.iter().cloned().map(JsonValue::Str).collect()));
    models.insert("3d".into(), JsonValue::Array(m3.iter().cloned().map(JsonValue::Str).collect()));

    let mut m = JsonValue::obj();
    m.insert("software_kind".into(), software_kind.into());
    m.insert("input_schema".into(), software_input_schema(software_kind));
    m.insert("models".into(), JsonValue::Object(models));
    m.insert("primitives".into(), JsonValue::Array(prims));
    m.insert("texture_family".into(), texture_family.into());
    m.insert("material".into(), material_spec.clone());
    m.insert("sfx_bank".into(), JsonValue::Array(sfx_bank.iter().cloned().map(JsonValue::Str).collect()));
    m.insert("palette".into(), JsonValue::Array(palette));
    m.insert("lod_levels".into(), lod_levels.into());
    m.insert("decorations".into(), JsonValue::Array(decor));
    m.insert("generative".into(), true.into());
    JsonValue::Object(m)
}

/// Exact match for `videogame_engine.classify_from_composition`.
pub fn classify_from_composition(p: &ClassifyParams) -> GameIdentity {
    let s = safe_int_seed_vge(p.seed);
    let ot = p.ot;
    let ga = p.global_algo_fingerprint.as_deref().unwrap_or("0");
    let sa = p.step_algorithm_fingerprint.as_deref().unwrap_or("0");

    let fp_src = format!(
        "{}|bpm={}|L={}|R={}|G={}|RND={}|PL={}|DJG={}|DJR={}|GA={}|SA={}",
        s,
        format_python_f64(p.bpm),
        p.seq_length,
        p.playlist_rows,
        if p.goava_active { 1 } else { 0 },
        if p.randomizer_active { 1 } else { 0 },
        if p.phase_lock_active { 1 } else { 0 },
        if p.live_dj_goava { 1 } else { 0 },
        if p.live_dj_random { 1 } else { 0 },
        ga,
        sa,
    );
    let fingerprint = sha256_hex(fp_src.as_bytes())[..16].to_string();

    let g = GENRES[(mix_u64(s, "genre") as usize) % GENRES.len()];
    let cam_r = meum_game_residue(s, "camera_bias", ot);
    let cam = if cam_r < 0.25 {
        "first_person"
    } else if cam_r < 0.50 {
        "third_person"
    } else {
        CAMERAS[(mix_u64(s, "camera") as usize) % CAMERAS.len()]
    };

    let topo_r = meum_game_residue(s, "topology_bias", ot);
    let top = if topo_r < 0.55 {
        "open_world"
    } else if topo_r < 0.75 {
        "hub_spoke"
    } else {
        TOPOLOGIES[(mix_u64(s, "topology") as usize) % TOPOLOGIES.len()]
    };

    let soc = SOCIAL[(mix_u64(s, "social") as usize) % SOCIAL.len()];
    let mood = MOODS[(mix_u64(s, "mood") as usize) % MOODS.len()];
    let online = soc == "online_multiplayer";
    let host_port = (27015 + (mix_u64(s, "port") % 8000)) as u16;

    let n1 = (2 + (8.0 * meum_game_residue(s, "m1", ot) + 2.0 * PHI * meum_game_residue(s, "m1b", ot)) as usize).clamp(1, 12);
    let n2 = (3 + (10.0 * meum_game_residue(s, "m2", ot) + 3.0 * MEUM * meum_game_residue(s, "m2b", ot)) as usize).clamp(2, 16);
    let n3 = (1 + (7.0 * meum_game_residue(s, "m3", ot) + 2.0 * PHI * meum_game_residue(s, "m3b", ot)) as usize).clamp(1, 10);
    let models_1d: Vec<String> = (0..n1).map(|k| format!("filament_{}", k)).collect();
    let models_2d: Vec<String> = (0..n2).map(|k| format!("panel_{}", k)).collect();
    let models_3d: Vec<String> = (0..n3).map(|k| format!("polytope_{}", k)).collect();

    let mut palette = BTreeMap::new();
    palette.insert("bg".into(), format!("#{:06x}", mix_u64(s, "bg") & 0xFF_FFFF));
    palette.insert("accent".into(), format!("#{:06x}", mix_u64(s, "ac") & 0xFF_FFFF));
    palette.insert("danger".into(), format!("#{:06x}", mix_u64(s, "dg") & 0xFF_FFFF));
    palette.insert("text".into(), "#e8f0ff".into());

    let mut hooks = vec![
        format!("hook_score_meum_{}", mix_u64(s, "hk0") % 7),
        format!("hook_wave_{}", g),
        format!("hook_cam_{}", cam),
        format!("hook_top_{}", top),
    ];
    if let Some(gh) = genre_hook(g) {
        hooks.push(gh.into());
    }
    if p.goava_active { hooks.push("hook_goava_sine_portal".into()); }
    if p.randomizer_active { hooks.push("hook_randomizer_field".into()); }
    if p.phase_lock_active { hooks.push("hook_phase_lock_rings".into()); }
    if p.live_dj_goava { hooks.push("hook_live_dj_goava".into()); }
    if p.live_dj_random { hooks.push("hook_live_dj_parametric".into()); }

    let title = generate_title(s, g, cam, top, mood, &fingerprint);
    let music_var = if online { "longform_dj_remix" } else { "seed_loop_with_parametric_drift" };
    let splash_bars = p.seq_length.max(4);

    let world_fp0 = fingerprint.clone();
    let objective = OBJECTIVES[res_idx(s, &format!("objective|{}", world_fp0), OBJECTIVES.len(), ot)];
    let difficulty = DIFFICULTIES[res_idx(s, &format!("difficulty|{}", world_fp0), DIFFICULTIES.len(), ot)];
    let level_type = LEVEL_TYPES[res_idx(s, &format!("level|{}", world_fp0), LEVEL_TYPES.len(), ot)];
    let sigil_count = 5 + res_idx(s, &format!("sigils|{}", world_fp0), 8, ot);
    let world_fp = sha256_hex(
        format!(
            "{}|ob={}|df={}|lv={}|s={}",
            world_fp0, objective, difficulty, level_type, sigil_count
        )
        .as_bytes(),
    )[..16]
        .to_string();

    let sk_r = meum_game_residue(s, &format!("software_kind|{}", fingerprint), ot);
    let software_kind = if sk_r < 0.55 {
        "videogame"
    } else {
        let idx = 1 + (((sk_r - 0.55) / 0.45) * ((SOFTWARE_KINDS.len() - 1) as f64)) as usize;
        SOFTWARE_KINDS[idx.clamp(1, SOFTWARE_KINDS.len() - 1)]
    };

    let texture_family = TEXTURE_FAMILIES[res_idx(s, &format!("tex|{}", fingerprint), TEXTURE_FAMILIES.len(), ot)];
    let material_spec = instrument_material_from_seed(s, &fingerprint, mood, texture_family, ot);
    let view = fibonacci_view(0, 64, s as f64);
    let vsid = visual_signal_id(s as f64, &fingerprint, &view);
    let spatial_state = build_spatial_state(s as u64, &fingerprint, 3, 8, p.goava_active);
    let sfx_bank = instrument_sfx_bank(s, 8, ot);
    let asset_manifest = build_asset_manifest(
        s, &models_1d, &models_2d, &models_3d, texture_family, &material_spec, &sfx_bank, software_kind, ot,
    );

    GameIdentity {
        seed: p.seed,
        title,
        genre: g.into(),
        camera: cam.into(),
        topology: top.into(),
        social: soc.into(),
        mood: mood.into(),
        online,
        host_port,
        model_sets_1d: models_1d,
        model_sets_2d: models_2d,
        model_sets_3d: models_3d,
        ui_palette: palette,
        gameplay_hooks: hooks,
        music_variation: music_var.into(),
        composition_fingerprint: fingerprint,
        splash_bars,
        objective: objective.into(),
        difficulty: difficulty.into(),
        level_type: level_type.into(),
        sigil_count,
        world_fingerprint: world_fp,
        software_kind: software_kind.into(),
        texture_family: texture_family.into(),
        material_spec,
        sfx_bank,
        asset_manifest,
        goava_active: p.goava_active,
        randomizer_active: p.randomizer_active,
        phase_lock_active: p.phase_lock_active,
        input_schema: software_input_schema(software_kind),
        visual_view: view,
        visual_signal_id: vsid,
        spatial_state,
    }
}

/// Empirical coverage proof over the safe SOFTWARE_KINDS lattice.
pub fn prove_software_lattice(n_samples: usize) -> JsonValue {
    let n = n_samples.max(16);
    let mut hits: BTreeMap<String, usize> = SOFTWARE_KINDS.iter().map(|&k| (k.to_string(), 0)).collect();
    let mut radio_examples = Vec::new();
    for i in 0..n {
        let seed = if i < n / 2 {
            i as f64
        } else {
            ((i as f64) * PHI).rem_euclid(10000.0)
        };
        let ident = classify_from_composition(&ClassifyParams { seed, ..Default::default() });
        *hits.entry(ident.software_kind.clone()).or_insert(0) += 1;
        if ident.software_kind == "radio_toolkit" && radio_examples.len() < 5 {
            let mut ex = JsonValue::obj();
            ex.insert("seed".into(), seed.into());
            ex.insert("title".into(), ident.title.into());
            ex.insert("fp".into(), ident.composition_fingerprint.into());
            radio_examples.push(JsonValue::Object(ex));
        }
    }
    let reached = hits.values().filter(|&&v| v > 0).count();
    let mut cov = JsonValue::obj();
    for (k, v) in hits {
        cov.insert(k, v.into());
    }
    let mut out = JsonValue::obj();
    out.insert("samples".into(), n.into());
    out.insert("kinds_reached".into(), reached.into());
    out.insert("total_kinds".into(), SOFTWARE_KINDS.len().into());
    out.insert("all_kinds_reached".into(), (reached == SOFTWARE_KINDS.len()).into());
    out.insert("coverage".into(), JsonValue::Object(cov));
    out.insert("radio_toolkit_examples".into(), JsonValue::Array(radio_examples));
    out.insert("note".into(), "radio_toolkit packages are software-defined radio study tools (waterfall, Morse, logbook) — simulation only, no live RF TX.".into());
    JsonValue::Object(out)
}
