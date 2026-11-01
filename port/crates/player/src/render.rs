//! The run-time stage as a draw list for the compositor: the sprites Lingo
//! left on stage (member, position, size, ink, visibility), their current
//! text, and linked media (JPEG files named by `member.fileName`).

use crate::host::{installed_path, PlayerHost};
use director::{Bitmap, Draw};
use rifx::score::Sprite;
use std::rc::Rc;

/// Decode linked media: JPEG or an uncompressed Windows BMP (1/4/8/24/32
/// bits, the module buttons are 24-bit).
pub fn decode_image(data: &[u8]) -> Option<Bitmap> {
    if data.starts_with(b"BM") {
        decode_bmp(data)
    } else {
        decode_jpeg(data)
    }
}

/// Uncompressed BMP (BI_RGB) to RGBA.
pub fn decode_bmp(d: &[u8]) -> Option<Bitmap> {
    let u16le = |o: usize| Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?));
    let u32le = |o: usize| Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?));
    let off = u32le(10)? as usize;
    let hdr = u32le(14)? as usize;
    let w = u32le(18)? as i32;
    let h = u32le(22)? as i32;
    let bpp = u16le(28)?;
    if u32le(30)? != 0 || w <= 0 || h == 0 {
        return None; // compressed or empty
    }
    let (w, top_down, h) = (w as usize, h < 0, h.unsigned_abs() as usize);
    let colors = match u32le(46)? {
        0 if bpp <= 8 => 1usize << bpp,
        n => n as usize,
    };
    let pal_at = 14 + hdr;
    let pal = |i: usize| -> [u8; 3] {
        let o = pal_at + i * 4;
        match d.get(o..o + 3) {
            Some(p) => [p[2], p[1], p[0]],
            None => [0, 0, 0],
        }
    };
    let _ = colors;
    let stride = (w * bpp as usize).div_ceil(32) * 4;
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        let row = off + stride * if top_down { y } else { h - 1 - y };
        let line = d.get(row..row + stride)?;
        for x in 0..w {
            let px = match bpp {
                24 => [line[x * 3 + 2], line[x * 3 + 1], line[x * 3]],
                32 => [line[x * 4 + 2], line[x * 4 + 1], line[x * 4]],
                8 => pal(line[x] as usize),
                4 => pal(((line[x / 2] >> if x % 2 == 0 { 4 } else { 0 }) & 15) as usize),
                1 => pal(((line[x / 8] >> (7 - x % 8)) & 1) as usize),
                _ => return None,
            };
            let o = (y * w + x) * 4;
            rgba[o..o + 3].copy_from_slice(&px);
            rgba[o + 3] = 255;
        }
    }
    Some(Bitmap {
        width: w,
        height: h,
        rgba,
        reg_x: 0,
        reg_y: 0,
    })
}

/// Decode a JPEG to RGBA (linked media are JPEG on the discs).
pub fn decode_jpeg(data: &[u8]) -> Option<Bitmap> {
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut d = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(data), opts);
    let rgba = d.decode().ok()?;
    let info = d.info()?;
    let (w, h) = (info.width as usize, info.height as usize);
    (rgba.len() == w * h * 4).then_some(Bitmap {
        width: w,
        height: h,
        rgba,
        reg_x: 0,
        reg_y: 0,
    })
}

impl PlayerHost {
    /// The decoded image of a linked-media member (cached by path).
    pub fn linked_image(&self, lib: i64, number: i64) -> Option<Rc<Bitmap>> {
        let m = self.member_info(lib, number)?;
        let path = installed_path(m.file_name.as_deref()?);
        if let Some(c) = self.images.borrow().get(&path) {
            return c.clone();
        }
        let img = self
            .vfs
            .read(&path)
            .and_then(|b| decode_image(&b))
            .map(|mut b| {
                if let Some(info) = m.bitmap {
                    b.reg_x = info.2 as i16;
                    b.reg_y = info.3 as i16;
                }
                Rc::new(b)
            });
        self.images.borrow_mut().insert(path, img.clone());
        img
    }

