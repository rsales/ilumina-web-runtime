//! Hash gate (ADR 0008): the port only accepts the disc set it was built
//! and tested against. The manifest (`manifests/*.sha256`, `sha256sum`
//! format) holds hashes and paths only, no content.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The supported release, embedded so the gate works without the repo.
pub const ILUMINA_GOLD_250_PTBR: &str =
    include_str!("../../../../manifests/ilumina-gold-2.50-ptbr.sha256");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub sha256: [u8; 32],
    /// Relative path with `/` separators, e.g. `cd1/Program/Files/Home.dxr`.
    pub path: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub ok: usize,
    pub missing: Vec<String>,
    pub mismatched: Vec<String>,
}

impl Report {
    pub fn is_ok(&self) -> bool {
        self.missing.is_empty() && self.mismatched.is_empty()
    }
}

pub fn parse_manifest(text: &str) -> Result<Vec<Entry>, String> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, line)| {
            let (hex, path) = line
                .split_once("  ")
                .ok_or(format!("line {}: expected '<sha256>  <path>'", i + 1))?;
            Ok(Entry {
                sha256: unhex(hex).ok_or(format!("line {}: bad hash", i + 1))?,
                path: path.to_string(),
            })
        })
        .collect()
}

fn unhex(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// Check every manifest entry under `root` (the folder holding cd1..cd4).
pub fn verify(root: &Path, entries: &[Entry]) -> std::io::Result<Report> {
    let mut r = Report::default();
    for e in entries {
        let p: PathBuf = root.join(&e.path);
        match std::fs::read(&p) {
            Ok(data) if sha256(&data) == e.sha256 => r.ok += 1,
            Ok(_) => r.mismatched.push(e.path.clone()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                r.missing.push(e.path.clone())
            }
            Err(err) => return Err(err),
        }
    }
    Ok(r)
}

/// Check a single file the engine is about to load (e.g. `Home.dxr`).
pub fn check_file(root: &Path, rel: &str, entries: &[Entry]) -> Result<Vec<u8>, String> {
    let data = std::fs::read(root.join(rel)).map_err(|err| format!("{rel}: {err}"))?;
    check_bytes(rel, &data, entries)?;
    Ok(data)
}

/// Check bytes that were obtained some other way (fetched by the browser).
pub fn check_bytes(rel: &str, data: &[u8], entries: &[Entry]) -> Result<(), String> {
    let e = entries
        .iter()
        .find(|e| e.path == rel)
        .ok_or(format!("{rel}: not part of the supported release"))?;
    if sha256(data) != e.sha256 {
        return Err(format!(
            "{rel}: hash mismatch (modified or different release)"
        ));
    }
    Ok(())
}

/// A file that every complete disc folder has; used to recognise one.
pub const MARKER: &str = "cd1/Program/iLumina.exe";

/// Check that `root` is the folder holding `cd1`…`cd4`.
pub fn check_root(root: &Path) -> Result<(), String> {
    if root.join(MARKER).is_file() {
        Ok(())
    } else {
        Err(format!(
            "{}: not a discs folder (no {MARKER}); give the folder that holds cd1..cd4",
            root.display()
        ))
    }
}

/// Find the discs folder. `port_dir` is the workspace (`port/`); `env` is
/// `ILUMINA_DISCS` if set (a relative value is resolved from `port_dir`, and
/// a wrong value is an error, never a silent fallback). Without it, the
/// usual layouts are tried: `<repo>/discs` and `<repo>/../discs`.
pub fn locate(port_dir: &Path, env: Option<&str>) -> Result<PathBuf, String> {
    let port_dir = &tidy(port_dir);
    let resolve = |p: &Path| {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            tidy(&port_dir.join(p))
        }
    };
    if let Some(v) = env.filter(|v| !v.is_empty()) {
        let p = resolve(Path::new(v));
        check_root(&p).map_err(|e| format!("ILUMINA_DISCS={v}: {e}"))?;
        return Ok(p);
    }
    let tried = [
        resolve(Path::new("../discs")),
        resolve(Path::new("../../discs")),
    ];
    for p in &tried {
        if check_root(p).is_ok() {
            return Ok(p.clone());
        }
    }
    Err(format!(
        "no discs found (tried {}); set ILUMINA_DISCS to the folder that holds cd1..cd4",
        tried
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Collapse `.` and `..` components without touching the file system.
fn tidy(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// For integration tests over the user's discs. Returns `None` (the test
/// skips, with a notice) when no discs are found, unless
/// `ILUMINA_REQUIRE_DISCS=1`. A wrong `ILUMINA_DISCS` always panics, so a typo
/// cannot turn into a green, skipped run.
pub fn for_tests(port_dir: &Path) -> Option<PathBuf> {
    let env = std::env::var("ILUMINA_DISCS").ok();
    match locate(port_dir, env.as_deref()) {
        Ok(p) => Some(p),
        Err(e) if env.is_some() || std::env::var("ILUMINA_REQUIRE_DISCS").is_ok() => panic!("{e}"),
        Err(e) => {
            eprintln!("skipping disc test: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `<tmp>/repo/port` plus a discs folder at `discs_at` (relative to tmp).
    fn layout(name: &str, discs_at: &str) -> (PathBuf, PathBuf) {
        let tmp = std::env::temp_dir().join(format!("discs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let port = tmp.join("repo/port");
        std::fs::create_dir_all(&port).unwrap();
        let exe = tmp.join(discs_at).join(MARKER);
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"x").unwrap();
        (tmp, port)
    }

    // Regression: the owner's layout keeps the discs next to the repo
    // (`iluminagold/discs`), so plain `cargo test` skipped every disc test.
    #[test]
    fn locate_finds_discs_next_to_the_repo() {
        let (tmp, port) = layout("sibling", "discs");
        let found = locate(&port, None).unwrap();
        assert!(found.join(MARKER).is_file());
        std::fs::remove_dir_all(tmp).unwrap();
    }

    // Regression: a relative ILUMINA_DISCS was resolved from the crate dir.
    #[test]
    fn relative_env_is_resolved_from_port() {
        let (tmp, port) = layout("relative", "discs");
        assert!(locate(&port, Some("../../discs")).is_ok());
        std::fs::remove_dir_all(tmp).unwrap();
    }

    // Regression: `xtask render ../discs` from port/ failed with a bare
    // "No such file or directory"; a wrong path must say what is missing.
    #[test]
    fn wrong_path_is_an_error_that_names_it() {
        let (tmp, port) = layout("wrong", "discs");
        let e = locate(&port, Some("../discs")).unwrap_err();
        assert!(
            e.contains("ILUMINA_DISCS=../discs") && e.contains(MARKER),
            "{e}"
        );
        assert!(e.contains("repo/discs:"), "path shown without `..`: {e}");
        assert!(check_root(&port).is_err());
        std::fs::remove_dir_all(tmp).unwrap();
    }

    // Regression: an unanchored `discs/` rule in .gitignore hid
    // `port/crates/discs`, so a fresh clone did not build.
    #[test]
    fn gitignore_does_not_hide_crates() {
        let gi = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.gitignore");
        let text = std::fs::read_to_string(gi).unwrap();
        for line in text.lines().map(str::trim) {
            assert!(
                !matches!(line, "discs" | "discs/" | "discs/*"),
                ".gitignore rule `{line}` also matches port/crates/discs; anchor it as /discs/"
            );
        }
    }

    #[test]
    fn embedded_manifest_parses() {
        let m = parse_manifest(ILUMINA_GOLD_250_PTBR).unwrap();
        assert_eq!(m.len(), 3297);
        assert!(m.iter().any(|e| e.path == "cd1/Program/Files/Home.dxr"));
    }

    #[test]
    fn verify_detects_missing_and_modified() {
        let dir = std::env::temp_dir().join(format!("discs-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("cd1")).unwrap();
        std::fs::write(dir.join("cd1/a.txt"), b"hello").unwrap();
        std::fs::write(dir.join("cd1/b.txt"), b"changed").unwrap();
        let hex = |d: &[u8]| {
            sha256(d)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        let text = format!(
            "{}  cd1/a.txt\n{}  cd1/b.txt\n{}  cd1/c.txt\n",
            hex(b"hello"),
            hex(b"orig"),
            hex(b"x")
        );
        let r = verify(&dir, &parse_manifest(&text).unwrap()).unwrap();
        assert_eq!((r.ok, r.mismatched.len(), r.missing.len()), (1, 1, 1));
        assert!(check_file(&dir, "cd1/a.txt", &parse_manifest(&text).unwrap()).is_ok());
        assert!(check_file(&dir, "cd1/b.txt", &parse_manifest(&text).unwrap()).is_err());
        let m = parse_manifest(&text).unwrap();
        assert!(check_bytes("cd1/a.txt", b"hello", &m).is_ok());
        assert!(check_bytes("cd1/a.txt", b"hellO", &m)
            .unwrap_err()
            .contains("mismatch"));
        assert!(check_bytes("cd9/x", b"", &m)
            .unwrap_err()
            .contains("not part"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
