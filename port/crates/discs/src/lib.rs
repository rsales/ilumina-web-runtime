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
    let e = entries
        .iter()
        .find(|e| e.path == rel)
        .ok_or(format!("{rel}: not part of the supported release"))?;
    let data = std::fs::read(root.join(rel)).map_err(|err| format!("{rel}: {err}"))?;
    if sha256(&data) != e.sha256 {
        return Err(format!(
            "{rel}: hash mismatch (modified or different release)"
        ));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
