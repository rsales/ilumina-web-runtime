//! Bounds-checked byte cursor with selectable endianness.

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endian {
    Big,
    Little,
}

#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    pub endian: Endian,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], endian: Endian) -> Self {
        Self {
            data,
            pos: 0,
            endian,
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn seek(&mut self, pos: usize) -> Result<(), Error> {
        if pos > self.data.len() {
            return Err(Error::Eof { at: pos, need: 0 });
        }
        self.pos = pos;
        Ok(())
    }

    pub fn skip(&mut self, n: usize) -> Result<(), Error> {
        self.bytes(n).map(|_| ())
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(n).ok_or(Error::Eof {
            at: self.pos,
            need: n,
        })?;
        if end > self.data.len() {
            return Err(Error::Eof {
                at: self.pos,
                need: n,
            });
        }
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn arr<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.bytes(N)?);
        Ok(a)
    }

    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.arr::<1>()?[0])
    }

    pub fn u16(&mut self) -> Result<u16, Error> {
        let a = self.arr::<2>()?;
        Ok(match self.endian {
            Endian::Big => u16::from_be_bytes(a),
            Endian::Little => u16::from_le_bytes(a),
        })
    }

    pub fn i16(&mut self) -> Result<i16, Error> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32, Error> {
        let a = self.arr::<4>()?;
        Ok(match self.endian {
            Endian::Big => u32::from_be_bytes(a),
            Endian::Little => u32::from_le_bytes(a),
        })
    }

    pub fn i32(&mut self) -> Result<i32, Error> {
        Ok(self.u32()? as i32)
    }

    pub fn f64(&mut self) -> Result<f64, Error> {
        let a = self.arr::<8>()?;
        Ok(match self.endian {
            Endian::Big => f64::from_be_bytes(a),
            Endian::Little => f64::from_le_bytes(a),
        })
    }

    /// A FourCC as it reads in the file's byte order, normalised to the
    /// big-endian spelling (`XFIR` files store `imap` as `pami`).
    pub fn fourcc(&mut self) -> Result<FourCC, Error> {
        let mut a = self.arr::<4>()?;
        if self.endian == Endian::Little {
            a.reverse();
        }
        Ok(FourCC(a))
    }

    /// Pascal string: one length byte, then bytes (Latin-1 / Mac Roman).
    pub fn pstring(&mut self) -> Result<Vec<u8>, Error> {
        let n = self.u8()? as usize;
        Ok(self.bytes(n)?.to_vec())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FourCC(pub [u8; 4]);

impl FourCC {
    pub const fn new(s: &[u8; 4]) -> Self {
        FourCC(*s)
    }
}

impl std::fmt::Debug for FourCC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}'", self)
    }
}

impl std::fmt::Display for FourCC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for &b in &self.0 {
            let c = if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '?'
            };
            write!(f, "{c}")?;
        }
        Ok(())
    }
}

/// Director stores text in the platform's 8-bit code page. The Windows build
/// of iLumina uses Windows-1252; for the inventory, Latin-1 is close enough.
pub fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}
