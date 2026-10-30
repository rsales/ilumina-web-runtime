//! Lingo script contexts: `Lctx`/`LctX`, names (`Lnam`) and scripts (`Lscr`).
//! All three are big-endian in every Director version.

use crate::container::Movie;
use crate::reader::{latin1, Endian, FourCC, Reader};
use crate::Error;

pub const LCTX: FourCC = FourCC::new(b"Lctx");
pub const LCTX_PROTECTED: FourCC = FourCC::new(b"LctX");
pub const LNAM: FourCC = FourCC::new(b"Lnam");
pub const LSCR: FourCC = FourCC::new(b"Lscr");

#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    String(String),
    Int(i32),
    Float(f64),
    Other { kind: u32, raw: Vec<u8> },
}

#[derive(Clone, Debug)]
pub struct Handler {
    pub name_id: i16,
    pub bytecode: Vec<u8>,
    pub argument_ids: Vec<i16>,
    pub local_ids: Vec<i16>,
    pub global_ids: Vec<i16>,
}

#[derive(Clone, Debug)]
pub struct Script {
    pub section: i32,
    /// 1-based position in the context, matches a member's `script_id`.
    pub index: u32,
    pub script_number: u16,
    pub flags: u32,
    pub cast_id: i32,
    pub property_ids: Vec<i16>,
    pub global_ids: Vec<i16>,
    pub handlers: Vec<Handler>,
    pub literals: Vec<Literal>,
}

#[derive(Clone, Debug)]
pub struct Context {
    pub section: i32,
    pub names: Vec<String>,
    pub scripts: Vec<Script>,
}

impl Context {
    pub fn name(&self, id: impl Into<i64>) -> &str {
        let id = id.into();
        usize::try_from(id)
            .ok()
            .and_then(|i| self.names.get(i))
            .map(String::as_str)
            .unwrap_or("<?>")
    }
}

/// Every script context in the file.
pub fn contexts(movie: &Movie) -> Result<Vec<Context>, Error> {
    movie
        .chunks
        .iter()
        .filter(|c| c.fourcc == LCTX || c.fourcc == LCTX_PROTECTED)
        .map(|c| context(movie, c.id))
        .collect()
}

pub fn context(movie: &Movie, section: i32) -> Result<Context, Error> {
    let mut r = Reader::new(movie.chunk_data(section)?, Endian::Big);
    let _unk0 = r.i32()?;
    let _unk1 = r.i32()?;
    let entry_count = r.u32()?;
    let _entry_count2 = r.u32()?;
    let entries_offset = r.u16()? as usize;
    let _unk2 = r.i16()?;
    let _unk3 = r.i32()?;
    let _unk4 = r.i32()?;
    let _unk5 = r.i32()?;
    let lnam_section = r.i32()?;

    let names = names(movie, lnam_section)?;

    r.seek(entries_offset)?;
    let mut scripts = Vec::new();
    for index in 1..=entry_count {
        let _unk = r.i32()?;
        let sec = r.i32()?;
        let _u1 = r.u16()?;
        let _u2 = r.u16()?;
        if sec <= 0 || movie.chunks.get(sec as usize).map(|c| c.fourcc) != Some(LSCR) {
            continue;
        }
        scripts.push(script(movie, sec, index)?);
    }
    Ok(Context {
        section,
        names,
        scripts,
    })
}

fn names(movie: &Movie, section: i32) -> Result<Vec<String>, Error> {
    let mut r = Reader::new(movie.chunk_data(section)?, Endian::Big);
    let _unk0 = r.i32()?;
    let _unk1 = r.i32()?;
    let _len1 = r.u32()?;
    let _len2 = r.u32()?;
    let offset = r.u16()? as usize;
    let count = r.u16()? as usize;
    r.seek(offset)?;
    (0..count)
        .map(|_| r.pstring().map(|b| latin1(&b)))
        .collect()
}

fn ids(r: &mut Reader, offset: usize, count: usize) -> Result<Vec<i16>, Error> {
    let save = r.pos();
    r.seek(offset)?;
    let v = (0..count).map(|_| r.i16()).collect::<Result<_, _>>()?;
    r.seek(save)?;
    Ok(v)
}

pub fn script(movie: &Movie, section: i32, index: u32) -> Result<Script, Error> {
    let data = movie.chunk_data(section)?;
    let mut r = Reader::new(data, Endian::Big);
    r.seek(8)?;
    let _total = r.u32()?;
    let _total2 = r.u32()?;
    let _header_len = r.u16()?;
    let script_number = r.u16()?;
    r.seek(38)?;
    let flags = r.u32()?;
    let _unk42 = r.i16()?;
    let cast_id = r.i32()?;
    let _factory_name = r.i16()?;
    let _hv_count = r.u16()?;
    let _hv_offset = r.u32()?;
    let _hv_size = r.u32()?;
    let prop_count = r.u16()? as usize;
    let prop_offset = r.u32()? as usize;
    let glob_count = r.u16()? as usize;
    let glob_offset = r.u32()? as usize;
    let handler_count = r.u16()? as usize;
    let handler_offset = r.u32()? as usize;
    let lit_count = r.u16()? as usize;
    let lit_offset = r.u32()? as usize;
    let _lit_data_count = r.u32()?;
    let lit_data_offset = r.u32()? as usize;

    let property_ids = ids(&mut r, prop_offset, prop_count)?;
    let global_ids = ids(&mut r, glob_offset, glob_count)?;

    // Handler records: 46 bytes each in D8.5+.
    let mut handlers = Vec::with_capacity(handler_count);
    for i in 0..handler_count {
        r.seek(handler_offset + i * 46)?;
        let name_id = r.i16()?;
        let _vector_pos = r.u16()?;
        let len = r.u32()? as usize;
        let off = r.u32()? as usize;
        let argc = r.u16()? as usize;
        let argo = r.u32()? as usize;
        let localc = r.u16()? as usize;
        let localo = r.u32()? as usize;
        let globc = r.u16()? as usize;
        let globo = r.u32()? as usize;
        let bytecode = data
            .get(off..off + len)
            .ok_or(Error::BadChunk {
                id: section,
                reason: "handler bytecode out of bounds".into(),
            })?
            .to_vec();
        handlers.push(Handler {
            name_id,
            bytecode,
            argument_ids: ids(&mut r, argo, argc)?,
            local_ids: ids(&mut r, localo, localc)?,
            global_ids: ids(&mut r, globo, globc)?,
        });
    }

    // Literals: (type, offset) records, data relative to lit_data_offset.
    let mut literals = Vec::with_capacity(lit_count);
    for i in 0..lit_count {
        r.seek(lit_offset + i * 8)?;
        let kind = r.u32()?;
        let off = r.u32()?;
        let lit = match kind {
            4 => Literal::Int(off as i32),
            1 | 9 => {
                r.seek(lit_data_offset + off as usize)?;
                let n = r.u32()? as usize;
                let raw = r.bytes(n)?;
                if kind == 1 {
                    Literal::String(latin1(raw.strip_suffix(&[0]).unwrap_or(raw)))
                } else if n == 8 {
                    Literal::Float(f64::from_be_bytes(raw.try_into().unwrap()))
                } else {
                    Literal::Other {
                        kind,
                        raw: raw.to_vec(),
                    }
                }
            }
            _ => Literal::Other {
                kind,
                raw: Vec::new(),
            },
        };
        literals.push(lit);
    }

    Ok(Script {
        section,
        index,
        script_number,
        flags,
        cast_id,
        property_ids,
        global_ids,
        handlers,
        literals,
    })
}
