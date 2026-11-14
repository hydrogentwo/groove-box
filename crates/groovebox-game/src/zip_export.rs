//! Pure-Rust deterministic ZIP archive builder (PKZIP store method + IEEE CRC32)
//! for exporting self-contained Groovebox game / software packages.

use groovebox_dsp::encode_wav_pcm16;
use groovebox_math::crc32;
use groovebox_state::ProjectState;
use groovebox_visual::{encode_png_rgb, VideoSynthEngine};
use crate::identity::{objective_brief, prove_software_lattice, GameIdentity};
use crate::sandbox::MusicBed;

pub struct ZipWriter {
    entries: Vec<(String, Vec<u8>)>,
}

impl ZipWriter {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn add_file(&mut self, name: impl Into<String>, data: Vec<u8>) {
        self.entries.push((name.into(), data));
    }

    /// Finish writing a valid deterministic PKZIP archive (fixed timestamp 1980-01-01 for byte-exact determinism).
    pub fn finish(self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();

        for (name, data) in &self.entries {
            let offset = out.len() as u32;
            let name_bytes = name.as_bytes();
            let crc = crc32(data);
            let size = data.len() as u32;

            // Local file header (30 bytes + filename)
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0u16.to_le_bytes());  // flags
            out.extend_from_slice(&0u16.to_le_bytes());  // compression: 0 (store)
            out.extend_from_slice(&0u16.to_le_bytes());  // mod time (fixed for determinism)
            out.extend_from_slice(&0x0021u16.to_le_bytes()); // mod date (1980-01-01)
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(data);

            // Central directory entry (46 bytes + filename)
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes()); // version made by
            central.extend_from_slice(&20u16.to_le_bytes()); // version needed
            central.extend_from_slice(&0u16.to_le_bytes());  // flags
            central.extend_from_slice(&0u16.to_le_bytes());  // compression: store
            central.extend_from_slice(&0u16.to_le_bytes());  // mod time
            central.extend_from_slice(&0x0021u16.to_le_bytes()); // mod date
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes()); // extra len
            central.extend_from_slice(&0u16.to_le_bytes()); // comment len
            central.extend_from_slice(&0u16.to_le_bytes()); // disk start
            central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }

        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        let count = self.entries.len() as u16;
        out.extend_from_slice(&central);

        // End of central directory record (22 bytes)
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // disk number
        out.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        out
    }
}

/// Export a complete deterministic Groovebox game/software package as `.zip` bytes.
pub fn export_game_package_zip(identity: &GameIdentity, project: &ProjectState) -> Vec<u8> {
    let mut zip = ZipWriter::new();

    zip.add_file("identity.json", identity.to_json().to_pretty_string().into_bytes());
    zip.add_file("asset_manifest.json", identity.asset_manifest.to_pretty_string().into_bytes());
    zip.add_file("spatial_state.json", identity.spatial_state.to_json().to_pretty_string().into_bytes());
    zip.add_file("visual_view.json", identity.visual_view.to_json().to_pretty_string().into_bytes());
    zip.add_file("software_lattice_proof.json", prove_software_lattice(64).to_pretty_string().into_bytes());
    zip.add_file("project_composition.json", project.to_json().to_pretty_string().into_bytes());

    let guide = format!(
        "# {title}\n\n\
        - **Software Kind:** `{kind}`\n\
        - **Genre / Camera / Topology:** `{genre}` / `{camera}` / `{topology}`\n\
        - **Social / Mood / Difficulty:** `{social}` / `{mood}` / `{difficulty}`\n\
        - **Level Type / Objective:** `{level}` / `{objective}`\n\
        - **Composition Fingerprint:** `{cfp}`\n\
        - **World Fingerprint:** `{wfp}`\n\
        - **Visual Signal ID:** `{vsid}`\n\
        - **Spatial Fingerprint:** `{sfp}`\n\n\
        ## Purpose\n\n\
        {brief}\n\n\
        ## Open-World Sandbox Controls & Console\n\n\
        - `W A S D`: Planar movement relative to look yaw\n\
        - `Space` / `Ctrl`: Ascend / Descend\n\
        - Mouse drag: Continuous camera look\n\
        - `/sandbox` or `/region`: Inspect editable Loom region state\n\
        - `/build <kind> <label>`: Place persistent sandbox object\n\
        - `/remove`: Remove most recently placed object\n\
        - `/note <text>`: Attach persistent note to current region\n\
        - `/report`: Print deterministic world report\n",
        title = identity.title,
        kind = identity.software_kind,
        genre = identity.genre,
        camera = identity.camera,
        topology = identity.topology,
        social = identity.social,
        mood = identity.mood,
        difficulty = identity.difficulty,
        level = identity.level_type,
        objective = identity.objective,
        cfp = identity.composition_fingerprint,
        wfp = identity.world_fingerprint,
        vsid = identity.visual_signal_id,
        sfp = identity.spatial_state.fingerprint,
        brief = objective_brief(&identity.objective, identity.is_free_play()),
    );
    zip.add_file("HOW_TO_PLAY.md", guide.into_bytes());

    // Render deterministic preview PNG frame
    let mut vs = VideoSynthEngine::new(320, 180);
    let fb = vs.render_frame(project, 0, 30.0, 0.55, &[0.5; 16]);
    let png = encode_png_rgb(320, 180, &fb.rgb);
    zip.add_file("preview_scene.png", png);

    // Render deterministic 2-second MusicBed WAV
    let mut bed = MusicBed::new(identity.seed as i64, project.base_freq_hz);
    let samples = bed.render_wav_samples(22050, 2.0);
    let wav = encode_wav_pcm16(&samples, 22050, Some(&format!("world_fp={}", identity.world_fingerprint)));
    zip.add_file("music_bed.wav", wav);

    zip.finish()
}
