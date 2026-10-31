//! Read-only reader for Valentina 1.x database files (`.dat`), the format
//! behind the V4MD Xtra that iLumina uses for its Bibles and encyclopedia.
//!
//! The format has no public documentation. Everything here was worked out
//! from iLumina's own files during milestone M0 of ADR 0008 and is checked
//! by `tests/discs.rs` against the KJV table (31,102 verses).
//!
//! # Layout (little-endian)
//!
//! * **Pages.** A 1 KB file header (magic, page size at offset 8: 32 KB in
//!   `.dat`), then whole pages. Pages are named by 1-based ids: page id `n`
//!   lives at byte `0x400 + (n - 1) * page_size`.
//! * **Page chain.** Page id 1 is a `u32` table indexed by page number `p`
//!   (0-based): the id of the next page of the same segment, or -1 at the end.
//! * **Segments.** The catalog (page id 2) lists segments as 24-byte records:
//!   `id, page_count, byte_size, 0, flags, kind:u16, field:u16`. A segment's
//!   id is also the id of its first page; its byte stream is the
//!   concatenation of its pages along the chain.
//!     * `field 0, kind 3`: record bitmap: `u32 count, u32 count`, then one bit per record.
//!     * fixed-width field, `kind 3`: `width = byte_size / record_count` bytes per record.
//!     * text field, `kind 3`: a **slot heap** (see below).
//!     * text field, `kind 12`: the heap's index: one `u32` per slot, the id of
//!       the first record stored in that slot (0 = slot unused).
//! * **Slot heap.** A 1 KB heap header (`u32 free?`, `u32 slot_size = 1024`)
//!   followed by 1 KB slots. Each slot packs consecutive records' values from
//!   offset 0. Its trailer is read from the end: `u16 count`, then
//!   `count + 1` `u16` offsets in reverse order (`offset[count]` is the end).
//!
//! Records are 1-based. Record `r`'s fixed-width value is at index `r - 1`.
//! Record order is the insertion order, not canonical book order.

use std::collections::HashMap;

