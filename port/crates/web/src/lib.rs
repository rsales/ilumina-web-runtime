//! Browser player (ADR 0008, M1): the `director` stage behind a small,
//! hand-written wasm ABI, so the only build requirement is the
//! `wasm32-unknown-unknown` target (no wasm-bindgen, no extra CLI).
//!
//! The page (`port/web/app.js`) fetches files from the local server, hands
//! them over with `file_add` (each one goes through the hash gate), asks
//! which external casts a movie needs (`needed_files`), loads it and renders
//! frames into an RGBA buffer it copies to a `<canvas>`.
//!
//! Fonts come from the browser: `js_glyph`/`js_line` (imported from `env`)
//! draw one character on a canvas and return its coverage and metrics.
//!
//! All logic lives in [`App`], which is tested natively; the `extern "C"`
//! layer at the bottom only moves bytes.

use director::{Files, Fonts, Glyph, LineMetrics, Stage};
use rifx::text::TextStyle;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

/// Folder of the movies inside the disc layout.
pub const MOVIE_DIR: &str = "cd1/Program/Files";
/// The projector, which holds the "System - Win" palette.
pub const PROJECTOR: &str = "cd1/Program/iLumina.exe";

/// Verified files by disc path, shared with the player's virtual FS.
type FileMap = Rc<RefCell<HashMap<String, Vec<u8>>>>;

/// Player state: verified files, the loaded stage, the Lingo run (when
/// playing) and the last frame.
pub struct App {
    manifest: Vec<discs::Entry>,
    files: FileMap,
    stage: Option<Stage>,
    /// The movie running its original Lingo (`play`).
    run: Option<(lingo_vm::Vm, player::PlayerHost)>,
    /// Disc files the run asked for that the page has not added yet.
    missing: Rc<RefCell<BTreeSet<String>>>,
    /// The owner's registration data, typed into the page (never generated,
    /// ADR 0005): name, e-mail, CEP.
    owner: Option<(String, String, String)>,
    /// The session's events since the startup, so a re-run (after
    /// fetching files or answering a dialog) replays the same session.
    log: Vec<Ev>,
    /// The page's dialog answers, given in order on each run.
    answers: Vec<Vec<player::mui::DialogAction>>,
    /// The movie the session started with, and the one on the stage.
    start: Option<String>,
    movie: String,
    /// The startup completed, so playback may continue.
    started: bool,
    pub frame: Vec<u8>,
}

/// One recorded event of a session.
#[derive(Clone, Debug, PartialEq)]
enum Ev {
    Step(i64),
    Mouse(player::stage::Mouse),
    /// `go to movie` (disc path).
    Movie(String),
}

/// The player's view of the disc: the files the page added; a file not
/// added yet is recorded as missing (the page fetches it and runs again).
struct PageDisc {
    files: FileMap,
    missing: Rc<RefCell<BTreeSet<String>>>,
}

impl player::DiscFiles for PageDisc {
    fn read(&self, rel: &str) -> Option<Vec<u8>> {
        let f = self.files.borrow();
        match f.get(rel) {
            Some(d) => Some(d.clone()),
            None => {
                self.missing.borrow_mut().insert(rel.to_string());
                None
            }
        }
    }
}

/// Result of [`App::play_step`].
#[derive(Debug, PartialEq)]
pub enum Step {
    /// The playhead is on this frame.
    Frame(usize),
    /// Add these files, then `play` again (the session is replayed).
    NeedFiles(Vec<String>),
    /// A dialog waits for the user (lines, see `PendingDialog::to_lines`):
    /// `answer`, then `play` again.
    Dialog(String),
    /// Lingo went to another movie (disc path), now on the stage.
    Movie(String),
}

/// Result of [`App::play`].
#[derive(Debug, PartialEq)]
pub enum Play {
    /// The run read these files, which the page must add before running
    /// again (the run is repeated from the start: Lingo is deterministic).
    NeedFiles(Vec<String>),
    /// The startup ran: one line per step ("prepareMovie: OK" or the stop).
    Ran(Vec<String>),
    /// A dialog waits for the user (see [`Step::Dialog`]).
    Dialog(String),
}

/// `Files` view of the verified files for one movie folder.
struct Folder<'a> {
    files: &'a HashMap<String, Vec<u8>>,
    dir: &'a str,
}

