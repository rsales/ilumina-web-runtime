//! `PlayerHost`: what the Director projector and its Xtras provide to the
//! Lingo VM. Behaviour follows the products' documentation and what the
//! original code expects; nothing here looks at registration, trial or
//! activation logic (ADR 0005): the program's own Lingo decides.

use crate::vfs::{bytes_of, text_of, wildcard, Vfs, APP_DIR};
use lingo_vm::value::HostRef;
use lingo_vm::{HostResult, Value, Vm};
use std::collections::{BTreeMap, HashMap};

/// A cast member as the host sees it (for `member("name")`).
#[derive(Clone, Debug)]
pub struct MemberInfo {
    pub lib: u16,
    pub number: u32,
    pub name: String,
    /// Lingo type symbol name (`bitmap`, `script`, `text`…).
    pub kind: String,
    /// Index of its script in the VM program, for script members.
    pub script: Option<usize>,
    /// Bitmap geometry: (width, height, regX, regY, depth).
    pub bitmap: Option<(i64, i64, i64, i64, i64)>,
}

/// State of one FileIO instance.
#[derive(Default)]
struct FileIo {
    path: Option<String>,
    data: Vec<u8>,
    pos: usize,
    writable: bool,
    dirty: bool,
    status: i64,
}

pub struct PlayerHost {
    pub vfs: Vfs,
    pub members: Vec<MemberInfo>,
    /// Cast library names by number (1-based).
    pub cast_libs: Vec<String>,
    /// File name of the current movie (`Home.dxr`).
    pub movie_name: String,
    /// Alerts and errors the program showed (dialogs are not modal here).
    pub alerts: Vec<String>,
    /// Requests nobody answered (name → count); the VM also gets an error.
    pub missing: BTreeMap<String, usize>,
    /// Milliseconds since start, advanced by the caller.
    pub clock_ms: i64,
    /// Today's date (year, month, day), set by the caller from the real clock.
    pub today: (i64, i64, i64),
    /// Stage size (from the movie's `DRCF`).
    pub stage: (i64, i64),
    /// Desktop size reported to the program. 1024×768 like the Wine
    /// reference (`ILUMINA_RES` in `runtime/lib.sh`): the size the program
    /// was made for, so it does not offer to change the display.
    pub screen: (i64, i64),
    settings: HashMap<String, Value>,
    files: HashMap<i64, FileIo>,
    next_id: i64,
    pub v4md: crate::v4md::V4md,
    /// Who answers Mui dialogs (the page, or a scripted user in tests).
    pub responder: Box<dyn crate::mui::Responder>,
    /// Dialogs shown: "title: item types".
    pub dialogs_shown: Vec<String>,
    dialogs: HashMap<i64, crate::mui::Dialog>,
}

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or_default()
}

/// Windows folders BuddyAPI reports (`baSysFolder`).
fn sys_folder(name: &str) -> &'static str {
    match name.to_ascii_lowercase().as_str() {
        "system" => "C:\\WINDOWS\\system32\\",
        "windows" => "C:\\WINDOWS\\",
        "program files" | "programfiles" => "C:\\Program Files\\",
        "personal" | "documents" | "mydocuments" => {
            "C:\\Documents and Settings\\User\\My Documents\\"
        }
        "desktop" => "C:\\Documents and Settings\\User\\Desktop\\",
        "appdata" => "C:\\Documents and Settings\\User\\Application Data\\",
        "temp" => "C:\\WINDOWS\\Temp\\",
        _ => "C:\\WINDOWS\\",
    }
}

/// `[section] key=value` lookup in INI text (case-insensitive).
pub fn ini_get(text: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') && l.ends_with(']') {
            in_section = l[1..l.len() - 1].trim().eq_ignore_ascii_case(section);
        } else if in_section {
            if let Some((k, v)) = l.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// Set `key=value` in `[section]`, adding either when missing.
pub fn ini_set(text: &str, section: &str, key: &str, value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let (mut in_section, mut found_section, mut done) = (false, false, false);
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') && l.ends_with(']') {
            if in_section && !done {
                out.push(format!("{key}={value}"));
                done = true;
            }
            in_section = l[1..l.len() - 1].trim().eq_ignore_ascii_case(section);
            found_section |= in_section;
        } else if in_section && !done {
            if let Some((k, _)) = l.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    out.push(format!("{key}={value}"));
                    done = true;
                    continue;
                }
            }
        }
        out.push(line.to_string());
    }
    if !done {
        if !found_section {
            out.push(format!("[{section}]"));
        }
        out.push(format!("{key}={value}"));
    }
    out.join("\r\n") + "\r\n"
}

