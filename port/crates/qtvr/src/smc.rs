//! Apple "Graphics" (`smc `) decoder: 8-bit palettised, 4×4 blocks with
//! colour caches. QTVR stores hotspot maps this way; each pixel value is a
//! hotspot id (0 = none). Written from the public format description
//! (MultimediaWiki "Apple SMC").

use crate::Error;

pub fn decode(
    buf: &[u8],
    width: usize,
    height: usize,
    prev: Option<&[u8]>,
) -> Result<Vec<u8>, Error> {
    if !width.is_multiple_of(4) || !height.is_multiple_of(4) {
        return Err(Error::Format(format!(
            "smc: {width}x{height} is not a multiple of 4"
        )));
    }
    let mut out = match prev {
        Some(p) if p.len() == width * height => p.to_vec(),
        _ => vec![0u8; width * height],
    };
    let bw = width / 4;
    let total = bw * (height / 4);
    let eof = |at: usize| Error::Format(format!("smc: truncated at byte {at}"));
    let mut pos = 4; // flags byte + 24-bit chunk size
    let mut blk = 0usize;
    let mut pairs = [[0u8; 2]; 256];
    let mut quads = [[0u8; 4]; 256];
    let mut octets = [[0u8; 8]; 256];
    let (mut pi, mut qi, mut oi) = (0usize, 0usize, 0usize);

    let byte = |pos: &mut usize| -> Result<u8, Error> {
        let b = *buf.get(*pos).ok_or_else(|| eof(*pos))?;
        *pos += 1;
        Ok(b)
    };
    let put = |out: &mut [u8], blk: usize, px: &[u8; 16]| {
        let (x, y) = ((blk % bw) * 4, (blk / bw) * 4);
        for r in 0..4 {
            out[(y + r) * width + x..(y + r) * width + x + 4]
                .copy_from_slice(&px[r * 4..r * 4 + 4]);
        }
    };
    let get = |out: &[u8], blk: usize| -> [u8; 16] {
        let (x, y) = ((blk % bw) * 4, (blk / bw) * 4);
        let mut px = [0u8; 16];
        for r in 0..4 {
            px[r * 4..r * 4 + 4]
                .copy_from_slice(&out[(y + r) * width + x..(y + r) * width + x + 4]);
        }
        px
    };

    while blk < total && pos < buf.len() {
        let op = byte(&mut pos)?;
        let long_count = |pos: &mut usize| -> Result<usize, Error> {
            Ok(if op & 0x10 != 0 {
                byte(pos)? as usize + 1
            } else {
                (op & 0x0f) as usize + 1
            })
        };
        let short_count = (op & 0x0f) as usize + 1;
        match op & 0xf0 {
            0x00 | 0x10 => blk += long_count(&mut pos)?,
            0x20 | 0x30 => {
                let n = long_count(&mut pos)?;
                let last = get(&out, blk.checked_sub(1).ok_or_else(|| eof(pos))?);
                for _ in 0..n.min(total - blk) {
                    put(&mut out, blk, &last);
                    blk += 1;
                }
            }
            0x40 | 0x50 => {
                let n = long_count(&mut pos)?;
                let start = blk.checked_sub(2).ok_or_else(|| eof(pos))?;
                let (a, b) = (get(&out, start), get(&out, start + 1));
                for _ in 0..n {
                    for px in [&a, &b] {
                        if blk < total {
                            put(&mut out, blk, px);
                            blk += 1;
                        }
                    }
                }
            }
            0x60 | 0x70 => {
                let n = long_count(&mut pos)?;
                let c = byte(&mut pos)?;
                for _ in 0..n.min(total - blk) {
                    put(&mut out, blk, &[c; 16]);
                    blk += 1;
                }
            }
            0x80 | 0x90 => {
                let idx = if op & 0xf0 == 0x80 {
                    pairs[pi] = [byte(&mut pos)?, byte(&mut pos)?];
                    pi = (pi + 1) & 0xff;
                    (pi + 255) & 0xff
                } else {
                    byte(&mut pos)? as usize
                };
                for _ in 0..short_count.min(total - blk) {
                    let f = ((byte(&mut pos)? as u16) << 8) | byte(&mut pos)? as u16;
                    let mut px = [0u8; 16];
                    for (i, p) in px.iter_mut().enumerate() {
                        *p = pairs[idx][((f >> (15 - i)) & 1) as usize];
                    }
                    put(&mut out, blk, &px);
                    blk += 1;
                }
            }
            0xa0 | 0xb0 => {
                let idx = if op & 0xf0 == 0xa0 {
                    for q in quads[qi].iter_mut() {
                        *q = byte(&mut pos)?;
                    }
                    qi = (qi + 1) & 0xff;
                    (qi + 255) & 0xff
                } else {
                    byte(&mut pos)? as usize
                };
                for _ in 0..short_count.min(total - blk) {
                    let mut px = [0u8; 16];
                    for k in 0..4 {
                        let v = byte(&mut pos)?;
                        for (j, s) in [6, 4, 2, 0].iter().enumerate() {
                            px[k * 4 + j] = quads[idx][((v >> s) & 3) as usize];
                        }
                    }
                    put(&mut out, blk, &px);
                    blk += 1;
                }
            }
            0xc0 | 0xd0 => {
                let idx = if op & 0xf0 == 0xc0 {
                    for o in octets[oi].iter_mut() {
                        *o = byte(&mut pos)?;
                    }
                    oi = (oi + 1) & 0xff;
                    (oi + 255) & 0xff
                } else {
                    byte(&mut pos)? as usize
                };
                for _ in 0..short_count.min(total - blk) {
                    let mut v = [0u32; 3];
                    for x in &mut v {
                        *x = ((byte(&mut pos)? as u32) << 8) | byte(&mut pos)? as u32;
                    }
                    let fa = ((v[0] & 0xfff0) << 8) | (v[1] >> 4);
                    let fb = ((v[2] & 0xfff0) << 8)
                        | ((v[0] & 0x0f) << 8)
                        | ((v[1] & 0x0f) << 4)
                        | (v[2] & 0x0f);
                    let mut px = [0u8; 16];
                    for row in 0..4 {
                        let f = if row < 2 { fa } else { fb };
                        let base = if row % 2 == 0 { 21 } else { 9 };
                        for c in 0..4 {
                            px[row * 4 + c] = octets[idx][((f >> (base - 3 * c)) & 7) as usize];
                        }
                    }
                    put(&mut out, blk, &px);
                    blk += 1;
                }
            }
            0xe0 => {
                for _ in 0..short_count.min(total - blk) {
                    let raw = buf.get(pos..pos + 16).ok_or_else(|| eof(pos))?;
                    put(&mut out, blk, raw.try_into().unwrap());
                    pos += 16;
                    blk += 1;
                }
            }
            _ => {
                return Err(Error::Format(format!(
                    "smc: opcode {op:#x} at byte {}",
                    pos - 1
                )))
            }
        }
    }
    if blk < total {
        return Err(Error::Format(format!("smc: only {blk} of {total} blocks")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_pair_and_repeat() {
        // 8x4 image = 2 blocks: block 0 one-colour 5, block 1 two-colour (1,9)
        // with a checker of the top bit row.
        let buf = [0x80, 0, 0, 12, 0x60, 5, 0x80, 1, 9, 0xf0, 0x00];
        let img = decode(&buf, 8, 4, None).unwrap();
        assert_eq!(&img[0..4], &[5, 5, 5, 5]);
        assert_eq!(&img[4..8], &[9, 9, 9, 9]);
        assert_eq!(&img[12..16], &[1, 1, 1, 1]);
    }

    #[test]
    fn truncated_input_is_an_error() {
        assert!(decode(&[0x80, 0, 0, 5, 0x60], 4, 4, None).is_err());
    }
}