impl Files for Folder<'_> {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        let want = format!("{}/{name}", self.dir).to_ascii_lowercase();
        self.files
            .iter()
            .find(|(k, _)| k.to_ascii_lowercase() == want)
            .map(|(_, v)| v.clone())
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        App {
            manifest: discs::parse_manifest(discs::ILUMINA_GOLD_250_PTBR).unwrap_or_default(),
            files: Rc::new(RefCell::new(HashMap::new())),
            stage: None,
            run: None,
            missing: Default::default(),
            owner: None,
            log: Vec::new(),
            answers: Vec::new(),
            start: None,
            movie: String::new(),
            started: false,
            frame: Vec::new(),
        }
    }

    /// Test hook: use another manifest (synthetic files).
    pub fn with_manifest(manifest: Vec<discs::Entry>) -> Self {
        App {
            manifest,
            ..Self::new()
        }
    }

    /// Add a file by its path in the disc layout (`cd1/Program/Files/Home.dxr`).
    /// Unknown or modified files are refused.
    pub fn add_file(&mut self, rel: &str, data: Vec<u8>) -> Result<(), String> {
        discs::check_bytes(rel, &data, &self.manifest)?;
        self.files.borrow_mut().insert(rel.to_string(), data);
        Ok(())
    }

    /// Disc paths of the external casts that `movie` (a disc path) uses.
    pub fn needed_files(&self, movie: &str) -> Result<Vec<String>, String> {
        let files = self.files.borrow();
        let data = files
            .get(movie)
            .ok_or_else(|| format!("{movie}: not added yet"))?;
        let m = rifx::Movie::parse(data).map_err(|e| format!("{movie}: {e}"))?;
        let dir = movie.rsplit_once('/').map(|p| p.0).unwrap_or("");
        let mut out = Vec::new();
        for lib in rifx::score::cast_libs(&m).map_err(|e| format!("{movie}: {e}"))? {
            if let Some(name) = director::cast_file_name(&lib.path) {
                // Use the manifest spelling (the cast list may differ in case).
                let want = format!("{dir}/{name}").to_ascii_lowercase();
                let rel = self
                    .manifest
                    .iter()
                    .find(|e| e.path.to_ascii_lowercase() == want)
                    .map(|e| e.path.clone())
                    .unwrap_or(format!("{dir}/{name}"));
                if !out.contains(&rel) {
                    out.push(rel);
                }
            }
        }
        Ok(out)
    }

    /// Load `movie` (a disc path) and its casts from the added files.
    pub fn load(&mut self, movie: &str) -> Result<(), String> {
        let files = self.files.borrow();
        let exe = files
            .get(PROJECTOR)
            .ok_or_else(|| format!("{PROJECTOR}: not added yet"))?;
        let palette = rifx::bitmap::system_win_palette(exe)
            .ok_or("System - Win palette not found in the projector")?;
        let (dir, name) = movie.rsplit_once('/').unwrap_or(("", movie));
        let folder = Folder { files: &files, dir };
        let stage = Stage::load(&folder, name, palette).map_err(|e| e.to_string())?;
        drop(files);
        self.stage = Some(stage);
        self.movie = movie.to_string();
        self.run = None;
        self.start = None;
        self.log.clear();
        self.answers.clear();
        Ok(())
    }

    /// Set (or clear, with empty values) the owner's registration data.
    pub fn set_owner(&mut self, name: &str, email: &str, cep: &str) {
        self.owner = (!name.is_empty() && !email.is_empty())
            .then(|| (name.to_string(), email.to_string(), cep.to_string()));
    }

    /// Run `movie`'s startup with its original Lingo, from the start, then
    /// replay this session's events (frames, mouse, movie changes; dialog
    /// answers are given in order). The stage must be loaded (`load`).
    pub fn play(&mut self, movie: &str) -> Result<Play, String> {
        self.stage()?;
        if self.start.as_deref() != Some(movie) {
            self.start = Some(movie.to_string());
            self.log.clear();
            self.answers.clear();
        }
        self.missing.borrow_mut().clear();
        let disc = PageDisc {
            files: self.files.clone(),
            missing: self.missing.clone(),
        };
        let vfs = player::Vfs::new(Box::new(disc), self.manifest.iter().map(|e| e.path.clone()));
        let (_, name) = movie.rsplit_once('/').unwrap_or(("", movie));
        let bytes = self
            .files
            .borrow()
            .get(movie)
            .cloned()
            .ok_or_else(|| format!("{movie}: not added yet"))?;
        let casts = player::cast_files(&bytes, name)?;
        let (mut vm, mut host) = player::load_movie(vfs, name, &casts)?;
        host.fonts = Box::new(PageFonts::default());
        host.responder = Box::new(player::mui::PageUser {
            owner: self.owner.as_ref().map(|(n, e, c)| player::mui::Owner {
                name: n.clone(),
                email: e.clone(),
                cep: c.clone(),
            }),
            answers: self.answers.iter().cloned().collect(),
        });
        let steps = player::stage::start_movie(&mut vm, &mut host);
        self.started = steps.iter().all(|(_, r)| r.is_ok());
        let mut lines: Vec<String> = steps
            .iter()
            .map(|(step, r)| match r {
                Ok(()) => format!("{step}: OK"),
                Err(e) => format!("{step}: {}", self.masked(&e.message)),
            })
            .collect();
        let mut current = movie.to_string();
        let mut outcome = None;
        if self.started {
            for ev in self.log.clone() {
                let r = match &ev {
                    Ev::Step(ms) => {
                        player::stage::step(&mut vm, &mut host, *ms).map_err(|e| e.message)
                    }
                    Ev::Mouse(m) => {
                        player::stage::mouse(&mut vm, &mut host, *m).map_err(|e| e.message)
                    }
                    Ev::Movie(m) => {
                        current = m.clone();
                        self.enter_movie(&mut vm, &mut host, m).map(|_| ())
                    }
                };
                if let Err(e) = r {
                    outcome = Some(e);
                    break;
                }
            }
        }
        let missing = self.missing();
        if !missing.is_empty() {
            return Ok(Play::NeedFiles(missing));
        }
        if let Some(d) = host.pending_dialog.take() {
            self.run = Some((vm, host));
            return Ok(Play::Dialog(d.to_lines()));
        }
        if let Some(e) = outcome {
            lines.push(format!("playback: {}", self.masked(&e)));
            self.started = false;
        }
        if current != self.movie {
            self.load_stage(&current)?;
        }
        self.run = Some((vm, host));
        Ok(Play::Ran(lines))
    }

    fn masked(&self, text: &str) -> String {
        let mut m = text.to_string();
        if let Some((n, e, c)) = &self.owner {
            for s in [n, e, c].into_iter().filter(|s| !s.is_empty()) {
                m = m.replace(s.as_str(), "<owner>");
            }
        }
        m
    }

    /// `go to movie` inside the run: switch the player and start the movie.
    /// Errors when files are missing (the page fetches and replays).
    fn enter_movie(
        &self,
        vm: &mut lingo_vm::Vm,
        host: &mut player::PlayerHost,
        movie: &str,
    ) -> Result<(), String> {
        let need = self.files_for(movie);
        if !need.is_empty() {
            self.missing.borrow_mut().extend(need);
            return Err(format!("{movie}: files not added yet"));
        }
        let bytes = self.files.borrow().get(movie).cloned().unwrap_or_default();
        let (_, name) = movie.rsplit_once('/').unwrap_or(("", movie));
        let casts = player::cast_files(&bytes, name)?;
        player::switch_movie(vm, host, name, &casts)?;
        for (step, r) in player::stage::start_movie(vm, host) {
            r.map_err(|e| format!("{name} {step}: {}", e.message))?;
        }
        Ok(())
    }

    /// The movie and the casts it needs that the page has not added.
    fn files_for(&self, movie: &str) -> Vec<String> {
        if !self.files.borrow().contains_key(movie) {
            return vec![movie.to_string()];
        }
        let casts = self.needed_files(movie).unwrap_or_default();
        let files = self.files.borrow();
        casts
            .into_iter()
            .filter(|c| !files.contains_key(c))
            .collect()
    }

    fn load_stage(&mut self, movie: &str) -> Result<(), String> {
        let files = self.files.borrow();
        let exe = files
            .get(PROJECTOR)
            .ok_or_else(|| format!("{PROJECTOR}: not added yet"))?;
        let palette = rifx::bitmap::system_win_palette(exe)
            .ok_or("System - Win palette not found in the projector")?;
        let (dir, name) = movie.rsplit_once('/').unwrap_or(("", movie));
        let folder = Folder { files: &files, dir };
        let stage = Stage::load(&folder, name, palette).map_err(|e| e.to_string())?;
        drop(files);
        self.stage = Some(stage);
        self.movie = movie.to_string();
        Ok(())
    }

    /// Run one event of the session (recorded for replays) and report what
    /// the page must do next.
    fn event(&mut self, ev: Ev) -> Result<Step, String> {
        self.missing.borrow_mut().clear();
        if !self.started {
            return Err("the startup did not complete".into());
        }
        let Some((mut vm, mut host)) = self.run.take() else {
            return Err("not playing".into());
        };
        self.log.push(ev.clone());
        let mut r = match ev {
            Ev::Step(ms) => player::stage::step(&mut vm, &mut host, ms).map_err(|e| e.message),
            Ev::Mouse(m) => player::stage::mouse(&mut vm, &mut host, m).map_err(|e| e.message),
            Ev::Movie(_) => Ok(()),
        };
        // `go to movie`: follow it now (recorded, so replays do the same).
        let mut switched = None;
        if r.is_ok() {
            if let Some((name, _)) = host.go_movie.take() {
                let path = self.movie_path(&name);
                self.log.push(Ev::Movie(path.clone()));
                r = self.enter_movie(&mut vm, &mut host, &path);
                switched = Some(path);
            }
        }
        let missing = self.missing();
        let pending = host.pending_dialog.take();
        let frame = host.frame;
        self.run = Some((vm, host));
        if !missing.is_empty() {
            return Ok(Step::NeedFiles(missing));
        }
        if let Some(d) = pending {
            return Ok(Step::Dialog(d.to_lines()));
        }
        if let Err(e) = r {
            self.started = false;
            return Err(self.masked(&e));
        }
        if let Some(path) = switched {
            self.load_stage(&path)?;
            return Ok(Step::Movie(path));
        }
        Ok(Step::Frame(frame))
    }

    /// The disc path of a movie named by Lingo (`"Bible"`, `"Bible.dxr"`).
    fn movie_path(&self, name: &str) -> String {
        let base = name.rsplit(['\\', '/', ':']).next().unwrap_or(name);
        let file = if base.to_ascii_lowercase().ends_with(".dxr") {
            base.to_string()
        } else {
            format!("{base}.dxr")
        };
        let want = format!("{MOVIE_DIR}/{file}").to_ascii_lowercase();
        self.manifest
            .iter()
            .find(|e| e.path.to_ascii_lowercase() == want)
            .map(|e| e.path.clone())
            .unwrap_or(format!("{MOVIE_DIR}/{file}"))
    }

    /// One frame of playback (`elapsed_ms` since the last).
    pub fn play_step(&mut self, elapsed_ms: i64) -> Result<Step, String> {
        self.event(Ev::Step(elapsed_ms))
    }

    /// A mouse event at stage coordinates.
    pub fn play_mouse(&mut self, m: player::stage::Mouse) -> Result<Step, String> {
        self.event(Ev::Mouse(m))
    }

    /// The page's answer to the waiting dialog (see
    /// [`player::mui::parse_actions`]); then `play` again to replay.
    pub fn answer(&mut self, actions: &str) {
        self.answers.push(player::mui::parse_actions(actions));
    }

    /// The running movie's current frame (0 when not playing).
    pub fn play_frame(&self) -> usize {
        self.run.as_ref().map(|r| r.1.frame).unwrap_or(0)
    }

    /// The movie on the stage (disc path).
    pub fn movie(&self) -> &str {
        &self.movie
    }

    /// Draw the stage as the running Lingo left it into `self.frame`.
    /// Returns the number of sprites drawn; linked media not added yet are
    /// listed by [`App::missing`] (add them and draw again).
    pub fn play_render(&mut self, fonts: &dyn Fonts) -> Result<usize, String> {
        self.missing.borrow_mut().clear();
        let (_, host) = self.run.as_ref().ok_or("not playing")?;
        let draws = host.draws();
        // Linked media that failed to read are retried after the page adds them.
        if !self.missing.borrow().is_empty() {
            host.images.borrow_mut().clear();
        }
        let (px, report) = self
            .stage()?
            .render_draws(&draws, fonts)
            .map_err(|e| e.to_string())?;
        self.frame = px;
        Ok(report.drawn.len())
    }

    /// Disc files the last `play`/`play_render` needed but did not have.
    pub fn missing(&self) -> Vec<String> {
        self.missing.borrow().iter().cloned().collect()
    }

    fn stage(&self) -> Result<&Stage, String> {
        self.stage
            .as_ref()
            .ok_or_else(|| "no movie loaded".to_string())
    }

    pub fn size(&self) -> (u32, u32) {
        self.stage
            .as_ref()
            .map(|s| (s.config.width as u32, s.config.height as u32))
            .unwrap_or((0, 0))
    }

    pub fn frame_count(&self) -> usize {
        self.stage
            .as_ref()
            .and_then(|s| s.score.as_ref())
            .map(|s| s.frame_count)
            .unwrap_or(0)
    }

    /// Render `frame` (1-based) into `self.frame` (RGBA). Returns the
    /// number of sprites drawn.
    pub fn render(&mut self, frame: usize, fonts: &dyn Fonts) -> Result<usize, String> {
        let (px, report) = self
            .stage()?
            .render(frame, fonts)
            .map_err(|e| e.to_string())?;
        self.frame = px;
        Ok(report.drawn.len())
    }
}

