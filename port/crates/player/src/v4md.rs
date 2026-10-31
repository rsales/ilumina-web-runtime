//! The V4MD Xtra (Valentina for Macromedia Director) over `crates/valentina`:
//! `VDatabase` (open a `.vdb` with its `.dat`/`.blb`), `VCursor` (an SQL
//! SELECT and its records) and the global `Valentina*` functions.
//!
//! Records come back as property lists keyed by field-name symbols, which is
//! how the original scripts read them (`rec.name`, `rec.module`). Writes
//! (`SqlExecute` of UPDATE/INSERT) are recorded, not applied, for now.

use crate::vfs::Vfs;
use lingo_vm::Value;
use std::collections::HashMap;
use valentina::db::{Cell, Database};
use valentina::sql::ResultSet;

pub struct Cursor {
    pub rs: ResultSet,
    /// Current record, 1-based (0 = before the first).
    pub pos: usize,
}

#[derive(Default)]
pub struct V4md {
    dbs: HashMap<i64, Database>,
    cursors: HashMap<i64, Cursor>,
    pub last_error: (i64, String),
    /// Write statements the program sent (not applied yet).
    pub writes: Vec<String>,
}

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or_default()
}

pub fn cell_value(c: &Cell) -> Value {
    match c {
        Cell::Null => Value::str(""),
        Cell::Int(i) => Value::Int(*i),
        Cell::Float(f) => Value::Float(*f),
        Cell::Text(s) => Value::Str(s.clone()),
    }
}

/// Generic V4MD error number for anything that went wrong here.
const ERR: i64 = 1;

impl V4md {
    fn fail(&mut self, msg: String) {
        self.last_error = (ERR, msg);
    }

    /// Global functions. `None` = not a V4MD name.
    pub fn call(&mut self, name: &str) -> Option<Value> {
        Some(match name.to_ascii_lowercase().as_str() {
            "valentinainit" | "valentinadebuglevel" | "valentinashutdown" => Value::Int(0),
            "valentinaerror" => Value::Int(self.last_error.0),
            "valentinaerrorstring" => Value::Str(self.last_error.1.clone()),
            _ => return None,
        })
    }

    /// `new(xtra("VCursor"), dbRef, sql, …)`: run the query now.
    pub fn new_cursor(&mut self, id: i64, args: &[Value]) {
        let db_id = match arg(args, 0) {
            Value::Host(lingo_vm::HostRef::Object { id, .. }) => id,
            _ => -1,
        };
        let sql = arg(args, 1).to_text();
        self.last_error = (0, String::new());
        let rs = match self.dbs.get(&db_id) {
            Some(db) => match valentina::sql::query(db, &sql) {
                Ok(rs) => rs,
                Err(e) => {
                    self.fail(e.to_string());
                    ResultSet {
                        columns: vec![],
                        rows: vec![],
                    }
                }
            },
            None => {
                self.fail("database not open".into());
                ResultSet {
                    columns: vec![],
                    rows: vec![],
                }
            }
        };
        self.cursors.insert(id, Cursor { rs, pos: 1 });
    }

    pub fn is_cursor(&self, id: i64) -> bool {
        self.cursors.contains_key(&id)
    }

    /// `VDatabase` methods.
    pub fn database_method(
        &mut self,
        vfs: &Vfs,
        id: i64,
        me: &Value,
        name: &str,
        args: &[Value],
    ) -> Option<Value> {
        Some(match name.to_ascii_lowercase().as_str() {
            "opendatabase" => {
                let path = arg(args, 0).to_text();
                self.last_error = (0, String::new());
                let stem = match path.rsplit_once('.') {
                    Some((s, _)) => s.to_string(),
                    None => path.clone(),
                };
                let find = |exts: &[&str]| {
                    exts.iter()
                        .map(|e| format!("{stem}{e}"))
                        .find_map(|p| vfs.read(&p))
                };
                match (vfs.read(&path), find(&[".dat", ".DAT"])) {
                    (Some(vdb), Some(dat)) => {
                        match Database::open(&vdb, dat, find(&[".blb", ".BLB"])) {
                            Ok(db) => {
                                self.dbs.insert(id, db);
                            }
                            Err(e) => self.fail(format!("{path}: {e}")),
                        }
                    }
                    _ => self.fail(format!("{path}: database files not found")),
                }
                Value::Int(0)
            }
            "closedatabase" => {
                self.dbs.remove(&id);
                Value::Int(0)
            }
            "getref" => me.clone(),
            "sqlexecute" => {
                self.writes.push(arg(args, 0).to_text());
                self.last_error = (0, String::new());
                Value::Int(0)
            }
            _ => return None,
        })
    }

