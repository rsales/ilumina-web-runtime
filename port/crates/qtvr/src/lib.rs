//! QuickTime VR 2.x reader for iLumina's virtual tours (ADR 0008, M0/M4).
//!
//! A QTVR movie has:
//! * a `qtvr` track: one sample per node, each a QT atom container
//!   (`sean` → `vrsg` name, `ndhd` header, `hspa` hotspots with `hsin`
//!   info and `link` targets);
//! * a `pano` track: one sample per panorama node (`pdat` image/hotspot
//!   track refs, pan/tilt/FOV limits, `cube` type; `cuvw` cubic view);
//! * an image track (MPEG-4 in iLumina, 6 frames per cubic node) and a
//!   hotspot track (`smc `, 6 frames per node, pixel value = hotspot id);
//! * optionally object nodes (`obje` track + JPEG frames) and sprite tracks.
//!
//! Faces are decoded by ffmpeg in `media-prep`; this crate provides the node
//! graph, the view limits and the hotspot maps.

pub mod mov;
pub mod smc;

#[derive(Debug)]
pub enum Error {
    Eof(usize),
    Format(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Eof(at) => write!(f, "unexpected end of data at {at:#x}"),
            Error::Format(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {}

pub(crate) fn be16(d: &[u8], at: usize) -> Result<u16, Error> {
    d.get(at..at + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .ok_or(Error::Eof(at))
}

pub(crate) fn be32(d: &[u8], at: usize) -> Result<u32, Error> {
    d.get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Error::Eof(at))
}

fn bef32(d: &[u8], at: usize) -> Result<f32, Error> {
    be32(d, at).map(f32::from_bits)
}

/// One atom of a QT atom container (20-byte header: size, type, id,
/// reserved, child count, reserved).
#[derive(Clone, Debug)]
pub struct QtAtom<'a> {
    pub kind: [u8; 4],
    pub id: u32,
    pub data: &'a [u8],
    pub children: Vec<QtAtom<'a>>,
}

impl<'a> QtAtom<'a> {
    pub fn find(&self, kind: &[u8; 4]) -> Option<&QtAtom<'a>> {
        self.children.iter().find(|a| &a.kind == kind)
    }
}

/// Parse a QT atom container (12-byte container header, then atoms).
pub fn atom_container(d: &[u8]) -> Result<Vec<QtAtom<'_>>, Error> {
    qt_atoms(d, 12, d.len())
}

fn qt_atoms(d: &[u8], mut a: usize, end: usize) -> Result<Vec<QtAtom<'_>>, Error> {
    let mut v = Vec::new();
    while a + 20 <= end {
        let size = be32(d, a)? as usize;
        if size < 20 || a + size > end {
            return Err(Error::Format(format!("QT atom at {a:#x}: bad size {size}")));
        }
        let kind: [u8; 4] = d[a + 4..a + 8].try_into().unwrap();
        let id = be32(d, a + 8)?;
        let nchildren = be16(d, a + 14)?;
        let (data, children) = if nchildren > 0 {
            (&d[a + 20..a + 20], qt_atoms(d, a + 20, a + size)?)
        } else {
            (&d[a + 20..a + size], Vec::new())
        };
        v.push(QtAtom {
            kind,
            id,
            data,
            children,
        });
        a += size;
    }
    Ok(v)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub to_node: u32,
    pub to_pan: f32,
    pub to_tilt: f32,
    pub to_fov: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hotspot {
    /// Matches pixel values in the hotspot track.
    pub id: u32,
    /// `link`, `undf`, `navg`, `url `…
    pub kind: [u8; 4],
    pub link: Option<Link>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: u32,
    pub name: String,
    /// `pano` or `obje`.
    pub kind: [u8; 4],
    pub hotspots: Vec<Hotspot>,
}

