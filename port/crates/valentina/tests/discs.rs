//! Integration test over the user's own discs (skipped unless
//! `ILUMINA_DISCS` is set). Asserts structure only, never text content.

use valentina::DataFile;

fn databases() -> Option<std::path::PathBuf> {
    let discs = std::env::var("ILUMINA_DISCS").ok()?;
    let p = std::path::Path::new(&discs);
    let p = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(p)
    };
    Some(p.join("cd1/Program/Files/Databases"))
}

#[test]
fn kjv_table_reads_completely() {
    let Some(dir) = databases() else {
        eprintln!("ILUMINA_DISCS not set; skipping");
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
