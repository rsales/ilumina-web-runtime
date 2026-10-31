//! Player services for the Lingo VM (ADR 0008, M2): the virtual Windows
//! file system laid out like a full install, movie/player properties, and
//! the Xtras the original code calls (BuddyAPI, FileIO; Valentina in M3).
//! No I/O: the disc files come through [`vfs::DiscFiles`].

pub mod host;
pub mod vfs;

pub use host::{MemberInfo, PlayerHost};
pub use vfs::{DiscFiles, Vfs};

use lingo_vm::{Program, Vm};
use std::collections::HashMap;

/// Cast libraries of a movie in score order: (name, file in the movie
/// folder). The internal cast is the movie file itself; external casts
/// (`F:\…\GCode.cst`) ship as `.cxt`.
pub fn cast_files(movie_bytes: &[u8], movie_file: &str) -> Result<Vec<(String, String)>, String> {
    let movie = rifx::Movie::parse(movie_bytes).map_err(|e| e.to_string())?;
    let libs = rifx::score::cast_libs(&movie).map_err(|e| e.to_string())?;
    Ok(libs
        .into_iter()
        .map(|l| {
            let base = l
                .path
                .rsplit(['\\', ':', '/'])
                .next()
                .unwrap_or("")
                .to_string();
            let file = if base.is_empty() {
                movie_file.to_string()
            } else {
                match base.rsplit_once('.') {
                    Some((stem, ext)) if ext.eq_ignore_ascii_case("cst") => format!("{stem}.cxt"),
                    _ => base,
                }
            };
            (l.name, file)
        })
        .collect())
}

/// Load a movie and its cast libraries (in score order) into a VM and a
/// host. `casts`: (cast library name, file name in the movie folder).
pub fn load_movie(
    vfs: Vfs,
    movie_file: &str,
    casts: &[(String, String)],
) -> Result<(Vm, PlayerHost), String> {
    let dir = format!("{}Files\\", vfs::APP_DIR);
    let mut program = Program::default();
    let mut host = PlayerHost::new(vfs);
    host.movie_name = movie_file.to_string();
    let mut loaded = Vec::new();
    for (i, (name, file)) in casts.iter().enumerate() {
        let lib = i as u16 + 1;
        let bytes = host
            .vfs
            .read(&format!("{dir}{file}"))
            .ok_or(format!("{file}: not found"))?;
        loaded.push((lib, name.clone(), bytes));
    }
    for (lib, name, bytes) in &loaded {
        let movie = rifx::Movie::parse(bytes).map_err(|e| e.to_string())?;
        let first = program.scripts.len();
        program.add_cast(&movie, *lib).map_err(|e| e.to_string())?;
        let scripts: HashMap<u32, usize> = program.scripts[first..]
            .iter()
            .enumerate()
            .map(|(i, s)| (s.member, first + i))
            .collect();
        host.add_cast(&movie, *lib, name, &scripts)
            .map_err(|e| e.to_string())?;
    }
    Ok((Vm::new(program), host))
}