#[derive(Debug)]
pub enum Error {
    Eof(usize),
    BadMagic,
    Format(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Eof(at) => write!(f, "unexpected end of file at {at:#x}"),
            Error::BadMagic => write!(f, "not a Valentina 1.x file"),
            Error::Format(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {}

const MAGIC: [u8; 8] = [0x00, 0x02, 0x20, 0x00, 0x90, 0x01, 0x01, 0x00];
/// The file starts with a 1 KB header; pages follow.
const FILE_HEADER: usize = 0x400;
/// A slot heap starts with a 1 KB header (`u32`, then `u32 slot_size`).
const HEAP_HEADER: usize = 0x400;

fn u16_at(d: &[u8], at: usize) -> Result<u16, Error> {
    d.get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or(Error::Eof(at))
}

fn u32_at(d: &[u8], at: usize) -> Result<u32, Error> {
    d.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Error::Eof(at))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub id: u32,
    pub pages: u32,
    pub size: u32,
    pub kind: u16,
    pub field: u16,
}

#[derive(Clone)]
pub struct DataFile<'a> {
    data: &'a [u8],
    pub page_size: usize,
    next: Vec<i32>,
    pub segments: Vec<Segment>,
}

impl<'a> DataFile<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        if data.get(..8) != Some(&MAGIC[..]) {
            return Err(Error::BadMagic);
        }
        let page_size = u32_at(data, 8)? as usize;
        if !page_size.is_power_of_two() || page_size < 0x1000 {
            return Err(Error::Format(format!("odd page size {page_size}")));
        }
        let pages = (data.len() - FILE_HEADER) / page_size;
        let mut next = Vec::with_capacity(pages);
        for p in 0..pages {
            next.push(u32_at(data, FILE_HEADER + p * 4)? as i32);
        }
        let mut f = DataFile {
            data,
            page_size,
            next,
            segments: Vec::new(),
        };
        // Catalog: page id 2.
        let cat = f.page(2)?;
        let mut at = cat;
        loop {
            let id = u32_at(data, at)?;
            if id == 0 || at + 24 > cat + page_size {
                break;
            }
            f.segments.push(Segment {
                id,
                pages: u32_at(data, at + 4)?,
                size: u32_at(data, at + 8)?,
                kind: u16_at(data, at + 20)?,
                field: u16_at(data, at + 22)?,
            });
            at += 24;
        }
        Ok(f)
    }

    /// Byte offset of page id `id` (1-based).
    fn page(&self, id: u32) -> Result<usize, Error> {
        let off = FILE_HEADER
            + (id as usize)
                .checked_sub(1)
                .ok_or(Error::Format("page id 0".into()))?
                * self.page_size;
        if off + self.page_size > self.data.len() {
            return Err(Error::Eof(off));
        }
        Ok(off)
    }

    /// The segment's bytes, following its page chain.
    pub fn stream(&self, seg: &Segment) -> Result<Vec<u8>, Error> {
        let mut out = Vec::with_capacity(seg.size as usize);
        let mut id = seg.id;
        for _ in 0..=self.next.len() {
            let start = self.page(id)?;
            out.extend_from_slice(&self.data[start..start + self.page_size]);
            if out.len() >= seg.size as usize {
                out.truncate(seg.size as usize);
                return Ok(out);
            }
            let n = *self
                .next
                .get(id as usize - 1)
                .ok_or(Error::Eof(id as usize))?;
            if n <= 0 {
                break;
            }
            id = n as u32;
        }
        Err(Error::Format(format!(
            "segment {} ends after {} of {} bytes",
            seg.id,
            out.len(),
            seg.size
        )))
    }

    /// One view per table. A table starts at each record-bitmap segment
    /// (`kind 3, field 0`); the catalog segment (`kind 1`) belongs to none.
    pub fn tables(&self) -> Vec<DataFile<'a>> {
        let mut out: Vec<DataFile<'a>> = Vec::new();
        for seg in &self.segments {
            if seg.kind == 3 && seg.field == 0 {
                let mut t = self.clone();
                t.segments = Vec::new();
                out.push(t);
            }
            if let Some(t) = out.last_mut() {
                t.segments.push(seg.clone());
            }
        }
        out
    }

    /// A BLOB value from a `.blb` file (this view must be the matching
    /// table). `first` is the segment number stored in the `.dat` pointer
    /// field. BLOB segments (`slot_size` bytes, 128 in iLumina) start with a
    /// 32-byte header: `next, first, part, part_len, total_len`.
    pub fn blob(&self, field: u16, first: u32) -> Result<Vec<u8>, Error> {
        self.blob_store(field)?.get(first)
    }

    /// Load a field's BLOB store once, for many lookups.
    pub fn blob_store(&self, field: u16) -> Result<BlobStore, Error> {
        let seg = self
            .segment(field, 7)
            .ok_or(Error::Format(format!("no BLOB store for field {field}")))?;
        let store = self.stream(seg)?;
        let seg_size = u32_at(&store, 0)? as usize;
        if !(64..=self.page_size).contains(&seg_size) {
            return Err(Error::Format(format!("BLOB segment size {seg_size}")));
        }
        Ok(BlobStore { store, seg_size })
    }

    pub fn segment(&self, field: u16, kind: u16) -> Option<&Segment> {
        self.segments
            .iter()
            .find(|s| s.field == field && s.kind == kind)
    }

    /// Number of records, from the record bitmap (field 0).
    pub fn record_count(&self) -> Result<u32, Error> {
        let seg = self
            .segment(0, 3)
            .ok_or(Error::Format("no record bitmap".into()))?;
        u32_at(&self.stream(seg)?, 0)
    }

    /// A fixed-width field as raw little-endian integers, one per record.
    pub fn fixed_field(&self, field: u16) -> Result<Vec<u32>, Error> {
        let n = self.record_count()? as usize;
        let seg = self
            .segment(field, 3)
            .ok_or(Error::Format(format!("no field {field}")))?;
        let bytes = self.stream(seg)?;
        let width = bytes.len() / n.max(1);
        if width == 0 && bytes.len() * 8 >= n {
            // Boolean fields are packed one bit per record (assumed LSB first).
            return Ok((0..n)
                .map(|i| ((bytes[i / 8] >> (i % 8)) & 1) as u32)
                .collect());
        }
        if !(1..=4).contains(&width) {
            return Err(Error::Format(format!(
                "field {field}: width {width} is not fixed"
            )));
        }
        Ok(bytes
            .chunks(width)
            .take(n)
            .map(|c| c.iter().rev().fold(0u32, |a, &b| (a << 8) | b as u32))
            .collect())
    }

    /// A text field (slot heap + index), keyed by 1-based record id.
    pub fn text_field(&self, field: u16) -> Result<HashMap<u32, &'a [u8]>, Error> {
        let heap = self
            .segment(field, 3)
            .ok_or(Error::Format(format!("no heap for field {field}")))?;
        let index = self
            .segment(field, 12)
            .ok_or(Error::Format(format!("no index for field {field}")))?;
        let heap_bytes = self.stream(heap)?;
        let index_bytes = self.stream(index)?;
        let slot_size = u32_at(&heap_bytes, 4)? as usize;
        if slot_size == 0 || slot_size > self.page_size {
            return Err(Error::Format(format!(
                "field {field}: slot size {slot_size}"
            )));
        }
        // Values borrow from the file, so map heap-stream offsets back to it.
        let chain = self.chain_offsets(heap)?;
        let mut out = HashMap::new();
        for (k, first) in index_bytes.chunks_exact(4).enumerate() {
            let first = u32::from_le_bytes(first.try_into().unwrap());
            if first == 0 {
                continue;
            }
            let start = HEAP_HEADER + k * slot_size;
            let slot = heap_bytes
                .get(start..start + slot_size)
                .ok_or(Error::Eof(start))?;
            let count = u16_at(slot, slot_size - 2)? as usize;
            if count == 0 || 2 * (count + 2) > slot_size {
                return Err(Error::Format(format!(
                    "field {field}: slot {k} has count {count}"
                )));
            }
            let offs: Vec<usize> = (0..=count)
                .map(|i| u16_at(slot, slot_size - 4 - 2 * i).map(|v| v as usize))
                .collect::<Result<_, _>>()?;
            for i in 0..count {
                let (a, b) = (offs[i], offs[i + 1]);
                if a > b || b > slot_size {
                    return Err(Error::Format(format!(
                        "field {field}: slot {k} item {i} out of bounds"
                    )));
                }
                let file_at = chain.locate(start + a)?;
                out.insert(first + i as u32, &self.data[file_at..file_at + (b - a)]);
            }
        }
        Ok(out)
    }

    fn chain_offsets(&self, seg: &Segment) -> Result<Chain, Error> {
        let mut runs = Vec::new();
        let mut id = seg.id;
        let mut total = 0usize;
        while total < seg.size as usize {
            runs.push((total, self.page(id)?, self.page_size));
            total += self.page_size;
            let n = *self
                .next
                .get(id as usize - 1)
                .ok_or(Error::Eof(id as usize))?;
            if n <= 0 {
                break;
            }
            id = n as u32;
        }
        Ok(Chain(runs))
    }
}

