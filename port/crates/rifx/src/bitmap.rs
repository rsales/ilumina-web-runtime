//! Bitmap cast members: the `CASt` bitmap header and `BITD` pixel data.
//!
//! Bitmap specific data (big-endian, D7+): `u16 pitch | flags`
//! (bit 15 = has depth/palette), initial rect (top, left, bottom, right),
//! 8 bytes (bounding rect, unused here), `i16 regY, regX`, then if bit 15:
//! `u8 ?, u8 bit depth, i16 palette cast lib, i16 palette member`.
//! Negative palette members are Director's built-in palettes
//! (-101 = "System - Win", which iLumina uses).
//!
//! `BITD` is either raw (`pitch × height` bytes) or PackBits-style RLE:
//! `n < 0x80` copies `n + 1` literal bytes, `n ≥ 0x80` repeats the next byte
//! `257 - n` times. Rows are stored top to bottom. Depths 16 and 32 are
//! planar per row: 16-bit = high bytes then low bytes (RGB555); 32-bit =
//! A, R, G, B planes.

use crate::container::Movie;
use crate::reader::FourCC;
use crate::Error;

pub const BITD: FourCC = FourCC::new(b"BITD");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitmapInfo {
    pub pitch: usize,
    pub top: i16,
    pub left: i16,
    pub width: usize,
    pub height: usize,
    pub reg_x: i16,
    pub reg_y: i16,
    pub depth: u8,
    pub palette_lib: i16,
    pub palette: i16,
}

impl BitmapInfo {
    pub fn parse(specific: &[u8]) -> Result<Self, Error> {
        let be16 = |i: usize| -> Result<i16, Error> {
            specific
                .get(i..i + 2)
                .map(|b| i16::from_be_bytes([b[0], b[1]]))
                .ok_or(Error::Eof { at: i, need: 2 })
        };
        let word = be16(0)? as u16;
        let (top, left, bottom, right) = (be16(2)?, be16(4)?, be16(6)?, be16(8)?);
        let (depth, palette_lib, palette) = if word & 0x8000 != 0 && specific.len() >= 28 {
            (specific[23], be16(24)?, be16(26)?)
        } else {
            (1, -1, 0)
        };
        Ok(BitmapInfo {
            pitch: (word & 0x3fff) as usize,
            top,
            left,
            width: (right as i32 - left as i32).max(0) as usize,
            height: (bottom as i32 - top as i32).max(0) as usize,
            reg_x: be16(20)?,
            reg_y: be16(18)?,
            depth,
            palette_lib,
            palette,
        })
    }
}

/// Expand `BITD` into exactly `pitch × height` bytes.
pub fn unpack(info: &BitmapInfo, bitd: &[u8]) -> Result<Vec<u8>, Error> {
    let want = info.pitch * info.height;
    if bitd.len() == want {
        return Ok(bitd.to_vec());
    }
    let mut out = Vec::with_capacity(want);
    let mut i = 0;
    while out.len() < want && i < bitd.len() {
        let n = bitd[i] as usize;
        i += 1;
        if n < 0x80 {
            let lit = bitd
                .get(i..i + n + 1)
                .ok_or(Error::Eof { at: i, need: n + 1 })?;
            out.extend_from_slice(lit);
            i += n + 1;
        } else {
            let b = *bitd.get(i).ok_or(Error::Eof { at: i, need: 1 })?;
            i += 1;
            out.extend(std::iter::repeat_n(b, 257 - n));
        }
    }
    if out.len() < want {
        return Err(Error::Unsupported(format!(
            "BITD expands to {} of {want} bytes",
            out.len()
        )));
    }
    out.truncate(want);
    Ok(out)
}

/// A 256-entry RGB palette.
pub type Palette = [[u8; 3]; 256];

/// Decode a member's `BITD` to RGBA8. Run-length compressed 16/32-bit data
/// is planar per row (high bytes then low bytes; A, R, G, B planes);
/// uncompressed data (exactly `pitch × height` bytes) is interleaved.
// Regression: small uncompressed icons (Home's 9×9 scroll arrows, 11×11
// minimize/close) were read as planar and came out as coloured noise.
pub fn decode(info: &BitmapInfo, bitd: &[u8], palette: Option<&Palette>) -> Result<Vec<u8>, Error> {
    let raw = bitd.len() == info.pitch * info.height;
    to_rgba(info, &unpack(info, bitd)?, palette, !raw)
}

