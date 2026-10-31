//! Integration test over the user's own discs (skipped unless
//! `ILUMINA_DISCS` is set). Asserts structure only.

use std::collections::{BTreeSet, HashSet};

fn discs() -> Option<std::path::PathBuf> {
    let d = std::env::var("ILUMINA_DISCS").ok()?;
    let p = std::path::Path::new(&d);
    Some(if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(p)
    })
}

fn tour_files(discs: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut v = Vec::new();
    for dir in [
        "cd1/Program/Files/Media/QTVRs",
        "cd2/Files/Media/QTVRs",
        "cd3/Files/Media/QTVRs",
        "cd4/Files/Media/QTVRs",
    ] {
        if let Ok(rd) = std::fs::read_dir(discs.join(dir)) {
            for e in rd.flatten() {
                let p = e.path();
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                if !name.starts_with("._")
                    && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mov"))
                {
                    v.push(p);
                }
            }
        }
    }
    v.sort();
    v
}

#[test]
fn nazare_tour_graph_and_hotspot_maps() {
    let Some(d) = discs() else {
        eprintln!("ILUMINA_DISCS not set; skipping");
        return;
    };
    let data = std::fs::read(d.join("cd2/Files/Media/QTVRs/NZV.mov")).unwrap();
    let t = qtvr::tour(&data).unwrap();
    let panos: Vec<_> = t.nodes.iter().filter(|n| &n.kind == b"pano").collect();
    assert_eq!(t.nodes.len(), 50);
    assert_eq!(panos.len(), 28);
    assert_eq!(t.panos.len(), 28);
    assert!(t.panos.iter().all(|p| &p.pano_type == b"cube"));

    let ids: HashSet<u32> = t.nodes.iter().map(|n| n.id).collect();
    for n in &t.nodes {
        for h in &n.hotspots {
            if let Some(l) = &h.link {
                assert!(
                    ids.contains(&l.to_node),
                    "node {} hotspot {} links to missing node {}",
                    n.id,
                    h.id,
                    l.to_node
                );
            }
        }
    }

    // Hotspot track: 6 smc frames per cubic node; pixel values are hotspot ids.
    let hot = t.tracks.iter().find(|tr| &tr.format == b"smc ").unwrap();
    assert_eq!(hot.samples.len(), 6 * 28);
    let (w, h) = (hot.width as usize, hot.height as usize);
    for (i, node) in panos.iter().enumerate() {
        let known: BTreeSet<u32> = node.hotspots.iter().map(|h| h.id).collect();
        let mut seen = BTreeSet::new();
        for f in 0..6 {
            let img = qtvr::smc::decode(hot.sample(&data, i * 6 + f).unwrap(), w, h, None).unwrap();
            let mut present = [false; 256];
            for &v in &img {
                present[v as usize] = true;
            }
            seen.extend((1..256).filter(|&v| present[v]).map(|v| v as u32));
        }
        assert!(
            seen.is_subset(&known),
            "node {}: map ids {seen:?} not in {known:?}",
            node.id
        );
    }
}

#[test]
fn every_tour_parses() {
    let Some(d) = discs() else { return };
    let files = tour_files(&d);
    assert!(files.len() >= 20, "found {} tours", files.len());
    let mut tours = 0;
    for f in &files {
        let data = std::fs::read(f).unwrap();
        let tracks = qtvr::mov::tracks(&data).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        if !tracks.iter().any(|t| &t.handler == b"qtvr") {
            eprintln!("{}: plain movie, not QTVR", f.display());
            continue;
        }
        let t = qtvr::tour(&data).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert!(!t.nodes.is_empty(), "{}", f.display());
        tours += 1;
    }
    eprintln!("{tours} QTVR tours of {} .mov files", files.len());
    assert!(tours >= 20);
}
