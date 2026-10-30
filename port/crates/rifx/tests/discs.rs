//! Integration test over the user's own discs. Skipped unless
//! `ILUMINA_DISCS` points at the folder that holds cd1..cd4.
//! Asserts structure only (counts, no content), so it is safe to run in CI
//! on a machine that has the discs.

use rifx::bytecode::{disassemble, Op};
use rifx::cast::members;
use rifx::lingo::contexts;
use rifx::Movie;

#[test]
fn every_movie_parses_and_all_bytecode_decodes() {
    let Ok(discs) = std::env::var("ILUMINA_DISCS") else {
        eprintln!("ILUMINA_DISCS not set; skipping");
        return;
    };
    let dir = std::path::Path::new(&discs).join("cd1/Program/Files");
    let mut files = 0;
    let (mut n_members, mut n_scripts, mut n_handlers) = (0, 0, 0);
    for entry in std::fs::read_dir(&dir).expect("cd1/Program/Files") {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.starts_with("._") || !(ext == "dxr" || ext == "cxt") {
            continue;
        }
        files += 1;
        let data = std::fs::read(&path).unwrap();
        let movie = Movie::parse(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(movie.version, 0x73A, "{name}: expected Director MX");
        n_members += members(&movie)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .iter()
            .map(Vec::len)
            .sum::<usize>();
        for ctx in contexts(&movie).unwrap_or_else(|e| panic!("{name}: {e}")) {
            for s in &ctx.scripts {
                n_scripts += 1;
                for h in &s.handlers {
                    n_handlers += 1;
                    for ins in disassemble(&h.bytecode).unwrap_or_else(|e| panic!("{name}: {e}")) {
                        assert_ne!(
                            ins.op.name(),
                            "unknown",
                            "{name}: unknown opcode {:#x}",
                            ins.op.0
                        );
                        if ins.op.arg_is_name() {
                            assert!(
                                (ins.arg as usize) < ctx.names.len(),
                                "{name}: name id out of range"
                            );
                        }
                        let _: Op = ins.op;
                    }
                }
            }
        }
    }
    // Inventory of iLumina Gold 2.50 pt-BR (2026-10-30).
    assert_eq!(files, 29);
    assert_eq!(n_members, 2018);
    assert_eq!(n_scripts, 446);
    assert_eq!(n_handlers, 2751);
}
