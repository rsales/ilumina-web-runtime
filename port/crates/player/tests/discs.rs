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
    fn size(&self, rel: &str) -> Option<usize> {
        std::fs::metadata(self.0.join(rel))
            .ok()
            .map(|m| m.len() as usize)
    }
    fn read_at(&self, rel: &str, at: usize, len: usize) -> Option<Vec<u8>> {
        player::vfs::file_read_at(&self.0.join(rel), at, len)
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

type Steps = Vec<(&'static str, Result<Value, lingo_vm::LingoError>)>;

/// Home's startup by components (the registered path, without the
/// registration dialog): managers, then the rest of init and the first
/// frame's events. Returns the managers that failed and the steps.
fn home_component(vm: &mut Vm, host: &mut PlayerHost) -> (Vec<(&'static str, String)>, Steps) {
    let _ = vm.movie_event(host, "prepareMovie", vec![]);
    let db_dir = vm
        .call(
            host,
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
        match vm.call_method(host, "new", args) {
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
    let anims = media("Animations", vm, host).unwrap();
    let o = vm.global("oAnims");
    rest.push((
        "UnlockAnimations",
        vm.call_method(
            host,
            "UnlockAnimations",
            vec![o, Value::str(anims), Value::str(".mov")],
        ),
    ));
    let tours = media("GuidedTours", vm, host).unwrap();
    let o = vm.global("oTours");
    rest.push((
        "UnlockGTVs",
        vm.call_method(host, "UnlockGTVs", vec![o, Value::str(tours)]),
    ));
    let module = vm.program.script_named("Home Module").expect("Home Module");
    let r = vm.call_method(host, "new", vec![Value::Script(module)]);
    if let Ok(v) = &r {
        vm.set_global("goCurrentModule", v.clone());
    }
    rest.push(("new(script \"Home Module\")", r));
    let began = player::stage::begin_frame(vm, host, 1).map(|v| Value::Int(v.len() as i64));
    let began_ok = began.is_ok();
    rest.push(("beginSprite", began));
    for ev in ["prepareFrame", "startMovie", "enterFrame", "exitFrame"] {
        if !began_ok {
            break;
        }
        let r = if ev == "startMovie" {
            vm.movie_event(host, ev, vec![]).map(|_| Value::Void)
        } else {
            player::stage::frame_event(vm, host, ev).map(|_| Value::Void)
        };
        let stop = r.is_err();
        rest.push((ev, r));
        if stop {
            break;
        }
    }
    (failures, rest)
}

fn home_managers_build_inner() {
    let Some((mut vm, mut host)) = load("Home.dxr") else {
        return;
    };
    let (failures, rest) = home_component(&mut vm, &mut host);
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

/// Home's side navigation, module by module: a fresh Home (component
/// startup, playback), a click on the module, `go to movie` followed, the
/// module's startup and a second of playback. Prints where each stops.
#[test]
fn home_modules() {
    big_stack(home_modules_inner);
}

/// Home at frame 2, ready for the mouse.
fn home_ready() -> Option<(Vm, PlayerHost)> {
    let (mut vm, mut host) = load("Home.dxr")?;
    host.fonts = Box::new(fonts::HostFonts::new());
    let (failures, rest) = home_component(&mut vm, &mut host);
    assert!(failures.is_empty() && rest.iter().all(|(_, r)| r.is_ok()));
    for _ in 0..5 {
        player::stage::step(&mut vm, &mut host, 66).expect("Home plays");
    }
    Some((vm, host))
}

fn home_modules_inner() {
    use player::stage::{mouse, Mouse};
    let Some((mut vm, mut host)) = home_ready() else {
        return;
    };
    // The side navigation: which sprite is under the mouse down the column.
    let x = std::env::var("ILUMINA_NAV_X")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(980);
    let mut runs: Vec<(u16, i64, i64)> = Vec::new();
    for y in (140..=560).step_by(6) {
        mouse(&mut vm, &mut host, Mouse::Move(x, y)).expect("move");
        let s = host.mouse_over;
        match runs.last_mut() {
            Some((o, _, end)) if *o == s => *end = y,
            _ => runs.push((s, y, y)),
        }
    }
    eprintln!("column x={x}: {runs:?}");
    drop((vm, host));
    // One fresh Home per module (Bíblia … Extras, then the add-on list).
    let ys: Vec<i64> = std::env::var("ILUMINA_NAV_YS")
        .ok()
        .map(|v| v.split(',').filter_map(|y| y.parse().ok()).collect())
        .unwrap_or_else(|| vec![190, 248, 302, 356, 418, 500]);
    let mut report = Vec::new();
    for y in ys {
        let (mut vm, mut host) = home_ready().unwrap();
        let r = click_into_module(&mut vm, &mut host, x, y);
        eprintln!("module at y={y}: {r}");
        report.push((y, r));
    }
    for (y, r) in &report {
        eprintln!("  y={y}: {r}");
    }
    // Ratchet (2026-11-01): every module of the side navigation starts and
    // plays a second. Regressions fixed on the way: locVToLinePos, member
    // chunk refs, updateControl, quad, hexString, hyperlinks, #paragraph,
    // the TreeView Xtra, timeouts, member.member, scrollTop.
    if std::env::var("ILUMINA_NAV_YS").is_err() {
        let names = [
            "Bible",
            "Encyclopedia",
            "TimeTravel",
            "MediaCenter",
            "Extras",
            "MyStudyCenter",
        ];
        for ((_, r), name) in report.iter().zip(names) {
            assert!(r.starts_with(&format!("{name} OK")), "{name}: {r}");
        }
    }
}

/// Click at (x, y) on Home, follow `go to movie`, start the module and play
/// a second: a one-line outcome.
fn click_into_module(vm: &mut Vm, host: &mut PlayerHost, x: i64, y: i64) -> String {
    use player::stage::{mouse, step, Mouse};
    for ev in [Mouse::Move(x, y), Mouse::Down(x, y), Mouse::Up(x, y)] {
        if let Err(e) = mouse(vm, host, ev) {
            return format!("{ev:?} stopped: {e}");
        }
    }
    for _ in 0..3 {
        if let Err(e) = step(vm, host, 66) {
            return format!("Home step stopped: {e}");
        }
        if host.go_movie.is_some() {
            break;
        }
    }
    let Some((movie, _)) = host.go_movie.clone() else {
        return format!(
            "no go to movie (frame {}, over sprite {})",
            host.frame, host.mouse_over
        );
    };
    let file = format!("{movie}.dxr");
    let Some(bytes) = host
        .vfs
        .read(&format!("{}Files\\{file}", player::vfs::APP_DIR))
    else {
        return format!("{movie}: movie file not found");
    };
    let casts = player::cast_files(&bytes, &file).unwrap();
    if let Err(e) = player::switch_movie(vm, host, &file, &casts) {
        return format!("{movie}: switch failed: {e}");
    }
    host.go_movie = None;
    for (ev, r) in player::stage::start_movie(vm, host) {
        if let Err(e) = r {
            return format!("{movie} {ev} stopped: {e}");
        }
    }
    let mut frames = Vec::new();
    for _ in 0..15 {
        if let Err(e) = step(vm, host, 66) {
            return format!("{movie} playback stopped at frame {}: {e}", host.frame);
        }
        frames.push(host.frame);
        if let Some((m, _)) = &host.go_movie {
            return format!("{movie} plays, then goes to movie {m}");
        }
    }
    frames.dedup();
    // ILUMINA_TREE_DUMP: the TreeView trees as built (local; content).
    if std::env::var("ILUMINA_TREE_DUMP").is_ok() {
        for (n, t) in &host.trees {
            let st = &host.sprites[n];
            eprintln!(
                "tree sprite {n} loc {:?} {}x{} spacing {} fonts {:?} icons {:?}",
                st.loc, st.width, st.height, t.spacing, t.fonts, t.icons
            );
            let mut keys: Vec<_> = t.settings.iter().collect();
            keys.sort_by(|a, b| a.0.cmp(b.0));
            for (k, v) in keys {
                eprintln!("  set {k} {v:?}");
            }
            let r = &t.nodes[1];
            eprintln!(
                "  root {:?} expanded {} icons {:?} font {} kids {}",
                r.name,
                r.expanded,
                r.icons,
                r.font,
                r.children.len()
            );
            for id in t.visible().into_iter().take(6) {
                let nd = &t.nodes[id as usize];
                eprintln!(
                    "  node {id} {:?} kids {} icons {:?} font {} color {:?} user {:?}",
                    nd.name,
                    nd.children.len(),
                    nd.icons,
                    nd.font,
                    nd.color,
                    nd.user_strings
                );
                for &c in nd.children.iter().take(2) {
                    let cn = &t.nodes[c as usize];
                    eprintln!(
                        "    child {c} {:?} kids {} icons {:?} user {:?}",
                        cn.name,
                        cn.children.len(),
                        cn.icons,
                        cn.user_strings
                    );
                }
            }
        }
    }
    let trees: Vec<usize> = host.trees.values().map(|t| t.nodes.len() - 2).collect();
    format!(
        "{movie} OK, frames {frames:?}, dialogs {}, tree nodes {trees:?}, unanswered {:?}",
        host.dialogs_shown.len(),
        host.missing.keys().collect::<Vec<_>>()
    )
}

/// M3: the Bible's book tree. Home → Bible, click "Gênesis" in the tree
/// (the TreeView's first row), play on: where the run goes.
#[test]
fn bible_book_tree() {
    big_stack(bible_book_tree_inner);
}

fn bible_book_tree_inner() {
    use player::stage::{mouse, step, Mouse};
    let Some((mut vm, mut host)) = home_ready() else {
        return;
    };
    let r = click_into_module(&mut vm, &mut host, 980, 190);
    assert!(r.starts_with("Bible OK"), "{r}");
    let (&n, tree) = host.trees.iter().next().expect("the book tree");
    let rh = tree.row_height(host.fonts.as_ref());
    let st = host.sprites[&n].clone();
    if let Ok(out) = std::env::var("ILUMINA_RENDER_BIBLE") {
        render_png(&host, "Bible.dxr", Path::new(&out));
    }
    // The second row (under the named root; the sprite's loc is its centre).
    let (x, y) = (
        st.loc.0 - st.width / 2 + 40,
        st.loc.1 - st.height / 2 + rh + rh / 2,
    );
    let mut log = Vec::new();
    for ev in [Mouse::Move(x, y), Mouse::Down(x, y), Mouse::Up(x, y)] {
        log.push(format!(
            "{ev:?}: {:?}",
            mouse(&mut vm, &mut host, ev).map_err(|e| e.message)
        ));
    }
    eprintln!(
        "gsTreeLink after click: {:?}",
        vm.global("gsTreeLink").to_text()
    );
    let alerts_before = host.alerts.len();
    if std::env::var("ILUMINA_CALL_LOG").is_ok() {
        vm.call_log = Some(Vec::new());
    }
    let mut frames = Vec::new();
    let mut stop = None;
    for _ in 0..30 {
        if let Err(e) = step(&mut vm, &mut host, 66) {
            stop = Some(e.to_string());
            break;
        }
        frames.push(host.frame);
    }
    frames.dedup();
    if let Some(log) = vm.call_log.take() {
        for l in log
            .iter()
            .filter(|l| !l.contains("exitFrame") || l.trim_start().len() == l.len())
            .take(150)
        {
            eprintln!("call {l}");
        }
    }
    let label = host
        .score
        .as_ref()
        .and_then(|s| s.labels.iter().rfind(|(f, _)| *f <= host.frame).cloned());
    eprintln!("tree click: {log:?}");
    eprintln!(
        "after the click: frames {frames:?}, label {label:?}, stop {stop:?}, selected {}",
        host.trees[&n].selected
    );
    eprintln!("unanswered: {:?}", host.missing.keys().collect::<Vec<_>>());
    eprintln!(
        "gsTreeLink after frames: {:?}; alerts {:?}; go_movie {:?}",
        vm.global("gsTreeLink").to_text(),
        &host.alerts[alerts_before..],
        host.go_movie
    );
    if std::env::var("ILUMINA_TEXT_SPRITES").is_ok() {
        let mut ns: Vec<_> = host.sprites.keys().copied().collect();
        ns.sort();
        for k in ns {
            let st = &host.sprites[&k];
            if let Some((l, m)) = st.member {
                if let Some(mi) = host.member_info(l, m) {
                    if let Some(t) = &mi.text {
                        eprintln!(
                            "text sprite {k}: {} {l}:{m} loc {:?} {}x{} str {} vis {} {:?}",
                            mi.kind,
                            st.loc,
                            st.width,
                            st.height,
                            st.stretched,
                            st.visible,
                            t.chars().take(24).collect::<String>()
                        );
                    }
                }
            }
        }
    }
    if let Ok(out) = std::env::var("ILUMINA_RENDER_BIBLE_CLICK") {
        render_png(&host, "Bible.dxr", Path::new(&out));
    }
    assert!(log.iter().all(|l| l.ends_with("Ok(())")), "{log:?}");
    assert_eq!(host.trees[&n].selected, 2, "the click selects Gênesis");
    // Ratchet (2026-11-01): the click opens the book's introduction.
    let label = host
        .score
        .as_ref()
        .and_then(|s| s.labels.iter().rfind(|(f, _)| *f <= host.frame).cloned());
    assert_eq!(label.map(|l| l.1).as_deref(), Some("BookIntro"));

    // The first hyperlink of the introduction's body (a verse reference):
    // click it and see where the run goes.
    use lingo_vm::Host;
    let candidates: Vec<(u16, player::stage::SpriteState)> = host
        .sprites
        .iter()
        .filter(|(_, st)| st.visible && st.loc.1 > 150 && st.width > 300 && st.member.is_some())
        .map(|(&k, st)| (k, st.clone()))
        .collect();
    let mut body = None;
    for (k, st) in candidates {
        let (l, m) = st.member.unwrap();
        let r = lingo_vm::HostRef::Member { lib: l, number: m };
        if let Ok(Some(Value::List(v))) = host.get_prop(&mut vm, &r, "hyperlinks") {
            if let Some(first) = v.borrow().first().cloned() {
                body = Some((k, st, (l, m), first));
                break;
            }
        }
    }
    let Some((k, st, (l, m), first)) = body else {
        panic!("no body text with hyperlinks")
    };
    let a = match &first {
        Value::List(r) => r.borrow()[0].to_int(),
        _ => 1,
    };
    let (ms, _) = host.measure(l, m).unwrap();
    let (cx, cy) = ms.char_loc(a as usize);
    let (x, y) = (st.loc.0 + cx as i64 + 3, st.loc.1 + cy as i64 - 4);
    if std::env::var("ILUMINA_CALL_LOG").is_ok() {
        vm.call_log = Some(Vec::new());
    }
    let mut log = Vec::new();
    for ev in [Mouse::Move(x, y), Mouse::Down(x, y), Mouse::Up(x, y)] {
        log.push(format!(
            "{ev:?}: {:?}",
            mouse(&mut vm, &mut host, ev).map_err(|e| e.message)
        ));
    }
    let mut frames = Vec::new();
    let mut stop = None;
    for _ in 0..30 {
        if let Err(e) = step(&mut vm, &mut host, 66) {
            stop = Some(e.to_string());
            break;
        }
        frames.push(host.frame);
    }
    frames.dedup();
    if let Some(cl) = vm.call_log.take() {
        for l in cl
            .iter()
            .filter(|l| {
                !l.contains("exitFrame")
                    && !l.contains("stepFrame")
                    && !l.contains("CharToUpper")
                    && !l.contains("Scroll")
            })
            .take(
                std::env::var("ILUMINA_CALL_LOG_N")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(80),
            )
        {
            eprintln!("link call {l}");
        }
    }
    let label = host
        .score
        .as_ref()
        .and_then(|s| s.labels.iter().rfind(|(f, _)| *f <= host.frame).cloned());
    eprintln!("link click on sprite {k} at ({x}, {y}): {log:?}; frames {frames:?}, label {label:?}, stop {stop:?}");
    eprintln!("unanswered: {:?}", host.missing.keys().collect::<Vec<_>>());
    if std::env::var("ILUMINA_XTRA_SPRITES").is_ok() {
        let mut ns: Vec<_> = host.sprites.keys().copied().collect();
        ns.sort();
        for k in ns {
            let st = &host.sprites[&k];
            let Some((l, m)) = st.member else { continue };
            let Some(mi) = host.member_info(l, m) else {
                continue;
            };
            if mi.kind == "xtra" || !st.xtra_props.is_empty() {
                eprintln!(
                    "xtra sprite {k} {l}:{m} {:?} loc {:?} {}x{} vis {} props {:?}",
                    mi.name,
                    st.loc,
                    st.width,
                    st.height,
                    st.visible,
                    st.xtra_props
                        .iter()
                        .map(|(k, v)| format!(
                            "{k}={}",
                            v.to_string().chars().take(60).collect::<String>()
                        ))
                        .collect::<Vec<_>>()
                );
                if let Some((_, data, first)) =
                    host.cast_data.iter().find(|(lib, _, _)| *lib as i64 == l)
                {
                    let mv = rifx::Movie::parse(data).unwrap();
                    let ms = rifx::cast::members_from(&mv, *first).unwrap();
                    if let Some(mm) = ms.iter().flatten().find(|x| x.number as i64 == m) {
                        let hex: String = mm
                            .specific
                            .iter()
                            .take(64)
                            .map(|b| format!("{b:02x}"))
                            .collect();
                        eprintln!(
                            "  member {:?} xtra {:?} specific {} bytes: {hex}",
                            mm.kind,
                            mm.xtra,
                            mm.specific.len()
                        );
                    }
                }
                if let Some(sc) = &st.score {
                    eprintln!(
                        "  score left {} top {} {}x{} ink {}",
                        sc.left, sc.top, sc.width, sc.height, sc.ink
                    );
                }
                if let Some(t) = host.trees.get(&k) {
                    let r = &t.nodes[1];
                    eprintln!("  tree: root {:?} expanded {} visible {} nodes {} selected {} settings {:?}", r.name, r.expanded, t.visible().len(), t.nodes.len(), t.selected, t.settings.keys().collect::<Vec<_>>());
                    for id in t.visible().into_iter().take(4) {
                        eprintln!(
                            "    row {id} {:?} kids {}",
                            t.nodes[id as usize].name,
                            t.nodes[id as usize].children.len()
                        );
                    }
                }
            }
        }
    }
    if let Ok(out) = std::env::var("ILUMINA_RENDER_BIBLE_LINK") {
        render_png(&host, "Bible.dxr", Path::new(&out));
    }
    // Ratchet (2026-11-01): the verse reference opens the chapter view and
    // the chapter arrives as RTF from the database (no content checked).
    assert!(stop.is_none(), "{stop:?}");
    assert_eq!(label.map(|l| l.1).as_deref(), Some("ViewSingle"));
    let chapter = host
        .members
        .iter()
        .find(|m| m.style.iter().any(|(k, _)| k == "rtf"))
        .and_then(|m| m.text.clone())
        .unwrap_or_default();
    assert!(
        chapter.chars().count() > 1000,
        "chapter text: {} chars",
        chapter.chars().count()
    );
}
