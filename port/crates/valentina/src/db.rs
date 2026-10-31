//! A Valentina database opened from its three files: `.vdb` (schema), `.dat`
//! (records) and `.blb` (Text fields). Values are read per column and cached.

use crate::schema::{schema, types, FieldDef, TableDef};
use crate::{DataFile, Error};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
}

impl Cell {
    pub fn as_text(&self) -> String {
        match self {
            Cell::Null => String::new(),
            Cell::Int(i) => i.to_string(),
            Cell::Float(f) => f.to_string(),
            Cell::Text(s) => s.clone(),
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Cell::Int(i) => Some(*i as f64),
            Cell::Float(f) => Some(*f),
            Cell::Text(s) => s.trim().parse().ok(),
            Cell::Null => None,
        }
    }
}

/// Windows-1252 decoding (the databases were authored on Windows).
pub fn cp1252(b: &[u8]) -> String {
    const HI: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    b.iter()
        .map(|&c| {
            if (0x80..0xa0).contains(&c) {
                HI[(c - 0x80) as usize]
            } else {
                c as char
            }
        })
        .collect()
}

/// One field's values for every record, shared from the cache.
pub type Column = std::rc::Rc<Vec<Cell>>;

pub struct Database {
    pub tables: Vec<TableDef>,
    dat: Vec<u8>,
    blb: Option<Vec<u8>>,
    columns: RefCell<HashMap<(u32, u16), Column>>,
}

impl Database {
    pub fn open(vdb: &[u8], dat: Vec<u8>, blb: Option<Vec<u8>>) -> Result<Self, Error> {
        DataFile::parse(&dat)?;
        Ok(Database {
            tables: schema(vdb)?,
            dat,
            blb,
            columns: RefCell::new(HashMap::new()),
        })
    }

    pub fn table(&self, name: &str) -> Option<&TableDef> {
        self.tables
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
    }

    fn view(&self, t: &TableDef) -> Result<DataFile<'_>, Error> {
        DataFile::parse(&self.dat)?
            .table(t.id)
            .ok_or(Error::Format(format!("table {} has no data", t.name)))
    }

    /// Number of records in `t`.
    pub fn row_count(&self, t: &TableDef) -> Result<usize, Error> {
        Ok(self.view(t)?.record_count()? as usize)
    }

    /// Every record's value of field `f` (index = record - 1).
    pub fn column(&self, t: &TableDef, f: &FieldDef) -> Result<std::rc::Rc<Vec<Cell>>, Error> {
        if let Some(c) = self.columns.borrow().get(&(t.id, f.number)) {
            return Ok(c.clone());
        }
        let view = self.view(t)?;
        let n = view.record_count()? as usize;
        let cells: Vec<Cell> = if n == 0 {
            Vec::new()
        } else {
            match f.kind {
                types::VARCHAR => {
                    let m = view.text_field(f.number)?;
                    (1..=n as u32)
                        .map(|r| {
                            m.get(&r)
                                .map(|b| Cell::Text(cp1252(b)))
                                .unwrap_or(Cell::Null)
                        })
                        .collect()
                }
                types::STRING => {
                    let seg = view
                        .data_segment(f.number)
                        .ok_or(Error::Format(format!("no data for {}", f.name)))?;
                    let bytes = view.stream(seg)?;
                    let w = bytes.len() / n;
                    (0..n)
                        .map(|i| {
                            let raw = &bytes[i * w..(i + 1) * w];
                            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
                            Cell::Text(cp1252(&raw[..end]))
                        })
                        .collect()
                }
                types::TEXT => {
                    let ptrs = view.fixed_field(f.number)?;
                    let store = match &self.blb {
                        Some(b) => {
                            let file = DataFile::parse(b)?;
                            match file.table(t.id) {
                                Some(v) => Some(v.blob_store(f.number)?),
                                None => None,
                            }
                        }
                        None => None,
                    };
                    ptrs.iter()
                        .map(|&p| match (&store, p) {
                            (_, 0) | (None, _) => Cell::Null,
                            (Some(s), p) => s
                                .get(p)
                                .map(|b| {
                                    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
                                    Cell::Text(cp1252(&b[..end]))
                                })
                                .unwrap_or(Cell::Null),
                        })
                        .collect()
                }
                types::FLOAT => {
                    let raw = view.fixed_field(f.number)?;
                    raw.into_iter()
                        .map(|v| Cell::Float(f32::from_bits(v) as f64))
                        .collect()
                }
                kind => {
                    let raw = view.fixed_field(f.number)?;
                    raw.into_iter()
                        .map(|v| {
                            Cell::Int(match kind {
                                types::SHORT_SIGNED => v as u16 as i16 as i64,
                                types::LONG => v as i32 as i64,
                                _ => v as i64,
                            })
                        })
                        .collect()
                }
            }
        };
        let rc = std::rc::Rc::new(cells);
        self.columns
            .borrow_mut()
            .insert((t.id, f.number), rc.clone());
        Ok(rc)
    }
}
