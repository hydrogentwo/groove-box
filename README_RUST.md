# Groovebox — Pure Rust Implementation (`v3.9.0`)

This Cargo workspace implements the complete deterministic Groovebox audiovisual & game system in **pure Rust (2021 edition, zero external crate dependencies)** alongside the original Python reference implementation.

## Workspace Architecture

| Crate | Purpose |
|---|---|
| `crates/groovebox-math` | `MEUM` / `PHI` / `EQR_FINITE_INFINITY` constants, 36- & 48-slot harmonic tables, Operator Theory (Book p.49 `isn`/`ics`, EQR tensor, OT master/game transforms), 6 Eski fractal sets & `compositional_xyz`, SHA-256, BLAKE2b, CRC32, Adler-32, radix-2 FFT/RFFT, canonical Python-compatible JSON parser/serializer, safe math/script evaluator, number theory & GOAVA stream |
| `crates/groovebox-state` | `CompositionToggleState`, `meum_effect_residue`, order-independent `CompositionEngine`, `CompositionMemoryLedger`, `ToggleUndoStack`, `DeterministicTriggerSculptor`, `GoavaVoice`, `MeumEffectChain`, `NonRedundantEuclidean`, `GlobalPlayAlgo`, `ProjectState`, and `audit_project` |
| `crates/groovebox-dsp` | 48-instrument catalog & N-independent harmonic ratios, `MeumModulatedOscillator` (sample-0 silence contract), `CommutativePairSpace` & `LiveDJEffects`, PKP follow envelope, `MusicFractallizer`, `EQRTensorEngine`, PCM16 WAV encoder with `GROV` provenance chunk, and multi-track `render_project` |
| `crates/groovebox-visual` | Fibonacci sphere views & max-min `select_views`, `composition_fingerprint`, `golden_composition_fingerprint`, `visual_signal_id`, 4-engine 5-channel `FractalSpatialEngine` (+ optional GOAVA adapter), pure-Rust RFC 2083 RGB8 PNG encoder, and 1D/2D/3D `VideoSynthEngine` rasterizer |
| `crates/groovebox-game` | `GameIdentity` & `classify_from_composition`, `GoavaSeedRandomizer`, procedural material/SFX/asset manifests, `prove_software_lattice`, BLAKE2b LOD `WorldChunkStreamer`, `OpenWorldSandbox` (Loom region persistence + slash-command console), and deterministic pure-Rust PKZIP game exporter |
| `crates/groovebox-cli` | Headless CLI for WAV/PNG/ZIP generation, game classification, lattice proofs, spatial snapshots, determinism audit, and Python parity checks |
| `crates/groovebox-server` | Multi-threaded HTTP/1.1 studio server (`0.0.0.0:8080`) embedding the offline-capable Groovebox Web Studio UI (Sequencer, Synth Rack, WebAudio WAV player + live Oscilloscope/FFT, 1D/2D/3D VideoSynth, Global Play Panel, Fractal Spatial 3D orbit, Game Engine & Open-World Sandbox, Auditor) |

## Quick Start

```bash
# Run full unit & Python-parity test suite (22 tests)
cargo test --workspace

# Build optimized release binaries
cargo build --release --workspace

# Launch interactive Groovebox Web Studio on http://0.0.0.0:8080
./target/release/groovebox-server --port 8080
```

## CLI Usage

```bash
./target/release/groovebox-cli parity
./target/release/groovebox-cli audit --seed 42
./target/release/groovebox-cli render-wav --seed 42 --bpm 120 --voices 8 --bars 2 --out mix.wav
./target/release/groovebox-cli render-frame --seed 42 --frame 0 --width 640 --height 360 --out frame.png
./target/release/groovebox-cli classify-game --seed 42 --bpm 120 --steps 16 --voices 8
./target/release/groovebox-cli export-game --seed 42 --out game_package.zip
./target/release/groovebox-cli spatial --seed 42 --depth 3 --roots 6
./target/release/groovebox-cli visual --seed 42 --count 16
./target/release/groovebox-cli prove-lattice --samples 64
```

## HTTP API Endpoints

- `GET /` — Embedded Groovebox Web Studio UI (`/style.css`, `/app.js`)
- `GET /api/health` — Engine health and version
- `GET / POST /api/state` — Canonical `ProjectState`, DJ pair descriptor, and render summary
- `POST /api/undo` — Undo last project state mutation
- `POST /api/render` — Render deterministic 48 kHz mono PCM16 WAV mixdown
- `GET /api/audio.wav` — Download/stream rendered WAV with `GROV` provenance chunk
- `POST /api/tracks/euclidean` · `POST /api/tracks/sculpt` — Euclidean & trigger sculptor pattern generators
- `POST /api/global-play/randomize` · `POST /api/global-play/eval` — Authoring-only randomizer & expression/script curve evaluator
- `GET /api/visual/slots` · `GET /api/visual/frame.png` — Canonical visual slots, Fibonacci views, and live PNG rasterizer
- `GET /api/spatial` — 4-engine 5-channel `FractalSpatialEngine` snapshot
- `GET /api/game/classify` · `GET /api/game/export.zip` — `GameIdentity`, software lattice proof, and deterministic `.zip` game package
- `GET /api/sandbox/state` · `POST /api/sandbox/{reset,input,command}` — Interactive Open-World Loom Sandbox
- `GET /api/audit` — Determinism contract & live Python-reference parity verification
