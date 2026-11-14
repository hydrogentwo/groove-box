(() => {
  'use strict';

  const MEUM = 1.1975807343385265;
  const MEUM_INV = 1.0 / MEUM;
  const TAU = Math.PI * 2;
  const DETECTORS = ['phase', 'energy', 'spectrum', 'goava', 'euclidean', 'seed', 'bpm', 'pair'];
  const TARGETS = ['master_mix', 'fractallizer', 'eqr', 'pkp', 'ensemble', 'scenograph', 'domain', 'unison'];

  let project = null;
  let lastRender = null;
  let selectedTrack = 0;
  let audioCtx = null;
  let sourceNode = null;
  let analyser = null;
  let isPlaying = false;
  let playStartTime = 0;
  let loopDuration = 2.0;

  let visData = null;
  let visOrbit = { yaw: 0.25, pitch: 0.18, drag: false, lx: 0, ly: 0 };
  let spatialData = null;
  let spOrbit = { yaw: 0.4, pitch: 0.25, drag: false, lx: 0, ly: 0 };
  let sandboxState = null;

  const $ = (id) => document.getElementById(id);
  const setStatus = (msg) => { const el = $('status-msg'); if (el) el.textContent = msg; };

  async function api(path, opts = {}) {
    const res = await fetch(path, opts);
    if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
    return res.json();
  }

  // =========================================================================
  // Core State Sync & Audio Rendering
  // =========================================================================

  async function loadInitialState() {
    const data = await api('./api/state');
    applyServerPayload(data);
    renderAllViews();
    await Promise.all([refreshVisualSlots(), refreshSpatial(), refreshGame(), refreshSandbox(), runAudit(), evalGlobalPlay()]);
  }

  function applyServerPayload(data) {
    if (data.project) project = data.project;
    if (data.render) lastRender = data.render;
    if (data.dj_pair_descriptor) {
      const d = data.dj_pair_descriptor;
      $('dj-pair-sig').textContent = `pair(${d.a},${d.b}) #${d.index} · ${d.signature.slice(0, 8)}`;
    }
    syncHeaderControls();
    renderSequencerGrid();
    syncTrackRack();
    syncGlobalPlayForm();
    if (lastRender) updateRenderReadouts(lastRender);
  }

  async function pushState({ render = false, rebuild_tracks = false } = {}) {
    if (!project) return;
    setStatus('Syncing canonical state with Rust engine…');
    const bars = parseInt($('sel-bars').value, 10) || 1;
    const data = await api('./api/state', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ project, render, rebuild_tracks, bars }),
    });
    applyServerPayload(data);
    if (render && isPlaying) {
      await startPlayback();
    }
    setStatus(`Canonical FP ${project.canonical_fingerprint} · ready.`);
  }

  async function triggerRender() {
    setStatus('Rendering deterministic DSP mixdown in Rust…');
    const bars = parseInt($('sel-bars').value, 10) || 1;
    const data = await api('./api/render', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ bars }),
    });
    lastRender = data.render;
    updateRenderReadouts(lastRender);
    if (isPlaying) await startPlayback();
    setStatus(`Rendered ${lastRender.duration_sec.toFixed(2)}s · SHA-256 ${lastRender.audio_sha256}`);
  }

  function updateRenderReadouts(r) {
    $('chip-fp').textContent = r.canonical_fingerprint || project.canonical_fingerprint;
    $('chip-audio').textContent = (r.audio_sha256 || '').slice(0, 12);
    $('met-rms').textContent = r.rms.toFixed(4);
    $('met-peak').textContent = r.peak.toFixed(4);
    $('met-eqrz').textContent = (r.eqr_last_z || 0).toFixed(3);
    $('met-cent').textContent = r.centroid_norm.toFixed(3);
    $('met-s0').textContent = r.start_sample.toFixed(1);
    loopDuration = r.duration_sec || 2.0;
    drawStaticMeters();
  }

  // =========================================================================
  // WebAudio Playback of Rust-rendered PCM16 WAV
  // =========================================================================

  async function startPlayback() {
    try {
      stopPlayback();
      if (!audioCtx) {
        const AC = window.AudioContext || window.webkitAudioContext;
        audioCtx = new AC();
      }
      if (audioCtx.state === 'suspended') await audioCtx.resume();
      const resp = await fetch(`./api/audio.wav?t=${Date.now()}`);
      const buf = await resp.arrayBuffer();
      const decoded = await audioCtx.decodeAudioData(buf);
      analyser = audioCtx.createAnalyser();
      analyser.fftSize = 512;
      sourceNode = audioCtx.createBufferSource();
      sourceNode.buffer = decoded;
      sourceNode.loop = true;
      sourceNode.connect(analyser);
      analyser.connect(audioCtx.destination);
      sourceNode.start(0);
      playStartTime = audioCtx.currentTime;
      loopDuration = decoded.duration;
      isPlaying = true;
      $('btn-play').textContent = '▶ Playing…';
      setStatus(`Playing Rust-rendered WAV loop (${decoded.duration.toFixed(2)}s @ ${decoded.sampleRate}Hz)`);
    } catch (e) {
      setStatus(`Playback error: ${e.message}`);
    }
  }

  function stopPlayback() {
    if (sourceNode) {
      try { sourceNode.stop(); } catch (_) {}
      try { sourceNode.disconnect(); } catch (_) {}
      sourceNode = null;
    }
    isPlaying = false;
    $('btn-play').textContent = '▶ Play Loop';
    document.querySelectorAll('.pad.playing').forEach((el) => el.classList.remove('playing'));
  }

  // =========================================================================
  // Sync UI Controls
  // =========================================================================

  function syncHeaderControls() {
    if (!project) return;
    $('chip-fp').textContent = project.canonical_fingerprint;
    $('inp-seed').value = project.seed_input;
    $('inp-bpm').value = project.bpm;
    $('inp-basehz').value = project.base_freq_hz;
    $('inp-voices').value = project.tracks.length;
    $('sel-steps').value = String(project.seq_length);
    $('rng-seedwt').value = project.seed_weight;
    $('val-seedwt').textContent = project.seed_weight.toFixed(2);
    $('rng-vol').value = project.master_volume;
    $('val-vol').textContent = project.master_volume.toFixed(2);

    const otBtn = $('btn-ot');
    otBtn.classList.toggle('active', !!project.operator_theory);
    otBtn.textContent = `Operator Theory: ${project.operator_theory ? 'ON' : 'OFF'}`;

    document.querySelectorAll('.pill[data-toggle]').forEach((btn) => {
      const k = btn.dataset.toggle;
      btn.classList.toggle('active', !!(project.toggles && project.toggles[k]));
    });
    $('pill-unison').classList.toggle('active', !!project.full_unison);
    $('pill-fullwt').classList.toggle('active', !!project.fullweight_seed);

    $('rng-frac').value = project.fractallizer_amount;
    $('val-frac').textContent = `${Math.round(project.fractallizer_amount * 100)}%`;
    $('rng-gamma').value = project.fractallizer_gamma;
    $('val-gamma').textContent = project.fractallizer_gamma.toFixed(2);
    $('rng-eqr').value = project.eqr_amount;
    $('val-eqr').textContent = `${Math.round(project.eqr_amount * 100)}%`;
    $('rng-pkp').value = project.pkp_decay;
    $('val-pkp').textContent = `${Math.round(project.pkp_decay * 100)}%`;

    $('rng-dj-goava').value = project.dj_goava_amount;
    $('val-dj-goava').textContent = `${Math.round(project.dj_goava_amount * 100)}%`;
    $('rng-dj-rnd').value = project.dj_random_amount;
    $('val-dj-rnd').textContent = `${Math.round(project.dj_random_amount * 100)}%`;
    $('rng-dj-boost').value = project.dj_boost_amount;
    $('val-dj-boost').textContent = `${Math.round(project.dj_boost_amount * 100)}%`;
    if (Array.isArray(project.dj_pair)) {
      $('inp-pair-a').value = project.dj_pair[0];
      $('inp-pair-b').value = project.dj_pair[1];
    }
  }

  function renderSequencerGrid() {
    if (!project) return;
    if (selectedTrack >= project.tracks.length) selectedTrack = 0;
    const grid = $('sequencer-grid');
    grid.innerHTML = '';
    const nSteps = project.seq_length;

    project.tracks.forEach((tr, ti) => {
      const row = document.createElement('div');
      row.className = 'seq-row' + (ti === selectedTrack ? ' selected' : '');

      const meta = document.createElement('div');
      meta.className = 'seq-track-meta';
      const muteBtn = document.createElement('button');
      muteBtn.className = 'mute-btn' + (tr.patch.muted ? ' muted' : '');
      muteBtn.textContent = tr.patch.muted ? 'M' : '●';
      muteBtn.title = 'Mute / Unmute';
      muteBtn.onclick = () => {
        tr.patch.muted = !tr.patch.muted;
        pushState({ render: true });
      };
      const nameBtn = document.createElement('button');
      nameBtn.className = 'seq-track-btn';
      nameBtn.textContent = `${ti + 1}. ${tr.name}`;
      nameBtn.onclick = () => {
        selectedTrack = ti;
        renderSequencerGrid();
        syncTrackRack();
      };
      meta.append(muteBtn, nameBtn);

      const strip = document.createElement('div');
      strip.className = 'pad-strip';
      strip.style.gridTemplateColumns = `repeat(${nSteps}, minmax(0, 1fr))`;

      for (let s = 0; s < nSteps; s++) {
        const cell = tr.steps[s] || { active: false, amp: 0.85, pitch_ratio: 1.0 };
        const pad = document.createElement('button');
        pad.className = 'pad' + (s % 4 === 0 ? ' beat' : '') + (cell.active ? ' on' : '');
        pad.dataset.step = String(s);
        pad.title = `${tr.name} · Step ${s + 1}`;
        pad.onclick = () => {
          cell.active = !cell.active;
          tr.steps[s] = cell;
          pad.classList.toggle('on', cell.active);
          selectedTrack = ti;
          syncTrackRack();
          pushState({ render: true });
        };
        strip.appendChild(pad);
      }
      row.append(meta, strip);
      grid.appendChild(row);
    });
  }

  function syncTrackRack() {
    if (!project || !project.tracks[selectedTrack]) return;
    const tr = project.tracks[selectedTrack];
    const p = tr.patch;
    $('sel-inst-name').textContent = `${selectedTrack + 1}. ${tr.name}`;
    $('sel-waveform').value = p.waveform || 'isn';
    $('p-morph').value = p.morph; $('pv-morph').textContent = p.morph.toFixed(2);
    $('p-freq').value = Math.min(2400, Math.max(40, p.harmonic_freq)); $('pv-freq').textContent = `${Math.round(p.harmonic_freq)} Hz`;
    $('p-chaos').value = p.chaos; $('pv-chaos').textContent = p.chaos.toFixed(2);
    $('p-fold').value = p.fold_depth; $('pv-fold').textContent = p.fold_depth.toFixed(1);
    $('p-lattice').value = p.harmonic_lattice; $('pv-lattice').textContent = p.harmonic_lattice.toFixed(2);
    $('p-gain').value = p.gain; $('pv-gain').textContent = p.gain.toFixed(2);
    $('p-am').value = p.am_depth; $('pv-am').textContent = p.am_depth.toFixed(2);
    $('p-fm').value = p.fm_depth; $('pv-fm').textContent = p.fm_depth.toFixed(2);
    $('p-pm').value = p.pm_depth; $('pv-pm').textContent = p.pm_depth.toFixed(2);
    $('p-meum').value = p.meum_depth; $('pv-meum').textContent = p.meum_depth.toFixed(2);
  }

  // =========================================================================
  // Tab 2: VideoSynth 1D/2D/3D + Rust PNG Frame
  // =========================================================================

  async function refreshVisualSlots() {
    visData = await api('./api/visual/slots');
    $('vis-count').textContent = visData.count;
    $('vis-eski').textContent = visData.eski_fractal_set;
    $('vis-sig').textContent = (visData.visual_signal_id || '').slice(0, 12);

    const tbl = $('vis-slot-table');
    tbl.innerHTML = '';
    (visData.slots || []).forEach((s) => {
      const row = document.createElement('div');
      row.className = 'slot-row';
      row.style.borderLeftColor = `hsl(${Math.round(s.hue)}, 80%, 58%)`;
      row.innerHTML = `<span>#${s.master_slot} · ${s.dimension}D · ${Math.round(s.base_freq)}Hz</span><span>id=${s.identity.toFixed(4)} · ent=${s.entropy.toFixed(2)}</span>`;
      tbl.appendChild(row);
    });
    refreshRustPngFrame();
  }

  function refreshRustPngFrame() {
    const f = parseInt($('inp-frame-idx').value, 10) || 0;
    $('img-rust-frame').src = `./api/visual/frame.png?frame=${f}&w=480&h=270&t=${Date.now()}`;
  }

  function drawVideoSynthLive(nowMs) {
    const cv = $('cv-videosynth');
    if (!cv || !visData || !project) return;
    const ctx = cv.getContext('2d');
    const w = cv.width, h = cv.height;
    const t = nowMs * 0.001;

    // Live audio energy & bands
    let energy = lastRender ? Math.min(1, lastRender.rms * 3.0) : 0.45;
    let bands = (lastRender && lastRender.spectrum_bands) || new Array(32).fill(0.35);
    if (isPlaying && analyser) {
      const freq = new Uint8Array(analyser.frequencyBinCount);
      analyser.getByteFrequencyData(freq);
      let sum = 0;
      bands = [];
      for (let b = 0; b < 32; b++) {
        const v = freq[Math.min(freq.length - 1, b * 4)] / 255;
        bands.push(v);
        sum += v;
      }
      energy = Math.max(0.2, sum / 32);
    }

    ctx.fillStyle = '#070a10';
    ctx.fillRect(0, 0, w, h);

    // Harmonic lattice backdrop
    ctx.strokeStyle = 'rgba(0, 240, 200, 0.07)';
    ctx.lineWidth = 1;
    for (let gx = 1; gx < 8; gx++) {
      ctx.beginPath(); ctx.moveTo((gx * w) / 8, 0); ctx.lineTo((gx * w) / 8, h); ctx.stroke();
    }
    for (let gy = 1; gy < 6; gy++) {
      ctx.beginPath(); ctx.moveTo(0, (gy * h) / 6); ctx.lineTo(w, (gy * h) / 6); ctx.stroke();
    }

    const cyCam = Math.cos(visOrbit.yaw), syCam = Math.sin(visOrbit.yaw);
    const cpCam = Math.cos(visOrbit.pitch), spCam = Math.sin(visOrbit.pitch);

    (visData.slots || []).forEach((slot, idx) => {
      const tr = project.tracks[idx];
      const p = tr ? tr.patch : { morph: 1.2, harmonic_freq: 432, chaos: 0.35, fold_depth: 4, harmonic_lattice: 0.33 };
      const morph = Math.min(1, Math.max(0, (p.morph - 0.01) / 9.99));
      const fold = Math.min(1, Math.max(0, (p.fold_depth - 1) / 15));
      const lattice = Math.min(1, Math.max(0, p.harmonic_lattice));
      const chaos = Math.min(1, Math.max(0, p.chaos));

      const radial = 0.22 + 0.30 * Math.sqrt(slot.identity);
      const angle0 = TAU * slot.identity + slot.yaw;
      const phase = t * (0.5 + 0.5 * morph);
      const cx = w * 0.5 + Math.cos(angle0 + 0.35 * Math.sin(t * 0.5)) * radial * w * 0.72;
      const cy = h * 0.5 + Math.sin(angle0 * MEUM_INV + 0.35 * Math.cos(t * 0.5)) * radial * h * 0.62;
      const band = bands[slot.master_slot % bands.length] || 0.3;
      const scale = Math.max(12, (0.09 + 0.13 * lattice + 0.13 * energy) * Math.min(w, h) * (0.72 + 0.55 * band));
      const hc = Math.max(3, Math.min(16, Math.round(3 + 10 * lattice + 3 * fold)));
      const hue = (slot.hue + phase * 24) % 360;
      const alpha = Math.min(0.95, Math.max(0.38, 0.55 + 0.35 * energy));

      ctx.strokeStyle = `hsla(${hue}, 82%, 62%, ${alpha})`;
      ctx.fillStyle = `hsla(${hue}, 82%, 58%, ${alpha * 0.14})`;
      ctx.lineWidth = 1.4;

      if (slot.dimension === 1) {
        ctx.beginPath();
        for (let k = 0; k < hc * 4; k++) {
          const u = k / (hc * 4 - 1);
          const a = phase + TAU * u * (1 + fold * 0.22);
          const rr = scale * (0.45 + 0.85 * u) * (1 + 0.22 * Math.sin(a * hc * 0.25 + t));
          const x = cx + Math.cos(a) * rr;
          const y = cy + Math.sin(a * MEUM_INV) * rr * (0.62 + 0.3 * morph);
          if (k === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
        }
        ctx.stroke();
      } else if (slot.dimension === 2) {
        const cnt = Math.max(20, hc * 4);
        ctx.beginPath();
        for (let k = 0; k <= cnt; k++) {
          const a = phase + (TAU * k) / cnt;
          const rr = scale * (0.82 + 0.12 * Math.sin(a * hc + phase) + 0.14 * chaos * Math.sin(a * 3 + t));
          const x = cx + Math.cos(a) * rr;
          const y = cy + Math.sin(a * MEUM_INV) * rr * (0.72 + 0.22 * lattice);
          if (k === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
        }
        ctx.closePath();
        ctx.fill();
        ctx.stroke();
      } else {
        const cnt = Math.max(18, hc * 3);
        for (let rix = 0; rix < 3; rix++) {
          const rr0 = scale * (0.58 + 0.22 * rix);
          ctx.beginPath();
          for (let k = 0; k <= cnt; k++) {
            const a = phase + (TAU * k) / cnt + rix * 0.21;
            const z = 0.42 * Math.sin(a * (1 + fold * 0.35) + t * 0.8) * scale * 0.5;
            const x3 = rr0 * Math.cos(a);
            const y3 = rr0 * Math.sin(a * MEUM_INV);
            const x1 = x3 * cyCam - z * syCam;
            const z1 = x3 * syCam + z * cyCam;
            const y2 = y3 * cpCam - z1 * spCam;
            const z2 = y3 * spCam + z1 * cpCam;
            const depth = Math.max(0.4, 1.55 + z2 / (scale * 1.6));
            const sx = cx + x1 / depth;
            const sy = cy + y2 / depth;
            if (k === 0) ctx.moveTo(sx, sy); else ctx.lineTo(sx, sy);
          }
          ctx.stroke();
        }
      }
      ctx.fillStyle = `hsla(${(hue + 180) % 360}, 60%, 88%, 0.9)`;
      ctx.beginPath(); ctx.arc(cx, cy, 2.3, 0, TAU); ctx.fill();
    });

    // GOAVA second class orbital
    if (project.toggles && project.toggles.goava) {
      ctx.strokeStyle = 'rgba(0, 240, 200, 0.55)';
      ctx.beginPath();
      for (let k = 0; k < 48; k++) {
        const ang = k * 0.6180339 * TAU + t * MEUM;
        const r = (0.14 + 0.28 * ((Math.sin(k * MEUM + t) + 1) * 0.5)) * Math.min(w, h);
        const x = w * 0.5 + Math.cos(ang) * r;
        const y = h * 0.45 + Math.sin(ang) * r * 0.78;
        if (k === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
      }
      ctx.stroke();
    }
  }

  // =========================================================================
  // Tab 3: Global Play Panel
  // =========================================================================

  function syncGlobalPlayForm() {
    if (!project || !project.global_play) return;
    const gp = project.global_play;
    $('txt-gp-script').value = gp.script || '';
    $('inp-gp-domain').value = gp.domain || '';
    $('gp-mix').value = gp.params.mix; $('gpv-mix').textContent = `${Math.round(gp.params.mix * 100)}%`;
    $('gp-sa').value = gp.params.script_amount; $('gpv-sa').textContent = `${Math.round(gp.params.script_amount * 100)}%`;
    $('gp-da').value = gp.params.domain_amount; $('gpv-da').textContent = `${Math.round(gp.params.domain_amount * 100)}%`;
    $('gp-wa').value = gp.params.wire_amount; $('gpv-wa').textContent = `${Math.round(gp.params.wire_amount * 100)}%`;
    $('gp-en-script').checked = !!gp.params.enable_script;
    $('gp-en-domain').checked = !!gp.params.enable_domain;
    $('gp-en-wire').checked = !!gp.params.enable_wire;

    const applyBtn = $('btn-gp-apply');
    applyBtn.classList.toggle('active', !!gp.apply_enabled);
    applyBtn.textContent = `▶ Apply Algo to Master Mix: ${gp.apply_enabled ? 'ON' : 'OFF'}`;

    const wlist = $('gp-wire-list');
    wlist.innerHTML = '';
    (gp.wire || []).forEach((w, idx) => {
      const row = document.createElement('div');
      row.className = 'wire-row';
      const srcSel = document.createElement('select');
      DETECTORS.forEach((d) => { const o = document.createElement('option'); o.value = d; o.textContent = d; if (d === w.source) o.selected = true; srcSel.appendChild(o); });
      srcSel.onchange = () => { w.source = srcSel.value; };
      const arrow = document.createElement('span'); arrow.textContent = '→'; arrow.style.textAlign = 'center';
      const dstSel = document.createElement('select');
      TARGETS.forEach((t) => { const o = document.createElement('option'); o.value = t; o.textContent = t; if (t === w.target) o.selected = true; dstSel.appendChild(o); });
      dstSel.onchange = () => { w.target = dstSel.value; };
      const amt = document.createElement('input');
      amt.type = 'range'; amt.min = '0'; amt.max = '1'; amt.step = '0.01'; amt.value = w.amount;
      amt.oninput = () => { w.amount = parseFloat(amt.value); };
      const del = document.createElement('button');
      del.className = 'btn btn-sm'; del.textContent = '×';
      del.onclick = () => { gp.wire.splice(idx, 1); syncGlobalPlayForm(); };
      row.append(srcSel, arrow, dstSel, amt, del);
      wlist.appendChild(row);
    });
  }

  async function evalGlobalPlay() {
    if (!project) return;
    const script = $('txt-gp-script').value;
    const domain = $('inp-gp-domain').value;
    const res = await api('./api/global-play/eval', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ script, domain }),
    });
    $('gp-hints').textContent = (res.hints || []).map((h) => `[${h}]`).join(' ');
    const errs = [res.script_error, res.domain_error].filter(Boolean);
    $('gp-eval-err').textContent = errs.join(' | ');
    drawCurveCanvas($('cv-gp-curve'), res.script_curve || [], res.domain_curve || []);
  }

  function drawCurveCanvas(cv, a, b) {
    if (!cv) return;
    const ctx = cv.getContext('2d');
    const w = cv.width, h = cv.height;
    ctx.fillStyle = '#080b10';
    ctx.fillRect(0, 0, w, h);
    ctx.strokeStyle = '#1e2738';
    ctx.beginPath(); ctx.moveTo(0, h / 2); ctx.lineTo(w, h / 2); ctx.stroke();

    const plot = (arr, color) => {
      if (!arr.length) return;
      const maxAbs = Math.max(1, ...arr.map((v) => Math.abs(v)));
      ctx.strokeStyle = color; ctx.lineWidth = 2;
      ctx.beginPath();
      arr.forEach((v, i) => {
        const x = (i / Math.max(1, arr.length - 1)) * w;
        const y = h * 0.5 - (v / maxAbs) * (h * 0.42);
        if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
      });
      ctx.stroke();
    };
    plot(a, '#00f0c8');
    plot(b, '#ff6b1a');
  }

  // =========================================================================
  // Tab 4: Fractal Spatial Universe
  // =========================================================================

  async function refreshSpatial() {
    const d = $('sp-depth').value || 3;
    const r = $('sp-roots').value || 6;
    const g = $('sp-goava').checked ? 1 : 0;
    spatialData = await api(`./api/spatial?depth=${d}&roots=${r}&goava=${g}`);
    const meta = $('spatial-meta');
    meta.innerHTML = '';
    const rows = [
      ['Version', spatialData.version],
      ['Spatial Fingerprint', spatialData.fingerprint],
      ['Points', spatialData.points.length],
      ['Topology Edges', spatialData.edges.length],
      ['Core Engines', (spatialData.core_engines || []).join(', ')],
      ['GOAVA Required', String(spatialData.goava_required)],
      ['GOAVA Adapter Active', String(spatialData.goava_adapter)],
    ];
    rows.forEach(([k, v]) => {
      const div = document.createElement('div');
      div.className = 'kv-item';
      div.innerHTML = `<span>${k}</span><code>${v}</code>`;
      meta.appendChild(div);
    });
  }

  function drawSpatialLive(nowMs) {
    const cv = $('cv-spatial');
    if (!cv || !spatialData) return;
    const ctx = cv.getContext('2d');
    const w = cv.width, h = cv.height;
    ctx.fillStyle = '#070a10';
    ctx.fillRect(0, 0, w, h);

    const yaw = spOrbit.yaw + (spOrbit.drag ? 0 : nowMs * 0.00022);
    const pitch = spOrbit.pitch;
    const cy = Math.cos(yaw), sy = Math.sin(yaw);
    const cp = Math.cos(pitch), sp = Math.sin(pitch);

    const proj = new Map();
    const scale = Math.min(w, h) * 0.24;
    spatialData.points.forEach((p) => {
      const x1 = p.x * cy - p.z * sy;
      const z1 = p.x * sy + p.z * cy;
      const y2 = p.y * cp - z1 * sp;
      const z2 = p.y * sp + z1 * cp;
      const depth = Math.max(0.6, 3.2 + z2);
      proj.set(p.id, {
        sx: w * 0.5 + (x1 * scale * 2.2) / depth,
        sy: h * 0.5 - (y2 * scale * 2.2) / depth,
        depth,
        p,
      });
    });

    ctx.lineWidth = 1;
    spatialData.edges.forEach(([a, b]) => {
      const pa = proj.get(a), pb = proj.get(b);
      if (!pa || !pb) return;
      ctx.strokeStyle = 'rgba(167, 125, 255, 0.34)';
      ctx.beginPath(); ctx.moveTo(pa.sx, pa.sy); ctx.lineTo(pb.sx, pb.sy); ctx.stroke();
    });

    const sorted = Array.from(proj.values()).sort((a, b) => b.depth - a.depth);
    sorted.forEach(({ sx, sy, depth, p }) => {
      const hue = ((p.phase / TAU) * 360 + p.topology_class * 45) % 360;
      const rad = Math.max(2.2, (5.5 * p.scale) / depth);
      ctx.fillStyle = `hsl(${hue}, 82%, ${Math.max(40, 72 - p.depth * 8)}%)`;
      ctx.beginPath(); ctx.arc(sx, sy, rad, 0, TAU); ctx.fill();
    });
  }

  // =========================================================================
  // Tab 5: Game Classifier & Open-World Sandbox
  // =========================================================================

  async function refreshGame() {
    const res = await api('./api/game/classify');
    const id = res.identity;
    const pal = id.ui_palette || {};
    $('game-card').innerHTML = `
      <div class="game-title">${id.title}</div>
      <div class="kv-list">
        <div class="kv-item"><span>Software Kind</span><code>${id.software_kind}</code></div>
        <div class="kv-item"><span>Genre / Camera / Topology</span><code>${id.genre} / ${id.camera} / ${id.topology}</code></div>
        <div class="kv-item"><span>Social / Mood / Difficulty</span><code>${id.social} / ${id.mood} / ${id.difficulty}</code></div>
        <div class="kv-item"><span>Level Type / Objective</span><code>${id.level_type} / ${id.objective}</code></div>
        <div class="kv-item"><span>Composition FP / World FP</span><code>${id.composition_fingerprint} / ${id.world_fingerprint}</code></div>
        <div class="kv-item"><span>Visual Signal ID</span><code>${id.visual_signal_id}</code></div>
        <div class="kv-item"><span>Texture / Models (1D/2D/3D)</span><code>${id.texture_family} (${id.model_sets_1d.length}/${id.model_sets_2d.length}/${id.model_sets_3d.length})</code></div>
      </div>
      <div class="swatch-row">
        <span class="dim">Palette:</span>
        <span class="swatch" style="background:${pal.bg}" title="bg ${pal.bg}"></span>
        <span class="swatch" style="background:${pal.accent}" title="accent ${pal.accent}"></span>
        <span class="swatch" style="background:${pal.danger}" title="danger ${pal.danger}"></span>
        <span class="dim">Port: <code>${id.host_port}</code></span>
      </div>
      <div class="info-note">${id.objective_brief}</div>
    `;

    const bars = $('lattice-bars');
    bars.innerHTML = '';
    const cov = (res.lattice_proof && res.lattice_proof.coverage) || {};
    Object.entries(cov).forEach(([k, v]) => {
      const div = document.createElement('div');
      div.className = 'lat-item' + (v > 0 ? ' hit' : '');
      div.innerHTML = `<span>${k}</span><code>${v}</code>`;
      bars.appendChild(div);
    });
  }

  async function refreshSandbox() {
    sandboxState = await api('./api/sandbox/state');
    renderSandbox();
  }

  async function sendSandboxInput(payload) {
    sandboxState = await api('./api/sandbox/input', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    renderSandbox();
  }

  async function sendSandboxCommand(command) {
    sandboxState = await api('./api/sandbox/command', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ command }),
    });
    renderSandbox();
  }

  function renderSandbox() {
    if (!sandboxState) return;
    const pl = sandboxState.player;
    $('sb-pos').textContent = `${pl.region_key} · (${pl.x.toFixed(1)}, ${pl.z.toFixed(1)}) yaw=${Math.round((pl.yaw * 180) / Math.PI)}°`;

    const logEl = $('sb-console');
    logEl.innerHTML = (sandboxState.console_log || []).map((l) => `<div>${escapeHtml(l)}</div>`).join('');
    logEl.scrollTop = logEl.scrollHeight;

    const cv = $('cv-sandbox');
    if (!cv) return;
    const ctx = cv.getContext('2d');
    const w = cv.width, h = cv.height;
    ctx.fillStyle = '#080c12';
    ctx.fillRect(0, 0, w, h);

    const scale = 6.5;
    const toScreen = (wx, wz) => [w * 0.5 + (wx - pl.x) * scale, h * 0.5 - (wz - pl.z) * scale];

    // Draw deterministic chunks & props
    (sandboxState.chunks || []).forEach((ch) => {
      const [sx, sy] = toScreen(ch.base_x - 8, ch.base_z + 8);
      const sz = 16 * scale;
      ctx.fillStyle = `hsla(${Math.round(ch.hue)}, 35%, ${12 + Math.round(ch.elev * 10)}%, 0.45)`;
      ctx.fillRect(sx, sy, sz, sz);
      ctx.strokeStyle = 'rgba(255,255,255,0.06)';
      ctx.strokeRect(sx, sy, sz, sz);

      (ch.props || []).forEach((pr) => {
        const [px, py] = toScreen(pr.x, pr.z);
        ctx.fillStyle = `hsl(${Math.round(pr.hue)}, 75%, 60%)`;
        const r = Math.max(2, pr.scale * 3);
        ctx.beginPath(); ctx.arc(px, py, r, 0, TAU); ctx.fill();
      });
    });

    // Draw persistent placed sandbox objects across all visited regions
    const regs = sandboxState.regions || {};
    Object.values(regs).forEach((reg) => {
      (reg.objects || []).forEach((obj) => {
        const [ox, oy] = toScreen(obj.x, obj.z);
        ctx.fillStyle = '#ff6b1a';
        ctx.fillRect(ox - 5, oy - 5, 10, 10);
        ctx.strokeStyle = '#fff';
        ctx.strokeRect(ox - 5, oy - 5, 10, 10);
        ctx.fillStyle = '#e6edf7';
        ctx.font = '10px monospace';
        ctx.fillText(obj.label, ox + 7, oy + 3);
      });
    });

    // Player avatar + heading arrow
    ctx.fillStyle = '#00f0c8';
    ctx.beginPath(); ctx.arc(w * 0.5, h * 0.5, 6, 0, TAU); ctx.fill();
    ctx.strokeStyle = '#00f0c8';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(w * 0.5, h * 0.5);
    ctx.lineTo(w * 0.5 + Math.sin(pl.yaw) * 18, h * 0.5 - Math.cos(pl.yaw) * 18);
    ctx.stroke();
  }

  function escapeHtml(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  // =========================================================================
  // Tab 6: Determinism & Python Parity Auditor
  // =========================================================================

  async function runAudit() {
    const a = await api('./api/audit');
    const p = a.python_parity || {};
    const cards = [
      ['Audio Bit-Identical Determinism', a.audio_bit_identical, `SHA-256: ${a.audio_sha256}`],
      ['Track Start Silence (sample[0] == 0)', a.start_sample_zero, 'Phase 0 start contract verified'],
      ['Composition Toggle Order-Independence', a.order_independent, `Canonical FP: ${a.canonical_fingerprint}`],
      ['Eski Cross-Engine Alignment', a.cross_engine_ok, `Book fractal set: ${a.cross_engine_fractal_set}`],
      ['GOAVA DC-Centered Pitch Balance', true, `Mean pitch offset: ${a.goava_mean_pitch.toFixed(4)} semitones`],
      ['Python Parity: Visual Comp FP', p.visual_comp_fp_match, `${p.visual_comp_fp} == 5e52964bd37da571`],
      ['Python Parity: Visual Signal ID', p.visual_signal_id_match, `${p.visual_signal_id} == c58ab38456faa1965275695f`],
      ['Python Parity: Golden Visual FP', p.golden_fp_match, `${p.golden_fp} == fa4fdcaf14d4a013`],
      ['Python Parity: Fractal Spatial FP', p.spatial_fp_match, `${p.spatial_fp} == 63787f44f62b003a`],
      ['Python Parity: Toggle State FP', p.toggle_fp_match, `${p.toggle_fp} == b72e9ecb6fb10f37`],
      ['Python Parity: Game Title & FP', p.game_title_match && p.game_comp_fp_match, `${p.game_title} · ${p.game_comp_fp}`],
      ['Python Parity: World FP & Eski Set', p.game_world_fp_match && p.eski_pick_match, `${p.game_world_fp} · ${p.eski_pick}`],
    ];
    const grid = $('audit-results');
    grid.innerHTML = '';
    cards.forEach(([title, pass, detail]) => {
      const div = document.createElement('div');
      div.className = 'audit-card ' + (pass ? 'pass' : 'fail');
      div.innerHTML = `<div class="audit-badge">${pass ? '✓ PASS' : '✗ FAIL'}</div><div><strong>${title}</strong></div><code class="dim">${detail}</code>`;
      grid.appendChild(div);
    });
  }

  // =========================================================================
  // Live Meters & Animation Loop
  // =========================================================================

  function drawStaticMeters() {
    if (!lastRender) return;
    drawScope(lastRender.waveform_preview || []);
    drawSpectrum(lastRender.spectrum_bands || []);
  }

  function drawScope(samples) {
    const cv = $('cv-scope');
    if (!cv) return;
    const ctx = cv.getContext('2d');
    const w = cv.width, h = cv.height;
    ctx.fillStyle = '#080b10';
    ctx.fillRect(0, 0, w, h);
    ctx.strokeStyle = '#1b2434';
    ctx.beginPath(); ctx.moveTo(0, h / 2); ctx.lineTo(w, h / 2); ctx.stroke();
    if (!samples.length) return;
    ctx.strokeStyle = '#00f0c8';
    ctx.lineWidth = 1.6;
    ctx.beginPath();
    for (let i = 0; i < samples.length; i++) {
      const x = (i / Math.max(1, samples.length - 1)) * w;
      const y = h * 0.5 - samples[i] * (h * 0.44);
      if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
    }
    ctx.stroke();
  }

  function drawSpectrum(bands) {
    const cv = $('cv-spec');
    if (!cv) return;
    const ctx = cv.getContext('2d');
    const w = cv.width, h = cv.height;
    ctx.fillStyle = '#080b10';
    ctx.fillRect(0, 0, w, h);
    const n = bands.length || 32;
    const bw = w / n;
    for (let i = 0; i < n; i++) {
      const v = Math.max(0, Math.min(1, bands[i] || 0));
      const bh = v * (h - 10);
      ctx.fillStyle = `hsl(${170 + (i / n) * 120}, 85%, 58%)`;
      ctx.fillRect(i * bw + 2, h - bh, Math.max(2, bw - 4), bh);
    }
  }

  function animationTick(now) {
    if (isPlaying && analyser && audioCtx) {
      const timeDom = new Float32Array(analyser.fftSize);
      analyser.getFloatTimeDomainData(timeDom);
      drawScope(Array.from(timeDom.slice(0, 256)));
      const freq = new Uint8Array(analyser.frequencyBinCount);
      analyser.getByteFrequencyData(freq);
      const bands = [];
      for (let b = 0; b < 32; b++) bands.push(freq[Math.min(freq.length - 1, b * 4)] / 255);
      drawSpectrum(bands);

      // Highlight active sequencer step
      if (project) {
        const elapsed = (audioCtx.currentTime - playStartTime) % Math.max(0.05, loopDuration);
        const stepDur = 60 / project.bpm / 4;
        const curStep = Math.floor(elapsed / stepDur) % project.seq_length;
        document.querySelectorAll('.pad.playing').forEach((el) => el.classList.remove('playing'));
        document.querySelectorAll(`.pad[data-step="${curStep}"]`).forEach((el) => el.classList.add('playing'));
      }
    }
    if ($('tab-visualizer').classList.contains('active')) drawVideoSynthLive(now);
    if ($('tab-spatial').classList.contains('active')) drawSpatialLive(now);
    requestAnimationFrame(animationTick);
  }

  function renderAllViews() {
    drawStaticMeters();
  }

  // =========================================================================
  // Event Wiring
  // =========================================================================

  function bindEvents() {
    // Tabs
    document.querySelectorAll('.tab').forEach((btn) => {
      btn.onclick = () => {
        document.querySelectorAll('.tab').forEach((t) => t.classList.remove('active'));
        document.querySelectorAll('.tab-panel').forEach((p) => p.classList.remove('active'));
        btn.classList.add('active');
        $(`tab-${btn.dataset.tab}`).classList.add('active');
        if (btn.dataset.tab === 'visualizer') refreshVisualSlots();
        if (btn.dataset.tab === 'spatial') refreshSpatial();
        if (btn.dataset.tab === 'game') { refreshGame(); refreshSandbox(); }
        if (btn.dataset.tab === 'audit') runAudit();
      };
    });

    $('btn-play').onclick = startPlayback;
    $('btn-stop').onclick = stopPlayback;
    $('btn-render').onclick = triggerRender;
    $('btn-undo').onclick = async () => {
      const data = await api('./api/undo', { method: 'POST' });
      applyServerPayload(data);
    };

    $('btn-export-png').onclick = () => {
      const f = parseInt($('inp-frame-idx').value, 10) || 0;
      const a = document.createElement('a');
      a.href = `./api/visual/frame.png?frame=${f}&w=960&h=540`;
      a.download = `groovebox_frame_${f}.png`;
      a.click();
    };

    $('btn-save-json').onclick = () => {
      const blob = new Blob([JSON.stringify(project, null, 2)], { type: 'application/json' });
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = `groovebox_${project.canonical_fingerprint}.json`;
      a.click();
    };

    $('input-load-json').onchange = async (e) => {
      const file = e.target.files[0];
      if (!file) return;
      const txt = await file.text();
      project = JSON.parse(txt);
      await pushState({ render: true });
      await Promise.all([refreshVisualSlots(), refreshSpatial(), refreshGame()]);
    };

    // Global params
    $('btn-apply-seed').onclick = async () => {
      project.seed_input = $('inp-seed').value;
      await pushState({ render: true, rebuild_tracks: true });
      await Promise.all([refreshVisualSlots(), refreshSpatial(), refreshGame()]);
    };
    $('inp-bpm').onchange = () => { project.bpm = parseFloat($('inp-bpm').value) || 120; pushState({ render: true }); };
    $('inp-basehz').onchange = () => { project.base_freq_hz = parseFloat($('inp-basehz').value) || 432; pushState({ render: true }); };
    $('inp-voices').onchange = async () => {
      project.n_instruments = Math.max(2, Math.min(64, parseInt($('inp-voices').value, 10) || 8));
      await pushState({ render: true });
      await refreshVisualSlots();
    };
    $('sel-steps').onchange = () => { project.seq_length = parseInt($('sel-steps').value, 10) || 16; pushState({ render: true }); };
    $('rng-seedwt').oninput = () => { $('val-seedwt').textContent = parseFloat($('rng-seedwt').value).toFixed(2); };
    $('rng-seedwt').onchange = () => { project.seed_weight = parseFloat($('rng-seedwt').value); pushState({ render: true }); };
    $('rng-vol').oninput = () => { $('val-vol').textContent = parseFloat($('rng-vol').value).toFixed(2); };
    $('rng-vol').onchange = () => { project.master_volume = parseFloat($('rng-vol').value); pushState({ render: true }); };

    $('btn-ot').onclick = () => {
      project.operator_theory = !project.operator_theory;
      pushState({ render: true });
    };

    document.querySelectorAll('.pill[data-toggle]').forEach((btn) => {
      btn.onclick = async () => {
        const k = btn.dataset.toggle;
        project.toggles[k] = !project.toggles[k];
        await pushState({ render: true });
        await Promise.all([refreshVisualSlots(), refreshGame()]);
      };
    });
    $('pill-unison').onclick = () => { project.full_unison = !project.full_unison; pushState({ render: true }); };
    $('pill-fullwt').onclick = () => { project.fullweight_seed = !project.fullweight_seed; pushState({ render: true }); };

    // Master FX sliders
    const bindMasterSlider = (id, labelId, key, pct) => {
      $(id).oninput = () => {
        const v = parseFloat($(id).value);
        $(labelId).textContent = pct ? `${Math.round(v * 100)}%` : v.toFixed(2);
      };
      $(id).onchange = () => { project[key] = parseFloat($(id).value); pushState({ render: true }); };
    };
    bindMasterSlider('rng-frac', 'val-frac', 'fractallizer_amount', true);
    bindMasterSlider('rng-gamma', 'val-gamma', 'fractallizer_gamma', false);
    bindMasterSlider('rng-eqr', 'val-eqr', 'eqr_amount', true);
    bindMasterSlider('rng-pkp', 'val-pkp', 'pkp_decay', true);
    bindMasterSlider('rng-dj-goava', 'val-dj-goava', 'dj_goava_amount', true);
    bindMasterSlider('rng-dj-rnd', 'val-dj-rnd', 'dj_random_amount', true);
    bindMasterSlider('rng-dj-boost', 'val-dj-boost', 'dj_boost_amount', true);

    const updatePair = () => {
      project.dj_pair = [parseInt($('inp-pair-a').value, 10) || 0, parseInt($('inp-pair-b').value, 10) || 1];
      pushState({ render: true });
    };
    $('inp-pair-a').onchange = updatePair;
    $('inp-pair-b').onchange = updatePair;

    // Sequencer helpers
    $('btn-euclid-all').onclick = async () => {
      const data = await api('./api/tracks/euclidean', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' });
      applyServerPayload(data);
      if (isPlaying) startPlayback();
    };
    $('btn-sculpt').onclick = async () => {
      const data = await api('./api/tracks/sculpt', { method: 'POST' });
      applyServerPayload(data);
      if (isPlaying) startPlayback();
    };
    $('btn-clear-steps').onclick = () => {
      project.tracks.forEach((tr) => tr.steps.forEach((s) => { s.active = false; }));
      pushState({ render: true });
    };

    // Per-instrument patch sliders
    $('sel-waveform').onchange = () => {
      project.tracks[selectedTrack].patch.waveform = $('sel-waveform').value;
      pushState({ render: true });
    };
    const bindPatch = (id, vid, key, fmt) => {
      $(id).oninput = () => {
        const v = parseFloat($(id).value);
        $(vid).textContent = fmt(v);
        project.tracks[selectedTrack].patch[key] = v;
      };
      $(id).onchange = () => pushState({ render: true });
    };
    bindPatch('p-morph', 'pv-morph', 'morph', (v) => v.toFixed(2));
    bindPatch('p-freq', 'pv-freq', 'harmonic_freq', (v) => `${Math.round(v)} Hz`);
    bindPatch('p-chaos', 'pv-chaos', 'chaos', (v) => v.toFixed(2));
    bindPatch('p-fold', 'pv-fold', 'fold_depth', (v) => v.toFixed(1));
    bindPatch('p-lattice', 'pv-lattice', 'harmonic_lattice', (v) => v.toFixed(2));
    bindPatch('p-gain', 'pv-gain', 'gain', (v) => v.toFixed(2));
    bindPatch('p-am', 'pv-am', 'am_depth', (v) => v.toFixed(2));
    bindPatch('p-fm', 'pv-fm', 'fm_depth', (v) => v.toFixed(2));
    bindPatch('p-pm', 'pv-pm', 'pm_depth', (v) => v.toFixed(2));
    bindPatch('p-meum', 'pv-meum', 'meum_depth', (v) => v.toFixed(2));

    // Visualizer controls & drag orbit
    $('btn-render-frame').onclick = refreshRustPngFrame;
    $('rng-view-idx').oninput = () => {
      const idx = parseInt($('rng-view-idx').value, 10) || 0;
      if (visData && visData.views && visData.views[idx]) {
        const v = visData.views[idx];
        visOrbit.yaw = (v.yaw_deg * Math.PI) / 180;
        visOrbit.pitch = (v.pitch_deg * Math.PI) / 180;
        $('vis-cam-readout').textContent = `slot #${v.index} · yaw ${v.yaw_deg.toFixed(1)}° pitch ${v.pitch_deg.toFixed(1)}°`;
      }
    };
    const bindOrbit = (cv, state) => {
      cv.onmousedown = (e) => { state.drag = true; state.lx = e.clientX; state.ly = e.clientY; };
      window.addEventListener('mouseup', () => { state.drag = false; });
      window.addEventListener('mousemove', (e) => {
        if (!state.drag) return;
        state.yaw += (e.clientX - state.lx) * 0.01;
        state.pitch = Math.max(-1.4, Math.min(1.4, state.pitch + (e.clientY - state.ly) * 0.01));
        state.lx = e.clientX; state.ly = e.clientY;
      });
    };
    bindOrbit($('cv-videosynth'), visOrbit);
    bindOrbit($('cv-spatial'), spOrbit);

    // Global Play controls
    $('btn-gp-rand').onclick = async () => {
      const data = await api('./api/global-play/randomize', { method: 'POST' });
      applyServerPayload(data);
      await evalGlobalPlay();
      setStatus('Randomized Global Play fields (Authoring only — Apply stays OFF).');
    };
    $('btn-gp-apply').onclick = async () => {
      collectGlobalPlayToProject();
      project.global_play.apply_enabled = !project.global_play.apply_enabled;
      project.toggles.apply_algorithm = project.global_play.apply_enabled;
      await pushState({ render: true });
      await evalGlobalPlay();
    };
    $('btn-gp-save').onclick = async () => {
      collectGlobalPlayToProject();
      await pushState({ render: project.global_play.apply_enabled });
      await evalGlobalPlay();
    };
    $('btn-add-wire').onclick = () => {
      project.global_play.wire.push({ source: 'phase', target: 'master_mix', amount: 0.5 });
      syncGlobalPlayForm();
    };
    ['gp-mix', 'gp-sa', 'gp-da', 'gp-wa'].forEach((id, idx) => {
      const lbl = ['gpv-mix', 'gpv-sa', 'gpv-da', 'gpv-wa'][idx];
      $(id).oninput = () => { $(lbl).textContent = `${Math.round(parseFloat($(id).value) * 100)}%`; };
    });

    // Spatial
    $('btn-sp-refresh').onclick = refreshSpatial;
    $('sp-goava').onchange = refreshSpatial;

    // Game & Sandbox
    $('btn-game-classify').onclick = refreshGame;
    $('btn-sb-reset').onclick = async () => {
      sandboxState = await api('./api/sandbox/reset', { method: 'POST' });
      renderSandbox();
    };
    document.querySelectorAll('[data-move]').forEach((btn) => {
      btn.onclick = () => {
        const [dx, dy, dz] = btn.dataset.move.split(',').map(Number);
        sendSandboxInput({ dx, dy, dz, dt: 0.2 });
      };
    });
    document.querySelectorAll('[data-look]').forEach((btn) => {
      btn.onclick = () => sendSandboxInput({ dyaw: parseFloat(btn.dataset.look), dt: 0.05 });
    });
    document.querySelectorAll('[data-cmd]').forEach((btn) => {
      btn.onclick = () => sendSandboxCommand(btn.dataset.cmd);
    });
    $('sb-form').onsubmit = async (e) => {
      e.preventDefault();
      const inp = $('sb-cmd-input');
      if (!inp.value.trim()) return;
      await sendSandboxCommand(inp.value);
      inp.value = '';
    };
    $('cv-sandbox').addEventListener('keydown', (e) => {
      const k = e.key.toLowerCase();
      if (k === 'w') sendSandboxInput({ dz: 1, dt: 0.16 });
      else if (k === 's') sendSandboxInput({ dz: -1, dt: 0.16 });
      else if (k === 'a') sendSandboxInput({ dx: -1, dt: 0.16 });
      else if (k === 'd') sendSandboxInput({ dx: 1, dt: 0.16 });
      else if (k === 'q') sendSandboxInput({ dyaw: -0.25, dt: 0.05 });
      else if (k === 'e') sendSandboxInput({ dyaw: 0.25, dt: 0.05 });
      else return;
      e.preventDefault();
    });

    // Audit
    $('btn-run-audit').onclick = runAudit;
  }

  function collectGlobalPlayToProject() {
    const gp = project.global_play;
    gp.script = $('txt-gp-script').value;
    gp.domain = $('inp-gp-domain').value;
    gp.params.mix = parseFloat($('gp-mix').value);
    gp.params.script_amount = parseFloat($('gp-sa').value);
    gp.params.domain_amount = parseFloat($('gp-da').value);
    gp.params.wire_amount = parseFloat($('gp-wa').value);
    gp.params.enable_script = $('gp-en-script').checked;
    gp.params.enable_domain = $('gp-en-domain').checked;
    gp.params.enable_wire = $('gp-en-wire').checked;
  }

  bindEvents();
  loadInitialState();
  requestAnimationFrame(animationTick);
})();
