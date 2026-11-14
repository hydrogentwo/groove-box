//! Pure-Rust FIPS 180-4 SHA-256, RFC 7693 BLAKE2b, IEEE CRC32, Adler-32,
//! and Groovebox deterministic seed/identity hashing utilities.

use std::fmt::Write as _;

// =============================================================================
// SHA-256 (FIPS 180-4)
// =============================================================================

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const SHA256_H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = SHA256_H0;
    let bit_len = (data.len() as u64).wrapping_mul(8);

    let mut padded = Vec::with_capacity(((data.len() + 9 + 63) / 64) * 64);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut w = [0u32; 64];
    for chunk in padded.chunks_exact(64) {
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

pub fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        let _ = write!(&mut s, "{:02x}", b);
    }
    s
}

pub fn sha256_hex(data: &[u8]) -> String {
    bytes_to_hex(&sha256(data))
}

pub fn sha256_u64_be(data: &[u8]) -> u64 {
    let d = sha256(data);
    u64::from_be_bytes([d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]])
}

// =============================================================================
// BLAKE2b (RFC 7693) — unkeyed, arbitrary digest_size in 1..=64
// =============================================================================

const BLAKE2B_IV: [u64; 8] = [
    0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
    0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
];

const BLAKE2B_SIGMA: [[usize; 16]; 12] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
];

#[inline(always)]
fn blake2b_g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
}

fn blake2b_compress(h: &mut [u64; 8], block: &[u8; 128], t: u128, last: bool) {
    let mut m = [0u64; 16];
    for i in 0..16 {
        m[i] = u64::from_le_bytes([
            block[i * 8],
            block[i * 8 + 1],
            block[i * 8 + 2],
            block[i * 8 + 3],
            block[i * 8 + 4],
            block[i * 8 + 5],
            block[i * 8 + 6],
            block[i * 8 + 7],
        ]);
    }
    let mut v = [0u64; 16];
    v[..8].copy_from_slice(h);
    v[8..].copy_from_slice(&BLAKE2B_IV);
    v[12] ^= t as u64;
    v[13] ^= (t >> 64) as u64;
    if last {
        v[14] = !v[14];
    }

    for r in 0..12 {
        let s = &BLAKE2B_SIGMA[r];
        blake2b_g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
        blake2b_g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
        blake2b_g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
        blake2b_g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
        blake2b_g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
        blake2b_g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
        blake2b_g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
        blake2b_g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
    }

    for i in 0..8 {
        h[i] ^= v[i] ^ v[i + 8];
    }
}

pub fn blake2b(data: &[u8], digest_size: usize) -> Vec<u8> {
    let out_len = digest_size.clamp(1, 64);
    let mut h = BLAKE2B_IV;
    h[0] ^= 0x0101_0000u64 ^ (out_len as u64);

    let mut pos = 0usize;
    let mut t: u128 = 0;
    if data.is_empty() {
        let block = [0u8; 128];
        blake2b_compress(&mut h, &block, 0, true);
    } else {
        while data.len() - pos > 128 {
            let mut block = [0u8; 128];
            block.copy_from_slice(&data[pos..pos + 128]);
            pos += 128;
            t += 128;
            blake2b_compress(&mut h, &block, t, false);
        }
        let rem = data.len() - pos;
        let mut block = [0u8; 128];
        block[..rem].copy_from_slice(&data[pos..]);
        t += rem as u128;
        blake2b_compress(&mut h, &block, t, true);
    }

    let mut full = [0u8; 64];
    for (i, word) in h.iter().enumerate() {
        full[i * 8..(i + 1) * 8].copy_from_slice(&word.to_le_bytes());
    }
    full[..out_len].to_vec()
}

pub fn blake2b_8(data: &[u8]) -> [u8; 8] {
    let v = blake2b(data, 8);
    let mut out = [0u8; 8];
    out.copy_from_slice(&v);
    out
}

pub fn blake2b_16(data: &[u8]) -> [u8; 16] {
    let v = blake2b(data, 16);
    let mut out = [0u8; 16];
    out.copy_from_slice(&v);
    out
}

pub fn blake2b_hex(data: &[u8], digest_size: usize) -> String {
    bytes_to_hex(&blake2b(data, digest_size))
}

// =============================================================================
// CRC32 (IEEE 802.3 — PNG & ZIP) & Adler-32 (zlib)
// =============================================================================

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

