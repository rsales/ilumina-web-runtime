//! The Windows file system the original program expects, built from the
//! user's discs the way the original installer lays them out for a full
//! install (verified under Wine, see `runtime/install-ilumina.sh`):
//!
//! * `C:\Program Files\iLuminaPO\` = `cd1/Program/`, with `Files\Media\`
//!   merged from `cd2..cd4/Files/Media/`;
//! * `C:\Program Files\iLuminaPO\iLuminaInfo.txt` = the installer's one-line
//!   property list;
//! * `D:\` = CD 1.
//!
//! Paths are case-insensitive. Only files in the release manifest are
//! visible. Writes (prefs, the info file the program rewrites) go to an
//! overlay that the host can persist.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

/// Reads a file of the disc set by its manifest path (`cd1/Program/…`).
pub trait DiscFiles {
    fn read(&self, rel: &str) -> Option<Vec<u8>>;
    /// The bytes shared, without a copy where the implementation keeps
    /// them (the page's fetched files).
    fn read_shared(&self, rel: &str) -> Option<Rc<[u8]>> {
        self.read(rel).map(Rc::from)
    }
    fn size(&self, rel: &str) -> Option<usize> {
        self.read_shared(rel).map(|b| b.len())
    }
    /// Up to `len` bytes at `at` (fewer past the end).
    fn read_at(&self, rel: &str, at: usize, len: usize) -> Option<Vec<u8>> {
        self.read_shared(rel).map(|b| slice(&b, at, len).to_vec())
    }
}

/// Part of a file on disk (native disc readers).
pub fn file_read_at(path: &std::path::Path, at: usize, len: usize) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(at as u64)).ok()?;
    let mut out = Vec::new();
    f.take(len as u64).read_to_end(&mut out).ok()?;
    Some(out)
}

/// `b[at..at + len]`, clamped to `b`.
pub fn slice(b: &[u8], at: usize, len: usize) -> &[u8] {
    let a = at.min(b.len());
    &b[a..at.saturating_add(len).min(b.len())]
}

pub const APP_DIR: &str = "C:\\Program Files\\iLuminaPO\\";
pub const CD_DRIVE: &str = "D:\\";

/// Byte-for-byte what the original installer writes for a full CD install.
pub const ILUMINA_INFO: &str = "[#applicationPath: \"C:\\Program Files\\iLuminaPO\\\", #Full: -1, #DVDOrCD: #cd, #DvDDrive: \"\", #CDDrive: \"D:\\\", #edition: \"Gold\", #version: 2.50, #installRanFromApp: 0]";

pub struct Vfs {
    disc: Box<dyn DiscFiles>,
    /// lower-case manifest path → manifest path
    index: HashMap<String, String>,
    /// lower-case Windows path → (Windows path, bytes)
    overlay: BTreeMap<String, (String, Vec<u8>)>,
    /// lower-case Windows paths deleted by the program
    deleted: Vec<String>,
    /// Byte writes into files that are still read from the discs (a
    /// program that rewrites a few header bytes of a large media file):
    /// lower-case path → (offset, bytes), applied in order on read.
    patches: HashMap<String, Vec<(usize, Vec<u8>)>>,
    /// Disc files read so far (manifest path → size): what a browser page
    /// must have fetched for the same run.
    pub reads: std::cell::RefCell<BTreeMap<String, usize>>,
}

fn norm(p: &str) -> String {
    p.replace('/', "\\").to_lowercase()
}

impl Vfs {
    /// `manifest`: every path of the release (from the hash manifest).
    pub fn new(disc: Box<dyn DiscFiles>, manifest: impl IntoIterator<Item = String>) -> Self {
        let index = manifest
            .into_iter()
            .map(|p| (p.to_lowercase(), p))
            .collect();
        let mut v = Vfs {
            disc,
            index,
            overlay: BTreeMap::new(),
            deleted: Vec::new(),
            patches: HashMap::new(),
            reads: Default::default(),
        };
        v.write(
            &format!("{APP_DIR}iLuminaInfo.txt"),
            ILUMINA_INFO.as_bytes().to_vec(),
        );
        v
    }

