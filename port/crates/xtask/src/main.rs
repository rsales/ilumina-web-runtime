//! Local-only tooling for the port (ADR 0008). Output goes to `work/`,
//! which is git-ignored: inventories contain names from the original movies.
//!
//!   cargo run -p xtask -- verify <discs-dir>
//!   cargo run -p xtask -- inventory <discs-dir> [out-dir]
//!   cargo run -p xtask -- valentina <file.dat> [out-dir]
//!   cargo run -p xtask -- qtvr <tour.mov> [out-dir]
//!
//! `<discs-dir>` holds cd1..cd4 (the same layout as the Wine runtime).

mod fonts;
mod serve;

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
            if let Err(e) = inventory(&discs_root(&args[1]), &out) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some("valentina") if args.len() >= 2 => {
            let out = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("work/valentina"));
            if let Err(e) = valentina_dump(Path::new(&args[1]), &out) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some("qtvr") if args.len() >= 2 => {
            let out = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("work/qtvr"));
            if let Err(e) = qtvr_dump(Path::new(&args[1]), &out) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some("verify") if args.len() >= 2 => {
            let entries =
                discs::parse_manifest(discs::ILUMINA_GOLD_250_PTBR).expect("embedded manifest");
            match discs::verify(&discs_root(&args[1]), &entries) {
                Ok(r) if r.is_ok() => println!("OK: {} files match ilumina-gold-2.50-ptbr", r.ok),
                Ok(r) => {
                    for p in &r.missing {
                        eprintln!("missing: {p}");
                    }
                    for p in &r.mismatched {
                        eprintln!("modified: {p}");
                    }
                    eprintln!(
                        "MISMATCH: {} ok, {} missing, {} modified",
                        r.ok,
                        r.missing.len(),
                        r.mismatched.len()
                    );
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some("render") if args.len() >= 3 => {
            let frame = args.get(3).and_then(|f| f.parse().ok()).unwrap_or(1);
            let out = args.get(4).map(PathBuf::from).unwrap_or_else(|| {
                PathBuf::from(format!(
                    "work/render/{}-{frame}.png",
                    args[2].trim_end_matches(".dxr")
                ))
            });
            if let Err(e) = render(&discs_root(&args[1]), &args[2], frame, &out) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some("score") if args.len() >= 2 => {
            let frame = args.get(2).and_then(|f| f.parse().ok()).unwrap_or(1);
            if let Err(e) = score_dump(Path::new(&args[1]), frame) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some("serve") if args.len() >= 2 => {
            let port = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(8800);
            if let Err(e) = serve::serve(&discs_root(&args[1]), port) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some("texts") if args.len() >= 2 => {
            let out = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("work/texts"));
            if let Err(e) = texts_dump(&discs_root(&args[1]), &out) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some("bitmaps") if args.len() >= 2 => {
            let out = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("work/bitmaps"));
            if let Err(e) = bitmaps_dump(&discs_root(&args[1]), &out) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!(
                "usage: xtask verify <discs-dir>                # hash gate against the manifest"
            );
            eprintln!(
                "       xtask bitmaps <discs-dir> [out-dir]      # every bitmap as PNG, local only"
            );
            eprintln!("       xtask inventory <discs-dir> [out-dir]");
            eprintln!(
                "       xtask serve <discs-dir> [port]           # browser player on localhost (default 8800)"
            );
            eprintln!(
                "       xtask texts <discs-dir> [out-dir]        # text and field members, local only"
            );
            eprintln!(
                "       xtask qtvr <tour.mov> [out-dir]         # node graph as JSON, local only"
            );
            eprintln!("       xtask valentina <file.dat> [out-dir]   # CSV per table, local only");
            std::process::exit(2);
        }
    }
}

/// The `<discs-dir>` argument, checked before any work: a wrong folder gets
/// an explanation and, when the discs are found elsewhere, the path to use.
fn discs_root(arg: &str) -> PathBuf {
    let p = PathBuf::from(arg);
    if let Err(e) = discs::check_root(&p) {
        eprintln!("error: {e}");
        let port = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        if let Ok(found) = discs::locate(&port, None) {
            eprintln!("hint: the discs are at {}", found.display());
        }
        std::process::exit(2);
    }
    p
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

/// Dump every table of a Valentina `.dat` (and its `.blb`, if present) to
/// CSV, one column per field number. Field names come later from the `.vdb`.
fn valentina_dump(dat_path: &Path, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use valentina::DataFile;
    let dat = std::fs::read(dat_path)?;
    let blb_path = ["blb", "BLB"]
        .iter()
        .map(|e| dat_path.with_extension(e))
        .find(|p| p.exists());
    let blb = blb_path.map(std::fs::read).transpose()?;
    let f = DataFile::parse(&dat)?;
    let store_file = blb.as_deref().map(DataFile::parse).transpose()?;
    let store_tables = store_file.as_ref().map(|s| {
        let t = s.tables();
        if t.is_empty() {
            vec![s.clone()]
        } else {
            t
        }
    });
    let stem = dat_path.file_stem().unwrap().to_string_lossy().to_string();
    std::fs::create_dir_all(out_dir)?;
    for (ti, t) in f.tables().iter().enumerate() {
        let n = t.record_count()? as usize;
        let mut fields: Vec<u16> = t
            .segments
            .iter()
            .filter(|s| s.field > 0)
            .map(|s| s.field)
            .collect();
        fields.sort();
        fields.dedup();
        let mut cols: Vec<(String, Vec<String>)> = Vec::new();
        for fld in fields {
            let is_heap = t.segment(fld, 12).is_some();
            let col: Vec<String> = if is_heap {
                let m = t.text_field(fld)?;
                (1..=n as u32)
                    .map(|r| m.get(&r).map(|b| rifx_latin1(b)).unwrap_or_default())
                    .collect()
            } else {
                let seg = t.segment(fld, 3).unwrap();
                let width = seg.size as usize / n.max(1);
                if width <= 4 {
                    let v = t.fixed_field(fld)?;
                    // A 4-byte column may be a BLOB pointer into the .blb.
                    match store_tables
                        .as_ref()
                        .and_then(|st| st.get(ti))
                        .and_then(|s| s.blob_store(fld).ok())
                    {
                        Some(bs) if width == 4 => v
                            .iter()
                            .map(|&p| {
                                if p == 0 {
                                    String::new()
                                } else {
                                    bs.get(p)
                                        .map(|b| rifx_latin1(&b))
                                        .unwrap_or_else(|e| format!("<{e}>"))
                                }
                            })
                            .collect(),
                        _ => v.iter().map(u32::to_string).collect(),
                    }
                } else {
                    let raw = t.stream(seg)?;
                    raw.chunks(width)
                        .take(n)
                        .map(|c| rifx_latin1(c.split(|&b| b == 0).next().unwrap_or(c)))
                        .collect()
                }
            };
            cols.push((format!("f{fld}"), col));
        }
        let mut csv = String::from("record");
        for (h, _) in &cols {
            csv.push(',');
            csv.push_str(h);
        }
        csv.push('\n');
        for r in 0..n {
            csv.push_str(&(r + 1).to_string());
            for (_, c) in &cols {
                csv.push(',');
                csv.push('"');
                csv.push_str(&c[r].replace('"', "\"\""));
                csv.push('"');
            }
            csv.push('\n');
        }
        let path = out_dir.join(format!("{stem}-t{ti}.csv"));
        std::fs::write(&path, csv)?;
        println!(
            "{}: table {ti}, {n} records, {} fields -> {}",
            stem,
            cols.len(),
            path.display()
        );
    }
    Ok(())
}

fn rifx_latin1(b: &[u8]) -> String {
    rifx::reader::latin1(b)
}

/// Write a tour's node graph (nodes, hotspots, links, view limits) as JSON.
fn qtvr_dump(mov: &Path, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read(mov)?;
    let t = qtvr::tour(&data)?;
    let s4 = |k: &[u8; 4]| String::from_utf8_lossy(k).trim().to_string();
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut j = String::from("{\n  \"nodes\": [\n");
    let panos: Vec<_> = t
        .nodes
        .iter()
        .filter(|n| &n.kind == b"pano")
        .map(|n| n.id)
        .collect();
    for (i, n) in t.nodes.iter().enumerate() {
        let view = panos
            .iter()
            .position(|&id| id == n.id)
            .and_then(|p| t.panos.get(p));
        let _ = write!(
            j,
            "    {{\"id\": {}, \"name\": \"{}\", \"kind\": \"{}\"",
            n.id,
            esc(&n.name),
            s4(&n.kind)
        );
        if let Some(v) = view {
            let _ = write!(
                j,
                ", \"pano\": {{\"type\": \"{}\", \"pan\": [{}, {}], \"tilt\": [{}, {}], \"fov\": [{}, {}], \"default\": [{}, {}, {}]}}",
                s4(&v.pano_type), v.pan.0, v.pan.1, v.tilt.0, v.tilt.1, v.fov.0, v.fov.1, v.default_view.0, v.default_view.1, v.default_view.2
            );
        }
        j.push_str(", \"hotspots\": [");
        for (k, h) in n.hotspots.iter().enumerate() {
            let _ = write!(
                j,
                "{}{{\"id\": {}, \"kind\": \"{}\"",
                if k > 0 { ", " } else { "" },
                h.id,
                s4(&h.kind)
            );
            if let Some(l) = &h.link {
                let _ = write!(
                    j,
                    ", \"to\": {}, \"pan\": {}, \"tilt\": {}, \"fov\": {}",
                    l.to_node, l.to_pan, l.to_tilt, l.to_fov
                );
            }
            j.push('}');
        }
        let _ = writeln!(j, "]}}{}", if i + 1 < t.nodes.len() { "," } else { "" });
    }
    j.push_str("  ]\n}\n");
    std::fs::create_dir_all(out_dir)?;
    let path = out_dir.join(format!(
        "{}.json",
        mov.file_stem().unwrap().to_string_lossy()
    ));
    std::fs::write(&path, j)?;
    println!(
        "{} nodes ({} panoramas) -> {}",
        t.nodes.len(),
        t.panos.len(),
        path.display()
    );
    Ok(())
}

pub fn write_png(
    path: &Path,
    w: usize,
    h: usize,
    rgba: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(f, w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgba)?;
    Ok(())
}

/// Export every bitmap member of every cast as PNG (local inspection only).
fn bitmaps_dump(discs: &Path, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use rifx::bitmap::{bitd_for, system_win_palette, to_rgba, unpack, BitmapInfo};
    let pal = system_win_palette(&std::fs::read(discs.join("cd1/Program/iLumina.exe"))?)
        .ok_or("System - Win palette not found")?;
    let mut n = 0;
    for path in movie_files(discs)? {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let data = std::fs::read(&path)?;
        let movie = Movie::parse(&data)?;
        let dir = out_dir.join(&stem);
        for m in members(&movie)?
            .iter()
            .flatten()
            .filter(|m| m.kind == MemberType::Bitmap)
        {
            let info = BitmapInfo::parse(&m.specific)?;
            let Some(bitd) = bitd_for(&movie, m.section) else {
                continue;
            };
            let rgba = to_rgba(&info, &unpack(&info, bitd?)?, Some(&pal))?;
            if info.width == 0 || info.height == 0 {
                continue;
            }
            std::fs::create_dir_all(&dir)?;
            let safe: String = m
                .name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect();
            write_png(
                &dir.join(format!("{:04}-{safe}.png", m.number)),
                info.width,
                info.height,
                &rgba,
            )?;
            n += 1;
        }
    }
    println!("{n} bitmaps -> {}", out_dir.display());
    Ok(())
}

/// Print a movie's cast libraries and the sprites of one frame.
fn score_dump(movie_path: &Path, frame: usize) -> Result<(), Box<dyn std::error::Error>> {
    use rifx::score::{cast_libs, Score};
    let data = std::fs::read(movie_path)?;
    let movie = Movie::parse(&data)?;
    let libs = cast_libs(&movie)?;
    for l in &libs {
        println!(
            "castLib {}: {:?} path={:?} members {}..{}",
            l.number, l.name, l.path, l.min_member, l.max_member
        );
    }
    let Some(score) = Score::from_movie(&movie)? else {
        println!("no score");
        return Ok(());
    };
    println!(
        "{} frames, {} channels x {} bytes; frame {frame} script {:?}",
        score.frame_count,
        score.channels,
        score.record_size,
        score.frame_script(frame)
    );
    let dir = movie_path.parent().unwrap_or(Path::new("."));
    let mut cache: std::collections::HashMap<u16, Vec<rifx::cast::Member>> = Default::default();
    for s in score.sprites(frame)? {
        let lib = libs.iter().find(|l| l.number == s.cast_lib);
        let name = lib.and_then(|l| {
            // Protected movies reference `.cst`; the shipped files are `.cxt`.
            let file = l
                .path
                .rsplit(['\\', ':', '/'])
                .next()?
                .replace(".cst", ".cxt");
            let members = cache.entry(l.number).or_insert_with(|| {
                std::fs::read(dir.join(&file))
                    .ok()
                    .and_then(|d| {
                        Movie::parse(&d)
                            .ok()
                            .and_then(|m| members(&m).ok())
                            .map(|c| c.into_iter().flatten().collect())
                    })
                    .unwrap_or_default()
            });
            members
                .iter()
                .find(|m| m.number == s.member as u32)
                .map(|m| format!("{:?} {:?}", m.kind, m.name))
        });
        println!(
            "  sprite {:3}: lib {} member {:4} ink {:2} blend {:3} rect ({}, {}) {}x{}  {}",
            s.number,
            s.cast_lib,
            s.member,
            s.ink,
            s.blend,
            s.left,
            s.top,
            s.width,
            s.height,
            name.unwrap_or_default()
        );
    }
    Ok(())
}

/// Movie files read from disk through the hash gate.
struct GatedFiles {
    root: PathBuf,
    manifest: Vec<discs::Entry>,
}

impl director::Files for GatedFiles {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        let rel = format!("cd1/Program/Files/{name}");
        match discs::check_file(&self.root, &rel, &self.manifest) {
            Ok(d) => Some(d),
            Err(e) => {
                eprintln!("hash gate: {e}");
                None
            }
        }
    }
}

/// Composite one score frame of a movie to PNG (local inspection only).
fn render(
    discs: &Path,
    movie: &str,
    frame: usize,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = discs::parse_manifest(discs::ILUMINA_GOLD_250_PTBR)?;
    let exe = discs::check_file(discs, "cd1/Program/iLumina.exe", &manifest)?;
    let palette = rifx::bitmap::system_win_palette(&exe).ok_or("System - Win palette not found")?;
    let files = GatedFiles {
        root: discs.to_path_buf(),
        manifest,
    };
    let stage = director::Stage::load(&files, movie, palette)?;
    let fonts = fonts::HostFonts::new();
    let (px, report) = stage.render(frame, &fonts)?;
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    write_png(
        out,
        stage.config.width as usize,
        stage.config.height as usize,
        &px,
    )?;
    println!(
        "{movie} frame {frame}: {}x{} stage, {} sprites drawn, {} skipped -> {}",
        stage.config.width,
        stage.config.height,
        report.drawn.len(),
        report.skipped.len(),
        out.display()
    );
    for (n, why) in &report.skipped {
        println!("  sprite {n}: {why}");
    }
    let mut used: Vec<_> = fonts.used.borrow().clone().into_iter().collect();
    used.sort();
    for (family, file) in used {
        println!("  font {family} -> {file}");
    }
    Ok(())
}

/// Every text (`XMED`) and field (`STXT`) member as a `.txt` per cast file,
/// with its runs (font, size, style, colour). Local only: this is content.
fn texts_dump(discs: &Path, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use rifx::text::{parse_fmap, parse_stxt, parse_xmed};
    use rifx::FourCC;
    let dir = discs.join("cd1/Program/Files");
    std::fs::create_dir_all(out)?;
    let mut names: Vec<_> = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            !n.starts_with("._") && (l.ends_with(".cxt") || l.ends_with(".dxr"))
        })
        .collect();
    names.sort();
    let mut total = 0;
    for name in names {
        let data = std::fs::read(dir.join(&name))?;
        let movie = Movie::parse(&data)?;
        let fmap = match movie.first(FourCC::new(b"Fmap")) {
            Some(c) => parse_fmap(movie.chunk_data(c.id)?)?,
            None => Default::default(),
        };
        let mut txt = String::new();
        for m in members(&movie)?.iter().flatten() {
            let t = if let Some(s) = movie.children(m.section, FourCC::new(b"XMED")).next() {
                let d = movie.chunk_data(s)?;
                if !d.starts_with(b"FFFF") {
                    continue;
                }
                parse_xmed(d)?
            } else if let Some(s) = movie.children(m.section, FourCC::new(b"STXT")).next() {
                parse_stxt(movie.chunk_data(s)?, &|id| fmap.get(&id).cloned())?
            } else {
                continue;
            };
            total += 1;
            let _ = writeln!(txt, "## #{} {:?} {:?}", m.number, m.kind, m.name);
            for r in &t.runs {
                let st = &r.style;
                let _ = writeln!(
                    txt,
                    "   [{}..{}] {} {}{}{}{} #{:02x}{:02x}{:02x}",
                    r.start,
                    r.end,
                    st.font,
                    st.size,
                    if st.bold { " b" } else { "" },
                    if st.italic { " i" } else { "" },
                    if st.underline { " u" } else { "" },
                    st.color[0],
                    st.color[1],
                    st.color[2]
                );
            }
            let _ = writeln!(txt, "{}\n", t.text.replace('\r', "\n"));
        }
        if !txt.is_empty() {
            std::fs::write(out.join(format!("{name}.txt")), txt)?;
        }
    }
    println!("{total} text members -> {}", out.display());
    Ok(())
}
