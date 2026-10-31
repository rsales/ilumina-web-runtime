//! The player over the user's discs (found by `discs::for_tests`): Home's
//! movie events with the real host. Prints counts and names, never content.

use lingo_vm::{Value, Vm};
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

/// Where the movie's startup (`player::stage::start_movie`: prepareMovie,
/// beginSprite, prepareFrame, startMovie, enterFrame, exitFrame) stops
/// today. A ratchet: the test fails if the run regresses *or* gets further,
/// so this (and the test matrix) must be updated with every step.
// Regression: the test used to pass while both events printed "stopped".
/// Without the owner's data: the first-run registration dialog (Mui), as on
/// a fresh Wine profile; a user who only closes it gets it again.
const STOP_WITHOUT_OWNER: (&str, &str) = (
    "prepareMovie",
    "Registre Sua Cópia do iLúmina\" shown 3 times",
);
/// With the owner's data (2026-10-31 run on the Mac: prepareMovie completes)
/// the run reaches the frame; it must not fall back to stops already fixed.
// Regression: registration accepted, then `the actorList` in Poster.POST.
const FIXED_OWNER_STOPS: &[&str] = &["Registre Sua C", "the actorList", "sendSprite"];

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
    let steps = player::stage::start_movie(&mut vm, &mut host);
    for (step, r) in &steps {
        match r {
            Ok(()) => eprintln!("{step}: OK"),
            Err(e) => eprintln!("{step}: stopped: {}", mask(&e.to_string())),
        }
    }
    let (last, r) = steps.last().expect("at least prepareMovie");
    if with_owner {
        assert_eq!(steps[0].0, "prepareMovie");
        assert!(
            steps[0].1.is_ok() || steps.len() == 1,
            "prepareMovie must complete"
        );
        if let Err(e) = r {
            for fixed in FIXED_OWNER_STOPS {
                assert!(
                    !e.message.contains(fixed),
                    "{last}: back at a fixed stop ({fixed})"
                );
            }
        }
    } else {
        let (step, stop) = STOP_WITHOUT_OWNER;
        match r {
            Err(e) => {
                assert_eq!(
                    *last, step,
                    "stopped at another step (update the ratchet): {e}"
                );
                assert!(e.message.contains(stop), "{last} stopped elsewhere: {e}");
            }
            Ok(()) => panic!("startup now completes without the owner: update the ratchet"),
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

/// The managers `init` creates after registration (script, global, extra args;
/// `DB` = the Databases folder + file name, as `init` builds it). Built one
/// by one after prepareMovie stops at the registration dialog, so the code
/// past registration is exercised without the owner's data.
const MANAGERS: &[(&str, &str, &[&str])] = &[
    ("AddOnMgr", "oAOMgr", &[]),
    ("Db.ModTables", "oModTables", &["DB:ModTables.vdb"]),
    ("Bible", "oBible", &["1", "1", "1", "DB:"]),
    ("Db.Comp", "oComp", &["DB:Comp.vdb"]),
    ("Db.Ency", "oEncy", &["DB:Ency.vdb"]),
    ("Db.Media", "oMda", &["DB:Media.vdb"]),
    ("Db.QTVR", "oTours", &["DB:Tours.vdb"]),
    ("Find", "oFind", &[]),
    ("MenuMgr", "goMenuMgr", &[]),
    ("Anims", "oAnims", &["ANIMATIONS"]),
    ("Photos", "oFotos", &["PHOTOS"]),
    ("Maps", "oMaps", &["MAPS"]),
    ("Audio", "oAudio", &["AUDIO"]),
    ("Db.TT", "oTime", &[]),
];

/// Managers that still fail, with the expected message (a ratchet, like
/// `EXPECTED`). Empty = every manager must build.
const MANAGER_STOPS: &[(&str, &str)] = &[];

/// Where the rest of the registered path stops (ratchet).
const REST_STOPS: &[(&str, &str)] = &[
    // Text layout metrics ("Single Scrollbar" measures its field).
    ("prepareFrame", "handler #charPosToLoc"),
];

#[test]
fn home_managers_build() {
    big_stack(home_managers_build_inner);
}

fn home_managers_build_inner() {
    let Some((mut vm, mut host)) = load("Home.dxr") else {
        return;
    };
    let _ = vm.movie_event(&mut host, "prepareMovie", vec![]);
    let db_dir = vm
        .call(
            &mut host,
            "BuildPath",
            vec![Value::list(vec![Value::str("Databases")])],
        )
        .expect("BuildPath")
        .to_text();
    let mut failures = Vec::new();
    for &(name, global, extra) in MANAGERS {
        let idx = vm
            .program
            .script_named(name)
            .unwrap_or_else(|| panic!("no script {name:?}"));
        let mut args = vec![Value::Script(idx)];
        for a in extra {
            args.push(match a.strip_prefix("DB:") {
                Some(file) => Value::str(format!("{db_dir}{file}")),
                None => match a.parse::<i64>() {
                    Ok(i) => Value::Int(i),
                    Err(_) => Value::str(*a),
                },
            });
        }
        match vm.call_method(&mut host, "new", args) {
            Ok(v) => {
                eprintln!("{name}: OK ({})", v.ilk());
                vm.set_global(global, v);
            }
            Err(e) => {
                eprintln!("{name}: stopped: {e}");
                failures.push((name, e.message.clone()));
            }
        }
    }
    // The rest of init, then SharedPrepareMovie's module object, then
    // startMovie: the path a registered copy takes.
    let media = |sub: &str, vm: &mut Vm, host: &mut PlayerHost| {
        vm.call(
            host,
            "BuildPath",
            vec![Value::list(vec![Value::str("Media"), Value::str(sub)])],
        )
        .map(|v| v.to_text())
    };
    let mut rest: Vec<(&str, Result<Value, lingo_vm::LingoError>)> = Vec::new();
    let anims = media("Animations", &mut vm, &mut host).unwrap();
    let o = vm.global("oAnims");
    rest.push((
        "UnlockAnimations",
        vm.call_method(
            &mut host,
            "UnlockAnimations",
            vec![o, Value::str(anims), Value::str(".mov")],
        ),
    ));
    let tours = media("GuidedTours", &mut vm, &mut host).unwrap();
    let o = vm.global("oTours");
    rest.push((
        "UnlockGTVs",
        vm.call_method(&mut host, "UnlockGTVs", vec![o, Value::str(tours)]),
    ));
    let module = vm.program.script_named("Home Module").expect("Home Module");
    let r = vm.call_method(&mut host, "new", vec![Value::Script(module)]);
    if let Ok(v) = &r {
        vm.set_global("goCurrentModule", v.clone());
    }
    rest.push(("new(script \"Home Module\")", r));
    let began =
        player::stage::begin_frame(&mut vm, &mut host, 1).map(|v| Value::Int(v.len() as i64));
    let began_ok = began.is_ok();
    rest.push(("beginSprite", began));
    for ev in ["prepareFrame", "startMovie", "enterFrame", "exitFrame"] {
        if !began_ok {
            break;
        }
        let r = if ev == "startMovie" {
            vm.movie_event(&mut host, ev, vec![]).map(|_| Value::Void)
        } else {
            player::stage::frame_event(&mut vm, &mut host, ev).map(|_| Value::Void)
        };
        let stop = r.is_err();
        rest.push((ev, r));
        if stop {
            break;
        }
    }
    for (step, r) in &rest {
        match r {
            Ok(_) => eprintln!("{step}: OK"),
            Err(e) => eprintln!("{step}: stopped: {e}"),
        }
    }
    eprintln!(
        "media files unlocked (patched): {}",
        host.vfs.patched().count()
    );
    eprintln!("alerts: {}", host.alerts.len());
    for a in &host.alerts {
        eprintln!("  alert: {}", a.chars().take(80).collect::<String>());
    }
    eprintln!("unanswered: {:?}", host.missing);
    eprintln!("network: {:?}", host.network);
    for (step, r) in &rest {
        let expected = REST_STOPS.iter().find(|(s, _)| s == step).map(|(_, m)| *m);
        match (expected, r) {
            (None, Ok(_)) => {}
            (None, Err(e)) => panic!("{step} must complete, stopped: {e}"),
            (Some(stop), Err(e)) => {
                assert!(e.message.contains(stop), "{step} stopped elsewhere: {e}")
            }
            (Some(stop), Ok(_)) => panic!("{step} now completes (was {stop:?}): update REST_STOPS"),
        }
    }
    for (name, msg) in &failures {
        match MANAGER_STOPS.iter().find(|(n, _)| n == name) {
            Some((_, stop)) => assert!(msg.contains(stop), "{name} stopped elsewhere: {msg}"),
            None => panic!("{name} must build, stopped: {msg}"),
        }
    }
    for (name, _) in MANAGER_STOPS {
        assert!(
            failures.iter().any(|(n, _)| n == name),
            "{name} now builds: update MANAGER_STOPS"
        );
    }
}
