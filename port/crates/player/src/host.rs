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
    /// Text of field and text members (plain, `\r` line breaks); set by
    /// `member.text = …` at run time.
    pub text: Option<String>,
    /// The authored styled text (fonts and sizes for measuring).
    pub styled: Option<rifx::text::StyledText>,
    /// Linked media: the path stored by the author (`F:\…\Files\Media\…`),
    /// or the one Lingo set.
    pub file_name: Option<String>,
    /// Display properties Lingo set (`color`, `font`, `fontSize`…; see
    /// [`MEMBER_STYLE_PROPS`]), for the renderer.
    pub style: Vec<(String, Value)>,
}

/// The error a run ends with when the program calls `quit` or `halt`.
pub const QUIT: &str = "the program quit";

/// A bitmap's registration point as drawn: scaled with a stretched sprite
/// (as the compositor draws it). `b` = (width, height, regX, regY, depth).
// Regression: setting a stretched sprite's rect placed it with the
// unscaled reg point, so Home's scroll-bar track started 47 px too high.
fn scaled_reg(b: (i64, i64, i64, i64, i64), w: i64, h: i64, stretched: bool) -> (i64, i64) {
    if !stretched || b.0 <= 0 || b.1 <= 0 {
        return (b.2, b.3);
    }
    (
        (b.2 as f64 * w as f64 / b.0 as f64).round() as i64,
        (b.3 as f64 * h as f64 / b.1 as f64).round() as i64,
    )
}

/// Underline characters `a..b` (0-based, end exclusive), splitting runs.
fn underline(runs: &mut Vec<rifx::text::Run>, a: usize, b: usize) {
    let mut out = Vec::new();
    for r in runs.drain(..) {
        let cuts = [
            r.start,
            a.clamp(r.start, r.end),
            b.clamp(r.start, r.end),
            r.end,
        ];
        for w in cuts.windows(2) {
            if w[0] < w[1] {
                let mut piece = r.clone();
                piece.start = w[0];
                piece.end = w[1];
                piece.style.underline |= w[0] >= a && w[1] <= b;
                out.push(piece);
            }
        }
        if r.start == r.end {
            out.push(r);
        }
    }
    *runs = out;
}

/// The user changed item `p` (1-based) of a Mui dialog. Check boxes and
/// radio buttons hold 1/0, and a selected radio button clears the others
/// (the Xtra does that itself).
// Regression: radio buttons got the string "1" and stayed selected
// together, so the program's callback found the wrong choice.
/// Whether dialog item `p` (1-based) is enabled (unset = enabled).
fn item_enabled(items: &[Value], p: usize) -> bool {
    let Some(it) = items.get(p.wrapping_sub(1)) else {
        return false;
    };
    let e = crate::mui::prop(it, "enabled");
    e.is_void() || e.truthy()
}

fn set_item_value(items: &[Value], p: usize, v: &str) {
    use crate::mui::{prop, set_prop};
    let Some(it) = items.get(p.wrapping_sub(1)) else {
        return;
    };
    let kind = prop(it, "type").to_text().to_ascii_lowercase();
    if kind != "radiobutton" && kind != "checkbox" {
        set_prop(it, "value", Value::Str(v.to_string()));
        return;
    }
    let on = v == "1" || v.eq_ignore_ascii_case("true");
    if kind == "radiobutton" && on {
        for (k, other) in items.iter().enumerate() {
            if k + 1 != p
                && prop(other, "type")
                    .to_text()
                    .eq_ignore_ascii_case("radioButton")
            {
                set_prop(other, "value", Value::Int(0));
            }
        }
    }
    set_prop(it, "value", Value::Int(on as i64));
}

/// A property of the first behavior instance that declares it.
fn behavior_prop(instances: &[Value], name: &str) -> Option<Value> {
    instances.iter().find_map(|i| match i {
        Value::Object(o) => o.borrow().get(name).cloned(),
        _ => None,
    })
}

fn set_behavior_prop(instances: &[Value], name: &str, v: &Value) -> bool {
    instances.iter().any(|i| match i {
        Value::Object(o) => o.borrow_mut().set(name, v.clone()),
        _ => false,
    })
}

/// Plain text of an HTML string as Director's text members show it: tags
/// dropped, `<br>` and paragraph ends as `\r`, entities decoded.
pub fn html_to_text(html: &str) -> String {
    html_parse(html).0
}