    /// Sprites on stage, back to front (`locZ`, then channel).
    pub fn draws(&self) -> Vec<Draw> {
        let mut order: Vec<(i64, u16)> = self
            .sprites
            .iter()
            .filter(|(_, s)| s.visible && s.member.is_some())
            .map(|(n, s)| (s.loc_z, *n))
            .collect();
        order.sort();
        order
            .into_iter()
            .map(|(_, n)| {
                let s = &self.sprites[&n];
                let (lib, num) = s.member.unwrap();
                let info = self.member_info(lib, num);
                let kind = info.map(|m| m.kind.as_str()).unwrap_or("empty");
                let text = matches!(kind, "field" | "text")
                    .then(|| self.styled_text(lib, num))
                    .flatten();
                let image = (kind == "bitmap" && info.is_some_and(|m| m.file_name.is_some()))
                    .then(|| self.linked_image(lib, num))
                    .flatten()
                    // A TreeView Xtra sprite: its rows, drawn by the host.
                    .or_else(|| {
                        let t = self.trees.get(&n)?;
                        Some(Rc::new(t.render(
                            s.width,
                            s.height,
                            self.fonts.as_ref(),
                            &|l, m| self.member_bitmap(l, m),
                        )))
                    });
                // A sprite shows its member at the member's size unless the
                // author stretched it (⏳ the score's stretch flag).
                let natural = match &image {
                    Some(b) => Some((b.width as i64, b.height as i64)),
                    None => info.and_then(|m| m.bitmap).map(|b| (b.0, b.1)),
                };
                let (w, h) = match natural {
                    Some((nw, nh)) if !s.stretched => (nw, nh),
                    _ => (s.width.max(0), s.height.max(0)),
                };
                Draw {
                    sprite: {
                        let mut sp = s.score.clone().unwrap_or(Sprite {
                            number: n,
                            kind: 16,
                            ink: 0,
                            fore: 255,
                            back: 0,
                            cast_lib: 0,
                            member: 0,
                            top: 0,
                            left: 0,
                            height: 0,
                            width: 0,
                            blend: 0,
                            stretch: false,
                            color_flag: 0,
                            fore_rgb: [0; 3],
                            back_rgb: [0; 3],
                        });
                        if sp.fore as i64 != s.fore_color || sp.back as i64 != s.back_color {
                            sp.color_flag = 0; // Lingo set a palette colour
                        }
                        sp.ink = s.ink as u8;
                        sp.fore = s.fore_color as u8;
                        sp.back = s.back_color as u8;
                        sp.cast_lib = lib as u16;
                        sp.member = num as u16;
                        sp.top = s.loc.1 as i16;
                        sp.left = s.loc.0 as i16;
                        sp.width = w as u16;
                        sp.height = h as u16;
                        sp.stretch = s.stretched;
                        sp.blend = if s.blend >= 100 { 0 } else { s.blend as u8 };
                        sp
                    },
                    text,
                    image,
                    scroll: info
                        .and_then(|m| m.style.iter().find(|(k, _)| k == "scrolltop"))
                        .map(|(_, v)| v.to_int().max(0))
                        .unwrap_or(0),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bmp_24_bit_bottom_up() {
        // Regression: linked module buttons are BMP files (`Bible
        // notcurrent.bmp`); only JPEG was decoded.
        let (w, h) = (2u32, 2u32);
        let stride = 8; // 2 px × 3 bytes, padded to 4
        let mut d = b"BM".to_vec();
        d.extend((54 + stride * h).to_le_bytes());
        d.extend([0u8; 4]);
        d.extend(54u32.to_le_bytes());
        d.extend(40u32.to_le_bytes());
        d.extend(w.to_le_bytes());
        d.extend(h.to_le_bytes());
        d.extend(1u16.to_le_bytes());
        d.extend(24u16.to_le_bytes());
        d.extend([0u8; 24]);
        // bottom row first: blue, green | top row: red, white (BGR)
        d.extend([255, 0, 0, 0, 255, 0, 0, 0]);
        d.extend([0, 0, 255, 255, 255, 255, 0, 0]);
        let b = decode_image(&d).unwrap();
        assert_eq!((b.width, b.height), (2, 2));
        assert_eq!(&b.rgba[0..4], &[255, 0, 0, 255]); // top-left red
        assert_eq!(&b.rgba[8..12], &[0, 0, 255, 255]); // bottom-left blue
    }
}