/// Font style flags passed to the page: bit 0 bold, bit 1 italic.
pub fn style_flags(st: &TextStyle) -> u32 {
    st.bold as u32 | (st.italic as u32) << 1
}

/// [`Fonts`] backed by the page, with caches (each glyph crosses the
/// boundary once).
#[derive(Default)]
pub struct PageFonts {
    glyphs: RefCell<HashMap<(String, u16, u32, char), Glyph>>,
    lines: RefCell<HashMap<(String, u16, u32), LineMetrics>>,
}

/// Largest glyph bitmap the page may return (side, in pixels).
pub const GLYPH_MAX: usize = 256;

impl Fonts for PageFonts {
    fn line(&self, st: &TextStyle) -> LineMetrics {
        let key = (st.font.clone(), st.size, style_flags(st));
        if let Some(m) = self.lines.borrow().get(&key) {
            return *m;
        }
        let mut out = [0i32; 3];
        ffi::line(&st.font, st.size as u32, style_flags(st), &mut out);
        let m = if out == [0, 0, 0] {
            let px = st.size as f32;
            LineMetrics {
                ascent: px * 0.8,
                descent: px * 0.2,
                gap: 0.0,
            }
        } else {
            LineMetrics {
                ascent: out[0] as f32 / 64.0,
                descent: out[1] as f32 / 64.0,
                gap: out[2] as f32 / 64.0,
            }
        };
        self.lines.borrow_mut().insert(key, m);
        m
    }

