//! Player services for the Lingo VM (ADR 0008, M2): the virtual Windows
//! file system laid out like a full install, movie/player properties, and
//! the Xtras the original code calls (BuddyAPI, FileIO; Valentina in M3).
//! No I/O: the disc files come through [`vfs::DiscFiles`].

pub mod binaryio;
pub mod fonts;
pub mod host;
pub mod mui;
pub mod render;
pub mod stage;
pub mod treeview;
pub mod v4md;
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
    let mut vm = Vm::new(Program::default());
    let mut host = PlayerHost::new(vfs);
    add_movie(&mut vm.program, &mut host, movie_file, casts)?;
    Ok((vm, host))
}

/// Read a movie's casts into `program` (appended; see
/// [`Program::begin_movie`]) and the host's member and score tables.
fn add_movie(
    program: &mut Program,
    host: &mut PlayerHost,
    movie_file: &str,
    casts: &[(String, String)],
) -> Result<(), String> {
    let dir = format!("{}Files\\", vfs::APP_DIR);
    program.begin_movie();
    host.movie_name = movie_file.to_string();
    host.members.clear();
    host.cast_libs.clear();
    host.cast_data.clear();
    host.bitmaps.borrow_mut().clear();
    host.score = None;
    let mut loaded = Vec::new();
    for (i, (name, file)) in casts.iter().enumerate() {
        let lib = i as u16 + 1;
        let bytes = host
            .vfs
            .read_shared(&format!("{dir}{file}"))
            .ok_or(format!("{file}: not found"))?;
        loaded.push((lib, name.clone(), bytes));
    }
    // First member number of each library, from the movie's cast list.
    let firsts: Vec<u32> = match loaded.first() {
        Some((_, _, bytes)) => {
            let movie = rifx::Movie::parse(bytes).map_err(|e| e.to_string())?;
            rifx::score::cast_libs(&movie)
                .map_err(|e| e.to_string())?
                .iter()
                .map(|l| l.min_member.max(1) as u32)
                .collect()
        }
        None => Vec::new(),
    };
    for (lib, name, bytes) in &loaded {
        let movie = rifx::Movie::parse(bytes).map_err(|e| e.to_string())?;
        let first_member = firsts.get(*lib as usize - 1).copied().unwrap_or(1);
        let first = program.scripts.len();
        program
            .add_cast_from(&movie, *lib, first_member)
            .map_err(|e| e.to_string())?;
        let scripts: HashMap<u32, usize> = program.scripts[first..]
            .iter()
            .enumerate()
            .map(|(i, s)| (s.member, first + i))
            .collect();
        host.add_cast(&movie, *lib, name, &scripts, first_member)
            .map_err(|e| e.to_string())?;
        host.cast_data.push((*lib, bytes.clone(), first_member));
        if *lib == 1 {
            host.score = rifx::score::Score::from_movie(&movie)
                .map_err(|e| e.to_string())?
                .map(std::rc::Rc::new);
        }
    }
    Ok(())
}

/// `go to movie`: the current movie ends (`endSprite` for its sprites,
/// `stopMovie`) and the next one is loaded into the same VM and host, so
/// globals, the actorList, the virtual file system (prefs, registration)
/// and the clock carry over, as in Director. The caller then runs
/// [`stage::start_movie`]-like events for the new movie.
pub fn switch_movie(
    vm: &mut Vm,
    host: &mut PlayerHost,
    movie_file: &str,
    casts: &[(String, String)],
) -> Result<(), String> {
    let on_stage: Vec<u16> = host.sprites.keys().copied().collect();
    for n in on_stage {
        stage::send_to_sprite(vm, host, n, "endSprite", &[]).map_err(|e| e.message)?;
    }
    vm.movie_event(host, "stopMovie", vec![])
        .map_err(|e| e.message)?;
    host.sprites.clear();
    host.trees.clear();
    host.frame_script = None;
    host.frame_script_span = None;
    host.frame = 0;
    host.go_target = None;
    host.go_movie = None;
    host.mouse_over = 0;
    host.images.borrow_mut().clear();
    add_movie(&mut vm.program, host, movie_file, casts)
}