    /// Manifest paths that may hold `win` (first match wins).
    fn candidates(&self, win: &str) -> Vec<String> {
        let w = norm(win);
        let app = norm(APP_DIR);
        let mut out = Vec::new();
        if let Some(rest) = w.strip_prefix(&app) {
            let rest = rest.replace('\\', "/");
            out.push(format!("cd1/program/{rest}"));
            if let Some(media) = rest.strip_prefix("files/media/") {
                for n in 2..=4 {
                    out.push(format!("cd{n}/files/media/{media}"));
                }
            }
        } else if let Some(rest) = w.strip_prefix(&norm(CD_DRIVE)) {
            out.push(format!("cd1/{}", rest.replace('\\', "/")));
        }
        out
    }

    fn resolve(&self, win: &str) -> Option<String> {
        self.candidates(win)
            .into_iter()
            .find_map(|c| self.index.get(&c).cloned())
    }

    pub fn read(&self, win: &str) -> Option<Vec<u8>> {
        let key = norm(win);
        if let Some((_, b)) = self.overlay.get(&key) {
            return Some(b.clone());
        }
        if self.deleted.contains(&key) {
            return None;
        }
        let rel = self.resolve(win)?;
        let mut b = self.disc.read(&rel)?;
        self.reads.borrow_mut().insert(rel, b.len());
        for (at, bytes) in self.patches.get(&key).into_iter().flatten() {
            apply(&mut b, *at, bytes);
        }
        Some(b)
    }

    /// The file's bytes shared with the disc reader when nothing changed
    /// them (no copy of large databases on every run).
    pub fn read_shared(&self, win: &str) -> Option<Rc<[u8]>> {
        let key = norm(win);
        if self.overlay.contains_key(&key) || self.patches.contains_key(&key) {
            return self.read(win).map(Rc::from);
        }
        if self.deleted.contains(&key) {
            return None;
        }
        let rel = self.resolve(win)?;
        let b = self.disc.read_shared(&rel)?;
        self.reads.borrow_mut().insert(rel, b.len());
        Some(b)
    }

    pub fn size(&self, win: &str) -> Option<usize> {
        let key = norm(win);
        if let Some((_, b)) = self.overlay.get(&key) {
            return Some(b.len());
        }
        if self.deleted.contains(&key) {
            return None;
        }
        let rel = self.resolve(win)?;
        let n = self.disc.size(&rel)?;
        // Patches may write past the end.
        let end = self
            .patches
            .get(&key)
            .into_iter()
            .flatten()
            .map(|(at, b)| at + b.len())
            .max()
            .unwrap_or(0);
        Some(n.max(end))
    }

    /// Up to `len` bytes at `at`, with the program's patches applied; reads
    /// only that part of a disc file.
    pub fn read_at(&self, win: &str, at: usize, len: usize) -> Option<Vec<u8>> {
        let key = norm(win);
        if let Some((_, b)) = self.overlay.get(&key) {
            return Some(slice(b, at, len).to_vec());
        }
        if self.deleted.contains(&key) {
            return None;
        }
        let rel = self.resolve(win)?;
        let size = self.size(win)?;
        let mut out = self.disc.read_at(&rel, at, len)?;
        let want = at.min(size).abs_diff(at.saturating_add(len).min(size));
        out.resize(want, 0);
        {
            let mut reads = self.reads.borrow_mut();
            let r = reads.entry(rel).or_insert(0);
            *r = (*r).max(at.saturating_add(out.len()).min(size));
        }
        for (p_at, bytes) in self.patches.get(&key).into_iter().flatten() {
            for (i, &byte) in bytes.iter().enumerate() {
                let pos = p_at + i;
                if pos >= at && pos < at + out.len() {
                    out[pos - at] = byte;
                }
            }
        }
        Some(out)
    }

    /// Overwrite `bytes` at `at` in an existing file without copying it into
    /// the overlay (the file may be a large disc file). Grows the file when
    /// writing past its end. False if the file does not exist.
    pub fn patch(&mut self, win: &str, at: usize, bytes: &[u8]) -> bool {
        let key = norm(win);
        if let Some((_, b)) = self.overlay.get_mut(&key) {
            apply(b, at, bytes);
            return true;
        }
        if !self.exists(win) {
            return false;
        }
        self.patches
            .entry(key)
            .or_default()
            .push((at, bytes.to_vec()));
        true
    }

