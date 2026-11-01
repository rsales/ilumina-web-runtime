//! Integration test over the user's own discs (found by `discs::for_tests`;
//! skipped when there are none). Asserts structure only, never text content.

use valentina::DataFile;

fn databases() -> Option<std::path::PathBuf> {
    Some(
        discs::for_tests(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))?
            .join("cd1/Program/Files/Databases"),
    )
}

#[test]
fn kjv_table_reads_completely() {
    let Some(dir) = databases() else {
        return;
    };
    let data = std::fs::read(dir.join("KJV.DAT")).expect("KJV.DAT");
    let f = DataFile::parse(&data).unwrap();
    let n = f.record_count().unwrap();
    assert_eq!(n, 31102, "KJV verse count");

    let (book, chapter, verse) = (
        f.fixed_field(2).unwrap(),
        f.fixed_field(3).unwrap(),
        f.fixed_field(4).unwrap(),
    );
    let text = f.text_field(5).unwrap();
    assert_eq!(text.len(), n as usize, "every verse has its text");

    // Record 1 is Genesis 1:1; its verse is 54 bytes long.
    assert_eq!((book[0], chapter[0], verse[0]), (1, 1, 1));
    assert_eq!(text[&1].len(), 54);

    // Canonical verse counts: Genesis 1533, Psalms 2461, Revelation 404.
    let count = |b| book.iter().filter(|&&x| x == b).count();
    assert_eq!((count(1), count(19), count(66)), (1533, 2461, 404));
    // (book, chapter, verse) is unique.
    let mut keys: Vec<_> = (0..n as usize)
        .map(|i| (book[i], chapter[i], verse[i]))
        .collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), n as usize);
}

#[test]
fn every_dat_file_opens() {
    let Some(dir) = databases() else { return };
    let mut opened = 0;
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().map(|x| x.eq_ignore_ascii_case("dat")) != Some(true) {
            continue;
        }
        let data = std::fs::read(&p).unwrap();
        let f = DataFile::parse(&data).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        f.record_count()
            .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        opened += 1;
    }
    assert_eq!(opened, 15);
}

#[test]
fn portuguese_bible_text_lives_in_the_blb() {
    let Some(dir) = databases() else { return };
    let dat = std::fs::read(dir.join("RA.DAT")).unwrap();
    let blb = std::fs::read(dir.join("RA.BLB")).unwrap();
    let t = DataFile::parse(&dat).unwrap().tables().remove(0);
    let b = DataFile::parse(&blb).unwrap().tables();
    // The .blb has no record bitmap; use the whole file as one view.
    let store = if b.is_empty() {
        DataFile::parse(&blb).unwrap()
    } else {
        b[0].clone()
    };
    let n = t.record_count().unwrap();
    let (book, chapter, verse) = (
        t.fixed_field(2).unwrap(),
        t.fixed_field(3).unwrap(),
        t.fixed_field(4).unwrap(),
    );
    let ptr = t.fixed_field(5).unwrap();
    assert_eq!((book[0], chapter[0], verse[0]), (1, 1, 1));
    let blobs = store.blob_store(5).unwrap();
    let mut texts = 0;
    for p in &ptr {
        if *p != 0 && !blobs.get(*p).unwrap().is_empty() {
            texts += 1;
        }
    }
    assert_eq!(texts, n as usize, "every RA verse has a BLOB");
    assert_eq!(blobs.get(ptr[0]).unwrap().len(), 43); // Gênesis 1:1 in RA
}

#[test]
fn multi_table_database_splits_into_tables() {
    let Some(dir) = databases() else { return };
    let data = std::fs::read(dir.join("Ency.dat")).unwrap();
    let tables = DataFile::parse(&data).unwrap().tables();
    // Ency.vdb declares Articles, Categories, RefTitles, ...
    assert!(tables.len() >= 3, "got {} tables", tables.len());
    assert_eq!(tables[0].record_count().unwrap(), 1774);
}