    fn glyph(&self, st: &TextStyle, ch: char) -> Glyph {
        let key = (st.font.clone(), st.size, style_flags(st), ch);
        if let Some(g) = self.glyphs.borrow().get(&key) {
            return g.clone();
        }
        let mut out = [0i32; 5];
        let mut buf = vec![0u8; GLYPH_MAX * GLYPH_MAX];
        ffi::glyph(
            &st.font,
            st.size as u32,
            style_flags(st),
            ch,
            &mut out,
            &mut buf,
        );
        let (w, h) = (out[0].max(0) as usize, out[1].max(0) as usize);
        let g = if w * h <= buf.len() {
            buf.truncate(w * h);
            Glyph {
                width: w,
                height: h,
                xmin: out[2],
                ymin: out[3],
                advance: out[4] as f32 / 64.0,
                coverage: buf,
            }
        } else {
            Glyph {
                advance: out[4] as f32 / 64.0,
                ..Default::default()
            }
        };
        self.glyphs.borrow_mut().insert(key, g.clone());
        g
    }
}

/// Calls into the page. Native builds (tests, `cargo build` of the
/// workspace) get stubs, so the cdylib links on every host.
mod ffi {
    #[cfg(target_arch = "wasm32")]
    #[link(wasm_import_module = "env")]
    extern "C" {
        /// out: width, height, xmin, ymin, advance × 64; coverage → buf.
        fn js_glyph(
            font: *const u8,
            font_len: usize,
            size: u32,
            flags: u32,
            ch: u32,
            out: *mut i32,
            buf: *mut u8,
            cap: usize,
        );
        /// out: ascent, descent, line gap, each × 64.
        fn js_line(font: *const u8, font_len: usize, size: u32, flags: u32, out: *mut i32);
    }

