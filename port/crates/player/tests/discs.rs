//! The player over the user's discs (found by `discs::for_tests`): Home's
//! movie events with the real host. Prints counts and names, never content.

use lingo_vm::{Value, Vm};

// The native fonts of `xtask` (fontdue over installed fonts), for the
// optional render of the component path.
#[allow(dead_code)]
#[path = "../../xtask/src/fonts.rs"]
mod fonts;
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
const FIXED_OWNER_STOPS: &[&str] = &[
    "Registre Sua C",
    "the actorList",
    "sendSprite",
    "charPosToLoc",
    "updateStage",
    "#nothing",
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
        code: get("ILUMINA_ACTIVATION_CODE").unwrap_or_default(),
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
            secrets = [
                o.name.clone(),
                o.email.clone(),
                o.cep.clone(),
                o.code.clone(),
            ]
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
    // Empty: the whole startup completes (2026-10-31). The next ratchet is
    // the frame loop (go, labels, endSprite, stepFrame).
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
    // ILUMINA_RENDER=<file.png>: draw the stage as Lingo left it (local
    // inspection only; the image is content and must not be committed).
    // ILUMINA_DUMP=lib:member,…: print members' run-time text and styles
    // (local debugging; content).
    if let Ok(list) = std::env::var("ILUMINA_DUMP") {
        for spec in list.split(',') {
            let (l, m) = spec.split_once(':').unwrap();
            if let Some(info) = host.member_info(l.parse().unwrap(), m.parse().unwrap()) {
                eprintln!(
                    "member {spec}: {:?}\n  text {:?}\n  runs {:?}",
                    info.style,
                    info.text,
                    info.styled.as_ref().map(|s| &s.runs)
                );
            }
        }
    }
    // ILUMINA_SPRITES=a,b,…: print sprites' run-time state (debugging).
    if let Ok(list) = std::env::var("ILUMINA_SPRITES") {
        for n in list.split(',').filter_map(|n| n.parse::<u16>().ok()) {
            if let Some(st) = host.sprites.get(&n) {
                eprintln!(
                    "sprite {n}: member {:?} loc {:?} {}x{} visible {} stretched {} ink {}",
                    st.member, st.loc, st.width, st.height, st.visible, st.stretched, st.ink
                );
            }
        }
    }
    if let Ok(out) = std::env::var("ILUMINA_RENDER") {
        render_png(&host, "Home.dxr", Path::new(&out));
    }
    // Playback after the startup: a few seconds of frames at 15 fps.
    let mut frames = Vec::new();
    let mut play_error = None;
    for _ in 0..45 {
        if let Err(e) = player::stage::step(&mut vm, &mut host, 66) {
            play_error = Some(e);
            break;
        }
        frames.push(host.frame);
    }
    frames.dedup();
    eprintln!("playback frames: {frames:?}");
    eprintln!("dialogs: {} shown", host.dialogs_shown.len());
    for d in host.dialogs_shown.iter().take(12) {
        eprintln!("  {}", d.chars().take(100).collect::<String>());
    }
    // The mouse: over the Bible module, then a click.
    use player::stage::{mouse, Mouse};
    let (bx, by) = (1000, 232);
    let mut mouse_log = Vec::new();
    for ev in [Mouse::Move(bx, by), Mouse::Move(bx + 1, by)] {
        mouse_log.push(format!(
            "{ev:?}: {:?}",
            mouse(&mut vm, &mut host, ev).map_err(|e| e.message)
        ));
        let _ = player::stage::step(&mut vm, &mut host, 66);
    }
    let over = host.mouse_over;
    if let Ok(out) = std::env::var("ILUMINA_RENDER_ROLLOVER") {
        render_png(&host, "Home.dxr", Path::new(&out));
    }
    for ev in [Mouse::Down(bx, by), Mouse::Up(bx, by)] {
        mouse_log.push(format!(
            "{ev:?}: {:?}",
            mouse(&mut vm, &mut host, ev).map_err(|e| e.message)
        ));
    }
    let _ = player::stage::step(&mut vm, &mut host, 66);
    eprintln!("mouse over sprite {over}; {mouse_log:?}");
    assert!(
        mouse_log.iter().all(|l| l.ends_with("Ok(())")),
        "{mouse_log:?}"
    );
    eprintln!(
        "after click: frame {}, go_movie {:?}",
        host.frame, host.go_movie
    );
    // Follow `go to movie`: globals, prefs and the actorList carry over.
    if let Some((movie, _)) = host.go_movie.clone() {
        let file = format!("{movie}.dxr");
        let bytes = host
            .vfs
            .read(&format!("{}Files\\{file}", player::vfs::APP_DIR))
            .expect("movie file");
        let casts = player::cast_files(&bytes, &file).unwrap();
        if let Err(e) = player::switch_movie(&mut vm, &mut host, &file, &casts) {
            panic!("go to movie {movie}: {e}");
        }
        let steps = player::stage::start_movie(&mut vm, &mut host);
        for (step, r) in &steps {
            match r {
                Ok(()) => eprintln!("{movie} {step}: OK"),
                Err(e) => eprintln!("{movie} {step}: stopped: {e}"),
            }
        }
        // Ratchet: clicking the Encyclopedia module goes there and its
        // startup completes with Home's globals (no second init).
        assert_eq!(movie, "Encyclopedia");
        assert!(
            steps.iter().all(|(_, r)| r.is_ok()),
            "Encyclopedia startup stopped"
        );
        if let Ok(out) = std::env::var("ILUMINA_RENDER_NEXT") {
            render_png(&host, &file, Path::new(&out));
        }
    }
    // Ratchet: Home holds on frame 2 ("trigger loop": oLite.TriggerLoop,
    // then `go the frame`) without errors.
    assert!(play_error.is_none(), "playback stopped: {play_error:?}");
    assert_eq!(frames, [2], "playback frames changed: update the ratchet");
    if let Some(e) = &play_error {
        eprintln!("playback: stopped: {e}");
    }
    // The unlocked media must be QuickTime/MP4 files: a known atom type
    // right after the first atom's size (checked on a few; they are large).
    // Regression: the matrix claimed this check before it existed (an edit
    // that silently did not apply).
    let patched: Vec<String> = host.vfs.patched().map(|(p, _)| p.to_string()).collect();
    const ATOMS: [&[u8; 4]; 7] = [
        b"moov", b"mdat", b"free", b"skip", b"wide", b"ftyp", b"pnot",
    ];
    for p in patched.iter().take(3) {
        let b = host.vfs.read(p).expect("patched file reads");
        assert!(
            ATOMS.iter().any(|a| &b[4..8] == *a),
            "{p}: not a QuickTime header after unlocking"
        );
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
    let reads = host.vfs.reads.borrow();
    let total: usize = reads.values().sum();
    eprintln!("disc files read: {} ({} MB)", reads.len(), total >> 20);
    if std::env::var("ILUMINA_DUMP_READS").is_ok() {
        for (p, n) in reads.iter() {
            eprintln!("  {p} {n}");
        }
    }
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

struct MovieFiles(PathBuf);
impl director::Files for MovieFiles {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.join("cd1/Program/Files").join(name)).ok()
    }
}

fn render_png(host: &PlayerHost, movie: &str, out: &Path) {
    let port = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = discs::for_tests(&port).unwrap();
    let exe = std::fs::read(root.join("cd1/Program/iLumina.exe")).unwrap();
    let palette = rifx::bitmap::system_win_palette(&exe).unwrap();
    let stage = director::Stage::load(&MovieFiles(root), movie, palette).unwrap();
    let (px, report) = stage
        .render_draws(&host.draws(), &fonts::HostFonts::new())
        .unwrap();
    let f = std::fs::File::create(out).unwrap();
    let mut enc = png::Encoder::new(f, stage.config.width as u32, stage.config.height as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&px).unwrap();
    eprintln!(
        "rendered {} sprites ({} skipped) -> {}",
        report.drawn.len(),
        report.skipped.len(),
        out.display()
    );
    for (n, why) in &report.skipped {
        eprintln!("  sprite {n}: {why}");
    }
}