impl PlayerHost {
    pub fn new(vfs: Vfs) -> Self {
        PlayerHost {
            vfs,
            members: Vec::new(),
            cast_libs: Vec::new(),
            movie_name: String::new(),
            alerts: Vec::new(),
            missing: BTreeMap::new(),
            clock_ms: 0,
            today: (2026, 1, 1),
            stage: (1024, 768),
            screen: (1024, 768),
            settings: HashMap::new(),
            files: HashMap::new(),
            next_id: 1,
            v4md: Default::default(),
            responder: Box::new(crate::mui::CloseAll),
            dialogs_shown: Vec::new(),
            dialogs: HashMap::new(),
        }
    }

    /// Index the members of one cast library (`lib` is 1-based). `scripts`
    /// maps a member number to its script index in the VM program.
    pub fn add_cast(
        &mut self,
        movie: &rifx::Movie,
        lib: u16,
        name: &str,
        scripts: &HashMap<u32, usize>,
    ) -> Result<(), rifx::Error> {
        while self.cast_libs.len() < lib as usize {
            self.cast_libs.push(String::new());
        }
        self.cast_libs[lib as usize - 1] = name.to_string();
        for m in rifx::cast::members(movie)?.into_iter().flatten() {
            use rifx::cast::MemberType::*;
            let kind = match m.kind {
                Bitmap => "bitmap",
                FilmLoop => "filmLoop",
                Field => "field",
                Palette => "palette",
                Picture => "picture",
                Sound => "sound",
                Button => "button",
                Shape => "shape",
                Movie => "movie",
                DigitalVideo => "digitalVideo",
                Script => "script",
                Text => "text",
                Transition => "transition",
                Xtra => match m.xtra.as_deref() {
                    Some(x) if x.eq_ignore_ascii_case("text") => "text",
                    Some(x) if x.eq_ignore_ascii_case("flash") => "flash",
                    _ => "xtra",
                },
                _ => "empty",
            };
            let bitmap = (m.kind == Bitmap)
                .then(|| rifx::bitmap::BitmapInfo::parse(&m.specific).ok())
                .flatten()
                .map(|b| {
                    (
                        b.width as i64,
                        b.height as i64,
                        b.reg_x as i64,
                        b.reg_y as i64,
                        b.depth as i64,
                    )
                });
            self.members.push(MemberInfo {
                lib,
                number: m.number,
                bitmap,
                script: scripts.get(&m.number).copied(),
                name: m.name,
                kind: kind.into(),
            });
        }
        Ok(())
    }

    fn note(&mut self, what: String) {
        *self.missing.entry(what).or_default() += 1;
    }