/// Convert unpacked pixels to RGBA8. `palette` is used for depth ≤ 8;
/// `planar` says how 16/32-bit rows are laid out (see [`decode`]).
pub fn to_rgba(
    info: &BitmapInfo,
    pixels: &[u8],
    palette: Option<&Palette>,
    planar: bool,
) -> Result<Vec<u8>, Error> {
    let (w, h, p) = (info.width, info.height, info.pitch);
    let mut out = vec![0u8; w * h * 4];
    let need = |n: usize| -> Result<(), Error> {
        if pixels.len() < n {
            Err(Error::Eof {
                at: pixels.len(),
                need: n,
            })
        } else {
            Ok(())
        }
    };
    need(p * h)?;
    for y in 0..h {
        let row = &pixels[y * p..(y + 1) * p];
        for x in 0..w {
            let px: [u8; 4] = match info.depth {
                1 => {
                    let bit = (row[x / 8] >> (7 - x % 8)) & 1;
                    if bit == 1 {
                        [0, 0, 0, 255]
                    } else {
                        [255, 255, 255, 255]
                    }
                }
                8 => {
                    let pal =
                        palette.ok_or(Error::Unsupported("8-bit bitmap without palette".into()))?;
                    let c = pal[row[x] as usize];
                    [c[0], c[1], c[2], 255]
                }
                16 => {
                    let v = if planar {
                        ((row[x] as u16) << 8) | row[p / 2 + x] as u16
                    } else {
                        u16::from_be_bytes([row[2 * x], row[2 * x + 1]])
                    };
                    let ex = |c: u16| ((c << 3) | (c >> 2)) as u8;
                    [ex((v >> 10) & 31), ex((v >> 5) & 31), ex(v & 31), 255]
                }
                32 if planar => {
                    let q = p / 4;
                    [row[q + x], row[2 * q + x], row[3 * q + x], row[x]]
                }
                32 => {
                    let o = 4 * x;
                    [row[o + 1], row[o + 2], row[o + 3], row[o]]
                }
                d => return Err(Error::Unsupported(format!("bit depth {d}"))),
            };
            out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&px);
        }
    }
    Ok(out)
}

/// The `BITD` chunk that `KEY*` attaches to a bitmap member's `CASt` section.
pub fn bitd_for<'a>(movie: &Movie<'a>, cast_section: i32) -> Option<Result<&'a [u8], Error>> {
    movie
        .children(cast_section, BITD)
        .next()
        .map(|s| movie.chunk_data(s))
}

/// Director's "System - Win" palette (-101), read from the user's own
/// projector (`iLumina.exe`) instead of being embedded: it is stored there
/// as 256 BGRX entries that start with the 20 Windows static colours
/// (10 at each end).
pub fn system_win_palette(projector: &[u8]) -> Option<Palette> {
    const HEAD: [u8; 40] = [
        0, 0, 0, 0, 0, 0, 128, 0, 0, 128, 0, 0, 0, 128, 128, 0, 128, 0, 0, 0, 128, 0, 128, 0, 128,
        128, 0, 0, 128, 128, 128, 0, 192, 220, 192, 0, 240, 202, 166, 0,
    ];
    let at = projector.windows(HEAD.len()).position(|w| w == HEAD)?;
    let table = projector.get(at..at + 1024)?;
    // The last entry is white in the Windows system palette.
    if table[1020..1024] != [255, 255, 255, 0] {
        return None;
    }
    let mut pal = [[0u8; 3]; 256];
    for (i, c) in pal.iter_mut().enumerate() {
        let e = &table[i * 4..i * 4 + 4];
        *c = [e[2], e[1], e[0]];
    }
    Some(pal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(depth: u8, w: usize, h: usize, pitch: usize) -> BitmapInfo {
        BitmapInfo {
            pitch,
            top: 0,
            left: 0,
            width: w,
            height: h,
            reg_x: 0,
            reg_y: 0,
            depth,
            palette_lib: -1,
            palette: -101,
        }
    }

    #[test]
    fn rle_literals_and_runs() {
        let i = info(8, 4, 2, 4);
        // literal [1,2,3,4], then run of 4 × 9
        let px = unpack(&i, &[0x03, 1, 2, 3, 4, 0xfd, 9]).unwrap();
        assert_eq!(px, [1, 2, 3, 4, 9, 9, 9, 9]);
    }

    #[test]
    fn planar_16_and_32_bit() {
        // 1×1 16-bit: 0x7c00 = pure red
        let i = info(16, 1, 1, 2);
        assert_eq!(
            to_rgba(&i, &[0x7c, 0x00], None, true).unwrap(),
            [255, 0, 0, 255]
        );
        // 1×1 32-bit planes A R G B
        let i = info(32, 1, 1, 4);
        assert_eq!(
            to_rgba(&i, &[200, 1, 2, 3], None, true).unwrap(),
            [1, 2, 3, 200]
        );
    }

    #[test]
    fn uncompressed_16_and_32_bit_rows_are_interleaved() {
        // Two pixels, uncompressed: 16-bit red then blue (big-endian words);
        // 32-bit ARGB per pixel.
        let i16 = info(16, 2, 1, 4);
        let px = decode(&i16, &[0x7c, 0x00, 0x00, 0x1f], None).unwrap();
        assert_eq!(px, [255, 0, 0, 255, 0, 0, 255, 255]);
        let i32 = info(32, 2, 1, 8);
        let px = decode(&i32, &[255, 1, 2, 3, 128, 4, 5, 6], None).unwrap();
        assert_eq!(px, [1, 2, 3, 255, 4, 5, 6, 128]);
    }

    #[test]
    fn parses_header() {
        let mut s = vec![0x82, 0x02, 0, 0, 0, 0, 0, 45, 1, 1];
        s.extend_from_slice(&[0; 8]);
        s.extend_from_slice(&[0, 0x16, 0, 0x80, 0, 16, 0xff, 0xff, 0xff, 0x9b]);
        let b = BitmapInfo::parse(&s).unwrap();
        assert_eq!(
            (b.pitch, b.width, b.height, b.depth, b.palette),
            (514, 257, 45, 16, -101)
        );
        assert_eq!((b.reg_x, b.reg_y), (0x80, 0x16));
    }
}