/// Parse one `qtvr` track sample (a node information atom container).
pub fn node(sample: &[u8]) -> Result<Node, Error> {
    let atoms = atom_container(sample)?;
    let sean = atoms
        .iter()
        .find(|a| &a.kind == b"sean")
        .ok_or(Error::Format("node without sean".into()))?;
    let ndhd = sean
        .find(b"ndhd")
        .ok_or(Error::Format("node without ndhd".into()))?;
    let kind: [u8; 4] = ndhd
        .data
        .get(4..8)
        .ok_or(Error::Eof(4))?
        .try_into()
        .unwrap();
    let id = be32(ndhd.data, 8)?;
    let name = sean
        .find(b"vrsg")
        .and_then(|a| {
            let n = be16(a.data, 2).ok()? as usize;
            a.data
                .get(4..4 + n)
                .map(|b| b.iter().map(|&c| c as char).collect())
        })
        .unwrap_or_default();
    let mut hotspots = Vec::new();
    if let Some(hspa) = sean.find(b"hspa") {
        for hots in hspa.children.iter().filter(|a| &a.kind == b"hots") {
            let hsin = hots
                .find(b"hsin")
                .ok_or(Error::Format(format!("hotspot {} without hsin", hots.id)))?;
            let hkind: [u8; 4] = hsin
                .data
                .get(4..8)
                .ok_or(Error::Eof(4))?
                .try_into()
                .unwrap();
            let link = match hots.find(b"link") {
                // LinkHotSpotAtom: version u16 ×2, toNodeID, fromValidFlags,
                // fromPan, fromTilt, fromFOV, fromViewCenter (2×f32),
                // toValidFlags, toPan, toTilt, toFOV, …
                Some(l) => Some(Link {
                    to_node: be32(l.data, 4)?,
                    to_pan: bef32(l.data, 36)?,
                    to_tilt: bef32(l.data, 40)?,
                    to_fov: bef32(l.data, 44)?,
                }),
                None => None,
            };
            hotspots.push(Hotspot {
                id: hots.id,
                kind: hkind,
                link,
            });
        }
    }
    Ok(Node {
        id,
        name,
        kind,
        hotspots,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pano {
    pub image_track_ref: u32,
    pub hotspot_track_ref: u32,
    pub pan: (f32, f32),
    pub tilt: (f32, f32),
    pub fov: (f32, f32),
    pub default_view: (f32, f32, f32),
    /// `cube` for iLumina's tours.
    pub pano_type: [u8; 4],
}

/// Parse one `pano` track sample (`pdat`, optionally `cuvw`).
pub fn pano(sample: &[u8]) -> Result<Pano, Error> {
    let atoms = atom_container(sample)?;
    let sean = atoms
        .iter()
        .find(|a| &a.kind == b"sean")
        .ok_or(Error::Format("pano without sean".into()))?;
    let p = sean
        .find(b"pdat")
        .ok_or(Error::Format("pano without pdat".into()))?
        .data;
    let pano_type: [u8; 4] = p
        .get(p.len().saturating_sub(8)..p.len() - 4)
        .ok_or(Error::Eof(0))?
        .try_into()
        .unwrap();
    let mut out = Pano {
        image_track_ref: be32(p, 4)?,
        hotspot_track_ref: be32(p, 8)?,
        pan: (bef32(p, 16)?, bef32(p, 20)?),
        tilt: (bef32(p, 24)?, bef32(p, 28)?),
        fov: (bef32(p, 32)?, bef32(p, 36)?),
        default_view: (bef32(p, 40)?, bef32(p, 44)?, bef32(p, 48)?),
        pano_type,
    };
    // Cubic panoramas override the limits in `cuvw`.
    if let Some(c) = sean.find(b"cuvw") {
        let c = c.data;
        out.pan = (bef32(c, 0)?, bef32(c, 4)?);
        out.tilt = (bef32(c, 8)?, bef32(c, 12)?);
        out.fov = (bef32(c, 16)?, bef32(c, 20)?);
        out.default_view = (bef32(c, 24)?, bef32(c, 28)?, bef32(c, 32)?);
    }
    Ok(out)
}

/// The whole tour: nodes from the `qtvr` track, panos from the `pano` track.
pub struct Tour {
    pub tracks: Vec<mov::Track>,
    pub nodes: Vec<Node>,
    pub panos: Vec<Pano>,
}

pub fn tour(data: &[u8]) -> Result<Tour, Error> {
    let tracks = mov::tracks(data)?;
    let qtvr = tracks
        .iter()
        .find(|t| &t.handler == b"qtvr")
        .ok_or(Error::Format("no qtvr track".into()))?;
    let nodes = (0..qtvr.samples.len())
        .map(|i| node(qtvr.sample(data, i)?))
        .collect::<Result<_, _>>()?;
    let panos = match tracks.iter().find(|t| &t.handler == b"pano") {
        Some(t) => (0..t.samples.len())
            .map(|i| pano(t.sample(data, i)?))
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };
    Ok(Tour {
        tracks,
        nodes,
        panos,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(kind: &[u8; 4], id: u32, children: u16, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&(20 + body.len() as u32).to_be_bytes());
        v.extend_from_slice(kind);
        v.extend_from_slice(&id.to_be_bytes());
        v.extend_from_slice(&[0, 0]);
        v.extend_from_slice(&children.to_be_bytes());
        v.extend_from_slice(&[0; 4]);
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn parses_a_node_with_a_link_hotspot() {
        let mut ndhd = vec![0, 2, 0, 0];
        ndhd.extend_from_slice(b"pano");
        ndhd.extend_from_slice(&7u32.to_be_bytes());
        ndhd.resize(28, 0);
        let mut vrsg = vec![0, 1, 0, 2];
        vrsg.extend_from_slice(b"N7");
        let mut hsin = vec![0, 2, 0, 0];
        hsin.extend_from_slice(b"link");
        hsin.resize(68, 0);
        let mut link = vec![0, 2, 0, 0];
        link.extend_from_slice(&9u32.to_be_bytes());
        link.resize(36, 0);
        link.extend_from_slice(&90f32.to_bits().to_be_bytes());
        link.extend_from_slice(&(-10f32).to_bits().to_be_bytes());
        link.extend_from_slice(&60f32.to_bits().to_be_bytes());
        link.resize(72, 0);
        let hots = atom(
            b"hots",
            3,
            2,
            &[atom(b"hsin", 1, 0, &hsin), atom(b"link", 1, 0, &link)].concat(),
        );
        let hspa = atom(b"hspa", 1, 1, &hots);
        let sean = atom(
            b"sean",
            1,
            3,
            &[atom(b"vrsg", 1, 0, &vrsg), atom(b"ndhd", 1, 0, &ndhd), hspa].concat(),
        );
        let mut sample = vec![0u8; 12];
        sample.extend_from_slice(&sean);

        let n = node(&sample).unwrap();
        assert_eq!((n.id, n.name.as_str(), &n.kind), (7, "N7", b"pano"));
        assert_eq!(n.hotspots.len(), 1);
        let h = &n.hotspots[0];
        assert_eq!((h.id, &h.kind), (3, b"link"));
        assert_eq!(
            h.link,
            Some(Link {
                to_node: 9,
                to_pan: 90.0,
                to_tilt: -10.0,
                to_fov: 60.0
            })
        );
    }
}
