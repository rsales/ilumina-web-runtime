//! Minimal QuickTime movie reader: tracks, handler types, sample
//! descriptions and sample tables (`stsz`/`stco`/`co64`/`stsc`).

use crate::{be16, be32, Error};

#[derive(Clone, Debug)]
pub struct Track {
    /// Media handler subtype: `vide`, `qtvr`, `pano`, `obje`, `sprt`, `soun`…
    pub handler: [u8; 4],
    /// Sample description format, e.g. `mp4v`, `smc `, `jpeg`.
    pub format: [u8; 4],
    pub width: u16,
    pub height: u16,
    /// (file offset, size) of every sample, in decode order.
    pub samples: Vec<(usize, usize)>,
}

impl Track {
    pub fn sample<'a>(&self, data: &'a [u8], i: usize) -> Result<&'a [u8], Error> {
        let &(off, len) = self
            .samples
            .get(i)
            .ok_or(Error::Format(format!("no sample {i}")))?;
        data.get(off..off + len).ok_or(Error::Eof(off))
    }
}

struct Atom {
    kind: [u8; 4],
    body: (usize, usize),
}

fn atoms(d: &[u8], start: usize, end: usize) -> Result<Vec<Atom>, Error> {
    let mut v = Vec::new();
    let mut a = start;
    while a + 8 <= end {
        let mut size = be32(d, a)? as usize;
        let kind: [u8; 4] = d[a + 4..a + 8].try_into().unwrap();
        let mut header = 8;
        if size == 1 {
            let hi = be32(d, a + 8)? as u64;
            let lo = be32(d, a + 12)? as u64;
            size = ((hi << 32) | lo) as usize;
            header = 16;
        } else if size == 0 {
            size = end - a;
        }
        if size < header || a + size > end {
            return Err(Error::Format(format!(
                "atom {:?} at {a:#x}: bad size {size}",
                String::from_utf8_lossy(&kind)
            )));
        }
        v.push(Atom {
            kind,
            body: (a + header, a + size),
        });
        a += size;
    }
    Ok(v)
}

fn child(
    d: &[u8],
    parent: (usize, usize),
    path: &[&[u8; 4]],
) -> Result<Option<(usize, usize)>, Error> {
    let mut cur = parent;
    for p in path {
        match atoms(d, cur.0, cur.1)?.into_iter().find(|a| &a.kind == *p) {
            Some(a) => cur = a.body,
            None => return Ok(None),
        }
    }
    Ok(Some(cur))
}

pub fn tracks(d: &[u8]) -> Result<Vec<Track>, Error> {
    let moov = child(d, (0, d.len()), &[b"moov"])?.ok_or(Error::Format("no moov".into()))?;
    let mut out = Vec::new();
    for trak in atoms(d, moov.0, moov.1)?
        .into_iter()
        .filter(|a| &a.kind == b"trak")
    {
        let need = |path: &[&[u8; 4]]| -> Result<(usize, usize), Error> {
            child(d, trak.body, path)?
                .ok_or(Error::Format(format!("track without {:?}", path.last())))
        };
        let hdlr = need(&[b"mdia", b"hdlr"])?;
        let handler: [u8; 4] = d
            .get(hdlr.0 + 8..hdlr.0 + 12)
            .ok_or(Error::Eof(hdlr.0))?
            .try_into()
            .unwrap();
        let stbl = need(&[b"mdia", b"minf", b"stbl"])?;
        let stsd = child(d, stbl, &[b"stsd"])?.ok_or(Error::Format("no stsd".into()))?;
        let format: [u8; 4] = d
            .get(stsd.0 + 12..stsd.0 + 16)
            .ok_or(Error::Eof(stsd.0))?
            .try_into()
            .unwrap();
        let (width, height) = if &handler == b"vide" {
            // Video sample description: 8 bytes header + 16 generic + 16 → width, height.
            (be16(d, stsd.0 + 8 + 32)?, be16(d, stsd.0 + 8 + 34)?)
        } else {
            (0, 0)
        };

        let stsz = child(d, stbl, &[b"stsz"])?.ok_or(Error::Format("no stsz".into()))?;
        let fixed = be32(d, stsz.0 + 4)? as usize;
        let n = be32(d, stsz.0 + 8)? as usize;
        let sizes: Vec<usize> = if fixed != 0 {
            vec![fixed; n]
        } else {
            (0..n)
                .map(|i| be32(d, stsz.0 + 12 + 4 * i).map(|v| v as usize))
                .collect::<Result<_, _>>()?
        };

        let chunks: Vec<usize> = if let Some(co) = child(d, stbl, &[b"stco"])? {
            let m = be32(d, co.0 + 4)? as usize;
            (0..m)
                .map(|i| be32(d, co.0 + 8 + 4 * i).map(|v| v as usize))
                .collect::<Result<_, _>>()?
        } else if let Some(co) = child(d, stbl, &[b"co64"])? {
            let m = be32(d, co.0 + 4)? as usize;
            (0..m)
                .map(|i| {
                    Ok(((be32(d, co.0 + 8 + 8 * i)? as usize) << 32)
                        | be32(d, co.0 + 12 + 8 * i)? as usize)
                })
                .collect::<Result<_, Error>>()?
        } else {
            return Err(Error::Format("no chunk offsets".into()));
        };

        let stsc = child(d, stbl, &[b"stsc"])?.ok_or(Error::Format("no stsc".into()))?;
        let k = be32(d, stsc.0 + 4)? as usize;
        let runs: Vec<(usize, usize)> = (0..k)
            .map(|i| {
                Ok((
                    be32(d, stsc.0 + 8 + 12 * i)? as usize,
                    be32(d, stsc.0 + 12 + 12 * i)? as usize,
                ))
            })
            .collect::<Result<_, Error>>()?;

        let mut samples = Vec::with_capacity(n);
        for (ci, &chunk) in chunks.iter().enumerate() {
            let per_chunk = runs
                .iter()
                .rfind(|r| r.0 <= ci + 1)
                .map(|r| r.1)
                .unwrap_or(0);
            let mut off = chunk;
            for _ in 0..per_chunk {
                if samples.len() == n {
                    break;
                }
                let len = sizes[samples.len()];
                samples.push((off, len));
                off += len;
            }
        }
        out.push(Track {
            handler,
            format,
            width,
            height,
            samples,
        });
    }
    Ok(out)
}
