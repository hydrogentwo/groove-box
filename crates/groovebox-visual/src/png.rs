//! Pure-Rust RFC 2083 PNG encoder (RGB8, stored zlib/DEFLATE blocks, IEEE CRC32 + Adler-32).

use groovebox_math::{adler32, crc32};

fn write_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let mut crc_buf = Vec::with_capacity(4 + data.len());
    crc_buf.extend_from_slice(chunk_type);
    crc_buf.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_buf).to_be_bytes());
}

/// Encode an RGB8 buffer (`width * height * 3` bytes, row-major) as a valid PNG image.
pub fn encode_png_rgb(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    assert_eq!(rgb.len(), w * h * 3, "RGB buffer size mismatch");

    // Raw PNG scanlines: 1 filter byte (0 = None) + w*3 RGB bytes per row
    let row_bytes = w * 3;
    let mut raw = Vec::with_capacity(h * (1 + row_bytes));
    for y in 0..h {
        raw.push(0u8);
        raw.extend_from_slice(&rgb[y * row_bytes..(y + 1) * row_bytes]);
    }

    // Wrap raw data in valid zlib stream using uncompressed DEFLATE blocks (max 65535 bytes each)
    let mut zlib = Vec::with_capacity(raw.len() + (raw.len() / 65535 + 1) * 5 + 6);
    zlib.push(0x78); // CMF: deflate, 32K window
    zlib.push(0x01); // FLG: no dict, check bits valid (0x7801 % 31 == 0)

    let mut pos = 0usize;
    while pos < raw.len() {
        let rem = raw.len() - pos;
        let blk = rem.min(65535);
        let is_final = if pos + blk >= raw.len() { 1u8 } else { 0u8 };
        zlib.push(is_final); // BFINAL | BTYPE=00 (stored)
        let len_u16 = blk as u16;
        let nlen_u16 = !len_u16;
        zlib.extend_from_slice(&len_u16.to_le_bytes());
        zlib.extend_from_slice(&nlen_u16.to_le_bytes());
        zlib.extend_from_slice(&raw[pos..pos + blk]);
        pos += blk;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::with_capacity(zlib.len() + 64);
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]); // PNG signature

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(2); // color type: Truecolor RGB
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_chunk(&mut png, b"IHDR", &ihdr);
    write_chunk(&mut png, b"IDAT", &zlib);
    write_chunk(&mut png, b"IEND", &[]);
    png
}