    /// Patches made to disc files, by Windows path (lower case).
    pub fn patched(&self) -> impl Iterator<Item = (&str, &[(usize, Vec<u8>)])> {
        self.patches.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    pub fn exists(&self, win: &str) -> bool {
        let key = norm(win);
        self.overlay.contains_key(&key)
            || (!self.deleted.contains(&key) && self.resolve(win).is_some())
    }

    /// True when some file lives under `win` (a folder path).
    pub fn folder_exists(&self, win: &str) -> bool {
        let mut w = norm(win);
        if !w.ends_with('\\') {
            w.push('\\');
        }
        if self.overlay.keys().any(|k| k.starts_with(&w)) {
            return true;
        }
        let prefixes: Vec<String> = self
            .candidates(&format!("{w}x"))
            .into_iter()
            .map(|c| c[..c.len() - 1].to_string())
            .collect();
        self.index
            .keys()
            .any(|k| prefixes.iter().any(|p| k.starts_with(p)))
    }

    pub fn write(&mut self, win: &str, bytes: Vec<u8>) {
        let key = norm(win);
        self.deleted.retain(|d| d != &key);
        self.patches.remove(&key);
        self.overlay.insert(key, (win.to_string(), bytes));
    }

    pub fn delete(&mut self, win: &str) -> bool {
        let key = norm(win);
        let existed = self.exists(win);
        self.overlay.remove(&key);
        if existed && !self.deleted.contains(&key) {
            self.deleted.push(key);
        }
        existed
    }

    /// What the program changed, as text lines to keep between sessions
    /// (the browser's storage stands in for the Windows disk): `W⇥path⇥hex`
    /// for a written file, `D⇥path` for a deleted one, `P⇥path⇥at⇥hex` for
    /// bytes patched into a disc file. Nothing is reset or altered: the
    /// program finds its own files again, as on Windows.
    pub fn export_state(&self) -> String {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        let mut out = Vec::new();
        for (p, b) in self.overlay.values() {
            out.push(format!("W\t{p}\t{}", hex(b)));
        }
        for d in &self.deleted {
            out.push(format!("D\t{d}"));
        }
        let mut patched: Vec<_> = self.patches.iter().collect();
        patched.sort_by(|a, b| a.0.cmp(b.0));
        for (p, list) in patched {
            for (at, b) in list {
                out.push(format!("P\t{p}\t{at}\t{}", hex(b)));
            }
        }
        out.join("\n")
    }

    /// Apply [`Vfs::export_state`] lines (unknown or malformed lines are
    /// skipped). Returns the number of lines applied.
    pub fn import_state(&mut self, text: &str) -> usize {
        let unhex = |h: &str| -> Option<Vec<u8>> {
            if h.len() % 2 != 0 {
                return None;
            }
            (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(h.get(i..i + 2)?, 16).ok())
                .collect()
        };
        let mut n = 0;
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let ok = match f.as_slice() {
                ["W", p, h] => unhex(h).map(|b| self.write(p, b)).is_some(),
                ["D", p] => {
                    self.delete(p);
                    true
                }
                ["P", p, at, h] => match (at.parse::<usize>(), unhex(h)) {
                    (Ok(at), Some(b)) => self.patch(p, at, &b),
                    _ => false,
                },
                _ => false,
            };
            n += ok as usize;
        }
        n
    }

    /// Files written by the program (to persist between sessions).
    pub fn written(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.overlay
            .values()
            .map(|(p, b)| (p.as_str(), b.as_slice()))
    }

    /// File names directly inside folder `win` (sorted, case as stored).
    pub fn list(&self, win: &str) -> Vec<String> {
        let mut w = norm(win);
        if !w.ends_with('\\') {
            w.push('\\');
        }
        let mut names: BTreeMap<String, String> = BTreeMap::new();
        for (k, (p, _)) in &self.overlay {
            if let Some(rest) = k.strip_prefix(&w) {
                if !rest.contains('\\') {
                    names.insert(
                        rest.to_string(),
                        p.rsplit('\\').next().unwrap_or(rest).to_string(),
                    );
                }
            }
        }
        for prefix in self.candidates(&format!("{w}x")) {
            let prefix = &prefix[..prefix.len() - 1];
            for (k, canon) in &self.index {
                if let Some(rest) = k.strip_prefix(prefix) {
                    if !rest.contains('/') && !rest.starts_with("._") {
                        names.entry(rest.to_string()).or_insert_with(|| {
                            canon.rsplit('/').next().unwrap_or(rest).to_string()
                        });
                    }
                }
            }
        }
        names.into_values().collect()
    }
}

