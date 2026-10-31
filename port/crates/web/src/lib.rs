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
use std::collections::HashMap;

/// Folder of the movies inside the disc layout.
pub const MOVIE_DIR: &str = "cd1/Program/Files";
/// The projector, which holds the "System - Win" palette.
pub const PROJECTOR: &str = "cd1/Program/iLumina.exe";

/// Player state: verified files, the loaded stage and the last frame.
pub struct App {
    manifest: Vec<discs::Entry>,
    files: HashMap<String, Vec<u8>>,
    stage: Option<Stage>,
    pub frame: Vec<u8>,
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
            files: HashMap::new(),
            stage: None,
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
        self.files.insert(rel.to_string(), data);
        Ok(())
    }

    /// Disc paths of the external casts that `movie` (a disc path) uses.
    pub fn needed_files(&self, movie: &str) -> Result<Vec<String>, String> {
        let data = self
            .files
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
        let exe = self
            .files
            .get(PROJECTOR)
            .ok_or_else(|| format!("{PROJECTOR}: not added yet"))?;
        let palette = rifx::bitmap::system_win_palette(exe)
            .ok_or("System - Win palette not found in the projector")?;
        let (dir, name) = movie.rsplit_once('/').unwrap_or(("", movie));
        let folder = Folder {
            files: &self.files,
            dir,
        };
        self.stage = Some(Stage::load(&folder, name, palette).map_err(|e| e.to_string())?);
        Ok(())
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