/// A `.blb` field store. BLOB segments (`seg_size` bytes, 128 in iLumina)
/// start with a 32-byte header: `next, first, part, part_len, total_len`.
pub struct BlobStore {
    store: Vec<u8>,
    seg_size: usize,
}

impl BlobStore {
    /// The value whose first segment is `first` (the `.dat` pointer value).
    pub fn get(&self, first: u32) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        let (mut k, mut total) = (first as usize, None);
        for _ in 0..self.store.len() / self.seg_size {
            if k == 0 {
                break;
            }
            let at = HEAP_HEADER + (k - 1) * self.seg_size;
            let next = u32_at(&self.store, at)? as usize;
            let len = u32_at(&self.store, at + 12)? as usize;
            if total.is_none() {
                total = Some(u32_at(&self.store, at + 16)? as usize);
            }
            if len > self.seg_size - 32 {
                return Err(Error::Format(format!("BLOB segment {k}: length {len}")));
            }
            out.extend_from_slice(
                self.store
                    .get(at + 32..at + 32 + len)
                    .ok_or(Error::Eof(at))?,
            );
            k = next;
        }
        match total {
            Some(t) if t != out.len() => Err(Error::Format(format!(
                "BLOB {first}: {} of {t} bytes",
                out.len()
            ))),
            _ => Ok(out),
        }
    }
}