    fn new_object(&mut self, kind: &str, args: &[Value]) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        if kind.eq_ignore_ascii_case("fileio") {
            self.files.insert(id, FileIo::default());
        }
        if kind.eq_ignore_ascii_case("vcursor") {
            self.v4md.new_cursor(id, args);
        }
        if kind.eq_ignore_ascii_case("mui") {
            self.dialogs.insert(id, crate::mui::Dialog::default());
        }
        Value::Host(HostRef::Object {
            kind: kind.to_string(),
            id,
        })
    }

    fn find_member(&self, which: &Value, lib: Option<&Value>) -> Option<&MemberInfo> {
        let lib_no = match lib {
            Some(Value::Str(n)) => self
                .cast_libs
                .iter()
                .position(|c| c.eq_ignore_ascii_case(n))
                .map(|i| i as u16 + 1),
            Some(v) if v.to_int() > 0 => Some(v.to_int() as u16),
            _ => None,
        };
        let in_lib = |m: &&MemberInfo| lib_no.is_none_or(|l| m.lib == l);
        match which {
            Value::Str(name) => self
                .members
                .iter()
                .filter(in_lib)
                .find(|m| m.name.eq_ignore_ascii_case(name)),
            v => {
                let n = v.to_int();
                // D6+ member numbers carry the cast library in the high word.
                let (l, num) = if n > 0xffff {
                    ((n >> 16) as u16 + 1, (n & 0xffff) as u32)
                } else {
                    (lib_no.unwrap_or(1), n as u32)
                };
                self.members.iter().find(|m| m.lib == l && m.number == num)
            }
        }
    }

    fn member_value(&self, which: &Value, lib: Option<&Value>) -> Value {
        match self.find_member(which, lib) {
            Some(m) => Value::Host(HostRef::Member {
                lib: m.lib as i64,
                number: m.number as i64,
            }),
            // A missing member still answers `.number` with -1.
            None => Value::Host(HostRef::Member { lib: 0, number: -1 }),
        }
    }

    fn file_method(&mut self, id: i64, name: &str, args: &[Value]) -> Option<Value> {
        let vfs = &mut self.vfs;
        let f = self.files.get_mut(&id)?;
        let v = match name.to_ascii_lowercase().as_str() {
            "openfile" => {
                let path = arg(args, 0).to_text();
                let mode = arg(args, 1).to_int(); // 0 read/write, 1 read, 2 write
                match vfs.read(&path) {
                    Some(data) => {
                        *f = FileIo {
                            path: Some(path),
                            data,
                            pos: 0,
                            writable: mode != 1,
                            dirty: false,
                            status: 0,
                        };
                    }
                    None => f.status = -37,
                }
                Value::Void
            }
            "createfile" => {
                let path = arg(args, 0).to_text();
                vfs.write(&path, Vec::new());
                f.status = 0;
                Value::Void
            }
            "closefile" => {
                if f.dirty {
                    if let Some(p) = &f.path {
                        vfs.write(p, f.data.clone());
                    }
                }
                *f = FileIo {
                    status: 0,
                    ..Default::default()
                };
                Value::Void
            }
            "readfile" => {
                let s = text_of(&f.data[f.pos.min(f.data.len())..]);
                f.pos = f.data.len();
                Value::Str(s)
            }
            "readline" => {
                let rest = &f.data[f.pos.min(f.data.len())..];
                let end = rest
                    .iter()
                    .position(|&b| b == b'\r' || b == b'\n')
                    .map(|i| i + 1)
                    .unwrap_or(rest.len());
                let s = text_of(&rest[..end]);
                f.pos += end;
                Value::Str(s)
            }
            "readchar" => match f.data.get(f.pos) {
                Some(&b) => {
                    f.pos += 1;
                    Value::Str(text_of(&[b]))
                }
                None => Value::Str(String::new()),
            },
            "writestring" | "writechar" => {
                let b = bytes_of(&arg(args, 0).to_text());
                let at = f.pos.min(f.data.len());
                f.data
                    .splice(at..(at + b.len()).min(f.data.len()), b.iter().copied());
                f.pos = at + b.len();
                f.dirty = true;
                Value::Void
            }
            "getlength" => Value::Int(f.data.len() as i64),
            "getposition" => Value::Int(f.pos as i64),
            "setposition" => {
                f.pos = arg(args, 0).to_int().max(0) as usize;
                Value::Void
            }
            "filename" => Value::Str(f.path.clone().unwrap_or_default()),
            "status" => Value::Int(f.status),
            "error" => Value::str(match arg(args, 0).to_int() {
                0 => "OK",
                -33 => "File directory full",
                -37 => "Bad file name",
                -43 => "File not found",
                -56 => "No such drive",
                _ => "Unknown error",
            }),
            "delete" => {
                if let Some(p) = f.path.take() {
                    vfs.delete(&p);
                }
                Value::Void
            }
            "setfiltermask" | "setfinderinfo" => Value::Void,
            // No file dialogs: the program gets "cancelled".
            "displayopen" | "displaysave" => Value::Str(String::new()),
            "version" => Value::str("FileIO 9.0"),
            _ => return None,
        };
        let _ = f.writable;
        Some(v)
    }
}

