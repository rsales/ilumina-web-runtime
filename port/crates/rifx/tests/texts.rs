//! Parse every styled text (`XMED`) and field (`STXT`) on the user's discs
//! (found by `discs::for_tests`; skipped when there are none). Asserts structure only; prints
//! counts, never the text itself.

use rifx::cast::{members, MemberType};
use rifx::text::{parse_fmap, parse_stxt, parse_xmed};
use rifx::{FourCC, Movie};

#[test]
fn every_text_parses() {
    let Some(root) =
        discs::for_tests(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
    else {
        return;
    };
    let dir = root.join("cd1/Program/Files");
    let (xmed, stxt) = (FourCC::new(b"XMED"), FourCC::new(b"STXT"));
    let (mut nx, mut ns, mut styled_runs, mut empty, mut flash) = (0, 0, 0, 0, 0);
    let mut aligns = std::collections::BTreeMap::<String, usize>::new();
    let unmapped = std::cell::Cell::new(0usize);
    let mut fonts = std::collections::BTreeMap::<String, usize>::new();
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = e.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let lower = name.to_ascii_lowercase();
        if name.starts_with("._") || !(lower.ends_with(".cxt") || lower.ends_with(".dxr")) {
            continue;
        }
        let data = std::fs::read(&path).unwrap();
        let movie = Movie::parse(&data).unwrap();
        let fmap = movie
            .all(FourCC::new(b"Fmap"))
            .next()
            .map(|c| parse_fmap(movie.chunk_data(c.id).unwrap()).unwrap())
            .unwrap_or_default();
        for m in members(&movie).unwrap().iter().flatten() {
            if !matches!(
                m.kind,
                MemberType::Text | MemberType::Field | MemberType::Button | MemberType::Xtra
            ) {
                continue;
            }
            for s in movie.children(m.section, xmed).collect::<Vec<_>>() {
                let raw = movie.chunk_data(s).unwrap();
                if raw.get(..4) != Some(b"FFFF") {
                    assert!(
                        raw.get(12..15) == Some(b"FWS") || raw.get(12..15) == Some(b"CWS"),
                        "{name} #{}: unknown Xtra data",
                        m.number
                    );
                    flash += 1;
                    continue;
                }
                let t = parse_xmed(raw).unwrap_or_else(|e| panic!("{name} #{}: {e}", m.number));
                nx += 1;
                for (_, a) in &t.paragraphs {
                    *aligns.entry(format!("{a:?}")).or_default() += 1;
                }
                empty += t.text.is_empty() as usize;
                styled_runs += t.runs.len();
                // Runs are non-empty, except the single zero-length run that
                // keeps an empty member's style.
                let n = t.text.chars().count();
                assert!(t
                    .runs
                    .iter()
                    .all(|r| (r.start < r.end || n == 0) && r.end <= n));
                for r in &t.runs {
                    *fonts
                        .entry(format!("{} {}", r.style.font, r.style.size))
                        .or_default() += 1;
                }
            }
            for s in movie.children(m.section, stxt).collect::<Vec<_>>() {
                let t = parse_stxt(movie.chunk_data(s).unwrap(), &|id| {
                    let f = fmap.get(&id).cloned();
                    unmapped.set(unmapped.get() + f.is_none() as usize);
                    f
                })
                .unwrap_or_else(|e| panic!("{name} #{}: {e}", m.number));
                ns += 1;
                empty += t.text.is_empty() as usize;
                styled_runs += t.runs.len();
                for r in &t.runs {
                    *fonts
                        .entry(format!("{} {}", r.style.font, r.style.size))
                        .or_default() += 1;
                }
            }
        }
    }
    eprintln!("XMED {nx} (+{flash} Flash), STXT {ns}, runs {styled_runs}, empty {empty}");
    eprintln!("fonts: {fonts:?}");
    eprintln!("XMED paragraph alignment: {aligns:?}");
    assert_eq!(unmapped.get(), 0, "STXT font ids missing from Fmap");
    assert!(nx > 0 && ns > 0);
}
