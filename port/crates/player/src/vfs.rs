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

/// Reads a file of the disc set by its manifest path (`cd1/Program/…`).
pub trait DiscFiles {
    fn read(&self, rel: &str) -> Option<Vec<u8>>;
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
        self.disc.read(&self.resolve(win)?)
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
    fn cp1252_round_trip() {
        assert_eq!(text_of(&bytes_of("Bíblia “x” €")), "Bíblia “x” €");
    }
}