impl lingo_vm::Host for PlayerHost {
    fn call(&mut self, _vm: &mut Vm, name: &str, args: &[Value]) -> HostResult<Option<Value>> {
        let a0 = arg(args, 0);
        let v = match name.to_ascii_lowercase().as_str() {
            "xtra" => Value::Host(HostRef::Xtra(a0.to_text())),
            "new" => match &a0 {
                Value::Host(HostRef::Xtra(x)) => {
                    let x = x.clone();
                    self.new_object(&x, &args[1..])
                }
                _ => return Ok(None),
            },
            "member" => self.member_value(&a0, args.get(1)),
            "__scriptofmember" => match &a0 {
                Value::Host(HostRef::Member { lib, number }) => {
                    let m = self
                        .members
                        .iter()
                        .find(|m| m.lib as i64 == *lib && m.number as i64 == *number);
                    match m.and_then(|m| m.script) {
                        Some(i) => Value::Script(i),
                        None => Value::Void,
                    }
                }
                _ => Value::Void,
            },
            "castlib" => match &a0 {
                Value::Str(n) => match self
                    .cast_libs
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case(n))
                {
                    Some(i) => Value::Host(HostRef::CastLib(i as i64 + 1)),
                    None => return Err(format!("castLib {n:?} not found")),
                },
                v => Value::Host(HostRef::CastLib(v.to_int())),
            },
            "systemdate" => Value::Date(lingo_vm::value::days_from_civil(
                self.today.0,
                self.today.1,
                self.today.2,
            )),
            "alert" => {
                self.alerts.push(a0.to_text());
                Value::Void
            }
            // --- Director preferences (`setPref` writes to the Prefs folder) ---
            "getpref" => match self.vfs.read(&format!("{APP_DIR}Prefs\\{}", a0.to_text())) {
                Some(b) => Value::Str(text_of(&b)),
                None => Value::Void,
            },
            "setpref" => {
                self.vfs.write(
                    &format!("{APP_DIR}Prefs\\{}", a0.to_text()),
                    bytes_of(&arg(args, 1).to_text()),
                );
                Value::Void
            }
            "getnthfilenameinfolder" => {
                let list = self.vfs.list(&a0.to_text());
                let n = arg(args, 1).to_int();
                Value::Str(
                    list.get((n - 1).max(0) as usize)
                        .cloned()
                        .unwrap_or_default(),
                )
            }
            // --- BuddyAPI ---
            "baregister" => Value::Void,
            "basysfolder" => Value::str(sys_folder(&a0.to_text())),
            "bareadini" => {
                let (section, key, default, file) = (
                    a0.to_text(),
                    arg(args, 1).to_text(),
                    arg(args, 2),
                    arg(args, 3).to_text(),
                );
                match self
                    .vfs
                    .read(&file)
                    .and_then(|b| ini_get(&text_of(&b), &section, &key))
                {
                    Some(v) => Value::Str(v),
                    None => Value::Str(default.to_text()),
                }
            }
            "bawriteini" => {
                let (section, key, value, file) = (
                    a0.to_text(),
                    arg(args, 1).to_text(),
                    arg(args, 2).to_text(),
                    arg(args, 3).to_text(),
                );
                let cur = self
                    .vfs
                    .read(&file)
                    .map(|b| text_of(&b))
                    .unwrap_or_default();
                self.vfs
                    .write(&file, bytes_of(&ini_set(&cur, &section, &key, &value)));
                Value::Int(1)
            }
            "bamemoryinfo" => Value::Int(512 * 1024 * 1024),
            "bafileexists" => Value::bool(self.vfs.exists(&a0.to_text())),
            "bafolderexists" => Value::bool(self.vfs.folder_exists(&a0.to_text())),
            "badeletefile" => Value::bool(self.vfs.delete(&a0.to_text())),
            "bacreatefolder" => Value::Int(1),
            "bafilelist" => {
                let pattern = arg(args, 1).to_text();
                let pattern = if pattern.is_empty() {
                    "*".to_string()
                } else {
                    pattern
                };
                Value::list(
                    self.vfs
                        .list(&a0.to_text())
                        .into_iter()
                        .filter(|n| wildcard(&pattern, n))
                        .map(Value::Str)
                        .collect(),
                )
            }
            "bafolderlist" => Value::list(
                self.vfs
                    .folders(&a0.to_text())
                    .into_iter()
                    .map(Value::Str)
                    .collect(),
            ),
            "badisklist" => Value::list(vec![Value::str("C:\\"), Value::str("D:\\")]),
            // 0 = success for baCopyFile; 1 = success for the others.
            "bacopyfile" => match self.vfs.read(&a0.to_text()) {
                Some(b) => {
                    let dst = arg(args, 1).to_text();
                    let overwrite = arg(args, 2).to_text().to_ascii_lowercase();
                    if self.vfs.exists(&dst) && overwrite == "ifnotexist" {
                        Value::Int(0)
                    } else {
                        self.vfs.write(&dst, b);
                        Value::Int(0)
                    }
                }
                None => Value::Int(1),
            },
            "barenamefile" => match self.vfs.read(&a0.to_text()) {
                Some(b) => {
                    self.vfs.write(&arg(args, 1).to_text(), b);
                    self.vfs.delete(&a0.to_text());
                    Value::Int(1)
                }
                None => Value::Int(0),
            },
            "basetfileattributes" | "basetfiledate" => Value::bool(self.vfs.exists(&a0.to_text())),
            "basetdisplay" => Value::Int(0),
            // Dialogs and external programs: recorded; the page provides the
            // real UI later. The neutral answer is the first/default button.
            "bamsgbox" => {
                self.alerts.push(format!("msgbox: {}", a0.to_text()));
                let buttons = arg(args, 2).to_text().to_ascii_lowercase();
                Value::str(if buttons.starts_with("yes") {
                    "Yes"
                } else if buttons.starts_with("retry") {
                    "Retry"
                } else {
                    "OK"
                })
            }
            "bagetfolder" | "bagetfilename" => Value::str(""),
            "baopenurl" => {
                self.alerts.push(format!("open url: {}", a0.to_text()));
                Value::Int(1)
            }
            "barunprogram" => {
                self.alerts.push(format!("run program: {}", a0.to_text()));
                Value::Int(0)
            }
            "bascreeninfo" => match a0.to_text().to_ascii_lowercase().as_str() {
                "width" => Value::Int(self.screen.0),
                "height" => Value::Int(self.screen.1),
                "depth" => Value::Int(32),
                _ => Value::Int(0),
            },
            _ if self.v4md.call(name).is_some() => self.v4md.call(name).unwrap(),
            _ => {
                self.note(format!("call {name}"));
                return Ok(None);
            }
        };
        Ok(Some(v))
    }

    fn method(
        &mut self,
        _vm: &mut Vm,
        target: &HostRef,
        name: &str,
        args: &[Value],
    ) -> HostResult<Option<Value>> {
        if let HostRef::Object { kind, id } = target {
            if kind.eq_ignore_ascii_case("fileio") {
                if let Some(v) = self.file_method(*id, name, args) {
                    return Ok(Some(v));
                }
            }
            if kind.eq_ignore_ascii_case("vdatabase") {
                let me = Value::Host(target.clone());
                if let Some(v) = self.v4md.database_method(&self.vfs, *id, &me, name, args) {
                    return Ok(Some(v));
                }
            }
            if kind.eq_ignore_ascii_case("vcursor") {
                if let Some(v) = self.v4md.cursor_method(*id, name, args) {
                    return Ok(Some(v));
                }
            }
            if kind.eq_ignore_ascii_case("mui") {
                if let Some(v) = self.mui_method(_vm, *id, name, args)? {
                    return Ok(Some(v));
                }
            }
        }
        if let HostRef::Xtra(x) = target {
            if name.eq_ignore_ascii_case("new") {
                let x = x.clone();
                return Ok(Some(self.new_object(&x, args)));
            }
        }
        self.note(format!(
            "method {}.{name}",
            Value::Host(target.clone()).ilk()
        ));
        Ok(None)
    }

    fn get_prop(
        &mut self,
        _vm: &mut Vm,
        target: &HostRef,
        name: &str,
    ) -> HostResult<Option<Value>> {
        let lname = name.to_ascii_lowercase();
        let v = match target {
            HostRef::Member { lib, number } => {
                let m = self
                    .members
                    .iter()
                    .find(|m| m.lib as i64 == *lib && m.number as i64 == *number)
                    .cloned();
                match (lname.as_str(), m) {
                    ("number", Some(m)) => Value::Int(((m.lib as i64 - 1) << 16) | m.number as i64),
                    ("number", None) => Value::Int(-1),
                    ("membernum", Some(m)) => Value::Int(m.number as i64),
                    ("castlibnum", Some(m)) => Value::Int(m.lib as i64),
                    ("name", Some(m)) => Value::Str(m.name),
                    ("type", Some(m)) => Value::Symbol(m.kind),
                    ("type", None) => Value::sym("empty"),
                    (
                        "width",
                        Some(MemberInfo {
                            bitmap: Some(b), ..
                        }),
                    ) => Value::Int(b.0),
                    (
                        "height",
                        Some(MemberInfo {
                            bitmap: Some(b), ..
                        }),
                    ) => Value::Int(b.1),
                    (
                        "rect",
                        Some(MemberInfo {
                            bitmap: Some(b), ..
                        }),
                    ) => Value::Rect(0.0, 0.0, b.0 as f64, b.1 as f64),
                    (
                        "regpoint",
                        Some(MemberInfo {
                            bitmap: Some(b), ..
                        }),
                    ) => Value::Point(b.2 as f64, b.3 as f64),
                    (
                        "depth",
                        Some(MemberInfo {
                            bitmap: Some(b), ..
                        }),
                    ) => Value::Int(b.4),
                    _ => return Ok(self.missing_prop(target, name)),
                }
            }
            HostRef::Object { kind, .. } if kind == "_player" || kind == "_movie" => {
                match self.the_value(&lname) {
                    Some(v) => v,
                    None => return Ok(self.missing_prop(target, name)),
                }
            }
            HostRef::Object { kind, .. } if kind == "stage" => match lname.as_str() {
                "rect" | "sourcerect" => {
                    Value::Rect(0.0, 0.0, self.stage.0 as f64, self.stage.1 as f64)
                }
                "drawrect" => Value::Rect(0.0, 0.0, self.stage.0 as f64, self.stage.1 as f64),
                "name" | "title" => Value::str("stage"),
                "visible" => Value::Int(1),
                _ => match self.settings.get(&format!("stage.{lname}")) {
                    Some(v) => v.clone(),
                    None => return Ok(self.missing_prop(target, name)),
                },
            },
            HostRef::CastLib(n) => match lname.as_str() {
                "name" => Value::Str(
                    self.cast_libs
                        .get((*n - 1).max(0) as usize)
                        .cloned()
                        .unwrap_or_default(),
                ),
                "number" => Value::Int(*n),
                _ => return Ok(self.missing_prop(target, name)),
            },
            _ => return Ok(self.missing_prop(target, name)),
        };
        Ok(Some(v))
    }

    fn set_prop(
        &mut self,
        _vm: &mut Vm,
        target: &HostRef,
        name: &str,
        _value: &Value,
    ) -> HostResult<bool> {
        self.note(format!("set {}.{name}", Value::Host(target.clone()).ilk()));
        Ok(false)
    }

    fn the(&mut self, _vm: &mut Vm, name: &str) -> HostResult<Option<Value>> {
        let lname = name.to_ascii_lowercase();
        if lname == "stage" {
            return Ok(Some(Value::Host(HostRef::Object {
                kind: "stage".into(),
                id: 0,
            })));
        }
        if matches!(
            lname.as_str(),
            "_player" | "_movie" | "_system" | "_global" | "_sound" | "_mouse" | "_key"
        ) {
            return Ok(Some(Value::Host(HostRef::Object { kind: lname, id: 0 })));
        }
        match self.the_value(&lname) {
            Some(v) => Ok(Some(v)),
            None => {
                self.note(format!("the {name}"));
                Ok(None)
            }
        }
    }

    fn set_the(&mut self, _vm: &mut Vm, name: &str, value: &Value) -> HostResult<bool> {
        let lname = name.to_ascii_lowercase();
        match lname.as_str() {
            "editshortcutsenabled"
            | "floatprecision"
            | "debugplaybackenabled"
            | "exitlock"
            | "keydownscript"
            | "keyupscript"
            | "mousedownscript"
            | "mouseupscript"
            | "timeoutscript"
            | "timeoutlength"
            | "updatelock"
            | "cursor"
            | "alertHook"
            | "alerthook" => {
                self.settings.insert(lname, value.clone());
                Ok(true)
            }
            _ => {
                self.note(format!("set the {name}"));
                Ok(false)
            }
        }
    }
}

