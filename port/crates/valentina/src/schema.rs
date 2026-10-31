//! The schema of a Valentina 1.x database, read from its `.vdb` file.
//!
//! A `.vdb` is itself a Valentina file (4 KB pages) whose system table
//! `0x7ffffffe` stores the structure as property records, one per row:
//!
//! * field 2 (`u32`): property id, or an object kind for object rows
//!   (1 database, 2 table, 3 field) whose value is `"0"`;
//! * field 3 (slot heap, segment kinds 13/14): the value as text;
//! * field 5 (`u32`): the row of the owning object.
//!
//! Table properties: 34 name, 36 table id (matches the `.dat` catalog).
//! Field properties: 42 name, 44 type, 56 maximum length (strings).
//! A table's fields are its field objects in row order, numbered from 2 in
//! the `.dat` (field 0 is the record bitmap).
//!
//! Worked out from iLumina's own files (ADR 0008, M2); checked against the
//! `.dat` catalogs of all 15 databases by `tests/discs.rs`.

use crate::{DataFile, Error};

pub const SYSTEM_PROPS: u32 = 0x7fff_fffe;

/// Field type codes seen in iLumina's databases, with the width measured in
/// the `.dat` files (`tests/discs.rs`).
pub mod types {
    /// 1 byte, unsigned.
    pub const BYTE: u16 = 3;
    /// 2 bytes, signed (e.g. panorama tilt).
    pub const SHORT_SIGNED: u16 = 4;
    /// 2 bytes, unsigned.
    pub const USHORT: u16 = 5;
    /// 4 bytes, signed.
    pub const LONG: u16 = 7;
    /// Assumed 4-byte float: only empty tables use it on the discs.
    pub const FLOAT: u16 = 9;
    /// Fixed `max_len + 1` bytes, NUL-terminated (garbage after the NUL).
    pub const STRING: u16 = 10;
    /// 4-byte pointer to the first segment of the value in the `.blb`.
    pub const TEXT: u16 = 17;
    /// Slot heap plus index (like the KJV verses).
    pub const VARCHAR: u16 = 26;
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldDef {
    /// Field number in the `.dat` catalog (from 2).
    pub number: u16,
    pub name: String,
    pub kind: u16,
    pub max_len: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableDef {
    pub id: u32,
    pub name: String,
    pub fields: Vec<FieldDef>,
}

impl TableDef {
    pub fn field(&self, name: &str) -> Option<&FieldDef> {
        self.fields
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
    }
}

/// All tables of the database described by `vdb`.
pub fn schema(vdb: &[u8]) -> Result<Vec<TableDef>, Error> {
    let file = DataFile::parse(vdb)?;
    let t = file
        .table(SYSTEM_PROPS)
        .ok_or(Error::Format("no schema table in .vdb".into()))?;
    let prop = t.fixed_field(2)?;
    let owner = t.fixed_field(5)?;
    let text = t.text_field_kinds(3, 13, 14)?;
    let value = |row: u32| -> String {
        text.get(&row)
            .map(|b| b.iter().map(|&c| c as char).collect())
            .unwrap_or_default()
    };
    let n = prop.len() as u32;
    // Properties of object row `obj`: (property id, value).
    let props_of = |obj: u32| -> Vec<(u32, String)> {
        (1..=n)
            .filter(|&r| owner[r as usize - 1] == obj && r != obj)
            .map(|r| (prop[r as usize - 1], value(r)))
            .collect()
    };
    let get =
        |props: &[(u32, String)], id: u32| props.iter().find(|p| p.0 == id).map(|p| p.1.clone());
    let is_object = |r: u32, kind: u32| prop[r as usize - 1] == kind && value(r) == "0";

    let mut tables = Vec::new();
    for r in 1..=n {
        if !is_object(r, 2) {
            continue;
        }
        let p = props_of(r);
        let (Some(name), Some(id)) = (get(&p, 34), get(&p, 36).and_then(|v| v.parse().ok())) else {
            continue;
        };
        let mut fields = Vec::new();
        for f in 1..=n {
            if owner[f as usize - 1] != r || !is_object(f, 3) {
                continue;
            }
            let fp = props_of(f);
            fields.push(FieldDef {
                number: fields.len() as u16 + 2,
                name: get(&fp, 42).unwrap_or_default(),
                kind: get(&fp, 44).and_then(|v| v.parse().ok()).unwrap_or(0),
                max_len: get(&fp, 56).and_then(|v| v.parse().ok()),
            });
        }
        tables.push(TableDef { id, name, fields });
    }
    Ok(tables)
}
