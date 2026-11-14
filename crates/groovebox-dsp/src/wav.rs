//! Standard 16-bit PCM RIFF/WAV encoder & Groovebox `GROV` provenance chunk writer/reader.

/// Encode mono `f32` samples into a standard 16-bit PCM WAV byte buffer,
/// optionally appending a `GROV` provenance subchunk with canonical metadata.
pub fn encode_wav_pcm16(samples: &[f32], sample_rate: u32, provenance: Option<&str>) -> Vec<u8> {
    let n = samples.len();
    let data_bytes = (n * 2) as u32;
    let prov_bytes = provenance.map(|p| p.as_bytes()).unwrap_or(&[]);
    let prov_pad = if prov_bytes.len() % 2 == 1 { 1 } else { 0 };
    let prov_chunk_total = if prov_bytes.is_empty() {
        0
    } else {
        8 + prov_bytes.len() + prov_pad
    };
    let riff_size = 36 + data_bytes + (prov_chunk_total as u32);

    let mut out = Vec::with_capacity(44 + n * 2 + prov_chunk_total);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");

    // fmt chunk
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes());  // AudioFormat = 1 (PCM)
    out.extend_from_slice(&1u16.to_le_bytes());  // Mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // ByteRate
    out.extend_from_slice(&2u16.to_le_bytes());  // BlockAlign
    out.extend_from_slice(&16u16.to_le_bytes()); // BitsPerSample

    // data chunk
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());
    for &s in samples {
        let clamped = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
        let v = (clamped * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }

    // Optional GROV provenance chunk
    if !prov_bytes.is_empty() {
        out.extend_from_slice(b"GROV");
        out.extend_from_slice(&(prov_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(prov_bytes);
        if prov_pad == 1 {
            out.push(0);
        }
    }
    out
}

/// Extract embedded `GROV` provenance string from a Groovebox WAV buffer.
pub fn extract_wav_provenance(wav: &[u8]) -> Option<String> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12usize;
    while pos + 8 <= wav.len() {
        let id = &wav[pos..pos + 4];
        let sz = u32::from_le_bytes([wav[pos + 4], wav[pos + 5], wav[pos + 6], wav[pos + 7]]) as usize;
        pos += 8;
        if pos + sz > wav.len() {
            break;
        }
        if id == b"GROV" {
            return String::from_utf8(wav[pos..pos + sz].to_vec()).ok();
        }
        pos += sz + (sz % 2);
    }
    None
}
