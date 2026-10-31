//! Score (`VWSC`, D6+ layout) and cast list (`MCsL`). Both are big-endian.
//!
//! `VWSC`: `u32 stream size, u32 version, u32 list start`; at the list start
//! `u32 entries, u32 list size, u32 max data len`, then `entries` u32
//! offsets relative to `list start + 12 + list size × 4`. Entry 0 is the
//! frame stream: `u32 length, u32 ?, u32 frame count, u16 frames version,
//! u16 sprite record size, u16 channels, u16 displayed channels`, then per
//! frame `u16 length` (including itself) followed by deltas `(u16 size,
//! u16 offset, bytes)` applied to a rolling `channels × record size` buffer.
//!
//! D6+ channels: 0 frame script, 1 tempo, 2 transition, 3–4 sound,
//! 5 palette, 6… sprites (sprite number = channel − 5).
//!
//! Sprite record: `u8 type, u8 ink (low 6 bits; 0x40 trails, 0x80
//! stretch), u8 fore, u8 back, u16 cast lib, u16 member, u32 sprite list
//! index, i16 top, i16 left, u16 height, u16 width, u8 colour code,
//! u8 blend, …`.
//!
//! Entry 1 lists sprite intervals: `u32 count`, then `count` entry numbers.
//! Each names a triple of entries: primary (`u32 start frame, u32 end frame,
//! u32 ?, u32 ?, u32 channel, …`), secondary (behaviors: `u16 cast lib,
//! u16 member, u32 entry of the parameter string` each, 0 = none) and a
//! tertiary entry (empty on the discs). Parameter strings are Lingo
//! property-list literals (`[#pTipText: "Anterior", …]`), NUL-terminated.
//!
//! Layout notes follow ScummVM/dirplayer-rs format research; no code copied.

use crate::container::Movie;
use crate::reader::{latin1, Endian, FourCC, Reader};
use crate::Error;

pub const VWSC: FourCC = FourCC::new(b"VWSC");
pub const MCSL: FourCC = FourCC::new(b"MCsL");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sprite {
    /// Sprite channel number (1-based, as Lingo sees it).
    pub number: u16,
    pub kind: u8,
    pub ink: u8,
    pub fore: u8,
    pub back: u8,
    pub cast_lib: u16,
    pub member: u16,
    pub top: i16,
    pub left: i16,
    pub height: u16,
    pub width: u16,
    pub blend: u8,
    /// The author resized the sprite (ink byte 0x80); otherwise it shows its
    /// member at the member's size.
    pub stretch: bool,
    /// D7+ RGB colours (used when `color_flag` says so) — else palette indices `fore`/`back`.
    pub color_flag: u8,
    pub fore_rgb: [u8; 3],
    pub back_rgb: [u8; 3],
}

/// A behavior attached to a sprite interval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BehaviorRef {
    pub cast_lib: u16,
    pub member: u16,
    /// Property-list literal with the author's settings, if any. Decoded as
    /// Mac Roman (the discs' strings are; ⏳ per-movie platform flag).
    pub params: Option<String>,
}

/// A sprite's span of frames and the behaviors attached to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpriteInterval {
    pub start: u32,
    pub end: u32,
    /// Sprite number (1-based, as Lingo sees it).
    pub sprite: u16,
    pub behaviors: Vec<BehaviorRef>,
}

#[derive(Clone, Debug)]
pub struct Score {
    pub frame_count: usize,
    pub record_size: usize,
    pub channels: usize,
    frames: Vec<Vec<u8>>,
    pub intervals: Vec<SpriteInterval>,
}

