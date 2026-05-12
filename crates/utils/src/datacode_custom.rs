pub const MAGIC: [u8; 4] = *b"AODD";
pub const VERSION: u8 = 1;
pub const KIND_COLOR_CODE: u8 = 1;
pub const KIND_JUST_MONO: u8 = 2;
pub const KIND_JUST_COLOR: u8 = 3;
pub const FLAG_RECOVERY: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedPayload {
    pub kind: u8,
    pub flags: u8,
    pub payload: Vec<u8>,
}

pub fn pack_payload(payload: &[u8], kind: u8, recovery: bool) -> Vec<u8> {
    let flags = if recovery { FLAG_RECOVERY } else { 0 };
    let mut out = Vec::with_capacity(MAGIC.len() + 8 + payload.len() + 3);
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(kind);
    out.push(flags);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out.push(checksum(kind, flags, payload));
    if recovery {
        out.push(payload.iter().fold(0_u8, |acc, b| acc ^ *b));
        out.push((payload.len() & 0xFF) as u8);
    }
    out
}

pub fn parse_payload(bytes: &[u8], expected_kinds: &[u8]) -> Result<ParsedPayload, String> {
    const HEADER_LEN: usize = 11;
    if bytes.len() < HEADER_LEN + 1 {
        return Err("CUSTOM PAYLOAD TOO SHORT".into());
    }
    if bytes[0..4] != MAGIC {
        return Err("CUSTOM PAYLOAD MAGIC MISMATCH".into());
    }
    if bytes[4] != VERSION {
        return Err("CUSTOM PAYLOAD VERSION MISMATCH".into());
    }
    let kind = bytes[5];
    if !expected_kinds.is_empty() && !expected_kinds.contains(&kind) {
        return Err("CUSTOM PAYLOAD KIND MISMATCH".into());
    }
    let flags = bytes[6];
    let len = u32::from_be_bytes([bytes[7], bytes[8], bytes[9], bytes[10]]) as usize;
    let payload_end = HEADER_LEN + len;
    if bytes.len() < payload_end + 1 {
        return Err("CUSTOM PAYLOAD LENGTH EXCEEDS DATA".into());
    }
    let payload = bytes[HEADER_LEN..payload_end].to_vec();
    if bytes[payload_end] != checksum(kind, flags, &payload) {
        return Err("CUSTOM PAYLOAD CHECKSUM MISMATCH".into());
    }
    if flags & FLAG_RECOVERY != 0 {
        if bytes.len() < payload_end + 3 {
            return Err("CUSTOM PAYLOAD RECOVERY DATA MISSING".into());
        }
        let parity = payload.iter().fold(0_u8, |acc, b| acc ^ *b);
        if bytes[payload_end + 1] != parity || bytes[payload_end + 2] != (len & 0xFF) as u8 {
            return Err("CUSTOM PAYLOAD RECOVERY CHECK FAILED".into());
        }
    }
    Ok(ParsedPayload {
        kind,
        flags,
        payload,
    })
}

pub fn bytes_to_bits(data: &[u8]) -> Vec<bool> {
    let mut bits = Vec::with_capacity(data.len() * 8);
    for b in data {
        for shift in (0..8).rev() {
            bits.push(((b >> shift) & 1) == 1);
        }
    }
    bits
}

pub fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bits.len().div_ceil(8));
    for chunk in bits.chunks(8) {
        let mut byte = 0_u8;
        for (i, bit) in chunk.iter().enumerate() {
            if *bit {
                byte |= 1 << (7 - i);
            }
        }
        out.push(byte);
    }
    out
}

pub fn palette(idx: usize) -> [f32; 3] {
    match idx & 7 {
        0 => [0.0, 0.0, 0.0],
        1 => [1.0, 0.0, 0.0],
        2 => [0.0, 1.0, 0.0],
        3 => [0.0, 0.0, 1.0],
        4 => [1.0, 1.0, 0.0],
        5 => [1.0, 0.0, 1.0],
        6 => [0.0, 1.0, 1.0],
        _ => [1.0, 1.0, 1.0],
    }
}

pub fn nearest_palette_index(rgb: [f32; 3]) -> usize {
    let mut best_idx = 0;
    let mut best_d2 = f32::INFINITY;
    for idx in 0..8 {
        let p = palette(idx);
        let d0 = rgb[0] - p[0];
        let d1 = rgb[1] - p[1];
        let d2 = rgb[2] - p[2];
        let dist = d0 * d0 + d1 * d1 + d2 * d2;
        if dist < best_d2 {
            best_d2 = dist;
            best_idx = idx;
        }
    }
    best_idx
}

pub fn jab_fill_positions(w: usize, h: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let tl = x < 5 && y < 5;
            let tr = x + 5 > w && y < 5;
            let bl = x < 5 && y + 5 > h;
            if !(tl || tr || bl) {
                out.push((x, y));
            }
        }
    }
    out
}

fn checksum(kind: u8, flags: u8, payload: &[u8]) -> u8 {
    let mut acc = VERSION ^ kind ^ flags;
    for b in (payload.len() as u32).to_be_bytes() {
        acc = acc.wrapping_mul(31).wrapping_add(b);
    }
    for b in payload {
        acc = acc.wrapping_mul(31).wrapping_add(*b);
    }
    acc
}
