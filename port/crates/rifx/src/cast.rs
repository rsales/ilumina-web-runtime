//! Cast tables (`CAS*`) and cast members (`CASt`).

use crate::container::Movie;
use crate::reader::{latin1, Endian, FourCC, Reader};
use crate::Error;

pub const CAS: FourCC = FourCC::new(b"CAS*");
pub const CAST: FourCC = FourCC::new(b"CASt");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MemberType {
    Null,
    Bitmap,
    FilmLoop,
    Field,
    Palette,
    Picture,
    Sound,
    Button,
    Shape,
    Movie,
    DigitalVideo,
    Script,
    Text,
    Ole,
    Transition,
    Xtra,
    Unknown(u32),
}

impl From<u32> for MemberType {
    fn from(v: u32) -> Self {
        use MemberType::*;
        match v {
            0 => Null,
            1 => Bitmap,
            2 => FilmLoop,
            3 => Field,
            4 => Palette,
            5 => Picture,
            6 => Sound,
            7 => Button,
            8 => Shape,
            9 => Movie,
            10 => DigitalVideo,
            11 => Script,
            12 => Text,
            13 => Ole,
            14 => Transition,
            15 => Xtra,
            n => Unknown(n),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptType {
    Score,
    Movie,
    Parent,
    Other(u16),
}

#[derive(Clone, Debug)]
pub struct Member {
    /// 1-based slot number inside its cast.
    pub number: u32,
    pub section: i32,
    pub kind: MemberType,
    pub name: String,
    /// Index into the cast's `Lctx` script list (1-based, 0 = none).
    pub script_id: u32,
    pub script_type: Option<ScriptType>,
    /// For Xtra members: the Xtra's symbol (e.g. "TreeView"), when present.
    pub xtra: Option<String>,
    pub info: Vec<Vec<u8>>,
    pub specific: Vec<u8>,
}

/// All members of every `CAS*` table in the file, in slot order.
pub fn members(movie: &Movie) -> Result<Vec<Vec<Member>>, Error> {
    let mut casts = Vec::new();
    for cas in movie.all(CAS) {
        let data = movie.chunk_data(cas.id)?;
        // Section ids in CAS* are big-endian in every Director version.
        let mut r = Reader::new(data, Endian::Big);
        let mut out = Vec::new();
        let mut number = 0u32;
        while r.remaining() >= 4 {
            number += 1;
            let section = r.i32()?;
            if section <= 0 {
                continue;
            }
            match member(movie, section, number) {
                Ok(m) => out.push(m),
                Err(e) => {
                    return Err(Error::BadChunk {
                        id: section,
                        reason: format!("CASt #{number}: {e}"),
                    })
                }
            }
        }
        casts.push(out);
    }
    Ok(casts)
}

fn member(movie: &Movie, section: i32, number: u32) -> Result<Member, Error> {
    let data = movie.chunk_data(section)?;
    // CASt payloads are big-endian regardless of the file's byte order.
    let mut r = Reader::new(data, Endian::Big);
    let kind = MemberType::from(r.u32()?);
    let info_len = r.u32()? as usize;
    let specific_len = r.u32()? as usize;
    let info_bytes = r.bytes(info_len)?;
    let specific = r.bytes(specific_len.min(r.remaining()))?.to_vec();

    let (script_id, info) = if info_len >= 20 {
        read_info(info_bytes)?
    } else {
        (0, Vec::new())
    };
    let name = info.get(1).map(|b| pascal_or_raw(b)).unwrap_or_default();

    let script_type = (kind == MemberType::Script && specific.len() >= 2).then(|| {
        match u16::from_be_bytes([specific[0], specific[1]]) {
            1 => ScriptType::Score,
            3 => ScriptType::Movie,
            7 => ScriptType::Parent,
            n => ScriptType::Other(n),
        }
    });
    let xtra = (kind == MemberType::Xtra && specific.len() >= 4).then(|| {
        let n = u32::from_be_bytes(specific[..4].try_into().unwrap()) as usize;
        latin1(specific.get(4..4 + n).unwrap_or_default())
            .trim_end_matches('\0')
            .to_string()
    });

    Ok(Member {
        number,
        section,
        kind,
        name,
        script_id,
        script_type,
        xtra,
        info,
        specific,
    })
}

/// The cast-info list: a 20-byte header (data offset, two unknowns, flags,
/// script id), an offset table and the items. Item 0 is the script source
/// (empty in protected movies), item 1 the member name.
fn read_info(b: &[u8]) -> Result<(u32, Vec<Vec<u8>>), Error> {
    let mut r = Reader::new(b, Endian::Big);
    let data_offset = r.u32()? as usize;
    let _unk1 = r.u32()?;
    let _unk2 = r.u32()?;
    let _flags = r.u32()?;
    let script_id = r.u32()?;
    if data_offset >= b.len() {
        return Ok((script_id, Vec::new()));
    }
    r.seek(data_offset)?;
    let n = r.u16()? as usize;
    let mut offsets = Vec::with_capacity(n);
    for _ in 0..n {
        offsets.push(r.u32()? as usize);
    }
    let items_len = r.u32()? as usize;
    let base = r.pos();
    let mut items = Vec::with_capacity(n);
    for i in 0..n {
        let start = offsets[i];
        let end = offsets.get(i + 1).copied().unwrap_or(items_len);
        items.push(
            b.get(base + start..base + end.max(start))
                .unwrap_or_default()
                .to_vec(),
        );
    }
    Ok((script_id, items))
}

fn pascal_or_raw(b: &[u8]) -> String {
    match b.first() {
        Some(&n) if n as usize + 1 == b.len() || (n as usize) < b.len() => {
            latin1(&b[1..1 + n as usize])
        }
        _ => latin1(b),
    }
}
