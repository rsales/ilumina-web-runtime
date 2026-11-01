//! Decode every bitmap member on the user's discs (found by `discs::for_tests`;
//! skipped when there are none). Asserts structure only.

use rifx::bitmap::{bitd_for, system_win_palette, to_rgba, unpack, BitmapInfo};
use rifx::cast::{members, MemberType};
use rifx::Movie;

#[test]
fn every_bitmap_decodes() {
    let Some(root) =
        discs::for_tests(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
    else {
        return;
    };
    let exe = std::fs::read(root.join("cd1/Program/iLumina.exe")).unwrap();
    let pal = system_win_palette(&exe).expect("System - Win palette in the projector");
    assert_eq!(pal[0], [0, 0, 0]);
    assert_eq!(pal[255], [255, 255, 255]);

    let dir = root.join("cd1/Program/Files");
    let (mut decoded, mut linked, mut by_depth) = (0, 0, std::collections::BTreeMap::new());
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = e.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name.starts_with("._") || !name.to_ascii_lowercase().ends_with(".cxt") {
            continue;
        }
        let data = std::fs::read(&path).unwrap();
        let movie = Movie::parse(&data).unwrap();
        for m in members(&movie)
            .unwrap()
            .iter()
            .flatten()
            .filter(|m| m.kind == MemberType::Bitmap)
        {
            let info = BitmapInfo::parse(&m.specific)
                .unwrap_or_else(|e| panic!("{name} #{}: {e}", m.number));
            // Linked bitmaps (a JPEG under Files/Media, loaded at runtime)
            // have no BITD; the member info holds the original path.
            let Some(bitd) = bitd_for(&movie, m.section) else {
                // ... or they are empty placeholders whose image Lingo sets.
                assert!(
                    m.linked_path().is_some() || info.width * info.height <= 1,
                    "{name} #{}: no BITD, no link, {}x{}",
                    m.number,
                    info.width,
                    info.height
                );
                linked += 1;
                continue;
            };
            let bitd = bitd.unwrap();
            let px = unpack(&info, bitd)
                .unwrap_or_else(|e| panic!("{name} #{} ({info:?}): {e}", m.number));
            let raw = bitd.len() == info.pitch * info.height;
            let rgba = to_rgba(&info, &px, Some(&pal), !raw)
                .unwrap_or_else(|e| panic!("{name} #{}: {e}", m.number));
            assert_eq!(rgba.len(), info.width * info.height * 4);
            *by_depth.entry(info.depth).or_insert(0) += 1;
            decoded += 1;
        }
    }
    eprintln!("decoded {decoded} bitmaps (+{linked} linked or empty), by depth {by_depth:?}");
    assert_eq!(decoded + linked, 1317);
}