impl PlayerHost {
    fn mui_method(
        &mut self,
        vm: &mut Vm,
        id: i64,
        name: &str,
        args: &[Value],
    ) -> HostResult<Option<Value>> {
        use crate::mui::{copy, item_defaults, prop, set_prop, window_defaults, DialogAction};
        let a0 = arg(args, 0);
        Ok(Some(match name.to_ascii_lowercase().as_str() {
            "getwindowproplist" => window_defaults(),
            "getitemproplist" => item_defaults(),
            "initialize" => {
                let d = self.dialogs.entry(id).or_default();
                d.window = copy(&prop(&a0, "windowPropList"));
                d.items = match prop(&a0, "windowItemList") {
                    Value::List(l) => l.borrow().iter().map(copy).collect(),
                    _ => Vec::new(),
                };
                d.stopped = false;
                Value::Void
            }
            "itemupdate" => {
                let pos = a0.to_int().max(1) as usize;
                if let Some(it) = self
                    .dialogs
                    .get_mut(&id)
                    .and_then(|d| d.items.get_mut(pos - 1))
                {
                    *it = copy(&arg(args, 1));
                }
                Value::Void
            }
            "windowoperation" => Value::Void,
            "stop" => {
                if let Some(d) = self.dialogs.get_mut(&id) {
                    d.stopped = true;
                }
                Value::Void
            }
            "alert" => {
                let (t, m, b) = (
                    prop(&a0, "title").to_text(),
                    prop(&a0, "message").to_text(),
                    prop(&a0, "buttons").to_text(),
                );
                self.dialogs_shown.push(format!("alert: {t}"));
                Value::Int(self.responder.alert(&t, &m, &b) as i64)
            }
            "run" => {
                let Some(d) = self.dialogs.get(&id) else {
                    return Ok(Some(Value::Void));
                };
                let callback = prop(&d.window, "callback").to_text();
                let (title, view) = d.view();
                let kinds: Vec<&str> = view.iter().map(|i| i.kind.as_str()).collect();
                let shown = format!("{title}: {}", kinds.join(" "));
                // The same dialog again and again: the program keeps asking
                // for an answer this responder will not give.
                let repeats = self
                    .dialogs_shown
                    .iter()
                    .rev()
                    .take_while(|d| **d == shown)
                    .count();
                self.dialogs_shown.push(shown);
                if repeats >= 2 {
                    return Err(format!(
                        "dialog {title:?} shown {} times in a row: the responder's answer is not accepted",
                        repeats + 1
                    ));
                }
                // Send one event to the program's callback (re-entering the VM).
                let send = |host: &mut PlayerHost,
                            vm: &mut Vm,
                            event: &str,
                            pos: usize|
                 -> HostResult<()> {
                    if callback.is_empty() || callback.eq_ignore_ascii_case("nothing") {
                        return Ok(());
                    }
                    let item = host
                        .dialogs
                        .get(&id)
                        .and_then(|d| d.items.get(pos.wrapping_sub(1)).cloned())
                        .unwrap_or_default();
                    let args = vec![Value::sym(event), Value::Int(pos as i64), item];
                    vm.call(host, &callback, args)
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                };
                send(self, vm, "windowOpening", 0)?;
                let actions = self.responder.dialog(&title, &view);
                let mut closed = false;
                for a in actions {
                    if self.dialogs.get(&id).is_none_or(|d| d.stopped) {
                        break;
                    }
                    match a {
                        DialogAction::SetValue(p, v) => {
                            if let Some(it) = self.dialogs.get(&id).and_then(|d| d.items.get(p - 1))
                            {
                                set_prop(it, "value", Value::Str(v));
                            }
                            send(self, vm, "itemChanged", p)?;
                        }
                        DialogAction::Click(p) => send(self, vm, "itemClicked", p)?,
                        DialogAction::Close => {
                            send(self, vm, "windowClosed", 0)?;
                            closed = true;
                            break;
                        }
                    }
                }
                if !closed && self.dialogs.get(&id).is_some_and(|d| !d.stopped) {
                    send(self, vm, "windowClosed", 0)?;
                }
                Value::Void
            }
            _ => return Ok(None),
        }))
    }