impl Vfs {
    /// Sub-folder names directly inside folder `win` (sorted).
    pub fn folders(&self, win: &str) -> Vec<String> {
        let mut w = norm(win);
        if !w.ends_with('\\') {
            w.push('\\');
        }
        let mut names: BTreeMap<String, String> = BTreeMap::new();
        for (k, (p, _)) in &self.overlay {
            if let Some(rest) = k.strip_prefix(&w) {
                if let Some((dir, _)) = rest.split_once('\\') {
                    let shown = p[p.len() - rest.len()..]
                        .split('\\')
                        .next()
                        .unwrap_or(dir)
                        .to_string();
                    names.insert(dir.to_string(), shown);
                }
            }
        }
        for prefix in self.candidates(&format!("{w}x")) {
            let prefix = &prefix[..prefix.len() - 1];
            for (k, canon) in &self.index {
                if let Some(rest) = k.strip_prefix(prefix) {
                    if let Some((dir, _)) = rest.split_once('/') {
                        let start = canon.len() - rest.len();
                        let shown = canon[start..start + dir.len()].to_string();
                        names.entry(dir.to_string()).or_insert(shown);
                    }
                }
            }
        }
        names.into_values().collect()
    }
}

/// `*` and `?` wildcard match, case-insensitive (BuddyAPI file patterns).
fn apply(b: &mut Vec<u8>, at: usize, bytes: &[u8]) {
    if b.len() < at + bytes.len() {
        b.resize(at + bytes.len(), 0);
    }
    b[at..at + bytes.len()].copy_from_slice(bytes);
}

pub fn wildcard(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (
        pattern.to_lowercase().chars().collect(),
        name.to_lowercase().chars().collect(),
    );
    fn m(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => m(&p[1..], n) || (!n.is_empty() && m(p, &n[1..])),
            (Some('?'), Some(_)) => m(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => m(&p[1..], &n[1..]),
            _ => false,
        }
    }
    m(&p, &n)
}

/// Windows-1252 text from file bytes.
pub fn text_of(bytes: &[u8]) -> String {
    rifx::text::cp1252(bytes)
}