pub fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    for &b in data {
        s1 = (s1 + b as u32) % MOD;
        s2 = (s2 + s1) % MOD;
    }
    (s2 << 16) | s1
}

// =============================================================================
// Groovebox deterministic identity & seed helpers (parity with Python)
// =============================================================================

/// Exact match for `groovebox.identity_unit(*key_parts)` using 8-byte BLAKE2b.
pub fn identity_unit(key_parts: &[&str]) -> f64 {
    let key = key_parts.join("|");
    let h = blake2b_8(key.as_bytes());
    (u64::from_be_bytes(h) as f64) / 18446744073709551616.0
}

/// Exact match for `groovebox.deterministic_u64(*parts)`.
pub fn deterministic_u64(parts: &[&str]) -> u64 {
    let payload = parts.join("|");
    sha256_u64_be(payload.as_bytes())
}

/// Exact match for `groovebox._stable_hash(value, modulus)`.
pub fn stable_hash(value: &str, modulus: Option<u64>) -> u64 {
    let n = sha256_u64_be(value.as_bytes());
    match modulus {
        Some(m) if m > 0 => n % m,
        _ => n,
    }
}

/// Exact match for `visual_determinism._seed_int` / `fractal_spatial_engine._seed_int`
/// for numeric seeds.
pub fn seed_int_f64(seed: f64) -> i64 {
    if seed.is_finite() {
        seed as i64
    } else {
        0
    }
}

/// Exact match for `visual_determinism._seed_int` / `fractal_spatial_engine._seed_int`
/// for string seeds (tries parsing as f64 first, else SHA-256 first 8 bytes big-endian).
pub fn seed_int_str(seed: &str) -> u64 {
    if let Ok(v) = seed.trim().parse::<f64>() {
        if v.is_finite() {
            return (v as i64) as u64;
        }
    }
    sha256_u64_be(seed.as_bytes())
}

/// Exact match for `videogame_engine._safe_int_seed(value)`.
pub fn safe_int_seed_vge(value: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    if value.fract() == 0.0 && value.abs() < 2147483648.0 {
        return (value as i64) & 0x7FFF_FFFF;
    }
    let repr = format_python_f64(value);
    let d = sha256(repr.as_bytes());
    let n = u32::from_be_bytes([d[0], d[1], d[2], d[3]]);
    (n & 0x7FFF_FFFF) as i64
}

/// Exact match for `groovebox._safe_int_seed(value)`.
pub fn safe_int_seed_host(value: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    if value.fract() == 0.0 && value.abs() < 2147483648.0 {
        return (value as i64) & 0x7FFF_FFFF;
    }
    let scaled = value.abs() * 1000.0;
    if scaled < 2147483648.0 {
        let n = (scaled.round() as i64) & 0x7FFF_FFFF;
        return if n != 0 { n } else { 1 };
    }
    let bits = value.to_bits();
    let hi = (bits >> 32) as u32;
    let lo = (bits & 0xFFFF_FFFF) as u32;
    let rem = (scaled % 2147483648.0) as u32;
    let mixed = (hi ^ lo.wrapping_mul(0x9E37_79B1) ^ rem) & 0x7FFF_FFFF;
    if mixed != 0 {
        mixed as i64
    } else {
        1
    }
}

/// Format an `f64` matching Python's `repr(float)` / `json.dumps(float)` rules.
pub fn format_python_f64(v: f64) -> String {
    if !v.is_finite() {
        return "0.0".to_string();
    }
    let v = if v == 0.0 { 0.0 } else { v };
    let abs = v.abs();
    // Python switches to scientific notation for abs < 1e-4 or abs >= 1e16
    if abs != 0.0 && (abs < 1e-4 || abs >= 1e16) {
        let s = format!("{:e}", v);
        // Normalize exponent like `1e-5` -> `1e-05` and `1e16` -> `1e+16`
        if let Some(pos) = s.find('e') {
            let (mant, exp_str) = s.split_at(pos);
            let exp_val: i32 = exp_str[1..].parse().unwrap_or(0);
            let sign = if exp_val < 0 { '-' } else { '+' };
            return format!("{}e{}{:02}", mant, sign, exp_val.abs());
        }
        return s;
    }
    let mut s = format!("{}", v);
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        s.push_str(".0");
    }
    s
}