/// Text of an HTML string and its hyperlinks as Director's `hyperlinks`
/// (1-based inclusive character ranges of each `<a>`).
pub fn html_parse(html: &str) -> (String, Vec<(usize, usize)>) {
    let mut out = String::new();
    let mut links = Vec::new();
    let mut link_start: Option<usize> = None;
    let mut rest = html;
    while let Some(c) = rest.chars().next() {
        if c == '<' {
            let end = rest.find('>').map(|e| e + 1).unwrap_or(rest.len());
            let tag = rest[1..end.saturating_sub(1).max(1)]
                .trim()
                .to_ascii_lowercase();
            let name = tag.split_whitespace().next().unwrap_or("");
            let len = out.chars().count();
            match name {
                "a" => link_start = Some(len),
                "/a" => {
                    if let Some(a) = link_start.take().filter(|&a| a < len) {
                        links.push((a + 1, len));
                    }
                }
                _ => {}
            }
            if matches!(
                name,
                "br" | "br/" | "/p" | "/h1" | "/h2" | "/h3" | "/li" | "/tr"
            ) {
                out.push('\r');
            }
            rest = &rest[end..];
        } else if c == '&' {
            let end = rest.find(';').filter(|&e| e <= 10);
            let decoded = end.and_then(|e| {
                let ent = &rest[1..e];
                match ent {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some('\u{a0}'),
                    "ccedil" => Some('ç'),
                    "Ccedil" => Some('Ç'),
                    "ordf" => Some('ª'),
                    "ordm" => Some('º'),
                    "copy" => Some('©'),
                    _ if !ent.starts_with('#') && ent.is_char_boundary(1) => latin1_accent(ent),
                    _ => ent
                        .strip_prefix('#')
                        .and_then(|n| match n.strip_prefix(['x', 'X']) {
                            Some(h) => u32::from_str_radix(h, 16).ok(),
                            None => n.parse().ok(),
                        })
                        .and_then(char::from_u32),
                }
            });
            match (decoded, end) {
                (Some(ch), Some(e)) => {
                    out.push(ch);
                    rest = &rest[e + 1..];
                }
                _ => {
                    out.push('&');
                    rest = &rest[1..];
                }
            }
        } else if c == '\n' || c == '\r' {
            // Source line breaks are white space in HTML.
            if !out.ends_with(' ') && !out.ends_with('\r') && !out.is_empty() {
                out.push(' ');
            }
            rest = &rest[c.len_utf8()..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    let text = out.trim_end_matches('\r').to_string();
    let n = text.chars().count();
    links.retain(|l| l.0 <= n);
    for l in &mut links {
        l.1 = l.1.min(n);
    }
    (text, links)
}

/// `&aacute;`-style entities for Latin-1 accented letters.
fn latin1_accent(ent: &str) -> Option<char> {
    let (base, mark) = ent.split_at(1);
    let table: &[(&str, &str, &str)] = &[
        ("acute", "aeiouyAEIOUY", "áéíóúýÁÉÍÓÚÝ"),
        ("grave", "aeiouAEIOU", "àèìòùÀÈÌÒÙ"),
        ("circ", "aeiouAEIOU", "âêîôûÂÊÎÔÛ"),
        ("tilde", "anoANO", "ãñõÃÑÕ"),
        ("uml", "aeiouyAEIOU", "äëïöüÿÄËÏÖÜ"),
    ];
    let (_, from, to) = table.iter().find(|(m, _, _)| *m == mark)?;
    let i = from.chars().position(|c| base.starts_with(c))?;
    to.chars().nth(i)
}

/// Member display properties scripts may set and read back.
pub const MEMBER_STYLE_PROPS: &[&str] = &[
    "color",
    "bgcolor",
    "forecolor",
    "backcolor",
    "font",
    "fontsize",
    "fontstyle",
    "alignment",
    "wordwrap",
    "boxtype",
    "border",
    "margin",
    "lineheight",
    "editable",
    "antialias",
    "autotab",
    "hilite",
    "fixedlinespace",
    "topspacing",
    "kerning",
];

/// Where Director finds an author's linked-media path in the installed
/// program: the part from `Files\` on, under the program folder (the
/// author's drive and folder no longer exist). ⏳ Check what `fileName`
/// reports on Wine.
pub fn installed_path(stored: &str) -> String {
    let lower = stored.to_ascii_lowercase();
    match lower.find("\\files\\") {
        Some(i) => format!("{APP_DIR}{}", &stored[i + 1..]),
        None => stored.to_string(),
    }
}

/// Styled text of a field (STXT) or text (XMED) member, if it has one.
fn member_text(
    movie: &rifx::Movie,
    m: &rifx::cast::Member,
    fmap: &HashMap<u16, String>,
) -> Option<rifx::text::StyledText> {
    use rifx::FourCC;
    if let Some(s) = movie.children(m.section, FourCC::new(b"XMED")).next() {
        let d = movie.chunk_data(s).ok()?;
        // Flash Asset members also use XMED; their data is a SWF.
        if !d.starts_with(b"FFFF") {
            return None;
        }
        return rifx::text::parse_xmed(d).ok();
    }
    let s = movie.children(m.section, FourCC::new(b"STXT")).next()?;
    rifx::text::parse_stxt(movie.chunk_data(s).ok()?, &|id| fmap.get(&id).cloned()).ok()
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
    pub(crate) settings: HashMap<String, Value>,
    files: HashMap<i64, FileIo>,
    next_id: i64,
    pub v4md: crate::v4md::V4md,
    /// Who answers Mui dialogs (the page, or a scripted user in tests).
    pub responder: Box<dyn crate::mui::Responder>,
    /// Dialogs shown: "title: item types".
    pub dialogs_shown: Vec<String>,
    /// A dialog waiting for the page's user (the run was interrupted).
    pub pending_dialog: Option<crate::mui::PendingDialog>,
    /// Network requests the program made (host part only: the bodies of the
    /// registration post carry the owner's data).
    pub network: Vec<String>,
    dialogs: HashMap<i64, crate::mui::Dialog>,
    binfiles: HashMap<i64, crate::binaryio::BinFile>,
    /// The movie's score (from its internal cast), if any.
    pub score: Option<std::rc::Rc<rifx::score::Score>>,
    /// Sprite channels with something on them, by sprite number.
    pub sprites: std::collections::BTreeMap<u16, crate::stage::SpriteState>,
    /// The current frame script's instance.
    pub frame_script: Option<Value>,
    /// Current frame (1-based; 0 = before the first).
    pub frame: usize,
    /// The frame script's score interval.
    pub frame_script_span: Option<(u32, u32)>,
    /// Where `go` sends the playhead at the end of this frame.
    pub go_target: Option<usize>,
    /// `go to movie`: the movie (and frame/label) to switch to; the page
    /// loads it (M2 slice 4).
    pub go_movie: Option<(String, Value)>,
    /// Font metrics for measuring text (the page's fonts, or an approximation).
    pub fonts: Box<dyn director::text::Fonts>,
    /// `updateStage` calls (the page redraws from the sprite state).
    pub stage_updates: u64,
    /// Decoded linked images by path (`None` = unreadable).
    pub images: std::cell::RefCell<HashMap<String, Option<std::rc::Rc<director::Bitmap>>>>,
    /// Mouse position on the stage; off the stage until the page sends one.
    pub mouse: (i64, i64),
    /// The sprite under the pointer (0 = none), for mouseEnter/mouseLeave.
    pub mouse_over: u16,
    /// The button is down.
    pub mouse_down: bool,
    /// The sprite the last press went to (`the clickOn`).
    pub click_on: u16,
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
        let mut settings = HashMap::new();
        // `the actorList`: objects that get `stepFrame` every frame.
        settings.insert("actorlist".to_string(), Value::list(vec![]));
        // No editable field has the keyboard focus until Lingo sets one.
        settings.insert("keyboardfocussprite".to_string(), Value::Int(-1));
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
            settings,
            files: HashMap::new(),
            next_id: 1,
            v4md: Default::default(),
            responder: Box::new(crate::mui::CloseAll),
            dialogs_shown: Vec::new(),
            pending_dialog: None,
            network: Vec::new(),
            dialogs: HashMap::new(),
            binfiles: HashMap::new(),
            score: None,
            sprites: Default::default(),
            frame_script: None,
            frame: 0,
            frame_script_span: None,
            go_target: None,
            go_movie: None,
            mouse: (-1, -1),
            mouse_over: 0,
            mouse_down: false,
            click_on: 0,
            fonts: Box::new(crate::fonts::ApproxFonts),
            stage_updates: 0,
            images: Default::default(),
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
        first: u32,
    ) -> Result<(), rifx::Error> {
        while self.cast_libs.len() < lib as usize {
            self.cast_libs.push(String::new());
        }
        self.cast_libs[lib as usize - 1] = name.to_string();
        let fmap = match movie.first(rifx::FourCC::new(b"Fmap")) {
            Some(c) => rifx::text::parse_fmap(movie.chunk_data(c.id)?)?,
            None => HashMap::new(),
        };
        for m in rifx::cast::members_from(movie, first)?
            .into_iter()
            .flatten()
        {
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
            let styled = matches!(kind, "field" | "text")
                .then(|| member_text(movie, &m, &fmap))
                .flatten();
            let text = styled.as_ref().map(|t| t.text.clone());
            let file_name = m.linked_path();
            self.members.push(MemberInfo {
                lib,
                number: m.number,
                bitmap,
                text,
                styled,
                file_name,
                style: Vec::new(),
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
        if kind.eq_ignore_ascii_case("binaryio") {
            self.binfiles.insert(id, Default::default());
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

    /// Layout of a field/text member's current text in its box: the width
    /// of a sprite showing it (else 10000, no wrapping), the authored styles,
    /// and font/size/style set from Lingo.
    pub fn measure(&self, lib: i64, number: i64) -> Option<(director::text::Measure, i64)> {
        let st = self.styled_text(lib, number)?;
        let width = self
            .sprites
            .values()
            .find(|s| s.member == Some((lib, number)))
            .map(|s| s.width)
            .filter(|w| *w > 0)
            .unwrap_or(10_000);
        Some((
            director::text::Measure::new(&st, width as f32, self.fonts.as_ref()),
            width,
        ))
    }

    /// A field/text member's current text with its authored styles and the
    /// font, size and style Lingo set.
    pub fn styled_text(&self, lib: i64, number: i64) -> Option<rifx::text::StyledText> {
        use rifx::text::{Run, StyledText};
        let m = self.member_info(lib, number)?;
        let text = m.text.clone()?;
        let mut st = match &m.styled {
            Some(s) if s.text == text => s.clone(),
            other => {
                let style = other
                    .as_ref()
                    .and_then(|s| s.runs.first().map(|r| r.style.clone()))
                    .unwrap_or_default();
                // The authored alignment stays with the new text.
                // Regression: Home's add-on list lost its right alignment.
                let paragraphs = other
                    .as_ref()
                    .and_then(|s| s.paragraphs.first())
                    .map(|&(_, a)| vec![(0, a)])
                    .unwrap_or_default();
                StyledText {
                    runs: vec![Run {
                        start: 0,
                        end: text.chars().count(),
                        style,
                    }],
                    text,
                    paragraphs,
                }
            }
        };
        if st.runs.is_empty() {
            st.runs.push(Run {
                start: 0,
                end: st.text.chars().count(),
                style: Default::default(),
            });
        }
        let get = |k: &str| m.style.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        for r in &mut st.runs {
            // Regression: `member.color = rgb(…)` was ignored when drawing.
            if let Some(Value::Color(cr, cg, cb)) = get("color").or_else(|| get("forecolor")) {
                r.style.color = [cr, cg, cb];
            }
            if let Some(f) = get("font") {
                r.style.font = f.to_text();
            }
            if let Some(sz) = get("fontsize") {
                r.style.size = sz.to_int().clamp(1, 500) as u16;
            }
            if let Some(Value::List(l)) = get("fontstyle") {
                let has = |n: &str| {
                    l.borrow()
                        .iter()
                        .any(|v| v.to_text().eq_ignore_ascii_case(n))
                };
                r.style.bold = has("bold");
                r.style.italic = has("italic");
                r.style.underline = has("underline");
            }
        }
        // Hyperlinks are drawn underlined (Director's link style).
        // Regression: the "aqui" link in Home's welcome text was plain.
        if let Some(Value::List(l)) = get("hyperlinks") {
            for link in l.borrow().iter() {
                if let Value::List(r) = link {
                    let r = r.borrow();
                    if let (Some(a), Some(b)) = (r.first(), r.get(1)) {
                        underline(&mut st.runs, a.to_int() as usize - 1, b.to_int() as usize);
                    }
                }
            }
        }
        Some(st)
    }

    /// `charPosToLoc`, `locToCharPos`, `linePosToLocV` on a member.
    fn text_function(&self, name: &str, target: &Value, args: &[Value]) -> Option<Value> {
        let Value::Host(HostRef::Member { lib, number }) = target else {
            return None;
        };
        let (m, _) = self.measure(*lib, *number)?;
        let a = arg(args, 0);
        Some(match name {
            "charpostoloc" => {
                let (x, y) = m.char_loc(a.to_int().max(1) as usize);
                Value::Point(x.round() as f64, y.round() as f64)
            }
            "loctocharpos" => match a {
                Value::Point(x, y) => Value::Int(m.char_at(x as f32, y as f32) as i64),
                _ => return None,
            },
            "linepostolocv" => {
                Value::Int(m.line_baseline(a.to_int().max(1) as usize).round() as i64)
            }
            _ => return None,
        })
    }

    /// The frame a `go` target names: a number or a label (case-insensitive).
    pub fn frame_of(&self, v: &Value) -> Option<usize> {
        match v {
            Value::Str(name) | Value::Symbol(name) => self
                .score
                .as_ref()?
                .labels
                .iter()
                .find(|(_, l)| l.eq_ignore_ascii_case(name))
                .map(|(f, _)| *f),
            v if v.to_int() > 0 => Some(v.to_int() as usize),
            _ => None,
        }
    }

    /// A member by (cast lib, number).
    pub fn member_info(&self, lib: i64, number: i64) -> Option<&MemberInfo> {
        self.members
            .iter()
            .find(|m| m.lib as i64 == lib && m.number as i64 == number)
    }

    /// The topmost visible sprite whose rect holds `p` (only `only`, if
    /// given). Rects come from the sprite's member geometry.
    pub fn sprite_under(&self, p: (i64, i64), only: Option<i64>) -> Option<i64> {
        let mut hits: Vec<(i64, i64)> = Vec::new();
        let numbers: Vec<u16> = self.sprites.keys().copied().collect();
        for n in numbers {
            if only.is_some_and(|o| o != n as i64) {
                continue;
            }
            let st = &self.sprites[&n];
            if !st.visible || st.member.is_none() {
                continue;
            }
            let z = st.loc_z;
            if let Some(Value::Rect(l, t, r, b)) = self.sprite_prop(n as i64, "rect") {
                let (x, y) = (p.0 as f64, p.1 as f64);
                if l <= x && x < r && t <= y && y < b {
                    hits.push((z, n as i64));
                }
            }
        }
        hits.into_iter().max().map(|(_, n)| n)
    }

    /// Get a sprite property (`sprite(n).prop`, `the prop of sprite n`).
    fn sprite_prop(&self, n: i64, lname: &str) -> Option<Value> {
        let st = self.sprites.get(&(n as u16)).cloned().unwrap_or_default();
        let member_ref = |m: Option<(i64, i64)>| match m {
            Some((lib, number)) => Value::Host(HostRef::Member { lib, number }),
            None => Value::Host(HostRef::Member { lib: 0, number: 0 }),
        };
        let reg = st
            .member
            .and_then(|(l, m)| self.member_info(l, m))
            .and_then(|m| m.bitmap)
            .map(|b| scaled_reg(b, st.width, st.height, st.stretched))
            .unwrap_or((0, 0));
        let (l, t) = (st.loc.0 - reg.0, st.loc.1 - reg.1);
        Some(match lname {
            "member" => member_ref(st.member),
            "membernum" => Value::Int(st.member.map(|m| m.1).unwrap_or(0)),
            "castnum" => Value::Int(st.member.map(|(l, m)| ((l - 1) << 16) | m).unwrap_or(0)),
            "castlibnum" => Value::Int(st.member.map(|m| m.0).unwrap_or(0)),
            "loc" => Value::Point(st.loc.0 as f64, st.loc.1 as f64),
            "loch" => Value::Int(st.loc.0),
            "locv" => Value::Int(st.loc.1),
            "locz" => Value::Int(st.loc_z),
            "width" => Value::Int(st.width),
            "height" => Value::Int(st.height),
            "rect" => Value::Rect(
                l as f64,
                t as f64,
                (l + st.width) as f64,
                (t + st.height) as f64,
            ),
            "left" => Value::Int(l),
            "top" => Value::Int(t),
            "right" => Value::Int(l + st.width),
            "bottom" => Value::Int(t + st.height),
            "visible" | "visibility" => Value::Int(st.visible as i64),
            "puppet" => Value::Int(st.puppet as i64),
            "ink" => Value::Int(st.ink),
            "blend" => Value::Int(st.blend),
            "forecolor" => Value::Int(st.fore_color),
            "backcolor" => Value::Int(st.back_color),
            "cursor" => st.cursor.clone(),
            "spritenum" => Value::Int(n),
            "scriptinstancelist" => Value::list(st.instances.clone()),
            // Otherwise a property of one of its behaviors (Director looks
            // through the scriptInstanceList), then of its Xtra.
            p => match behavior_prop(&st.instances, p) {
                Some(v) => v,
                None => st.xtra_props.iter().find(|(k, _)| k == p)?.1.clone(),
            },
        })
    }

    /// Set a sprite property; false when not supported.
    fn set_sprite_prop(&mut self, n: i64, lname: &str, v: &Value) -> bool {
        let new_member = match (lname, v) {
            ("member", Value::Host(HostRef::Member { lib, number })) => Some((*lib, *number)),
            ("membernum", v) => {
                let lib = self
                    .sprites
                    .get(&(n as u16))
                    .and_then(|s| s.member)
                    .map(|m| m.0)
                    .unwrap_or(1);
                Some((lib, v.to_int()))
            }
            ("member" | "castnum", v) if v.to_int() > 0 => {
                let c = v.to_int();
                Some(((c >> 16) + 1, c & 0xffff))
            }
            _ => None,
        };
        let dims = new_member
            .and_then(|(l, m)| self.member_info(l, m))
            .and_then(|m| m.bitmap)
            .map(|b| (b.0, b.1));
        let is_xtra = self
            .sprites
            .get(&(n as u16))
            .and_then(|s| s.member)
            .and_then(|(l, m)| self.member_info(l, m))
            .is_some_and(|m| matches!(m.kind.as_str(), "xtra" | "flash"));
        let bitmap = self
            .sprites
            .get(&(n as u16))
            .and_then(|s| s.member)
            .and_then(|(l, m)| self.member_info(l, m))
            .and_then(|m| m.bitmap);
        let st = self.sprites.entry(n as u16).or_default();
        match lname {
            // Moves and stretches the sprite (⏳ the reg point scales too).
            "rect" => match v {
                Value::Rect(l, t, r, b) => {
                    st.stretched = true;
                    st.width = (r - l) as i64;
                    st.height = (b - t) as i64;
                    let reg = bitmap
                        .map(|bm| scaled_reg(bm, st.width, st.height, true))
                        .unwrap_or((0, 0));
                    st.loc = (*l as i64 + reg.0, *t as i64 + reg.1);
                }
                _ => return false,
            },
            "member" | "membernum" | "castnum" => {
                let Some(m) = new_member else { return false };
                st.member = (m.1 > 0).then_some(m);
                if let Some((w, h)) = dims {
                    st.width = w;
                    st.height = h;
                }
            }
            "loc" => match v {
                Value::Point(x, y) => st.loc = (*x as i64, *y as i64),
                _ => return false,
            },
            "loch" => st.loc.0 = v.to_int(),
            "locv" => st.loc.1 = v.to_int(),
            "locz" => st.loc_z = v.to_int(),
            "width" => {
                st.width = v.to_int();
                st.stretched = true;
            }
            "height" => {
                st.height = v.to_int();
                st.stretched = true;
            }
            "visible" | "visibility" => st.visible = v.truthy(),
            "puppet" => st.puppet = v.truthy(),
            "ink" => st.ink = v.to_int(),
            "blend" => st.blend = v.to_int(),
            "forecolor" => st.fore_color = v.to_int(),
            "backcolor" => st.back_color = v.to_int(),
            "cursor" => st.cursor = v.clone(),
            "scriptinstancelist" => match v {
                Value::List(l) => st.instances = l.borrow().clone(),
                _ => return false,
            },
            p if set_behavior_prop(&st.instances, p, v) => {}
            // Any other property belongs to the Xtra of an Xtra sprite.
            p if is_xtra => match st.xtra_props.iter_mut().find(|(k, _)| k == p) {
                Some(slot) => slot.1 = v.clone(),
                None => st.xtra_props.push((p.to_string(), v.clone())),
            },
            _ => return false,
        }
        true
    }

    fn member_value(&self, which: &Value, lib: Option<&Value>) -> Value {
        if let Value::Host(HostRef::Member { .. }) = which {
            return which.clone(); // member(aMember)
        }
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
    fn call(&mut self, vm: &mut Vm, name: &str, args: &[Value]) -> HostResult<Option<Value>> {
        let a0 = arg(args, 0);
        let v = match name.to_ascii_lowercase().as_str() {
            "sendsprite" => {
                let n = match &a0 {
                    Value::Host(HostRef::Sprite(n)) => *n,
                    v => v.to_int(),
                };
                let msg = arg(args, 1).to_text();
                let rest = args.get(2..).unwrap_or(&[]).to_vec();
                return crate::stage::send_sprite(vm, self, n as u16, &msg, &rest)
                    .map(Some)
                    .map_err(|e| e.message);
            }
            "rollover" if !args.is_empty() => {
                let n = match &a0 {
                    Value::Host(HostRef::Sprite(n)) => *n,
                    v => v.to_int(),
                };
                Value::Int(self.sprite_under(self.mouse, Some(n)).is_some() as i64)
            }
            "charpostoloc" | "loctocharpos" | "linepostolocv" => {
                match self.text_function(&name.to_ascii_lowercase(), &a0, &args[1..]) {
                    Some(v) => v,
                    None => return Ok(None),
                }
            }
            // go(frame), go("label"), go(frame, "movie"), go("label", "movie")
            "go" | "gotoframe" => {
                if args.len() >= 2 {
                    self.go_movie = Some((arg(args, 1).to_text(), a0.clone()));
                } else if let Some(f) = self.frame_of(&a0) {
                    self.go_target = Some(f);
                } else {
                    return Err(format!("go: no frame {a0}"));
                }
                Value::Void
            }
            "label" => Value::Int(self.frame_of(&Value::str(a0.to_text())).unwrap_or(0) as i64),
            "marker" => {
                // marker(n): the n-th label after (n>0) / before the current one.
                let labels: Vec<usize> = self
                    .score
                    .as_ref()
                    .map(|s| s.labels.iter().map(|l| l.0).collect())
                    .unwrap_or_default();
                let cur = labels.iter().rposition(|&f| f <= self.frame);
                let n = a0.to_int();
                let idx = match cur {
                    Some(c) => c as i64 + n,
                    None => n - 1,
                };
                Value::Int(
                    usize::try_from(idx)
                        .ok()
                        .and_then(|i| labels.get(i))
                        .copied()
                        .unwrap_or(0) as i64,
                )
            }
            // cursor(n) / cursor([member, mask]): the page shows it (M2 slice 4).
            "cursor" => {
                self.settings.insert("cursor".into(), a0.clone());
                Value::Void
            }
            // quit / halt: the projector ends. The run stops here; the page
            // says the program closed.
            "quit" | "halt" => return Err(QUIT.to_string()),
            "updatestage" => {
                self.stage_updates += 1;
                Value::Void
            }
            "sprite" => match &a0 {
                v @ Value::Host(HostRef::Sprite(_)) => v.clone(),
                v => Value::Host(HostRef::Sprite(v.to_int())),
            },
            "sendallsprites" => {
                let msg = a0.to_text();
                let rest = args.get(1..).unwrap_or(&[]).to_vec();
                return crate::stage::send_all_sprites(vm, self, &msg, &rest)
                    .map(Some)
                    .map_err(|e| e.message);
            }
            "xtra" => Value::Host(HostRef::Xtra(a0.to_text())),
            "new" => match &a0 {
                Value::Host(HostRef::Xtra(x)) => {
                    let x = x.clone();
                    self.new_object(&x, &args[1..])
                }
                _ => return Ok(None),
            },
            "member" => self.member_value(&a0, args.get(1)),
            // `register(xtra "X", key)`: an Xtra vendor's licence string for
            // their own Xtra; this reimplementation needs none.
            "register" if matches!(a0, Value::Host(HostRef::Xtra(_))) => Value::Int(0),
            // --- NetLingo: the publisher's servers are gone; every request
            // fails to connect (4146), as it does on the Wine runtime today.
            "postnettext" | "getnettext" | "downloadnetthing" | "preloadnetthing" => {
                let url = a0.to_text();
                let host_part = url
                    .split("://")
                    .nth(1)
                    .unwrap_or(&url)
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .to_string();
                self.network.push(format!("{name} {host_part}"));
                let id = self.next_id;
                self.next_id += 1;
                Value::Int(id)
            }
            "netdone" => Value::Int(1),
            "neterror" => Value::Int(4146),
            "nettextresult" => Value::str(""),
            "netabort" => Value::Void,
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
            if kind.eq_ignore_ascii_case("binaryio") {
                if let Some(f) = self.binfiles.get_mut(id) {
                    if let Some(v) = f.method(&mut self.vfs, name, args) {
                        return Ok(Some(v));
                    }
                }
            }
            if kind.eq_ignore_ascii_case("mui") {
                if let Some(v) = self.mui_method(_vm, *id, name, args)? {
                    return Ok(Some(v));
                }
            }
        }
        // `member.charPosToLoc(n)` and friends.
        if let Some(v) = self.text_function(
            &name.to_ascii_lowercase(),
            &Value::Host(target.clone()),
            args,
        ) {
            return Ok(Some(v));
        }
        // `member.count(#char)` and friends on field/text members.
        if let HostRef::Member { lib, number } = target {
            if name.eq_ignore_ascii_case("count") {
                if let Some(t) = self.member_info(*lib, *number).and_then(|m| m.text.clone()) {
                    use lingo_vm::chunk::{count, Kind};
                    let kind = match arg(args, 0).to_text().to_ascii_lowercase().as_str() {
                        "char" | "chars" => Kind::Char,
                        "word" | "words" => Kind::Word,
                        "item" | "items" => Kind::Item,
                        "line" | "lines" => Kind::Line,
                        _ => return Ok(None),
                    };
                    let delim = self
                        .settings
                        .get("itemdelimiter")
                        .map(|d| d.to_text().chars().next().unwrap_or(','))
                        .unwrap_or(',');
                    return Ok(Some(Value::Int(count(&t, kind, delim) as i64)));
                }
            }
        }
        // `member(aMember)` reaches here first (object-first dispatch).
        if matches!(target, HostRef::Member { .. }) && name.eq_ignore_ascii_case("member") {
            return Ok(Some(Value::Host(target.clone())));
        }
        if let HostRef::Xtra(x) = target {
            // `xtra("X").register(key)`: see `register` in `call`.
            if name.eq_ignore_ascii_case("register") {
                return Ok(Some(Value::Int(0)));
            }
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
                    // An empty slot of a real cast (a sprite can still point
                    // at it): number and an empty name, type #empty.
                    ("number", None) if *lib > 0 && *number > 0 => {
                        Value::Int(((*lib - 1) << 16) | *number)
                    }
                    ("membernum", None) if *lib > 0 && *number > 0 => Value::Int(*number),
                    ("castlibnum", None) if *lib > 0 && *number > 0 => Value::Int(*lib),
                    ("name", None) if *lib > 0 && *number > 0 => Value::str(""),
                    ("number", None) => Value::Int(-1),
                    ("membernum", Some(m)) => Value::Int(m.number as i64),
                    ("castlibnum", Some(m)) => Value::Int(m.lib as i64),
                    ("name", Some(m)) => Value::Str(m.name),
                    ("type", Some(m)) => Value::Symbol(m.kind),
                    ("type", None) => Value::sym("empty"),
                    // Linked media: the size of the file Lingo pointed it at.
                    ("width" | "height" | "rect", Some(m))
                        if m.kind == "bitmap"
                            && m.file_name.is_some()
                            && self.linked_image(m.lib as i64, m.number as i64).is_some() =>
                    {
                        let b = self.linked_image(m.lib as i64, m.number as i64).unwrap();
                        match lname.as_str() {
                            "width" => Value::Int(b.width as i64),
                            "height" => Value::Int(b.height as i64),
                            _ => Value::Rect(0.0, 0.0, b.width as f64, b.height as f64),
                        }
                    }
                    ("text", Some(MemberInfo { text: Some(t), .. })) => Value::Str(t),
                    ("height" | "width" | "linecount", Some(m))
                        if matches!(m.kind.as_str(), "field" | "text") && m.text.is_some() =>
                    {
                        let (ms, w) = self.measure(m.lib as i64, m.number as i64).unwrap();
                        match lname.as_str() {
                            "height" => Value::Int(ms.height().round() as i64),
                            "linecount" => Value::Int(ms.line_count() as i64),
                            _ => Value::Int(w),
                        }
                    }
                    ("media", Some(m)) if matches!(m.kind.as_str(), "field" | "text") => {
                        let mut pl = vec![(
                            Value::sym("text"),
                            Value::Str(m.text.clone().unwrap_or_default()),
                        )];
                        if let Some((_, h)) = m.style.iter().find(|(k, _)| k == "html") {
                            pl.push((Value::sym("html"), h.clone()));
                        }
                        Value::prop_list(pl)
                    }
                    (p, Some(m)) if m.style.iter().any(|(k, _)| k == p) => m
                        .style
                        .iter()
                        .find(|(k, _)| k == p)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default(),
                    ("filename", Some(m)) => Value::Str(
                        m.file_name
                            .as_deref()
                            .map(installed_path)
                            .unwrap_or_default(),
                    ),
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
            HostRef::Sprite(n) => match self.sprite_prop(*n, &lname) {
                Some(v) => v,
                None => return Ok(self.missing_prop(target, name)),
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
        value: &Value,
    ) -> HostResult<bool> {
        if let HostRef::Sprite(n) = target {
            if self.set_sprite_prop(*n, &name.to_ascii_lowercase(), value) {
                return Ok(true);
            }
        }
        // The stage window (`the stage.rect`, `title`…): the projector
        // window moves; the page keeps the stage where it is.
        if let HostRef::Object { kind, .. } = target {
            if kind == "stage" {
                self.settings.insert(
                    format!("stage.{}", name.to_ascii_lowercase()),
                    value.clone(),
                );
                return Ok(true);
            }
        }
        if let HostRef::Member { lib, number } = target {
            if name.eq_ignore_ascii_case("text") {
                if let Some(m) = self
                    .members
                    .iter_mut()
                    .find(|m| m.lib as i64 == *lib && m.number as i64 == *number)
                {
                    if matches!(m.kind.as_str(), "field" | "text") {
                        m.text = Some(value.to_text());
                        return Ok(true);
                    }
                }
            }
            let lname = name.to_ascii_lowercase();
            if matches!(lname.as_str(), "html" | "media") {
                if let Some(m) = self
                    .members
                    .iter_mut()
                    .find(|m| m.lib as i64 == *lib && m.number as i64 == *number)
                {
                    if matches!(m.kind.as_str(), "field" | "text") {
                        // `media` of a text member: its text and HTML (see get).
                        let (text, html) = match (lname.as_str(), value) {
                            ("html", v) => (html_parse(&v.to_text()).0, Some(v.to_text())),
                            (_, v @ Value::PropList(_)) => (
                                crate::mui::prop(v, "text").to_text(),
                                Some(crate::mui::prop(v, "html"))
                                    .filter(|h| !h.is_void())
                                    .map(|h| h.to_text()),
                            ),
                            _ => return Ok(false),
                        };
                        m.text = Some(text);
                        m.style.retain(|(k, _)| k != "html" && k != "hyperlinks");
                        if let Some(h) = html {
                            // `member.hyperlinks`: [[start, end], …]
                            let links = html_parse(&h).1;
                            let list = links
                                .iter()
                                .map(|&(a, b)| {
                                    Value::list(vec![Value::Int(a as i64), Value::Int(b as i64)])
                                })
                                .collect();
                            m.style.push(("hyperlinks".into(), Value::list(list)));
                            m.style.push(("html".into(), Value::Str(h)));
                        }
                        return Ok(true);
                    }
                }
            }
            if MEMBER_STYLE_PROPS.contains(&lname.as_str()) {
                if let Some(m) = self
                    .members
                    .iter_mut()
                    .find(|m| m.lib as i64 == *lib && m.number as i64 == *number)
                {
                    match m.style.iter_mut().find(|(k, _)| *k == lname) {
                        Some(slot) => slot.1 = value.clone(),
                        None => m.style.push((lname, value.clone())),
                    }
                    return Ok(true);
                }
            }
            if let ("regpoint", Value::Point(x, y)) = (lname.as_str(), value) {
                if let Some(MemberInfo {
                    bitmap: Some(b), ..
                }) = self
                    .members
                    .iter_mut()
                    .find(|m| m.lib as i64 == *lib && m.number as i64 == *number)
                {
                    b.2 = *x as i64;
                    b.3 = *y as i64;
                    return Ok(true);
                }
            }
            if name.eq_ignore_ascii_case("fileName") {
                if let Some(m) = self
                    .members
                    .iter_mut()
                    .find(|m| m.lib as i64 == *lib && m.number as i64 == *number)
                {
                    m.file_name = Some(value.to_text());
                    return Ok(true);
                }
            }
        }
        self.note(format!("set {}.{name}", Value::Host(target.clone()).ilk()));
        Ok(false)
    }

    fn the(&mut self, _vm: &mut Vm, name: &str) -> HostResult<Option<Value>> {
        let lname = name.to_ascii_lowercase();
        // The stage is the only window (no MIAWs yet).
        if lname == "stage" || lname == "activewindow" {
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
            | "alerthook"
            | "actorlist"
            | "keyboardfocussprite"
            | "selstart"
            | "selend" => {
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
        use crate::mui::{copy, item_defaults, prop, window_defaults, DialogAction};
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
                // Ask the user until the program stops the dialog or the
                // user closes it (a modal loop).
                let mut round = 0;
                'modal: loop {
                    if self.dialogs.get(&id).is_none_or(|d| d.stopped) {
                        break;
                    }
                    let Some(d) = self.dialogs.get(&id) else {
                        break;
                    };
                    let (title, view) = d.view();
                    let size = (
                        prop(&d.window, "width").to_int(),
                        prop(&d.window, "height").to_int(),
                    );
                    let Some(actions) = self.responder.dialog(&title, &view, round) else {
                        self.pending_dialog = Some(crate::mui::PendingDialog {
                            title,
                            size,
                            items: view,
                        });
                        return Err(crate::mui::DIALOG_PENDING.to_string());
                    };
                    round += 1;
                    if round > 50 {
                        return Err(format!("dialog {title:?}: no end after 50 answers"));
                    }
                    for a in actions {
                        if self.dialogs.get(&id).is_none_or(|d| d.stopped) {
                            break 'modal;
                        }
                        match a {
                            DialogAction::SetValue(p, v) => {
                                if let Some(d) = self.dialogs.get(&id) {
                                    set_item_value(&d.items, p, &v);
                                }
                                send(self, vm, "itemChanged", p)?;
                            }
                            DialogAction::Click(p) => {
                                // A disabled item takes no clicks (the Xtra
                                // drops them); the program enables it first.
                                let on = self
                                    .dialogs
                                    .get(&id)
                                    .is_some_and(|d| item_enabled(&d.items, p));
                                if on {
                                    send(self, vm, "itemClicked", p)?;
                                }
                            }
                            DialogAction::Close => {
                                send(self, vm, "windowClosed", 0)?;
                                break 'modal;
                            }
                        }
                    }
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
            "frame" => Value::Int(self.frame as i64),
            "framelabel" => self
                .score
                .as_ref()
                .and_then(|s| s.labels.iter().find(|l| l.0 == self.frame))
                .map(|l| Value::Str(l.1.clone()))
                .unwrap_or(Value::Int(0)),
            "lastframe" => {
                Value::Int(self.score.as_ref().map(|s| s.frame_count).unwrap_or(0) as i64)
            }
            // The stage fills the 1024×768 desktop (as on the Wine reference).
            "stageleft" => Value::Int((self.screen.0 - self.stage.0) / 2),
            "stagetop" => Value::Int((self.screen.1 - self.stage.1) / 2),
            "stageright" => Value::Int((self.screen.0 + self.stage.0) / 2),
            "stagebottom" => Value::Int((self.screen.1 + self.stage.1) / 2),
            "rollover" => Value::Int(self.sprite_under(self.mouse, None).unwrap_or(0)),
            "mouseh" => Value::Int(self.mouse.0),
            "mousev" => Value::Int(self.mouse.1),
            "mouseloc" => Value::Point(self.mouse.0 as f64, self.mouse.1 as f64),
            "lastchannel" => Value::Int(
                self.score
                    .as_ref()
                    .map(|s| s.channels as i64 - 6)
                    .unwrap_or(0),
            ),
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
            "shiftdown" | "optiondown" | "commanddown" | "controldown" => Value::Int(0),
            "mousedown" | "stilldown" => Value::Int(self.mouse_down as i64),
            "clickon" => Value::Int(self.click_on as i64),
            "mousemember" => match self
                .sprite_under(self.mouse, None)
                .and_then(|n| self.sprites.get(&(n as u16)))
                .and_then(|s| s.member)
            {
                Some((lib, number)) => Value::Host(HostRef::Member { lib, number }),
                None => Value::Void,
            },
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
    use lingo_vm::Host;

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

    struct NoDisc;
    impl crate::vfs::DiscFiles for NoDisc {
        fn read(&self, _: &str) -> Option<Vec<u8>> {
            None
        }
    }

    #[test]
    fn field_text_can_be_set() {
        // Regression: Db.Comp.new stopped at `member("OSMenu_Reference").text
        // = …` (set member.text was not implemented).
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        let mut vm = Vm::new(lingo_vm::Program::default());
        for (number, kind) in [(1, "field"), (2, "bitmap")] {
            host.members.push(MemberInfo {
                lib: 1,
                number,
                name: String::new(),
                kind: kind.into(),
                script: None,
                bitmap: None,
                text: None,
                styled: None,
                file_name: None,
                style: Vec::new(),
            });
        }
        let field = HostRef::Member { lib: 1, number: 1 };
        assert!(host
            .set_prop(&mut vm, &field, "text", &Value::str("a\rb"))
            .unwrap());
        // Regression: SpriteMgr compares `string(the activeWindow)` with
        // "(the Stage)".
        let w = host.the(&mut vm, "activeWindow").unwrap().unwrap();
        assert_eq!(w.to_text(), "(the Stage)");
        let got = host.get_prop(&mut vm, &field, "TEXT").unwrap().unwrap();
        assert_eq!(got.to_text(), "a\rb");
        // Regression: Button.beginSprite read `sprite(n).member.name` of a
        // sprite on an empty cast slot (GlobalAssets 69) and stopped.
        let empty = HostRef::Member { lib: 1, number: 9 };
        let name = host.get_prop(&mut vm, &empty, "name").unwrap().unwrap();
        assert_eq!(name.to_text(), "");
        let ty = host.get_prop(&mut vm, &empty, "type").unwrap().unwrap();
        assert_eq!(ty.to_text(), "empty");
        // Regression: "Rollover Add-ons" sets `member(…).color` (stopped).
        assert!(host
            .set_prop(&mut vm, &field, "color", &Value::Color(1, 2, 3))
            .unwrap());
        let c = host.get_prop(&mut vm, &field, "COLOR").unwrap().unwrap();
        assert_eq!(c.to_string(), "rgb( 1, 2, 3 )");
        // Regression: "Single Scrollbar" sets `sprite(n).rect`.
        let sp = HostRef::Sprite(3);
        assert!(host
            .set_prop(&mut vm, &sp, "rect", &Value::Rect(10.0, 20.0, 40.0, 25.0))
            .unwrap());
        let r = host.get_prop(&mut vm, &sp, "rect").unwrap().unwrap();
        assert_eq!(r.to_string(), "rect(10, 20, 40, 25)");
        // Regression: "Single Scrollbar" counts `mFld1.count(#char)`.
        let n = host
            .method(&mut vm, &field, "count", &[Value::sym("char")])
            .unwrap()
            .unwrap();
        assert_eq!(n.to_int(), 3);
        let bitmap = HostRef::Member { lib: 1, number: 2 };
        assert!(!host
            .set_prop(&mut vm, &bitmap, "text", &Value::str("x"))
            .unwrap());
    }

    #[test]
    fn actor_list_and_offline_netlingo() {
        // Regression: with the owner's data, Poster.POST stopped at
        // `add the actorList, me` (the actorList was not implemented).
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        let mut vm = Vm::new(lingo_vm::Program::default());
        let l = host.the(&mut vm, "actorList").unwrap().unwrap();
        assert_eq!(l.ilk(), "list");
        assert!(host
            .set_the(&mut vm, "actorList", &Value::list(vec![]))
            .unwrap());
        // The publisher's servers are gone: requests fail like on Wine (4146),
        // and only the host name is recorded.
        let id = host
            .call(
                &mut vm,
                "postNetText",
                &[Value::str("http://example.invalid/a?b=c")],
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            host.call(&mut vm, "netDone", std::slice::from_ref(&id))
                .unwrap()
                .unwrap()
                .to_int(),
            1
        );
        assert_eq!(
            host.call(&mut vm, "netError", &[id])
                .unwrap()
                .unwrap()
                .to_int(),
            4146
        );
        assert_eq!(host.network, ["postNetText example.invalid"]);
    }

    #[test]
    fn linked_media_resolve_under_the_program_folder() {
        // Regression: ModuleButtonNew.beginSprite reads `member(m).fileName`
        // (and `member(aMember)` must return the member itself).
        assert_eq!(
            installed_path("F:\\iLumina Portuguese\\Files\\Media\\Thumbnails\\a.jpg"),
            format!("{APP_DIR}Files\\Media\\Thumbnails\\a.jpg")
        );
        assert_eq!(installed_path("x.jpg"), "x.jpg");
    }

    #[test]
    fn html_of_text_members() {
        // Regression: "Clickable text behavior" sets `member.html` (stopped).
        assert_eq!(
            html_to_text("<p>Bem-vindo &amp; <b>ol&aacute;</b><br>x&#233;</p>\n<p>y</p>"),
            "Bem-vindo & olá\rxé\ry"
        );
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        let mut vm = Vm::new(lingo_vm::Program::default());
        host.members.push(MemberInfo {
            lib: 1,
            number: 1,
            name: String::new(),
            kind: "text".into(),
            script: None,
            bitmap: None,
            text: None,
            styled: None,
            file_name: None,
            style: Vec::new(),
        });
        let t = HostRef::Member { lib: 1, number: 1 };
        assert!(host
            .set_prop(&mut vm, &t, "html", &Value::str("<b>a</b>"))
            .unwrap());
        let media = host.get_prop(&mut vm, &t, "media").unwrap().unwrap();
        assert!(host.set_prop(&mut vm, &t, "media", &media).unwrap());
        assert_eq!(
            host.get_prop(&mut vm, &t, "text")
                .unwrap()
                .unwrap()
                .to_text(),
            "a"
        );
        assert_eq!(
            host.get_prop(&mut vm, &t, "html")
                .unwrap()
                .unwrap()
                .to_text(),
            "<b>a</b>"
        );
    }

    #[test]
    fn text_measurements() {
        // Regression: "Single Scrollbar".GetMaxScroll calls
        // charPosToLoc(member, lastChar) (not implemented).
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        let mut vm = Vm::new(lingo_vm::Program::default());
        host.members.push(MemberInfo {
            lib: 1,
            number: 1,
            name: String::new(),
            kind: "text".into(),
            script: None,
            bitmap: None,
            text: Some("ab\rcd".into()),
            styled: None,
            file_name: None,
            style: vec![("fontsize".into(), Value::Int(10))],
        });
        let m = Value::Host(HostRef::Member { lib: 1, number: 1 });
        let p = host
            .call(&mut vm, "charPosToLoc", &[m.clone(), Value::Int(5)])
            .unwrap()
            .unwrap();
        // ApproxFonts at 10 px: lines 9 + 2 = 11 high, 'c' is 6 wide.
        assert_eq!(p.to_string(), "point(6, 22)");
        let h = host
            .get_prop(&mut vm, &HostRef::Member { lib: 1, number: 1 }, "height")
            .unwrap()
            .unwrap();
        assert_eq!(h.to_int(), 22);
        host.members[0]
            .style
            .push(("color".into(), Value::Color(1, 2, 3)));
        let st = host.styled_text(1, 1).unwrap();
        assert_eq!(st.runs[0].style.color, [1, 2, 3]);
        assert_eq!(st.runs[0].style.size, 10);
        let c = host
            .call(&mut vm, "locToCharPos", &[m, Value::Point(0.0, 15.0)])
            .unwrap()
            .unwrap();
        assert_eq!(c.to_int(), 4);
    }

    #[test]
    fn stretched_sprite_rect_round_trips() {
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        let mut vm = Vm::new(lingo_vm::Program::default());
        host.members.push(MemberInfo {
            lib: 1,
            number: 1,
            name: String::new(),
            kind: "bitmap".into(),
            script: None,
            bitmap: Some((9, 9, 4, 4, 16)),
            text: None,
            styled: None,
            file_name: None,
            style: Vec::new(),
        });
        let sp = HostRef::Sprite(4);
        let m = Value::Host(HostRef::Member { lib: 1, number: 1 });
        assert!(host.set_prop(&mut vm, &sp, "member", &m).unwrap());
        let r = Value::Rect(10.0, 100.0, 19.0, 206.0);
        assert!(host.set_prop(&mut vm, &sp, "rect", &r).unwrap());
        let got = host.get_prop(&mut vm, &sp, "rect").unwrap().unwrap();
        assert_eq!(got.to_string(), "rect(10, 100, 19, 206)");
        // Reg point scaled by 106/9: loc is 47 px below the top.
        assert_eq!(host.sprites[&4].loc, (14, 147));
    }

    #[test]
    fn go_and_labels() {
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        let mut vm = Vm::new(lingo_vm::Program::default());
        let mut score = rifx::score::Score::parse(&empty_score()).unwrap();
        score.labels = vec![(1, "Home".into()), (25, "Demo".into())];
        host.score = Some(std::rc::Rc::new(score));
        host.frame = 3;
        host.call(&mut vm, "go", &[Value::str("demo")]).unwrap();
        assert_eq!(host.go_target, Some(25));
        host.call(&mut vm, "go", &[Value::Int(2)]).unwrap();
        assert_eq!(host.go_target, Some(2));
        assert!(host.call(&mut vm, "go", &[Value::str("nope")]).is_err());
        let l = host
            .call(&mut vm, "label", &[Value::str("Home")])
            .unwrap()
            .unwrap();
        assert_eq!(l.to_int(), 1);
        let m = host
            .call(&mut vm, "marker", &[Value::Int(1)])
            .unwrap()
            .unwrap();
        assert_eq!(m.to_int(), 25);
        host.call(&mut vm, "go", &[Value::Int(1), Value::str("Bible")])
            .unwrap();
        assert_eq!(host.go_movie.as_ref().map(|m| m.0.as_str()), Some("Bible"));
    }

    /// A VWSC with no frames and no intervals.
    fn empty_score() -> Vec<u8> {
        let mut e0 = Vec::new();
        for v in [20u32, 0, 0] {
            e0.extend(v.to_be_bytes());
        }
        for v in [0u16, 48, 7, 7] {
            e0.extend(v.to_be_bytes());
        }
        let mut d = Vec::new();
        for v in [0u32, 0, 12, 1, 2, 0, 0, e0.len() as u32] {
            d.extend(v.to_be_bytes());
        }
        d.extend(e0);
        d
    }

    #[test]
    fn radio_buttons_are_exclusive_ints() {
        use crate::mui::{prop, set_prop};
        let item = |kind: &str| {
            let i = crate::mui::item_defaults();
            set_prop(&i, "type", Value::sym(kind));
            i
        };
        let items = vec![item("radioButton"), item("radioButton"), item("editText")];
        set_item_value(&items, 1, "1");
        set_item_value(&items, 2, "1");
        assert_eq!(prop(&items[0], "value").to_int(), 0);
        assert!(matches!(prop(&items[1], "value"), Value::Int(1)));
        set_item_value(&items, 3, "x");
        assert_eq!(prop(&items[2], "value").to_text(), "x");
    }

    #[test]
    fn disabled_items_take_no_clicks() {
        use crate::mui::set_prop;
        let items = vec![crate::mui::item_defaults(), crate::mui::item_defaults()];
        set_prop(&items[1], "enabled", Value::Int(0));
        // Regression: the owner's answer clicks "Continuar", which the
        // program enables only once the code is typed; a click on a disabled
        // button must not reach the callback.
        assert!(item_enabled(&items, 1));
        assert!(!item_enabled(&items, 2));
        assert!(!item_enabled(&items, 3));
    }

    #[test]
    fn quit_ends_the_run() {
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        let mut vm = Vm::new(lingo_vm::Program::default());
        // Regression: the activation wizard calls `quit`; it was not implemented.
        assert_eq!(host.call(&mut vm, "quit", &[]).unwrap_err(), QUIT);
    }
}