/// File bytes from text (Windows-1252; unmappable characters become `?`).
pub fn bytes_of(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| {
            let u = c as u32;
            if u < 0x80 || (0xa0..0x100).contains(&u) {
                return u as u8;
            }
            (0x80u8..0xa0)
                .find(|&b| rifx::text::cp1252(&[b]).starts_with(c))
                .unwrap_or(b'?')
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mem(HashMap<String, Vec<u8>>);
    impl DiscFiles for Mem {
        fn read(&self, rel: &str) -> Option<Vec<u8>> {
            self.0.get(rel).cloned()
        }
    }

    fn vfs() -> Vfs {
        let files = [
            ("cd1/Program/iLumina.ini", b"[Prefs]\r\nA=1\r\n".to_vec()),
            ("cd1/Program/Files/Home.dxr", b"dxr".to_vec()),
            ("cd2/Files/Media/Photos/a.jpg", b"jpg".to_vec()),
            ("cd1/Autorun.inf", b"inf".to_vec()),
        ];
        let manifest = files.iter().map(|(p, _)| p.to_string()).collect::<Vec<_>>();
        let mem = Mem(files.into_iter().map(|(p, b)| (p.to_string(), b)).collect());
        Vfs::new(Box::new(mem), manifest)
    }

    #[test]
    fn maps_the_full_install_layout() {
        let v = vfs();
        assert_eq!(
            v.read("C:\\Program Files\\iLuminaPO\\Files\\HOME.DXR")
                .as_deref(),
            Some(&b"dxr"[..])
        );
        assert_eq!(
            v.read("c:/program files/iluminapo/Files/Media/Photos/a.jpg")
                .as_deref(),
            Some(&b"jpg"[..])
        );
        assert_eq!(v.read("D:\\autorun.inf").as_deref(), Some(&b"inf"[..]));
        assert!(v.read("C:\\Windows\\win.ini").is_none());
        assert!(v
            .read("C:\\Program Files\\iLuminaPO\\iLuminaInfo.txt")
            .unwrap()
            .starts_with(b"[#applicationPath"));
        assert!(v.folder_exists("C:\\Program Files\\iLuminaPO\\Files\\Media\\Photos"));
        assert!(!v.folder_exists("C:\\Program Files\\iLuminaPO\\Nope"));
        assert_eq!(
            v.list("C:\\Program Files\\iLuminaPO\\Files\\Media\\Photos\\"),
            ["a.jpg"]
        );
    }

    #[test]
    fn overlay_writes_and_deletes() {
        let mut v = vfs();
        let p = "C:\\Program Files\\iLuminaPO\\Prefs\\x.txt";
        v.write(p, b"hi".to_vec());
        assert_eq!(v.read(&p.to_uppercase()).as_deref(), Some(&b"hi"[..]));
        assert!(v
            .list("C:\\Program Files\\iLuminaPO\\Prefs")
            .contains(&"x.txt".to_string()));
        assert!(v.delete(p));
        assert!(!v.exists(p));
        assert!(v.delete("C:\\Program Files\\iLuminaPO\\iLumina.ini"));
        assert!(v
            .read("C:\\Program Files\\iLuminaPO\\iLumina.ini")
            .is_none());
    }

    #[test]
    fn patches_rewrite_bytes_of_disc_files_without_copying_them() {
        let mut v = vfs();
        let p = "C:\\Program Files\\iLuminaPO\\Files\\Media\\Photos\\a.jpg";
        assert!(v.patch(p, 1, b"P"));
        assert!(v.patch(p, 3, b"!"));
        assert_eq!(v.read(p).as_deref(), Some(&b"jPg!"[..]));
        assert_eq!(v.patched().count(), 1);
        assert!(!v.patch("C:\\nope.bin", 0, b"x"));
        // A whole-file write replaces earlier patches.
        v.write(p, b"new".to_vec());
        assert_eq!(v.patched().count(), 0);
        assert!(v.patch(p, 0, b"N"));
        assert_eq!(v.read(p).as_deref(), Some(&b"New"[..]));
    }

    #[test]
    fn ranges_and_sizes_without_reading_whole_files() {
        // Regression: BinaryIO and the databases read whole disc files
        // (about 176 MB per run, copied again on every replay).
        let mut v = vfs();
        let p = "C:\\Program Files\\iLuminaPO\\Files\\Media\\Photos\\a.jpg";
        assert_eq!(v.size(p), Some(3));
        assert_eq!(v.read_at(p, 1, 5).as_deref(), Some(&b"pg"[..]));
        assert!(v.patch(p, 1, b"P"));
        assert!(v.patch(p, 4, b"!"));
        assert_eq!(v.size(p), Some(5));
        assert_eq!(v.read_at(p, 0, 9).as_deref(), Some(&b"jPg\0!"[..]));
        assert_eq!(v.read_at(p, 2, 2).as_deref(), Some(&b"g\0"[..]));
        // Patched files are copied; untouched ones are shared.
        assert_eq!(v.read_shared(p).as_deref(), Some(&b"jPg\0!"[..]));
        let home = "C:\\Program Files\\iLuminaPO\\Files\\Home.dxr";
        assert_eq!(v.read_shared(home).as_deref(), Some(&b"dxr"[..]));
        assert_eq!(v.read_at("C:\\nope", 0, 1), None);
    }

    #[test]
    fn state_round_trips_between_sessions() {
        // Regression: registration and activation were lost on every page
        // load (the overlay lived in memory only).
        let mut v = vfs();
        let prefs = "C:\\Program Files\\iLuminaPO\\Prefs\\p.dat";
        v.write(prefs, vec![0, 1, 0xff, b'\t']);
        v.delete("D:\\Autorun.inf");
        let jpg = "C:\\Program Files\\iLuminaPO\\Files\\Media\\Photos\\a.jpg";
        v.patch(jpg, 1, b"P");
        let state = v.export_state();
        let mut w = vfs();
        assert!(w.import_state(&state) >= 4);
        assert_eq!(w.read(prefs).as_deref(), Some(&[0, 1, 0xff, b'\t'][..]));
        assert!(!w.exists("D:\\Autorun.inf"));
        assert_eq!(w.read(jpg).as_deref(), Some(&b"jPg"[..]));
        assert_eq!(w.export_state(), state);
        // Malformed lines are skipped.
        assert_eq!(w.import_state("W\tx\tzz\nbogus\nP\ty\t1"), 0);
    }

    #[test]
    fn cp1252_round_trip() {
        assert_eq!(text_of(&bytes_of("Bíblia “x” €")), "Bíblia “x” €");
    }
}