/// Maps a segment-stream offset to a file offset. A value never crosses a
/// page boundary because slots divide pages evenly.
struct Chain(Vec<(usize, usize, usize)>);

impl Chain {
    fn locate(&self, at: usize) -> Result<usize, Error> {
        self.0
            .iter()
            .find(|(s, _, len)| at >= *s && at < s + len)
            .map(|(s, base, _)| base + (at - s))
            .ok_or(Error::Eof(at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a tiny synthetic file: header page with chain table, catalog
    /// page, a 1-byte field page and a one-slot text heap + index.
    fn synthetic() -> Vec<u8> {
        let ps = 0x1000usize;
        let mut d = vec![0u8; FILE_HEADER + ps * 6];
        d[..8].copy_from_slice(&MAGIC);
        d[8..12].copy_from_slice(&(ps as u32).to_le_bytes());
        for p in 0..6 {
            d[0x400 + p * 4..0x404 + p * 4].copy_from_slice(&(-1i32).to_le_bytes());
        }
        // catalog in page id 2: segments 3 (bitmap), 4 (field 2), 5 (heap 3), 6 (index 3)
        let page = |id: usize| FILE_HEADER + (id - 1) * ps;
        let cat = page(2);
        let segs: [(u32, u32, u16, u16); 4] = [
            (3, 9, 3, 0),
            (4, 2, 3, 2),
            (5, 0x400 + 0x400, 3, 3),
            (6, 4, 12, 3),
        ];
        for (i, (id, size, kind, field)) in segs.iter().enumerate() {
            let at = cat + i * 24;
            d[at..at + 4].copy_from_slice(&id.to_le_bytes());
            d[at + 4..at + 8].copy_from_slice(&1u32.to_le_bytes());
            d[at + 8..at + 12].copy_from_slice(&size.to_le_bytes());
            d[at + 20..at + 22].copy_from_slice(&kind.to_le_bytes());
            d[at + 22..at + 24].copy_from_slice(&field.to_le_bytes());
        }
        d[page(3)..page(3) + 8].copy_from_slice(&[2, 0, 0, 0, 2, 0, 0, 0]);
        d[page(4)..page(4) + 2].copy_from_slice(&[7, 9]);
        // heap: header then one slot with "ab" + "cde"
        let h = page(5);
        d[h + 4..h + 8].copy_from_slice(&0x400u32.to_le_bytes());
        let slot = h + 0x400;
        d[slot..slot + 5].copy_from_slice(b"abcde");
        let tail = slot + 0x400;
        d[tail - 2..tail].copy_from_slice(&2u16.to_le_bytes()); // count
        d[tail - 4..tail - 2].copy_from_slice(&0u16.to_le_bytes()); // offset[0]
        d[tail - 6..tail - 4].copy_from_slice(&2u16.to_le_bytes()); // offset[1]
        d[tail - 8..tail - 6].copy_from_slice(&5u16.to_le_bytes()); // offset[2] = end
        d[page(6)..page(6) + 4].copy_from_slice(&1u32.to_le_bytes());
        d
    }

    #[test]
    fn reads_synthetic_file() {
        let d = synthetic();
        let f = DataFile::parse(&d).unwrap();
        assert_eq!(f.record_count().unwrap(), 2);
        assert_eq!(f.fixed_field(2).unwrap(), vec![7, 9]);
        let t = f.text_field(3).unwrap();
        assert_eq!(t[&1], b"ab");
        assert_eq!(t[&2], b"cde");
    }

    #[test]
    fn rejects_other_files() {
        assert!(matches!(
            DataFile::parse(b"RIFX....").err(),
            Some(Error::BadMagic)
        ));
    }
}
