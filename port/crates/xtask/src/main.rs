//! Local-only tooling for the port (ADR 0008). Output goes to `work/`,
//! which is git-ignored: inventories contain names from the original movies.
//!
//!   cargo run -p xtask -- inventory <discs-dir> [out-dir]
//!
//! `<discs-dir>` holds cd1..cd4 (the same layout as the Wine runtime).

use rifx::bytecode::disassemble;
use rifx::cast::{members, MemberType};
use rifx::lingo::{contexts, Literal};
use rifx::Movie;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("inventory") if args.len() >= 2 => {
            let out = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("work/inventory"));
            if let Err(e) = inventory(Path::new(&args[1]), &out) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("usage: xtask inventory <discs-dir> [out-dir]");
            std::process::exit(2);
        }
    }
}

#[derive(Default)]
struct Tally(BTreeMap<String, usize>);

impl Tally {
    fn add(&mut self, k: &str) {
        *self.0.entry(k.to_string()).or_default() += 1;
    }
    fn render(&self, title: &str, out: &mut String) {
        let mut v: Vec<_> = self.0.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let _ = writeln!(out, "\n## {title} ({} distinct)\n", v.len());
        for (k, n) in v {
            let _ = writeln!(out, "- {n:>6}  {k}");
        }
    }
}

fn movie_files(discs: &Path) -> std::io::Result<Vec<PathBuf>> {
    let dir = discs.join("cd1/Program/Files");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            !n.starts_with("._")
                && matches!(
                    p.extension()
                        .and_then(|s| s.to_str())
                        .map(str::to_ascii_lowercase)
                        .as_deref(),
                    Some("dxr" | "cxt")
                )
        })
        .collect();
    v.sort();
    Ok(v)
}

fn inventory(discs: &Path, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let files = movie_files(discs)?;
    let mut summary = String::from("# Movie inventory (local only, do not commit)\n\n| File | Dir | Chunks | Members | Scripts | Handlers | Bytecode |\n|---|---|---|---|---|---|---|\n");
    let (
        mut ext,
        mut obj,
        mut movieprop,
        mut objprop,
        mut builtin,
        mut ops,
        mut kinds,
        mut fourccs,
        mut xtras,
        mut strings,
    ) = (
        Tally::default(),
        Tally::default(),
        Tally::default(),
        Tally::default(),
        Tally::default(),
        Tally::default(),
        Tally::default(),
        Tally::default(),
        Tally::default(),
        Tally::default(),
    );
    let mut totals = (0usize, 0usize, 0usize, 0usize);
    let mut failures = Vec::new();

    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let data = std::fs::read(path)?;
        let movie = match Movie::parse(&data) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        for c in &movie.chunks {
            fourccs.add(&c.fourcc.to_string());
        }
        let casts = members(&movie).map_err(|e| format!("{name}: {e}"))?;
        let n_members: usize = casts.iter().map(Vec::len).sum();
        for m in casts.iter().flatten() {
            kinds.add(&format!("{:?}", m.kind));
            if m.kind == MemberType::Xtra {
                xtras.add(m.xtra.as_deref().unwrap_or("?"));
            }
        }
        let ctxs = contexts(&movie).map_err(|e| format!("{name}: {e}"))?;
        let (mut n_scripts, mut n_handlers, mut n_bytes) = (0, 0, 0);
        for ctx in &ctxs {
            for s in &ctx.scripts {
                n_scripts += 1;
                for lit in &s.literals {
                    if let Literal::String(t) = lit {
                        // Keep only short, identifier-like strings: Xtra names,
                        // symbols, file names. Long text is app content.
                        if t.len() <= 40 && !t.contains(' ') && !t.is_empty() {
                            strings.add(t);
                        }
                    }
                }
                for h in &s.handlers {
                    n_handlers += 1;
                    n_bytes += h.bytecode.len();
                    for ins in disassemble(&h.bytecode).map_err(|e| format!("{name}: {e}"))? {
                        ops.add(ins.op.name());
                        let nm = || ctx.name(ins.arg).to_string();
                        match ins.op.0 {
                            0x57 => ext.add(&nm()),
                            0x67 | 0x58 | 0x63 => obj.add(&nm()),
                            0x5f | 0x60 => movieprop.add(&nm()),
                            0x61 | 0x62 | 0x70 => objprop.add(&nm()),
                            0x66 | 0x72 => builtin.add(&nm()),
                            _ => {}
                        }
                    }
                }
            }
        }
        totals.0 += movie.chunks.len();
        totals.1 += n_members;
        totals.2 += n_scripts;
        totals.3 += n_handlers;
        let _ = writeln!(
            summary,
            "| {name} | {} (0x{:X}) | {} | {n_members} | {n_scripts} | {n_handlers} | {n_bytes} |",
            movie.human_version(),
            movie.version,
            movie.chunks.len()
        );
    }
    let _ = writeln!(
        summary,
        "| **total {} files** | | {} | {} | {} | {} | |",
        files.len(),
        totals.0,
        totals.1,
        totals.2,
        totals.3
    );
    if !failures.is_empty() {
        let _ = writeln!(summary, "\n## Failures\n");
        for f in &failures {
            let _ = writeln!(summary, "- {f}");
        }
    }
    kinds.render("Member types", &mut summary);
    xtras.render("Xtra cast members", &mut summary);
    fourccs.render("Chunk types", &mut summary);
    ops.render("Opcodes", &mut summary);
    ext.render("extcall (global handlers and builtins)", &mut summary);
    obj.render("objcall (methods, incl. Xtra methods)", &mut summary);
    movieprop.render("movie properties", &mut summary);
    objprop.render("object properties", &mut summary);
    builtin.render("the-builtins / top-level props", &mut summary);
    strings.render("short string literals", &mut summary);

    std::fs::create_dir_all(out_dir)?;
    let path = out_dir.join("inventory.md");
    std::fs::write(&path, &summary)?;
    println!(
        "{} movies parsed, {} failed; {} members, {} scripts, {} handlers -> {}",
        files.len() - failures.len(),
        failures.len(),
        totals.1,
        totals.2,
        totals.3,
        path.display()
    );
    if !failures.is_empty() {
        return Err(failures.join("; ").into());
    }
    Ok(())
}
