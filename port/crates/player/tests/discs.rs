//! The player over the user's discs (found by `discs::for_tests`): Home's
//! movie events with the real host. Prints counts and names, never content.

use lingo_vm::Vm;
use player::{DiscFiles, PlayerHost, Vfs};
use std::path::{Path, PathBuf};

struct Disk(PathBuf);
impl DiscFiles for Disk {
    fn read(&self, rel: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(rel)).ok()
    }
}

fn load(movie: &str) -> Option<(Vm, PlayerHost)> {
    let port = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = discs::for_tests(&port)?;
    let manifest = discs::parse_manifest(discs::ILUMINA_GOLD_250_PTBR).unwrap();
    let vfs = Vfs::new(
        Box::new(Disk(root.clone())),
        manifest.into_iter().map(|e| e.path),
    );
    let bytes = std::fs::read(root.join("cd1/Program/Files").join(movie)).unwrap();
    let casts = player::cast_files(&bytes, movie).unwrap();
    Some(player::load_movie(vfs, movie, &casts).unwrap())
}

#[test]
fn home_movie_events() {
    let Some((mut vm, mut host)) = load("Home.dxr") else {
        return;
    };
    eprintln!("cast libs: {:?}", host.cast_libs);
    for event in ["prepareMovie", "startMovie"] {
        match vm.movie_event(&mut host, event, vec![]) {
            Ok(n) => eprintln!("{event}: OK ({n} handler(s))"),
            Err(e) => eprintln!("{event}: stopped: {e}"),
        }
    }
    eprintln!("alerts: {}", host.alerts.len());
    for a in &host.alerts {
        eprintln!("  alert: {}", a.chars().take(80).collect::<String>());
    }
    eprintln!("unanswered: {:?}", host.missing);
}