    #[cfg(target_arch = "wasm32")]
    pub fn glyph(font: &str, size: u32, flags: u32, ch: char, out: &mut [i32; 5], buf: &mut [u8]) {
        // SAFETY: the page writes at most 5 i32 to `out` and `cap` bytes to `buf`.
        unsafe {
            js_glyph(
                font.as_ptr(),
                font.len(),
                size,
                flags,
                ch as u32,
                out.as_mut_ptr(),
                buf.as_mut_ptr(),
                buf.len(),
            )
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn line(font: &str, size: u32, flags: u32, out: &mut [i32; 3]) {
        // SAFETY: the page writes at most 3 i32 to `out`.
        unsafe { js_line(font.as_ptr(), font.len(), size, flags, out.as_mut_ptr()) }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn glyph(_: &str, size: u32, _: u32, _: char, out: &mut [i32; 5], _: &mut [u8]) {
        out[4] = (size as i32 * 64) / 2;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn line(_: &str, _: u32, _: u32, _: &mut [i32; 3]) {}
}

// ---------------------------------------------------------------------------
// wasm ABI. Strings and byte arrays are (ptr, len) in linear memory; the page
// allocates with `alloc` and frees with `dealloc`. Functions return 0 (or a
// non-negative value) on success and -1 on error, with the message in the
// result buffer (`result_ptr`/`result_len`).

thread_local! {
    static APP: RefCell<App> = RefCell::new(App::new());
    static FONTS: PageFonts = PageFonts::default();
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn set_result(s: &str) {
    RESULT.with(|r| *r.borrow_mut() = s.as_bytes().to_vec());
}

fn status<T>(r: Result<T, String>, ok: impl FnOnce(T) -> i32) -> i32 {
    match r {
        Ok(v) => ok(v),
        Err(e) => {
            set_result(&e);
            -1
        }
    }
}

/// # Safety
/// `ptr`/`len` must describe memory obtained from `alloc` (or be valid UTF-8
/// bytes written by the page there).
unsafe fn str_arg<'a>(ptr: *const u8, len: usize) -> &'a str {
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).unwrap_or("")
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr` must come from `alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

#[no_mangle]
pub extern "C" fn result_ptr() -> *const u8 {
    RESULT.with(|r| r.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn result_len() -> usize {
    RESULT.with(|r| r.borrow().len())
}

/// Add a file; takes ownership of the data buffer (allocated with `alloc`).
///
/// # Safety
/// Pointers must come from `alloc` with the given lengths.
#[no_mangle]
pub unsafe extern "C" fn file_add(
    path: *const u8,
    path_len: usize,
    data: *mut u8,
    data_len: usize,
) -> i32 {
    let rel = str_arg(path, path_len).to_string();
    let bytes = Vec::from_raw_parts(data, data_len, data_len.max(1));
    status(APP.with(|a| a.borrow_mut().add_file(&rel, bytes)), |_| 0)
}

/// Needed cast files, newline-separated, in the result buffer.
///
/// # Safety
/// `movie`/`len` must be valid UTF-8 in linear memory.
#[no_mangle]
pub unsafe extern "C" fn needed_files(movie: *const u8, len: usize) -> i32 {
    let m = str_arg(movie, len).to_string();
    status(APP.with(|a| a.borrow().needed_files(&m)), |v| {
        set_result(&v.join("\n"));
        v.len() as i32
    })
}

/// # Safety
/// `movie`/`len` must be valid UTF-8 in linear memory.
#[no_mangle]
pub unsafe extern "C" fn load(movie: *const u8, len: usize) -> i32 {
    let m = str_arg(movie, len).to_string();
    status(APP.with(|a| a.borrow_mut().load(&m)), |_| 0)
}

#[no_mangle]
pub extern "C" fn stage_width() -> u32 {
    APP.with(|a| a.borrow().size().0)
}

#[no_mangle]
pub extern "C" fn stage_height() -> u32 {
    APP.with(|a| a.borrow().size().1)
}

#[no_mangle]
pub extern "C" fn frame_count() -> u32 {
    APP.with(|a| a.borrow().frame_count() as u32)
}

/// Render a frame; the RGBA pixels are at `frame_ptr` (width × height × 4).
#[no_mangle]
pub extern "C" fn render(frame: u32) -> i32 {
    FONTS.with(|f| {
        status(
            APP.with(|a| a.borrow_mut().render(frame as usize, f)),
            |n| n as i32,
        )
    })
}

/// Set the owner's registration data: "name\ne-mail\nCEP" (empty clears).
///
/// # Safety
/// `ptr`/`len` must be valid UTF-8 in linear memory.
#[no_mangle]
pub unsafe extern "C" fn set_owner(ptr: *const u8, len: usize) -> i32 {
    let s = str_arg(ptr, len);
    let mut it = s.split('\n');
    let (n, e, c) = (
        it.next().unwrap_or(""),
        it.next().unwrap_or(""),
        it.next().unwrap_or(""),
    );
    APP.with(|a| a.borrow_mut().set_owner(n.trim(), e.trim(), c.trim()));
    0
}

/// Result codes of `play`, `play_step` and `play_mouse` (details in the
/// result buffer): 0 ran (play: one line per startup step; step/mouse: the
/// frame), 1 add these files then `play` again (one path per line), 2 a
/// dialog waits (`PendingDialog` lines; `answer`, then `play` again), 3 the
/// run went to another movie (its disc path; it is on the stage now),
/// -1 error.
fn play_code(r: Result<Play, String>) -> i32 {
    status(r, |p| match p {
        Play::Ran(lines) => {
            set_result(&lines.join("\n"));
            0
        }
        Play::NeedFiles(v) => {
            set_result(&v.join("\n"));
            1
        }
        Play::Dialog(d) => {
            set_result(&d);
            2
        }
    })
}

fn step_code(r: Result<Step, String>) -> i32 {
    status(r, |s| match s {
        Step::Frame(f) => {
            set_result(&f.to_string());
            0
        }
        Step::NeedFiles(v) => {
            set_result(&v.join("\n"));
            1
        }
        Step::Dialog(d) => {
            set_result(&d);
            2
        }
        Step::Movie(m) => {
            set_result(&m);
            3
        }
    })
}

/// Run the loaded movie's startup with its Lingo and replay the session
/// (see `play_code`).
///
/// # Safety
/// `movie`/`len` must be valid UTF-8 in linear memory.
#[no_mangle]
pub unsafe extern "C" fn play(movie: *const u8, len: usize) -> i32 {
    let m = str_arg(movie, len).to_string();
    play_code(APP.with(|a| a.borrow_mut().play(&m)))
}

/// The page's answer to the waiting dialog: lines `set⇥pos⇥text`,
/// `click⇥pos`, `close`. Then call `play` again.
///
/// # Safety
/// `ptr`/`len` must be valid UTF-8 in linear memory.
#[no_mangle]
pub unsafe extern "C" fn answer(ptr: *const u8, len: usize) -> i32 {
    let s = str_arg(ptr, len).to_string();
    APP.with(|a| a.borrow_mut().answer(&s));
    0
}

/// Draw the running movie's stage (pixels at `frame_ptr`); returns the
/// sprites drawn. Then call `missing` for linked media to add.
#[no_mangle]
pub extern "C" fn play_render() -> i32 {
    FONTS.with(|f| status(APP.with(|a| a.borrow_mut().play_render(f)), |n| n as i32))
}

/// One frame of playback after `elapsed_ms` (see `play_code`).
#[no_mangle]
pub extern "C" fn play_step(elapsed_ms: u32) -> i32 {
    step_code(APP.with(|a| a.borrow_mut().play_step(elapsed_ms as i64)))
}

/// A mouse event at stage coordinates: kind 0 move, 1 down, 2 up.
#[no_mangle]
pub extern "C" fn play_mouse(kind: u32, x: i32, y: i32) -> i32 {
    use player::stage::Mouse;
    let (x, y) = (x as i64, y as i64);
    let m = match kind {
        1 => Mouse::Down(x, y),
        2 => Mouse::Up(x, y),
        _ => Mouse::Move(x, y),
    };
    step_code(APP.with(|a| a.borrow_mut().play_mouse(m)))
}

/// The disc path of the movie on the stage (in the result buffer).
#[no_mangle]
pub extern "C" fn current_movie() -> i32 {
    let m = APP.with(|a| a.borrow().movie().to_string());
    set_result(&m);
    0
}

/// The running movie's current frame.
#[no_mangle]
pub extern "C" fn play_frame() -> u32 {
    APP.with(|a| a.borrow().play_frame() as u32)
}

/// Files the last run or draw needed (one per line in the result buffer).
#[no_mangle]
pub extern "C" fn missing() -> i32 {
    let v = APP.with(|a| a.borrow().missing());
    set_result(&v.join("\n"));
    v.len() as i32
}

#[no_mangle]
pub extern "C" fn frame_ptr() -> *const u8 {
    APP.with(|a| a.borrow().frame.as_ptr())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, data: &[u8]) -> discs::Entry {
        let hex: String = discs::sha256(data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        discs::parse_manifest(&format!("{hex}  {path}\n"))
            .unwrap()
            .remove(0)
    }

    #[test]
    fn files_go_through_the_hash_gate() {
        let mut app = App::with_manifest(vec![entry("cd1/Program/Files/A.dxr", b"abc")]);
        assert!(app
            .add_file("cd1/Program/Files/A.dxr", b"abc".to_vec())
            .is_ok());
        assert!(app
            .add_file("cd1/Program/Files/A.dxr", b"abd".to_vec())
            .unwrap_err()
            .contains("mismatch"));
        assert!(app
            .add_file("cd1/Program/Files/B.dxr", b"x".to_vec())
            .unwrap_err()
            .contains("not part"));
    }

    #[test]
    fn folder_lookup_ignores_case() {
        let mut files = HashMap::new();
        files.insert("cd1/Program/Files/HOCode.cxt".to_string(), vec![1]);
        let f = Folder {
            files: &files,
            dir: "cd1/Program/Files",
        };
        assert_eq!(f.read("hocode.cxt"), Some(vec![1]));
        assert_eq!(f.read("other.cxt"), None);
    }

    #[test]
    fn errors_before_loading_are_reported() {
        let mut app = App::with_manifest(Vec::new());
        assert!(app.needed_files("cd1/Program/Files/Home.dxr").is_err());
        assert!(app
            .load("cd1/Program/Files/Home.dxr")
            .unwrap_err()
            .contains("iLumina.exe"));
        assert!(app.render(1, &PageFonts::default()).is_err());
        assert_eq!((app.size(), app.frame_count()), ((0, 0), 0));
    }

    #[test]
    fn style_flags_bits() {
        let st = TextStyle {
            bold: true,
            italic: true,
            ..Default::default()
        };
        assert_eq!(style_flags(&st), 3);
    }

    /// The page and the wasm module must agree on names: every export the
    /// page calls exists here, and every import declared here is provided by
    /// the page. (The wasm build only runs on the owner's Mac, so a typo
    /// would otherwise surface there as a link error at runtime.)
    #[test]
    fn page_and_module_agree_on_the_abi() {
        let js = include_str!("../../../web/app.js");
        let rs = include_str!("lib.rs");
        let exported = |name: &str| {
            rs.contains(&format!("pub extern \"C\" fn {name}("))
                || rs.contains(&format!("pub unsafe extern \"C\" fn {name}("))
        };
        let mut called: Vec<String> = Vec::new();
        for part in js.split("wasm.").skip(1) {
            let name: String = part
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name != "memory" && !called.contains(&name) {
                called.push(name);
            }
        }
        for part in js.split("callStr(\"").skip(1) {
            called.push(part.split('"').next().unwrap().to_string());
        }
        assert!(called.len() > 8, "{called:?}");
        for name in &called {
            assert!(
                exported(name),
                "app.js calls `{name}`, which lib.rs does not export"
            );
        }
        for import in ["js_glyph", "js_line"] {
            assert!(
                rs.contains(&format!("fn {import}(")),
                "{import} not declared"
            );
            assert!(
                js.contains(&format!("function {import}(")),
                "{import} not provided by app.js"
            );
            assert!(
                js.contains(import) && js.contains("env: {"),
                "{import} not passed in env"
            );
        }
    }

    /// The page's play loop over the real discs: add what the run asks for
    /// until it runs. Without the owner's data it stops at the registration
    /// dialog, as a fresh install does.
    #[test]
    fn play_fetch_loop_over_the_discs() {
        let port = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Some(root) = discs::for_tests(&port) else {
            return;
        };
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || {
                let mut app = App::new();
                let read = |rel: &str| std::fs::read(root.join(rel)).unwrap();
                let movie = "cd1/Program/Files/Home.dxr";
                for f in [PROJECTOR, movie] {
                    app.add_file(f, read(f)).unwrap();
                }
                for c in app.needed_files(movie).unwrap() {
                    app.add_file(&c, read(&c)).unwrap();
                }
                app.load(movie).unwrap();
                let mut rounds = 0;
                let first = loop {
                    rounds += 1;
                    assert!(rounds < 30, "the run keeps asking for files");
                    match app.play(movie).unwrap() {
                        Play::NeedFiles(v) => {
                            for f in v {
                                app.add_file(&f, read(&f)).unwrap();
                            }
                        }
                        other => break other,
                    }
                };
                eprintln!("play: {rounds} rounds, {} files", app.files.borrow().len());
                // Without the owner's data the registration dialog waits for
                // the page's user, with its items and geometry.
                let Play::Dialog(lines) = first else {
                    panic!("expected the registration dialog, got {first:?}")
                };
                assert!(lines.starts_with(" Registre Sua C"), "{lines}");
                assert!(
                    lines.lines().any(|l| l.starts_with("editText\t")),
                    "{lines}"
                );
                // The user closes it: the program asks again (registration
                // is required), so the dialog waits again after the replay.
                app.answer("close");
                assert!(matches!(app.play(movie).unwrap(), Play::Dialog(_)));
                app.play_render(&PageFonts::default()).unwrap();
                assert_eq!(app.frame.len(), 1024 * 768 * 4);
                assert!(app.play_step(66).unwrap_err().contains("did not complete"));
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// End to end over the real discs: the same calls the page makes.
    #[test]
    fn loads_and_renders_home_from_the_discs() {
        let port = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Some(root) = discs::for_tests(&port) else {
            return;
        };
        let mut app = App::new();
        let read = |rel: &str| std::fs::read(root.join(rel)).unwrap();
        let movie = "cd1/Program/Files/Home.dxr";
        app.add_file(PROJECTOR, read(PROJECTOR)).unwrap();
        app.add_file(movie, read(movie)).unwrap();
        let casts = app.needed_files(movie).unwrap();
        assert_eq!(casts.len(), 4, "{casts:?}");
        for c in &casts {
            app.add_file(c, read(c)).unwrap();
        }
        app.load(movie).unwrap();
        assert_eq!(app.size(), (1024, 768));
        assert_eq!(app.frame_count(), 160);
        let drawn = app.render(1, &PageFonts::default()).unwrap();
        assert!(drawn > 20, "{drawn} sprites drawn");
        assert_eq!(app.frame.len(), 1024 * 768 * 4);
    }
}
