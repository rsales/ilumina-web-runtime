//! Read-only parser for Macromedia Director MX (D9) movies and casts.
//!
//! Scope (milestone M0 of ADR 0008): open the uncompressed, protected
//! `XFIR`/`RIFX` files (`.dxr`, `.cxt`), walk the memory map, resolve the
//! key table, list cast members, and load Lingo script contexts (names,
//! handlers, literals, bytecode). No I/O: callers pass a byte slice, so the
//! crate builds the same for native and `wasm32`.
//!
//! The layout follows public format research (ProjectorRays, ScummVM's
//! Director engine, "Shockwave Director file format" notes); no code is
//! copied from those projects.

pub mod bytecode;
pub mod cast;
pub mod container;
pub mod lingo;
pub mod reader;

pub use container::{Chunk, Movie};
pub use reader::{Endian, FourCC};

#[derive(Debug)]
pub enum Error {
    Eof { at: usize, need: usize },
    BadMagic([u8; 4]),
    Unsupported(String),
    Missing(&'static str),
    BadChunk { id: i32, reason: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Eof { at, need } => {
                write!(f, "unexpected end of data at {at} (need {need} bytes)")
            }
            Error::BadMagic(m) => write!(
                f,
                "not a Director file (magic {:?})",
                String::from_utf8_lossy(m)
            ),
            Error::Unsupported(s) => write!(f, "unsupported: {s}"),
            Error::Missing(s) => write!(f, "missing chunk: {s}"),
            Error::BadChunk { id, reason } => write!(f, "chunk #{id}: {reason}"),
        }
    }
}

impl std::error::Error for Error {}