    fn missing_prop(&mut self, target: &HostRef, name: &str) -> Option<Value> {
        self.note(format!("get {}.{name}", Value::Host(target.clone()).ilk()));
        None
    }

    /// Movie, player and system properties.
    fn the_value(&self, lname: &str) -> Option<Value> {
        if let Some(v) = self.settings.get(lname) {
            return Some(v.clone());
        }
        Some(match lname {
            "moviepath" => Value::str(format!("{APP_DIR}Files\\")),
            "applicationpath" => Value::str(APP_DIR),
            "moviename" => Value::str(self.movie_name.clone()),
            "runmode" => Value::str("Projector"),
            "platform" => Value::str("Windows,32"),
            "productversion" => Value::str("9.0"),
            "environment" => Value::prop_list(vec![
                (Value::sym("shockMachine"), Value::Int(0)),
                (Value::sym("platform"), Value::str("Windows,32")),
                (Value::sym("runMode"), Value::str("Projector")),
                (Value::sym("colorDepth"), Value::Int(32)),
                (Value::sym("internetConnected"), Value::sym("offline")),
                (Value::sym("uiLanguage"), Value::str("Portuguese")),
                (Value::sym("osLanguage"), Value::str("Portuguese")),
                (Value::sym("productBuildVersion"), Value::str("9.0")),
            ]),
            "shiftdown" | "optiondown" | "commanddown" | "controldown" | "mousedown"
            | "stilldown" => Value::Int(0),
            "milliseconds" => Value::Int(self.clock_ms),
            "ticks" => Value::Int(self.clock_ms * 60 / 1000),
            "timer" => Value::Int(self.clock_ms * 60 / 1000),
            "colordepth" => Value::Int(32),
            "desktoprectlist" | "deskTopRectList" => Value::list(vec![Value::Rect(
                0.0,
                0.0,
                self.screen.0 as f64,
                self.screen.1 as f64,
            )]),
            "quicktimepresent" => Value::Int(1),
            "soundenabled" => Value::Int(1),
            "soundlevel" => Value::Int(7),
            "itemdelimiter" => Value::str(","),
            "maxinteger" => Value::Int(i32::MAX as i64),
            "pi" => Value::Float(std::f64::consts::PI),
            "romanlingo" => Value::Int(0),
            "safeplayer" => Value::Int(0),
            "searchcurrentfolder" => Value::Int(1),
            "lastevent" | "lastclick" | "lastkey" | "lastroll" => Value::Int(0),
            "key" => Value::str(""),
            "keycode" => Value::Int(0),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_read_and_write() {
        let t = "[Settings]\r\nMessageWindow=0\r\n[Prefs]\r\nRelease= 2.5 \r\n";
        assert_eq!(ini_get(t, "prefs", "release").as_deref(), Some("2.5"));
        assert_eq!(ini_get(t, "Prefs", "Nope"), None);
        let t2 = ini_set(t, "Prefs", "RegUrl", "x");
        assert_eq!(ini_get(&t2, "Prefs", "RegUrl").as_deref(), Some("x"));
        let t3 = ini_set(&t2, "Settings", "MessageWindow", "1");
        assert_eq!(
            ini_get(&t3, "Settings", "MessageWindow").as_deref(),
            Some("1")
        );
        let t4 = ini_set("", "New", "K", "V");
        assert_eq!(ini_get(&t4, "New", "K").as_deref(), Some("V"));
    }
}