    /// `VCursor` methods (also called as `GetColumn(cursor, …)`).
    pub fn cursor_method(&mut self, id: i64, name: &str, args: &[Value]) -> Option<Value> {
        let c = self.cursors.get_mut(&id)?;
        let n = c.rs.rows.len();
        let field_index = |c: &Cursor, v: &Value| -> Option<usize> {
            match v {
                Value::Int(i) => Some((*i as usize).wrapping_sub(1)),
                other => {
                    let name = other.to_text();
                    c.rs.columns
                        .iter()
                        .position(|col| col.eq_ignore_ascii_case(&name))
                }
            }
        };
        let record = |c: &Cursor, r: usize| -> Value {
            Value::prop_list(
                c.rs.columns
                    .iter()
                    .zip(c.rs.rows.get(r).into_iter().flatten())
                    .map(|(k, v)| (Value::sym(k.clone()), cell_value(v)))
                    .collect(),
            )
        };
        let as_string = |c: &Cursor, r: usize, fd: &str| -> String {
            c.rs.rows
                .get(r)
                .map(|row| row.iter().map(|v| v.as_text()).collect::<Vec<_>>().join(fd))
                .unwrap_or_default()
        };
        let mv = |c: &mut Cursor, to: usize| -> Value {
            if (1..=n).contains(&to) {
                c.pos = to;
                Value::Int(1)
            } else {
                Value::Int(0)
            }
        };
        Some(match name.to_ascii_lowercase().as_str() {
            "getrecordcount" | "recordcount" => Value::Int(n as i64),
            "getfieldcount" => Value::Int(c.rs.columns.len() as i64),
            "getfieldname" => Value::Str(
                c.rs.columns
                    .get(field_index(c, &arg(args, 0))?)
                    .cloned()
                    .unwrap_or_default(),
            ),
            "firstrecord" => mv(c, 1),
            "lastrecord" => mv(c, n),
            "nextrecord" => mv(c, c.pos + 1),
            "prevrecord" => mv(c, c.pos.wrapping_sub(1)),
            "gotorecord" => mv(c, arg(args, 0).to_int().max(0) as usize),
            "getcurrentposition" | "getposition" => Value::Int(c.pos as i64),
            "getfield" | "getfieldvalue" => {
                let i = field_index(c, &arg(args, 0))?;
                c.rs.rows
                    .get(c.pos.wrapping_sub(1))
                    .and_then(|r| r.get(i))
                    .map(cell_value)
                    .unwrap_or_default()
            }
            "getcolumn" => {
                let i = field_index(c, &arg(args, 0))?;
                Value::list(
                    c.rs.rows
                        .iter()
                        .map(|r| r.get(i).map(cell_value).unwrap_or_default())
                        .collect(),
                )
            }
            "getrecord" => Value::list(
                c.rs.rows
                    .get(c.pos.wrapping_sub(1))
                    .into_iter()
                    .flatten()
                    .map(cell_value)
                    .collect(),
            ),
            "getrecordaspropList" | "getrecordasproplist" => record(c, c.pos.wrapping_sub(1)),
            "getrecordsasproplist" | "getrecords" => {
                Value::list((0..n).map(|r| record(c, r)).collect())
            }
            "getrecordasstring" => {
                let fd = if args.is_empty() {
                    "\t".to_string()
                } else {
                    arg(args, 0).to_text()
                };
                Value::Str(as_string(c, c.pos.wrapping_sub(1), &fd))
            }
            "getrecordsasstring" => {
                let fd = if args.is_empty() {
                    "\t".to_string()
                } else {
                    arg(args, 0).to_text()
                };
                let rd = if args.len() < 2 {
                    "\r".to_string()
                } else {
                    arg(args, 1).to_text()
                };
                Value::Str(
                    (0..n)
                        .map(|r| as_string(c, r, &fd))
                        .collect::<Vec<_>>()
                        .join(&rd),
                )
            }
            _ => return None,
        })
    }
}
