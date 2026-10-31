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

/// Lingo recursion is native recursion in the VM: give the test thread a
/// big stack (the default 2 MB is too small for debug builds).
fn big_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

/// Where each movie event stops today. A ratchet: the test fails if the run
/// regresses *or* gets further, so this table (and the test matrix) must be
/// updated with every step. `None` means the event must complete.
// Regression: the test used to pass while both events printed "stopped".
const EXPECTED: &[(&str, Option<&str>)] = &[
    // The first-run registration dialog (Mui Xtra), as on a fresh Wine
    // profile: Main.init → ValidateRegistration → GetRegistrationData →
    // DoRegistrationDialogEx. Needs the Mui dialogs and a user to answer.
    ("prepareMovie", Some("GetWindowPropList")),
    // Consequence: goMenuMgr is created later in init.
    (
        "startMovie",
        Some("LocalizeModuleNameForHistory not found for void"),
    ),
];

#[test]
fn home_movie_events() {
    big_stack(home_movie_events_inner);
}

fn home_movie_events_inner() {
    let Some((mut vm, mut host)) = load("Home.dxr") else {
        return;
    };
    eprintln!("cast libs: {:?}", host.cast_libs);
    for &(event, expected) in EXPECTED {
        let r = vm.movie_event(&mut host, event, vec![]);
        match &r {
            Ok(n) => eprintln!("{event}: OK ({n} handler(s))"),
            Err(e) => eprintln!("{event}: stopped: {e}"),
        }
        match (expected, r) {
            (None, Ok(_)) => {}
            (None, Err(e)) => panic!("{event} must complete, stopped: {e}"),
            (Some(stop), Err(e)) => assert!(
                e.message.contains(stop),
                "{event} stopped elsewhere (update EXPECTED): {e}"
            ),
            (Some(stop), Ok(_)) => {
                panic!("{event} now completes (was stopping at {stop:?}): update EXPECTED")
            }
        }
    }
    eprintln!("alerts: {}", host.alerts.len());
    for a in &host.alerts {
        eprintln!("  alert: {}", a.chars().take(80).collect::<String>());
    }
    eprintln!("unanswered: {:?}", host.missing);
}
