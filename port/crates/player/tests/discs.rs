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

/// Where each movie event stops today without the owner's registration
/// data. A ratchet: the test fails if the run regresses *or* gets further,
/// so this table (and the test matrix) must be updated with every step.
/// `None` means the event must complete.
// Regression: the test used to pass while both events printed "stopped".
const EXPECTED: &[(&str, Option<&str>)] = &[
    // First-run registration dialog (Mui), as on a fresh Wine profile. A user
    // who only closes it gets it again: registration is required.
    (
        "prepareMovie",
        Some("Registre Sua Cópia do iLúmina\" shown 3 times"),
    ),
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

/// The owner's registration data, from the same variables the Wine runtime
/// uses (`.env`). Never generated (ADR 0005).
fn owner() -> Option<player::mui::Owner> {
    let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    Some(player::mui::Owner {
        name: get("ILUMINA_REG_NAME")?,
        email: get("ILUMINA_REG_EMAIL")?,
        cep: get("ILUMINA_REG_CEP").unwrap_or_default(),
    })
}

fn home_movie_events_inner() {
    let Some((mut vm, mut host)) = load("Home.dxr") else {
        return;
    };
    // The owner's values must never reach the log (AGENTS.md): mask them.
    let mut secrets: Vec<String> = Vec::new();
    let with_owner = match owner() {
        Some(o) => {
            secrets = [o.name.clone(), o.email.clone(), o.cep.clone()]
                .into_iter()
                .filter(|v| !v.is_empty())
                .collect();
            host.responder = Box::new(o);
            true
        }
        None => false,
    };
    let mask = |text: &str| -> String {
        let mut t = text.to_string();
        for s in &secrets {
            t = t.replace(s.as_str(), "<owner>");
        }
        t
    };
    eprintln!("cast libs: {:?}; owner data: {with_owner}", host.cast_libs);
    for &(event, expected) in EXPECTED {
        let r = vm.movie_event(&mut host, event, vec![]);
        match &r {
            Ok(n) => eprintln!("{event}: OK ({n} handler(s))"),
            Err(e) => eprintln!("{event}: stopped: {}", mask(&e.to_string())),
        }
        if with_owner {
            // With the owner's data the run goes further; report, don't
            // assert, until the ratchet for that path is set from a real run.
            continue;
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
    eprintln!("dialogs: {}", mask(&format!("{:?}", host.dialogs_shown)));
    eprintln!("alerts: {}", host.alerts.len());
    for a in &host.alerts {
        eprintln!("  alert: {}", mask(&a.chars().take(80).collect::<String>()));
    }
    eprintln!("unanswered: {:?}", host.missing);
    eprintln!("database writes recorded: {}", host.v4md.writes.len());
}
