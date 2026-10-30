//! RIFX/XFIR container: header, `imap`, `mmap` and `KEY*`.

use crate::reader::{Endian, FourCC, Reader};
use crate::Error;

pub const IMAP: FourCC = FourCC::new(b"imap");
pub const MMAP: FourCC = FourCC::new(b"mmap");
pub const KEY: FourCC = FourCC::new(b"KEY*");
pub const FREE: FourCC = FourCC::new(b"free");
pub const JUNK: FourCC = FourCC::new(b"junk");

/// One entry of the memory map. Its index is the chunk's "section id".
#[derive(Clone, Debug)]
pub struct Chunk {
    pub id: i32,
    pub fourcc: FourCC,
    pub len: u32,
    pub offset: u32,
}

/// `KEY*` entry: chunk `section` belongs to owner `cast_id` with type `fourcc`.
#[derive(Clone, Debug)]
pub struct Key {
    pub section: i32,
    pub owner: i32,
    pub fourcc: FourCC,
}

pub struct Movie<'a> {
    data: &'a [u8],
    pub endian: Endian,
    /// `MV93` for both `.dxr` and `.cxt` in iLumina.
    pub codec: FourCC,
    /// Raw `imap` file version (0x73A = Director MX for iLumina).
    pub version: u32,
    pub chunks: Vec<Chunk>,
    pub keys: Vec<Key>,
}

impl<'a> Movie<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        let magic: [u8; 4] = data
            .get(..4)
            .ok_or(Error::Eof { at: 0, need: 4 })?
            .try_into()
            .unwrap();
        let endian = match &magic {
            b"RIFX" => Endian::Big,
            b"XFIR" => Endian::Little,
            b"FGDM" | b"FGDC" => {
                return Err(Error::Unsupported(
                    "Afterburner-compressed (.dcr/.cct)".into(),
                ))
            }
            _ => return Err(Error::BadMagic(magic)),
        };
        let mut r = Reader::new(data, endian);
        r.skip(4)?;
        let _len = r.u32()?;
        let codec = r.fourcc()?;
        if codec != FourCC::new(b"MV93") && codec != FourCC::new(b"MC95") {
            return Err(Error::Unsupported(format!("codec {codec}")));
        }

        // imap
        if r.fourcc()? != IMAP {
            return Err(Error::Missing("imap"));
        }
        let _imap_len = r.u32()?;
        let _map_count = r.u32()?;
        let mmap_offset = r.u32()? as usize;
        let version = r.u32()?;

        // mmap
        r.seek(mmap_offset)?;
        if r.fourcc()? != MMAP {
            return Err(Error::Missing("mmap"));
        }
        let _mmap_len = r.u32()?;
        let header_len = r.u16()? as usize;
        let entry_len = r.u16()? as usize;
        let _count_max = r.i32()?;
        let count_used = r.i32()?;
        if !(0..=1_000_000).contains(&count_used) || entry_len < 20 {
            return Err(Error::Unsupported(format!(
                "mmap: {count_used} entries of {entry_len} bytes"
            )));
        }
        let first = mmap_offset + 8 + header_len;
        let mut chunks = Vec::with_capacity(count_used as usize);
        for i in 0..count_used as usize {
            r.seek(first + i * entry_len)?;
            let fourcc = r.fourcc()?;
            let len = r.u32()?;
            let offset = r.u32()?;
            chunks.push(Chunk {
                id: i as i32,
                fourcc,
                len,
                offset,
            });
        }

        let mut movie = Movie {
            data,
            endian,
            codec,
            version,
            chunks,
            keys: Vec::new(),
        };
        movie.keys = movie.read_keys()?;
        Ok(movie)
    }

    /// The payload of a chunk (after its 8-byte FourCC + length header).
    pub fn chunk_data(&self, id: i32) -> Result<&'a [u8], Error> {
        let c = self.chunks.get(id as usize).ok_or(Error::BadChunk {
            id,
            reason: "no such section".into(),
        })?;
        let start = c.offset as usize + 8;
        let end = start + c.len as usize;
        self.data.get(start..end).ok_or(Error::BadChunk {
            id,
            reason: "payload out of file bounds".into(),
        })
    }

    pub fn first(&self, fourcc: FourCC) -> Option<&Chunk> {
        self.chunks.iter().find(|c| c.fourcc == fourcc)
    }

    pub fn all(&self, fourcc: FourCC) -> impl Iterator<Item = &Chunk> {
        self.chunks.iter().filter(move |c| c.fourcc == fourcc)
    }

    /// Chunks of type `fourcc` that `KEY*` attaches to `owner`.
    pub fn children(&self, owner: i32, fourcc: FourCC) -> impl Iterator<Item = i32> + '_ {
        self.keys
            .iter()
            .filter(move |k| k.owner == owner && k.fourcc == fourcc)
            .map(|k| k.section)
    }

    fn read_keys(&self) -> Result<Vec<Key>, Error> {
        let Some(k) = self.first(KEY) else {
            return Ok(Vec::new());
        };
        let mut r = Reader::new(self.chunk_data(k.id)?, self.endian);
        let _entry_size = r.u16()?;
        let _entry_size2 = r.u16()?;
        let _count = r.u32()?;
        let used = r.u32()?;
        let mut keys = Vec::with_capacity(used as usize);
        for _ in 0..used {
            let section = r.i32()?;
            let owner = r.i32()?;
            let fourcc = r.fourcc()?;
            keys.push(Key {
                section,
                owner,
                fourcc,
            });
        }
        Ok(keys)
    }

    /// Director version as Director's own "human" number, e.g. 850 for MX.
    pub fn human_version(&self) -> u32 {
        human_version(self.version)
    }
}

/// Map the `imap` file version to a Director release (after ProjectorRays'
/// table). Only the bands relevant to iLumina are named precisely.
pub fn human_version(v: u32) -> u32 {
    match v {
        v if v >= 0x79F => 1201,
        v if v >= 0x783 => 1200,
        v if v >= 0x782 => 1150,
        v if v >= 0x781 => 1100,
        v if v >= 0x73B => 1000,
        v if v >= 0x6A4 => 850,
        v if v >= 0x582 => 800,
        v if v >= 0x4C8 => 700,
        v if v >= 0x4C2 => 600,
        v if v >= 0x4B1 => 501,
        v if v >= 0x45D => 500,
        _ => 404,
    }
}
