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
    /// D7+ RGB colours (used when `color_flag` says so) — else palette indices `fore`/`back`.
    pub color_flag: u8,
    pub fore_rgb: [u8; 3],
    pub back_rgb: [u8; 3],
}

#[derive(Clone, Debug)]
pub struct Score {
    pub frame_count: usize,
    pub record_size: usize,
    pub channels: usize,
    frames: Vec<Vec<u8>>,
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
            let ink = r.u8()? & 0x3f;
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
                color_flag: (color_code >> 4) & 3,
                fore_rgb: [fore, fg, fb],
                back_rgb: [back, bg, bb],
            });
        }
        Ok(out)
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
    pub min_member: u16,
    pub max_member: u16,
    pub id: u32,
}

/// The movie's cast libraries (`MCsL`).
pub fn cast_libs(movie: &Movie) -> Result<Vec<CastLib>, Error> {
    let Some(c) = movie.first(MCSL) else {
        return Ok(Vec::new());
    };
    let data = movie.chunk_data(c.id)?;
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
    let file_endian = movie.endian;
    (0..count)
        .map(|i| {
            let k = i * per;
            let (mut min_member, mut max_member, mut id) = (0, 0, 0);
            if per >= 4 {
                let mut ir = Reader::new(item(k + 4), file_endian);
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