/// Every field the `.vdb` schemas declare has its segment in the `.dat`
/// catalog (by table id and field number). Prints the measured width per
/// field type, never values.
#[test]
fn schemas_match_the_data_files() {
    let Some(dir) = databases() else {
        return;
    };
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.to_ascii_lowercase().ends_with(".vdb") && !n.starts_with("._"))
        .collect();
    names.sort();
    let mut widths: std::collections::BTreeMap<u16, std::collections::BTreeSet<String>> =
        Default::default();
    let (mut n_tables, mut n_fields) = (0, 0);
    for vdb_name in &names {
        let stem = &vdb_name[..vdb_name.len() - 4];
        let dat_name = [".dat", ".DAT"]
            .iter()
            .map(|e| format!("{stem}{e}"))
            .find(|n| dir.join(n).exists())
            .unwrap();
        let vdb = std::fs::read(dir.join(vdb_name)).unwrap();
        let dat = std::fs::read(dir.join(&dat_name)).unwrap();
        let schema = valentina::schema::schema(&vdb).unwrap_or_else(|e| panic!("{vdb_name}: {e}"));
        let file = DataFile::parse(&dat).unwrap();
        for t in &schema {
            n_tables += 1;
            let view = file.table(t.id).unwrap_or_else(|| {
                panic!("{vdb_name}: table {} ({}) not in {dat_name}", t.id, t.name)
            });
            // Empty tables have zero-size segments: nothing to measure.
            let n = view.record_count().unwrap();
            if n == 0 {
                n_fields += t.fields.len();
                continue;
            }
            for f in &t.fields {
                n_fields += 1;
                let kinds: Vec<u16> = view
                    .segments
                    .iter()
                    .filter(|s| s.field == f.number)
                    .map(|s| s.kind)
                    .collect();
                assert!(
                    !kinds.is_empty(),
                    "{vdb_name}: {}.{} (field {}) has no segment",
                    t.name,
                    f.name,
                    f.number
                );
                let desc = match view.data_segment(f.number) {
                    Some(s) if kinds == [3] => format!("{}B", s.size / n),
                    _ => format!("kinds {kinds:?}"),
                };
                widths.entry(f.kind).or_default().insert(desc);
            }
        }
    }
    eprintln!(
        "{} databases, {n_tables} tables, {n_fields} fields",
        names.len()
    );
    for (k, w) in &widths {
        eprintln!("  type {k}: {w:?}");
    }
    assert_eq!(names.len(), 15);
}

/// Open every database through its schema and read every column of every
/// table (fixed, String, VarChar and `.blb` Text). Prints counts only.
#[test]
fn every_column_of_every_database_reads() {
    let Some(dir) = databases() else {
        return;
    };
    let find = |stem: &str, exts: &[&str]| {
        exts.iter()
            .map(|e| dir.join(format!("{stem}{e}")))
            .find(|p| p.exists())
    };
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.to_ascii_lowercase().ends_with(".vdb") && !n.starts_with("._"))
        .collect();
    names.sort();
    let (mut cells, mut texts) = (0usize, 0usize);
    for vdb_name in &names {
        let stem = &vdb_name[..vdb_name.len() - 4];
        let vdb = std::fs::read(dir.join(vdb_name)).unwrap();
        let dat = std::fs::read(find(stem, &[".dat", ".DAT"]).unwrap()).unwrap();
        let blb = find(stem, &[".blb", ".BLB"]).map(|p| std::fs::read(p).unwrap());
        let db = valentina::db::Database::open(&vdb, dat, blb)
            .unwrap_or_else(|e| panic!("{vdb_name}: {e}"));
        for t in &db.tables {
            for f in &t.fields {
                let col = db
                    .column(t, f)
                    .unwrap_or_else(|e| panic!("{vdb_name}: {}.{}: {e}", t.name, f.name));
                cells += col.len();
                texts += col
                    .iter()
                    .filter(|c| matches!(c, valentina::db::Cell::Text(s) if !s.is_empty()))
                    .count();
            }
        }
    }
    eprintln!(
        "{} databases: {cells} cells read, {texts} non-empty texts",
        names.len()
    );

    // A real query: the Home "MAIS:" list comes from AppTables.Addons.
    let vdb = std::fs::read(dir.join("AppTables.vdb")).unwrap();
    let dat = std::fs::read(dir.join("AppTables.dat")).unwrap();
    let blb = std::fs::read(dir.join("AppTables.blb")).ok();
    let db = valentina::db::Database::open(&vdb, dat, blb).unwrap();
    let rs = valentina::sql::query(&db, "select Edition, Name from Addons order by Name").unwrap();
    assert_eq!(rs.columns, ["Edition", "Name"]);
    assert!(!rs.rows.is_empty());
    let rs = valentina::sql::query(&db, "select count(*) from StringTable").unwrap();
    eprintln!("Addons query ok; StringTable rows: {:?}", rs.rows[0][0]);

    // Opened without the .blb (regression: every run read all .blb files,
    // ~110 MB): Text columns ask for it, other columns work without it.
    let lazy = valentina::db::Database::open_lazy(
        &vdb,
        std::fs::read(dir.join("AppTables.dat")).unwrap(),
        true,
    )
    .unwrap();
    let mut asked = false;
    for t in &lazy.tables {
        for f in &t.fields {
            match lazy.column(t, f) {
                Err(valentina::Error::NeedBlob) => asked = true,
                Err(e) => panic!("{}.{}: {e}", t.name, f.name),
                Ok(_) => {}
            }
        }
    }
    assert!(asked, "AppTables has Text fields");
    lazy.set_blob_file(std::fs::read(dir.join("AppTables.blb")).unwrap().into());
    for t in &lazy.tables {
        for f in &t.fields {
            lazy.column(t, f).unwrap();
        }
    }
}