/// Sprite intervals from the score's entry list (see the module notes).
fn intervals<'a>(entry: &dyn Fn(usize) -> Option<&'a [u8]>) -> Result<Vec<SpriteInterval>, Error> {
    let Some(list) = entry(1) else {
        return Ok(Vec::new());
    };
    let mut r = Reader::new(list, Endian::Big);
    if r.remaining() < 4 {
        return Ok(Vec::new());
    }
    let count = r.u32()? as usize;
    let mut out = Vec::new();
    for _ in 0..count.min(r.remaining() / 4) {
        let i = r.u32()? as usize;
        let Some(primary) = entry(i) else { continue };
        if primary.len() < 20 {
            continue;
        }
        let mut p = Reader::new(primary, Endian::Big);
        let start = p.u32()?;
        let end = p.u32()?;
        let _ = (p.u32()?, p.u32()?);
        let channel = p.u32()?;
        if channel < 6 {
            continue; // not a sprite channel
        }
        let mut behaviors = Vec::new();
        if let Some(sec) = entry(i + 1) {
            let mut b = Reader::new(sec, Endian::Big);
            while b.remaining() >= 8 {
                let cast_lib = b.u16()?;
                let member = b.u16()?;
                let pi = b.u32()? as usize;
                let params = (pi != 0)
                    .then(|| entry(pi))
                    .flatten()
                    .map(|d| {
                        let end = d.iter().position(|&c| c == 0).unwrap_or(d.len());
                        crate::text::mac_roman(&d[..end])
                    })
                    .filter(|t| !t.is_empty());
                behaviors.push(BehaviorRef {
                    cast_lib,
                    member,
                    params,
                });
            }
        }
        out.push(SpriteInterval {
            start,
            end,
            sprite: (channel - 5) as u16,
            behaviors,
        });
    }
    Ok(out)
}

