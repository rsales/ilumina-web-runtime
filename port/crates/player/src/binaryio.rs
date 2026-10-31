//! The BinaryIO Xtra as the original scripts use it: `openFile(perm, path)`,
//! byte reads and writes at a 0-based position, `closeFile()`. Status
//! strings are "OK" on success, as the scripts test.
//!
//! Writes become [`Vfs::patch`]es, so rewriting a few bytes of a large media
//! file read from the discs does not copy it (originals are never touched).

use crate::vfs::Vfs;
use lingo_vm::Value;

#[derive(Default)]
pub struct BinFile {
    path: Option<String>,
    pos: usize,
    writable: bool,
    /// Contents, loaded on the first read.
    data: Option<Vec<u8>>,
    /// Bytes written since the last flush: (start, bytes), contiguous.
    pending: Option<(usize, Vec<u8>)>,
}

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or_default()
}

const OK: &str = "OK";

impl BinFile {
    fn flush(&mut self, vfs: &mut Vfs) {
        if let (Some(p), Some((at, bytes))) = (&self.path, self.pending.take()) {
            vfs.patch(p, at, &bytes);
            if let Some(d) = &mut self.data {
                if d.len() < at + bytes.len() {
                    d.resize(at + bytes.len(), 0);
                }
                d[at..at + bytes.len()].copy_from_slice(&bytes);
            }
        }
    }

    fn data(&mut self, vfs: &mut Vfs) -> &[u8] {
        self.flush(vfs);
        if self.data.is_none() {
            self.data = self.path.as_deref().and_then(|p| vfs.read(p));
        }
        self.data.as_deref().unwrap_or(&[])
    }

    fn write(&mut self, vfs: &mut Vfs, byte: u8) {
        match &mut self.pending {
            Some((at, b)) if *at + b.len() == self.pos => b.push(byte),
            _ => {
                self.flush(vfs);
                self.pending = Some((self.pos, vec![byte]));
            }
        }
        self.pos += 1;
    }

    /// `None` = not a BinaryIO method.
    pub fn method(&mut self, vfs: &mut Vfs, name: &str, args: &[Value]) -> Option<Value> {
        Some(match name.to_ascii_lowercase().as_str() {
            "openfile" => {
                let (perm, path) = (arg(args, 0).to_int(), arg(args, 1).to_text());
                if !vfs.exists(&path) {
                    return Some(Value::str(format!("File not found: {path}")));
                }
                *self = BinFile {
                    path: Some(path),
                    writable: perm >= 2,
                    ..Default::default()
                };
                Value::str(OK)
            }
            "closefile" => {
                if self.path.is_none() {
                    return Some(Value::str("No file open"));
                }
                self.flush(vfs);
                *self = BinFile::default();
                Value::str(OK)
            }
            "getfilesize" => Value::Int(self.data(vfs).len() as i64),
            "getfileposition" => Value::Int(self.pos as i64),
            "setfileposition" => {
                self.pos = arg(args, 0).to_int().max(0) as usize;
                Value::str(OK)
            }
            "readunsignedchar" | "readchar" => {
                let pos = self.pos;
                let b = self.data(vfs).get(pos).copied();
                self.pos += 1;
                match (b, name.eq_ignore_ascii_case("readchar")) {
                    (Some(b), false) => Value::Int(b as i64),
                    (Some(b), true) => Value::Int(b as i8 as i64),
                    (None, _) => Value::Void,
                }
            }
            "writechar" | "writeunsignedchar" => {
                if !self.writable || self.path.is_none() {
                    return Some(Value::str("File not open for writing"));
                }
                self.write(vfs, arg(args, 0).to_int() as u8);
                Value::str(OK)
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::DiscFiles;

    struct One;
    impl DiscFiles for One {
        fn read(&self, rel: &str) -> Option<Vec<u8>> {
            (rel == "cd1/program/files/media/x.mov").then(|| b"abcdef".to_vec())
        }
    }

    #[test]
    fn writes_patch_the_file_and_reads_see_them() {
        let mut vfs = Vfs::new(Box::new(One), ["cd1/program/files/media/x.mov".to_string()]);
        let p = "C:\\Program Files\\iLuminaPO\\Files\\Media\\x.mov";
        let mut f = BinFile::default();
        let call =
            |f: &mut BinFile, vfs: &mut Vfs, n: &str, a: Vec<Value>| f.method(vfs, n, &a).unwrap();
        assert_eq!(
            call(
                &mut f,
                &mut vfs,
                "openFile",
                vec![Value::Int(2), Value::str(p)]
            )
            .to_text(),
            "OK"
        );
        for b in *b"XY" {
            call(&mut f, &mut vfs, "writeChar", vec![Value::Int(b as i64)]);
        }
        call(&mut f, &mut vfs, "setFilePosition", vec![Value::Int(1)]);
        assert_eq!(
            call(&mut f, &mut vfs, "readUnsignedChar", vec![]).to_int(),
            b'Y' as i64
        );
        assert_eq!(call(&mut f, &mut vfs, "getFileSize", vec![]).to_int(), 6);
        assert_eq!(call(&mut f, &mut vfs, "closeFile", vec![]).to_text(), "OK");
        assert_eq!(vfs.read(p).as_deref(), Some(&b"XYcdef"[..]));
        // Not found: an error string, not "OK".
        assert_ne!(
            call(
                &mut f,
                &mut vfs,
                "openFile",
                vec![Value::Int(2), Value::str("C:\\no")]
            )
            .to_text(),
            "OK"
        );
    }
}