impl Score {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data, Endian::Big);
        let _size = r.u32()?;
        let _ver = r.u32()?;
        let list_start = r.u32()? as usize;
        r.seek(list_start)?;
        let entries = r.u32()? as usize;
        let list_size = r.u32()? as usize;
        let _max = r.u32()?;
        if entries == 0 || entries > 1_000_000 {
            return Err(Error::Unsupported(format!("VWSC with {entries} entries")));
        }
        let mut offs = Vec::with_capacity(entries);
        for _ in 0..entries {
            offs.push(r.u32()? as usize);
        }
        let base = list_start + 12 + list_size * 4;
        let entry = |i: usize| -> Option<&[u8]> {
            let s = base + *offs.get(i)?;
            let e = offs.get(i + 1).map(|o| base + o).unwrap_or(data.len());
            data.get(s..e)
        };
        let intervals = intervals(&entry)?;
        let start = base + offs[0];
        let end = offs.get(1).map(|o| base + o).unwrap_or(data.len());
        let entry0 = data.get(start..end).ok_or(Error::Eof {
            at: start,
            need: end.saturating_sub(start),
        })?;

        let mut r = Reader::new(entry0, Endian::Big);
        let _len = r.u32()?;
        let _unk = r.u32()?;
        let frame_count_hdr = r.u32()? as usize;
        let _frames_version = r.u16()?;
        let record_size = r.u16()? as usize;
        let channels = r.u16()? as usize;
        let _displayed = r.u16()?;
        if record_size < 20 || channels < 6 {
            return Err(Error::Unsupported(format!(
                "score: {channels} channels of {record_size} bytes"
            )));
        }
        let mut buf = vec![0u8; channels * record_size];
        let mut frames = Vec::new();
        while r.remaining() >= 2 {
            let len = r.u16()? as usize;
            if len < 2 {
                break;
            }
            let body = r.bytes(len - 2)?;
            let mut d = Reader::new(body, Endian::Big);
            while d.remaining() >= 4 {
                let size = d.u16()? as usize;
                let off = d.u16()? as usize;
                let bytes = d.bytes(size)?;
                buf.get_mut(off..off + size)
                    .ok_or(Error::Unsupported(format!(
                        "score delta {off}+{size} outside frame"
                    )))?
                    .copy_from_slice(bytes);
            }
            frames.push(buf.clone());
        }
        let _ = frame_count_hdr; // the header count is unreliable; trust the stream
        Ok(Score {
            frame_count: frames.len(),
            record_size,
            channels,
            frames,
            intervals,
        })
    }

    pub fn from_movie(movie: &Movie) -> Result<Option<Self>, Error> {
        match movie.first(VWSC) {
            Some(c) => Ok(Some(Self::parse(movie.chunk_data(c.id)?)?)),
            None => Ok(None),
        }
    }

    /// Non-empty sprite channels of frame `frame` (1-based).
    pub fn sprites(&self, frame: usize) -> Result<Vec<Sprite>, Error> {
        let f = self
            .frames
            .get(frame.wrapping_sub(1))
            .ok_or(Error::Unsupported(format!("no frame {frame}")))?;
        let mut out = Vec::new();
        for ch in 6..self.channels {
            let rec = &f[ch * self.record_size..(ch + 1) * self.record_size];
            let mut r = Reader::new(rec, Endian::Big);
            let kind = r.u8()?;
            let ink_raw = r.u8()?;
            let ink = ink_raw & 0x3f;
            let fore = r.u8()?;
            let back = r.u8()?;
            let cast_lib = r.u16()?;
            let member = r.u16()?;
            let _list = r.u32()?;
            let top = r.i16()?;
            let left = r.i16()?;
            let height = r.u16()?;
            let width = r.u16()?;
            let color_code = r.u8()?;
            let blend = r.u8()?;
            let _flags = r.u16()?;
            let (fg, bg, fb, bb) = if self.record_size >= 28 {
                (r.u8()?, r.u8()?, r.u8()?, r.u8()?)
            } else {
                (0, 0, 0, 0)
            };
            if member == 0 && kind == 0 {
                continue;
            }
            out.push(Sprite {
                number: (ch - 5) as u16,
                kind,
                ink,
                fore,
                back,
                cast_lib,
                member,
                top,
                left,
                height,
                width,
                blend,
                stretch: ink_raw & 0x80 != 0,
                color_flag: (color_code >> 4) & 3,
                fore_rgb: [fore, fg, fb],
                back_rgb: [back, bg, bb],
            });
        }
        Ok(out)
    }

    /// Sprite intervals covering frame `frame` (1-based).
    pub fn intervals_at(&self, frame: usize) -> impl Iterator<Item = &SpriteInterval> {
        let f = frame as u32;
        self.intervals
            .iter()
            .filter(move |i| i.start <= f && f <= i.end)
    }

    /// Frame script reference of frame `frame` (channel 0): `(cast lib, member)`.
    pub fn frame_script(&self, frame: usize) -> Option<(u16, u16)> {
        let f = self.frames.get(frame.wrapping_sub(1))?;
        let lib = u16::from_be_bytes([f[4], f[5]]);
        let member = u16::from_be_bytes([f[6], f[7]]);
        (member != 0).then_some((lib, member))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastLib {
    /// 1-based cast library number, as used in sprite references.
    pub number: u16,
    pub name: String,
    /// Original path of an external cast, e.g. `Files\\GCode.cxt` (empty = internal).
    pub path: String,
    /// First member number (slot 0 of the cast); 0 when unknown.
    pub min_member: u16,
    pub max_member: u16,
    pub id: u32,
}

/// The movie's cast libraries (`MCsL`).
pub fn cast_libs(movie: &Movie) -> Result<Vec<CastLib>, Error> {
    let Some(c) = movie.first(MCSL) else {
        return Ok(Vec::new());
    };
    parse_cast_libs(movie.chunk_data(c.id)?)
}

/// The `MCsL` chunk's cast libraries.
pub fn parse_cast_libs(data: &[u8]) -> Result<Vec<CastLib>, Error> {
    let mut r = Reader::new(data, Endian::Big);
    let data_offset = r.u32()? as usize;
    let _unk0 = r.u16()?;
    let count = r.u16()? as usize;
    let per = r.u16()? as usize;
    let _unk1 = r.u16()?;
    r.seek(data_offset)?;
    let n = r.u16()? as usize;
    let mut offsets = Vec::with_capacity(n);
    for _ in 0..n {
        offsets.push(r.u32()? as usize);
    }
    let items_len = r.u32()? as usize;
    let base = r.pos();
    let item = |i: usize| -> &[u8] {
        let (Some(&a), b) = (
            offsets.get(i),
            offsets.get(i + 1).copied().unwrap_or(items_len),
        ) else {
            return &[];
        };
        data.get(base + a..base + b.max(a)).unwrap_or(&[])
    };
    let pstr = |b: &[u8]| -> String {
        match b.first() {
            Some(&len) => latin1(b.get(1..1 + len as usize).unwrap_or(&b[1..])),
            None => String::new(),
        }
    };
    (0..count)
        .map(|i| {
            let k = i * per;
            let (mut min_member, mut max_member, mut id) = (0, 0, 0);
            if per >= 4 {
                // Big-endian like the rest of MCsL. Regression: read in the
                // file's byte order, GlobalAssets' first member 2 became 512.
                let mut ir = Reader::new(item(k + 4), Endian::Big);
                min_member = ir.u16().unwrap_or(0);
                max_member = ir.u16().unwrap_or(0);
                id = ir.u32().unwrap_or(0);
            }
            Ok(CastLib {
                number: (i + 1) as u16,
                name: if per >= 1 {
                    pstr(item(k + 1))
                } else {
                    String::new()
                },
                path: if per >= 2 {
                    pstr(item(k + 2))
                } else {
                    String::new()
                },
                min_member,
                max_member,
                id,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A VWSC with the given entries (entry 0 = an empty frame stream).
    fn vwsc(rest: &[Vec<u8>]) -> Vec<u8> {
        let mut e0 = Vec::new();
        for v in [20u32, 0, 0] {
            e0.extend(v.to_be_bytes());
        }
        for v in [0u16, 48, 7, 7] {
            e0.extend(v.to_be_bytes());
        }
        let mut entries = vec![e0];
        entries.extend(rest.iter().cloned());
        let mut offs = vec![0u32];
        for e in &entries {
            offs.push(offs.last().unwrap() + e.len() as u32);
        }
        let mut d = Vec::new();
        for v in [0u32, 0, 12, entries.len() as u32, offs.len() as u32, 0] {
            d.extend(v.to_be_bytes());
        }
        for o in offs {
            d.extend(o.to_be_bytes());
        }
        for e in entries {
            d.extend(e);
        }
        d
    }

    fn u32s(v: &[u32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_be_bytes()).collect()
    }

    #[test]
    fn sprite_intervals_with_behaviors_and_parameters() {
        let mut primary = u32s(&[1, 94, 0, 0, 7]); // sprite 2, frames 1–94
        primary.extend([0u8; 24]);
        let mut behaviors = Vec::new();
        behaviors.extend([0, 4, 0, 0x2a, 0, 0, 0, 0]); // castLib 4 member 42, no params
        behaviors.extend([0, 4, 0, 0x30, 0, 0, 0, 5]); // castLib 4 member 48, params in entry 5
        let e = vwsc(&[
            u32s(&[1, 2]),                             // 1: one interval, at entry 2
            primary,                                   // 2
            behaviors,                                 // 3
            Vec::new(),                                // 4 (tertiary)
            b"[#pTipText: \"Pr\x97ximo\"]\0".to_vec(), // 5 (Mac Roman)
        ]);
        let s = Score::parse(&e).unwrap();
        assert_eq!(s.frame_count, 0);
        assert_eq!(
            s.intervals,
            vec![SpriteInterval {
                start: 1,
                end: 94,
                sprite: 2,
                behaviors: vec![
                    BehaviorRef {
                        cast_lib: 4,
                        member: 42,
                        params: None
                    },
                    BehaviorRef {
                        cast_lib: 4,
                        member: 48,
                        params: Some("[#pTipText: \"Próximo\"]".into())
                    },
                ],
            }]
        );
        assert_eq!(s.intervals_at(94).count(), 1);
        assert_eq!(s.intervals_at(95).count(), 0);
    }

    #[test]
    fn cast_list_member_range_is_big_endian() {
        // Regression: min/max member were read in the file's byte order
        // (little-endian XFIR), so GlobalAssets' first member 2 read as 512.
        let items: Vec<Vec<u8>> = vec![
            vec![],                                // 0
            b"\x0cGlobalAssets".to_vec(),          // 1 name
            b"\x05F:\\GA".to_vec(),                // 2 path
            vec![],                                // 3
            vec![0, 2, 0, 0xb6, 0, 0, 0x04, 0x01], // 4: min 2, max 182, id 1025
        ];
        let mut offs = vec![0u32];
        for it in &items {
            offs.push(offs.last().unwrap() + it.len() as u32);
        }
        let total = offs.pop().unwrap();
        let mut d = Vec::new();
        d.extend(12u32.to_be_bytes()); // data offset
        d.extend(0u16.to_be_bytes());
        d.extend(1u16.to_be_bytes()); // one library
        d.extend((items.len() as u16).to_be_bytes()); // items per library
        d.extend(0u16.to_be_bytes());
        d.extend((offs.len() as u16).to_be_bytes());
        for o in &offs {
            d.extend(o.to_be_bytes());
        }
        d.extend(total.to_be_bytes());
        for it in &items {
            d.extend(it);
        }
        let libs = parse_cast_libs(&d).unwrap();
        assert_eq!(libs.len(), 1);
        assert_eq!(libs[0].name, "GlobalAssets");
        assert_eq!(
            (libs[0].min_member, libs[0].max_member, libs[0].id),
            (2, 182, 1025)
        );
    }
}
