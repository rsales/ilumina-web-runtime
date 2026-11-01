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

/// A style set on a chunk of member text: char range (0-based, end
/// exclusive), property, value.
pub type ChunkStyle = (usize, usize, String, Value);

pub type SharedBitmap = std::rc::Rc<director::Bitmap>;

/// A timeout object: named, and running once `new` gave it a period.
#[derive(Clone, Debug, Default)]
pub struct Timeout {
    pub name: String,
    pub period: i64,
    pub handler: String,
    pub target: Value,
    pub persistent: bool,
    /// Clock time of the next call; `None` = not running (forgotten).
    pub due: Option<i64>,
}

/// (year, month, day) of a day count since 1970-01-01 (proleptic
/// Gregorian), for callers that set [`PlayerHost::today`] from a clock.
pub fn civil_from_unix_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

/// Today's date from the system clock (native callers).
pub fn system_today() -> (i64, i64, i64) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    civil_from_unix_days(secs.div_euclid(86_400))
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
    restyle(runs, a, b, |s| s.underline = true);
}

/// Change the style of chars `a..b` (0-based), splitting runs.
fn restyle(
    runs: &mut Vec<rifx::text::Run>,
    a: usize,
    b: usize,
    f: impl Fn(&mut rifx::text::TextStyle),
) {
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
                if w[0] >= a && w[1] <= b {
                    f(&mut piece.style);
                }
                out.push(piece);
            }
        }
        if r.start == r.end {
            out.push(r);
        }
    }
    *runs = out;
}

/// Whether dialog item `p` (1-based) is enabled (unset = enabled).
fn item_enabled(items: &[Value], p: usize) -> bool {
    let Some(it) = items.get(p.wrapping_sub(1)) else {
        return false;
    };
    let e = crate::mui::prop(it, "enabled");
    e.is_void() || e.truthy()
}

/// The user changed item `p` (1-based) of a Mui dialog. Check boxes and
/// radio buttons hold 1/0, and a selected radio button clears the others
/// (the Xtra does that itself).
// Regression: radio buttons got the string "1" and stayed selected
// together, so the program's callback found the wrong choice.
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
/// (1-based inclusive character ranges of each `<a>`, with its `href`).
pub fn html_parse(html: &str) -> (String, Vec<(usize, usize, String)>) {
    let mut out = String::new();
    let mut links = Vec::new();
    let mut link_start: Option<(usize, String)> = None;
    let mut rest = html;
    while let Some(c) = rest.chars().next() {
        if c == '<' {
            let end = rest.find('>').map(|e| e + 1).unwrap_or(rest.len());
            let raw = rest[1..end.saturating_sub(1).max(1)].trim();
            let tag = raw.to_ascii_lowercase();
            let name = tag.split_whitespace().next().unwrap_or("");
            let len = out.chars().count();
            match name {
                "a" => link_start = Some((len, href_of(raw))),
                "/a" => {
                    if let Some((a, href)) = link_start.take().filter(|(a, _)| *a < len) {
                        links.push((a + 1, len, href));
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

/// The `href` of an `<a …>` tag (quoted or bare), "" without one.
fn href_of(tag: &str) -> String {
    let lower = tag.to_ascii_lowercase();
    let Some(i) = lower.find("href") else {
        return String::new();
    };
    let rest = tag[i + 4..].trim_start();
    let Some(rest) = rest.strip_prefix('=') else {
        return String::new();
    };
    let rest = rest.trim_start();
    match rest.chars().next() {
        Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or("").to_string(),
        _ => rest.split_whitespace().next().unwrap_or("").to_string(),
    }
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
/// One text style property (lower-case name) set by Lingo.
fn apply_style(s: &mut rifx::text::TextStyle, k: &str, v: &Value) {
    match (k, v) {
        ("color" | "forecolor", Value::Color(r, g, b)) => s.color = [*r, *g, *b],
        ("font", v) => s.font = v.to_text(),
        ("fontsize", v) => s.size = v.to_int().clamp(1, 500) as u16,
        ("fontstyle", Value::List(l)) => {
            let has = |n: &str| {
                l.borrow()
                    .iter()
                    .any(|v| v.to_text().eq_ignore_ascii_case(n))
            };
            s.bold = has("bold");
            s.italic = has("italic");
            s.underline = has("underline");
        }
        _ => {}
    }
}

/// A chunk of a member's text (`member.line[3]`): a host object whose kind
/// names the member and its char range (0-based, end exclusive).
fn chunk_ref(lib: i64, number: i64, a: usize, b: usize) -> HostRef {
    HostRef::Object {
        kind: format!("chunk {lib} {number} {a} {b}"),
        id: 0,
    }
}

fn parse_chunk_ref(target: &HostRef) -> Option<(i64, i64, usize, usize)> {
    let HostRef::Object { kind, .. } = target else {
        return None;
    };
    let mut it = kind.strip_prefix("chunk ")?.split(' ');
    Some((
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    ))
}

pub const MEMBER_STYLE_PROPS: &[&str] = &[
    "color",
    "bgcolor",
    "forecolor",
    "backcolor",
    "font",
    "fontsize",
    "fontstyle",
    "usehypertextstyles",
    // ⏳ Drawn from the top for now; kept for reading back.
    "scrolltop",
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
    /// Styles set on chunks of member text (`member.line[n].color`):
    /// (lib, number) → (char range, property, value), in order. Cleared
    /// when the text is replaced.
    pub member_ranges: HashMap<(i64, i64), Vec<ChunkStyle>>,
    /// Each cast library's file (shared bytes) and first member number,
    /// for decoding bitmaps on demand (hit testing).
    pub cast_data: Vec<(u16, std::rc::Rc<[u8]>, u32)>,
    /// Decoded cast bitmaps by (lib, member) (`None` = not a bitmap).
    pub bitmaps: std::cell::RefCell<HashMap<(i64, i64), Option<SharedBitmap>>>,
    /// The projector's "System - Win" palette (8-bit bitmaps).
    palette: std::cell::OnceCell<Option<rifx::bitmap::Palette>>,
    /// The last key from the page (`the key`, `the keyCode`, modifiers).
    pub key: crate::stage::Key,
    /// `pass` (Some(true)) or `dontPassEvent`/`stopEvent` (Some(false))
    /// called during the current event.
    pub pass_event: Option<bool>,
    /// Director timeout objects (`timeout("name").new(period, #h, obj)`).
    pub timeouts: Vec<Timeout>,
    /// TreeView Xtra sprites' trees, by channel.
    pub trees: HashMap<u16, crate::treeview::Tree>,
    /// `puppetSound` per channel (member or name; 0 = stopped).
    pub sounds: HashMap<i64, Value>,
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
            member_ranges: HashMap::new(),
            sounds: HashMap::new(),
            trees: HashMap::new(),
            timeouts: Vec::new(),
            key: Default::default(),
            pass_event: None,
            cast_data: Vec::new(),
            bitmaps: Default::default(),
            palette: Default::default(),
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
            self.v4md.new_cursor(&self.vfs, id, args);
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
        let boxed = self.member_info(lib, number).and_then(|m| {
            m.style
                .iter()
                .find(|(k, _)| k == "boxwidth")
                .map(|(_, v)| v.to_int())
        });
        let width = boxed
            .or_else(|| {
                self.sprites
                    .values()
                    .find(|s| s.member == Some((lib, number)))
                    .map(|s| s.width)
            })
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
        // Regression: `member.color = rgb(…)` was ignored when drawing.
        for (k, v) in &m.style {
            for r in &mut st.runs {
                apply_style(&mut r.style, k, v);
            }
        }
        // `member.line[n].color = …` and other chunk styles, in order.
        // Regression: Home's add-on rollover colors a line of the list.
        if let Some(ranges) = self.member_ranges.get(&(lib, number)) {
            for (a, b, k, v) in ranges {
                restyle(&mut st.runs, *a, *b, |s| apply_style(s, k, v));
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

    /// The index of the timeout named `name`, created (not running) if new.
    fn timeout_id(&mut self, name: &str) -> i64 {
        if let Some(i) = self
            .timeouts
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(name))
        {
            return i as i64;
        }
        self.timeouts.push(Timeout {
            name: name.to_string(),
            ..Default::default()
        });
        self.timeouts.len() as i64 - 1
    }

    /// Timeouts due at the current clock: (index, handler, target), each
    /// rescheduled one period later.
    pub fn due_timeouts(&mut self) -> Vec<(i64, String, Value)> {
        let now = self.clock_ms;
        let mut out = Vec::new();
        for (i, t) in self.timeouts.iter_mut().enumerate() {
            if let Some(d) = t.due.filter(|&d| d <= now) {
                // Late calls are not queued up: one call, next period on.
                t.due = Some(d.max(now - t.period) + t.period);
                out.push((i as i64, t.handler.clone(), t.target.clone()));
            }
        }
        out
    }

    /// A member's hyperlinks: (first, last char, 1-based inclusive; href).
    fn member_links(&self, lib: i64, number: i64) -> Vec<(usize, usize, String)> {
        let Some(m) = self.member_info(lib, number) else {
            return Vec::new();
        };
        let get = |k: &str| m.style.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        let (Some(Value::List(ranges)), targets) = (get("hyperlinks"), get("hyperlinktargets"))
        else {
            return Vec::new();
        };
        let targets = match targets {
            Some(Value::List(t)) => t.borrow().clone(),
            _ => Vec::new(),
        };
        let out = ranges
            .borrow()
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                let Value::List(r) = r else { return None };
                let r = r.borrow();
                Some((
                    r.first()?.to_int().max(1) as usize,
                    r.get(1)?.to_int().max(0) as usize,
                    targets.get(i).map(|t| t.to_text()).unwrap_or_default(),
                ))
            })
            .collect();
        out
    }

    /// `pointToChar(sprite, point)`: the char (1-based) of a text sprite
    /// under a stage point, -1 outside the sprite.
    fn point_to_char(&self, sprite: i64, p: (f64, f64)) -> i64 {
        let Some(st) = self.sprites.get(&(sprite as u16)) else {
            return -1;
        };
        let Some((lib, number)) = st.member else {
            return -1;
        };
        let (x, y) = (p.0 as i64 - st.loc.0, p.1 as i64 - st.loc.1);
        if x < 0 || y < 0 || x >= st.width || y >= st.height {
            return -1;
        }
        match self.measure(lib, number) {
            Some((m, _)) => m.char_under(x as f32, y as f32).map_or(-1, |c| c as i64),
            None => -1,
        }
    }

    /// A chunk of a member's text from `(#line, first[, last])`: its char
    /// range (0-based, end exclusive) and text.
    fn member_chunk(
        &self,
        lib: i64,
        number: i64,
        args: &[Value],
    ) -> Option<(usize, usize, String)> {
        let text = self.member_info(lib, number)?.text.clone()?;
        self.text_chunk(&text, args)
    }

    /// The chunk `(#kind, first[, last])` of `text`: char range (0-based,
    /// end exclusive) and its text.
    fn text_chunk(&self, text: &str, args: &[Value]) -> Option<(usize, usize, String)> {
        use lingo_vm::chunk::{locate, ChunkSpec};
        let first = arg(args, 1).to_int();
        let last = if args.len() >= 3 {
            arg(args, 2).to_int()
        } else {
            0
        };
        let mut spec = ChunkSpec::default();
        match arg(args, 0).to_text().to_ascii_lowercase().as_str() {
            "char" => spec.char = (first, last),
            "word" => spec.word = (first, last),
            "item" => spec.item = (first, last),
            "line" | "paragraph" => spec.line = (first, last),
            _ => return None,
        }
        let delim = self
            .settings
            .get("itemdelimiter")
            .map(|d| d.to_text().chars().next().unwrap_or(','))
            .unwrap_or(',');
        let r = locate(text, &spec, delim);
        let a = text[..r.start].chars().count();
        let b = a + text[r.clone()].chars().count();
        Some((a, b, text[r].to_string()))
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
            "locvtolinepos" => Value::Int(m.line_at(a.to_int() as f32) as i64),
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
                if l <= x
                    && x < r
                    && t <= y
                    && y < b
                    && self.opaque_at(n, (x - l, y - t), (r - l, b - t))
                {
                    hits.push((z, n as i64));
                }
            }
        }
        hits.into_iter().max().map(|(_, n)| n)
    }

    /// A typed key into sprite `n`'s field if it is editable (Lingo set
    /// `editable`): printable characters and return are added at the end,
    /// backspace removes the last one (⏳ caret and selection).
    pub fn type_into_sprite(&mut self, n: u16, key: &str) {
        let Some((lib, number)) = self.sprites.get(&n).and_then(|s| s.member) else {
            return;
        };
        let Some(m) = self
            .members
            .iter_mut()
            .find(|m| m.lib as i64 == lib && m.number as i64 == number)
        else {
            return;
        };
        let editable = m.style.iter().any(|(k, v)| k == "editable" && v.truthy());
        if !editable || !matches!(m.kind.as_str(), "field" | "text") {
            return;
        }
        let mut t = m.text.clone().unwrap_or_default();
        match key {
            "\u{8}" => {
                t.pop();
            }
            "\r" => t.push('\r'),
            k if k.chars().all(|c| !c.is_control()) => t.push_str(k),
            _ => return,
        }
        m.text = Some(t);
        self.member_ranges.remove(&(lib, number));
    }

    /// Matte ink (8) takes the mouse only where the bitmap is drawn: not on
    /// its white (cleared) or fully transparent pixels. Other inks use the
    /// rectangle, as in Director. `at` is relative to the sprite's rect of
    /// size `size`. ⏳ Director's matte is the white connected to the edges;
    /// the renderer clears all white, and this matches the renderer.
    // Regression: overlapping matte sprites took clicks on their corners.
    fn opaque_at(&self, n: u16, at: (f64, f64), size: (f64, f64)) -> bool {
        let Some(st) = self.sprites.get(&n) else {
            return true;
        };
        if st.ink != 8 {
            return true;
        }
        let Some((lib, number)) = st.member else {
            return true;
        };
        let Some(b) = self.member_bitmap(lib, number) else {
            return true;
        };
        let x = ((at.0 * b.width as f64 / size.0.max(1.0)) as usize).min(b.width.saturating_sub(1));
        let y =
            ((at.1 * b.height as f64 / size.1.max(1.0)) as usize).min(b.height.saturating_sub(1));
        match b.rgba.get((y * b.width + x) * 4..(y * b.width + x) * 4 + 4) {
            Some(p) => p[3] > 0 && !(p[0] == 255 && p[1] == 255 && p[2] == 255),
            None => true,
        }
    }

    /// A bitmap member's pixels: linked media, else decoded from its cast.
    pub fn member_bitmap(&self, lib: i64, number: i64) -> Option<std::rc::Rc<director::Bitmap>> {
        if let Some(b) = self.bitmaps.borrow().get(&(lib, number)) {
            return b.clone();
        }
        let b = self
            .linked_image(lib, number)
            .or_else(|| self.decode_cast_bitmap(lib, number).map(std::rc::Rc::new));
        self.bitmaps.borrow_mut().insert((lib, number), b.clone());
        b
    }

    fn decode_cast_bitmap(&self, lib: i64, number: i64) -> Option<director::Bitmap> {
        use rifx::bitmap::{bitd_for, decode, BitmapInfo};
        let (_, data, first) = self.cast_data.iter().find(|(l, _, _)| *l as i64 == lib)?;
        let movie = rifx::Movie::parse(data).ok()?;
        let members = rifx::cast::members_from(&movie, *first).ok()?;
        let m = members
            .iter()
            .flatten()
            .find(|m| m.number as i64 == number)?;
        if m.kind != rifx::cast::MemberType::Bitmap {
            return None;
        }
        let info = BitmapInfo::parse(&m.specific).ok()?;
        let bitd = bitd_for(&movie, m.section)?.ok()?;
        let palette = self.palette.get_or_init(|| {
            self.vfs
                .read_shared(&format!("{APP_DIR}iLumina.exe"))
                .and_then(|exe| rifx::bitmap::system_win_palette(&exe))
        });
        let rgba = decode(&info, bitd, palette.as_ref()).ok()?;
        Some(director::Bitmap {
            width: info.width,
            height: info.height,
            rgba,
            reg_x: info.reg_x,
            reg_y: info.reg_y,
        })
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
            // Regression: the Extras easter egg reads `sprite.quad` to flip.
            "quad" => {
                let (l, t, r, b) = (
                    l as f64,
                    t as f64,
                    (l + st.width) as f64,
                    (t + st.height) as f64,
                );
                let q = st.quad.unwrap_or([(l, t), (r, t), (r, b), (l, b)]);
                Value::list(q.iter().map(|&(x, y)| Value::Point(x, y)).collect())
            }
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
            // The corners as given; the sprite moves to their bounding rect.
            "quad" => {
                let Value::List(l) = v else { return false };
                let pts: Vec<(f64, f64)> = l
                    .borrow()
                    .iter()
                    .filter_map(|p| match p {
                        Value::Point(x, y) => Some((*x, *y)),
                        _ => None,
                    })
                    .collect();
                let Ok(q) = <[(f64, f64); 4]>::try_from(pts) else {
                    return false;
                };
                let (mut l, mut t, mut r, mut b) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                for (x, y) in q {
                    (l, t, r, b) = (l.min(x), t.min(y), r.max(x), b.max(y));
                }
                st.quad = Some(q);
                st.stretched = true;
                st.width = (r - l) as i64;
                st.height = (b - t) as i64;
                let reg = bitmap
                    .map(|bm| scaled_reg(bm, st.width, st.height, true))
                    .unwrap_or((0, 0));
                st.loc = (l as i64 + reg.0, t as i64 + reg.1);
            }
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
            "charpostoloc" | "loctocharpos" | "linepostolocv" | "locvtolinepos" => {
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
            // `timeout("name")`: the timeout of that name (case-insensitive).
            "timeout" => Value::Host(HostRef::Object {
                kind: "timeout".into(),
                id: self.timeout_id(&a0.to_text()),
            }),
            // `GetNodeLocationXTV(sprite(n), node)`: the TreeView Xtra.
            l if l.ends_with("xtv") && matches!(a0, Value::Host(HostRef::Sprite(_))) => {
                let Value::Host(HostRef::Sprite(n)) = a0 else {
                    unreachable!()
                };
                let tree = self.trees.entry(n as u16).or_default();
                match tree.method(l, &args[1..]) {
                    Some(v) => v,
                    None => return Ok(None),
                }
            }
            "pointtochar" | "pointinhyperlink" => {
                let n = match &a0 {
                    Value::Host(HostRef::Sprite(n)) => *n,
                    v => v.to_int(),
                };
                let Value::Point(x, y) = arg(args, 1) else {
                    return Ok(None);
                };
                let c = self.point_to_char(n, (x, y));
                if name.eq_ignore_ascii_case("pointtochar") {
                    Value::Int(c)
                } else {
                    let member = self.sprites.get(&(n as u16)).and_then(|s| s.member);
                    let inside = c > 0
                        && member.is_some_and(|(l, m)| {
                            self.member_links(l, m)
                                .iter()
                                .any(|(s, e, _)| *s as i64 <= c && c <= *e as i64)
                        });
                    Value::Int(inside as i64)
                }
            }
            // Event passing: `pass` lets an editable field get the key;
            // `dontPassEvent`/`stopEvent` keep it.
            "pass" => {
                self.pass_event = Some(true);
                Value::Void
            }
            "dontpassevent" | "stopevent" => {
                self.pass_event = Some(false);
                Value::Void
            }
            // Sound channels: remembered, not played yet (⏳ WebAudio, M4).
            // Regression: Home's credits easter egg calls puppetSound.
            "puppetsound" => {
                let (ch, m) = match args {
                    [m] => (1, m.clone()),
                    [ch, m, ..] => (ch.to_int(), m.clone()),
                    [] => (1, Value::Int(0)),
                };
                self.sounds.insert(ch, m);
                Value::Void
            }
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
            if kind == "timeout" {
                let now = self.clock_ms;
                if let Some(t) = self.timeouts.get_mut(*id as usize) {
                    match name.to_ascii_lowercase().as_str() {
                        // new(period, #handler[, target])
                        "new" => {
                            t.period = arg(args, 0).to_int().max(1);
                            t.handler = arg(args, 1).to_text();
                            t.target = arg(args, 2);
                            t.due = Some(now + t.period);
                            return Ok(Some(Value::Host(target.clone())));
                        }
                        "forget" => {
                            t.due = None;
                            return Ok(Some(Value::Void));
                        }
                        _ => {}
                    }
                }
            }
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
        // Chunks of chunks (`member.line[2].char[1]`), relative to the chunk.
        // Regression: Encyclopedia's ShowHtmlText reads line n's first char.
        if let Some((lib, number, a, b)) = parse_chunk_ref(target) {
            let lname = name.to_ascii_lowercase();
            let text: String = self
                .member_info(lib, number)
                .and_then(|m| m.text.clone())
                .unwrap_or_default()
                .chars()
                .skip(a)
                .take(b.saturating_sub(a))
                .collect();
            if matches!(lname.as_str(), "getpropref" | "getprop") {
                if let Some((sa, sb, sub)) = self.text_chunk(&text, args) {
                    return Ok(Some(if lname == "getprop" {
                        Value::Str(sub)
                    } else {
                        Value::Host(chunk_ref(lib, number, a + sa, a + sb))
                    }));
                }
            }
        }
        // `member.setProp(#line, n, text)`: replaces that chunk of the text.
        if let HostRef::Member { lib, number } = target {
            if name.eq_ignore_ascii_case("setprop") && args.len() >= 3 {
                let (lib, number) = (*lib, *number);
                if let Some((a, b, _)) = self.member_chunk(lib, number, &args[..args.len() - 1]) {
                    let new = args[args.len() - 1].to_text();
                    if let Some(m) = self
                        .members
                        .iter_mut()
                        .find(|m| m.lib as i64 == lib && m.number as i64 == number)
                    {
                        let t = m.text.clone().unwrap_or_default();
                        let mut out: String = t.chars().take(a).collect();
                        out.push_str(&new);
                        out.extend(t.chars().skip(b));
                        m.text = Some(out);
                        // ⏳ chunk styles after the edit shift in Director.
                        self.member_ranges.remove(&(lib, number));
                        return Ok(Some(Value::Void));
                    }
                }
            }
        }
        // `member.line[3]` (getPropRef) and `member.line[3]` read as text.
        if let HostRef::Member { lib, number } = target {
            let lname = name.to_ascii_lowercase();
            if matches!(lname.as_str(), "getpropref" | "getprop") {
                if let Some((a, b, text)) = self.member_chunk(*lib, *number, args) {
                    return Ok(Some(if lname == "getprop" {
                        Value::Str(text)
                    } else {
                        Value::Host(chunk_ref(*lib, *number, a, b))
                    }));
                }
            }
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
                        // A paragraph is a line of the text (`\r`-separated).
                        // Regression: Encyclopedia counts #paragraph.
                        "line" | "lines" | "paragraph" | "paragraphs" => Kind::Line,
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
        // TreeView Xtra sprites: `sprite(n).addChildXTV(…)`.
        if let HostRef::Sprite(n) = target {
            let lname = name.to_ascii_lowercase();
            // `sprite(n).pointToChar(pt)` = `pointToChar(sprite(n), pt)`.
            if matches!(lname.as_str(), "pointtochar" | "pointinhyperlink") {
                let mut all = vec![Value::Host(target.clone())];
                all.extend_from_slice(args);
                return self.call(_vm, name, &all);
            }
            if lname.ends_with("xtv") {
                let tree = self.trees.entry(*n as u16).or_default();
                if let Some(v) = tree.method(&lname, args) {
                    return Ok(Some(v));
                }
            }
        }
        // OSControl sprites redraw on `updateControl`; the page draws from
        // the sprite state every frame. Regression: Bible's startMovie.
        if matches!(target, HostRef::Sprite(_)) && name.eq_ignore_ascii_case("updatecontrol") {
            return Ok(Some(Value::Void));
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
        if let HostRef::Object { kind, id } = target {
            if kind == "timeout" {
                if let Some(t) = self.timeouts.get(*id as usize) {
                    return Ok(match lname.as_str() {
                        "name" => Some(Value::Str(t.name.clone())),
                        "period" => Some(Value::Int(t.period)),
                        "target" => Some(t.target.clone()),
                        "timeouthandler" => Some(Value::sym(&t.handler)),
                        "persistent" => Some(Value::Int(t.persistent as i64)),
                        _ => None,
                    });
                }
            }
        }
        if let Some((lib, number, a, b)) = parse_chunk_ref(target) {
            // Hyperlinks of the chunk's first char (1-based inclusive).
            let link = self
                .member_links(lib, number)
                .into_iter()
                .find(|(s, e, _)| *s <= a + 1 && a < *e);
            match lname.as_str() {
                "ref" => return Ok(Some(Value::Host(target.clone()))),
                "hyperlinkrange" => {
                    return Ok(Some(Value::list(match &link {
                        Some((s, e, _)) => vec![Value::Int(*s as i64), Value::Int(*e as i64)],
                        None => vec![],
                    })))
                }
                "hyperlink" => return Ok(Some(Value::Str(link.map(|l| l.2).unwrap_or_default()))),
                "hyperlinkstate" => {
                    let set = self.member_ranges.get(&(lib, number)).and_then(|r| {
                        r.iter()
                            .rev()
                            .find(|(s, e, k, _)| k == "hyperlinkstate" && *s <= a && a < *e)
                            .map(|(_, _, _, v)| v.clone())
                    });
                    return Ok(Some(set.unwrap_or_else(|| Value::sym("normal"))));
                }
                _ => {}
            }
            let st = self.styled_text(lib, number);
            let style = st.as_ref().and_then(|st| {
                st.runs
                    .iter()
                    .find(|r| r.start <= a && a < r.end)
                    .map(|r| r.style.clone())
            });
            return Ok(match (lname.as_str(), style) {
                ("text" | "string", _) => st.map(|st| {
                    Value::Str(st.text.chars().skip(a).take(b.saturating_sub(a)).collect())
                }),
                ("color" | "forecolor", Some(s)) => {
                    Some(Value::Color(s.color[0], s.color[1], s.color[2]))
                }
                ("font", Some(s)) => Some(Value::Str(s.font)),
                ("fontsize", Some(s)) => Some(Value::Int(s.size as i64)),
                _ => None,
            });
        }
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
                    // `member("x").member` is the member itself.
                    // Regression: Extras' "Copy text" behavior.
                    ("member", Some(_)) => Value::Host(target.clone()),
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
                    // No links set: none (⏳ links authored in XMED).
                    ("hyperlinks", Some(m)) if matches!(m.kind.as_str(), "field" | "text") => {
                        Value::list(vec![])
                    }
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
        if let HostRef::Object { kind, id } = target {
            if kind == "timeout" {
                let now = self.clock_ms;
                if let Some(t) = self.timeouts.get_mut(*id as usize) {
                    match name.to_ascii_lowercase().as_str() {
                        "period" => {
                            t.period = value.to_int().max(1);
                            if t.due.is_some() {
                                t.due = Some(now + t.period);
                            }
                        }
                        "target" => t.target = value.clone(),
                        "timeouthandler" => t.handler = value.to_text(),
                        "persistent" => t.persistent = value.truthy(),
                        _ => return Ok(false),
                    }
                    return Ok(true);
                }
            }
        }
        if let Some((lib, number, a, b)) = parse_chunk_ref(target) {
            let lname = name.to_ascii_lowercase();
            // Text styles are drawn; paragraph props (firstIndent…) and
            // hyperlinkState are kept for reading back.
            if !matches!(lname.as_str(), "text" | "string" | "ref") {
                self.member_ranges.entry((lib, number)).or_default().push((
                    a,
                    b,
                    lname,
                    value.clone(),
                ));
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
                        self.member_ranges.remove(&(*lib, *number));
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
                        m.style.retain(|(k, _)| {
                            !matches!(k.as_str(), "html" | "hyperlinks" | "hyperlinktargets")
                        });
                        if let Some(h) = html {
                            // `member.hyperlinks`: [[start, end], …]
                            let links = html_parse(&h).1;
                            let list = links
                                .iter()
                                .map(|(a, b, _)| {
                                    Value::list(vec![Value::Int(*a as i64), Value::Int(*b as i64)])
                                })
                                .collect();
                            let targets = links
                                .iter()
                                .map(|(_, _, t)| Value::str(t.clone()))
                                .collect();
                            m.style.push(("hyperlinks".into(), Value::list(list)));
                            m.style
                                .push(("hyperlinktargets".into(), Value::list(targets)));
                            m.style.push(("html".into(), Value::Str(h)));
                        }
                        return Ok(true);
                    }
                }
            }
            // A field/text member's box: `rect`, `width`, `height` (kept as
            // width/height for measuring). Regression: Home sets
            // `member.rect` and `member.width` on text members.
            if matches!(lname.as_str(), "rect" | "width" | "height") {
                if let Some(m) = self.members.iter_mut().find(|m| {
                    m.lib as i64 == *lib
                        && m.number as i64 == *number
                        && matches!(m.kind.as_str(), "field" | "text")
                }) {
                    let mut set = |k: &str, v: i64| match m.style.iter_mut().find(|(n, _)| n == k) {
                        Some(slot) => slot.1 = Value::Int(v),
                        None => m.style.push((k.to_string(), Value::Int(v))),
                    };
                    match (lname.as_str(), value) {
                        ("rect", Value::Rect(l, t, r, b)) => {
                            set("boxwidth", (r - l) as i64);
                            set("boxheight", (b - t) as i64);
                        }
                        ("width", v) => set("boxwidth", v.to_int()),
                        ("height", v) => set("boxheight", v.to_int()),
                        _ => return Ok(false),
                    }
                    return Ok(true);
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
            "shiftdown" => Value::Int(self.key.shift as i64),
            "optiondown" | "altdown" => Value::Int(self.key.option as i64),
            "commanddown" => Value::Int(self.key.command as i64),
            "controldown" => Value::Int(self.key.control as i64),
            "mousedown" | "stilldown" => Value::Int(self.mouse_down as i64),
            // Regression: Encyclopedia polls `the mouseUp` (not implemented).
            "mouseup" => Value::Int(!self.mouse_down as i64),
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
            "key" => Value::str(self.key.key.clone()),
            "keycode" => Value::Int(self.key.code),
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
            .call(
                &mut vm,
                "locToCharPos",
                &[m.clone(), Value::Point(0.0, 15.0)],
            )
            .unwrap()
            .unwrap();
        assert_eq!(c.to_int(), 4);
        // Regression: Home's add-on rollover calls `locVToLinePos`.
        let mut line = |y: i64| {
            host.call(&mut vm, "locVToLinePos", &[m.clone(), Value::Int(y)])
                .unwrap()
                .unwrap()
                .to_int()
        };
        assert_eq!((line(0), line(12), line(500)), (1, 2, 2));
    }

    #[test]
    fn module_startup_calls() {
        // Regressions from the Home → module tour (2026-11-01): Bible calls
        // sprite.updateControl(), Extras reads/sets sprite.quad, Home's
        // credits call puppetSound, Encyclopedia sets useHypertextStyles.
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        let mut vm = Vm::new(lingo_vm::Program::default());
        let s = HostRef::Sprite(2);
        host.set_prop(&mut vm, &s, "rect", &Value::Rect(10.0, 20.0, 30.0, 60.0))
            .unwrap();
        let q = host.get_prop(&mut vm, &s, "quad").unwrap().unwrap();
        assert_eq!(
            q.to_string(),
            "[point(10, 20), point(30, 20), point(30, 60), point(10, 60)]"
        );
        let flipped = Value::list(vec![
            Value::Point(30.0, 20.0),
            Value::Point(10.0, 20.0),
            Value::Point(10.0, 60.0),
            Value::Point(30.0, 60.0),
        ]);
        assert!(host.set_prop(&mut vm, &s, "quad", &flipped).unwrap());
        let q = host.get_prop(&mut vm, &s, "quad").unwrap().unwrap();
        assert_eq!(q.to_string(), flipped.to_string());
        let r = host.get_prop(&mut vm, &s, "rect").unwrap().unwrap();
        assert_eq!(r.to_string(), "rect(10, 20, 30, 60)");
        assert!(host
            .method(&mut vm, &s, "updateControl", &[])
            .unwrap()
            .is_some());
        host.call(
            &mut vm,
            "puppetSound",
            &[Value::Int(2), Value::str("BennyHill")],
        )
        .unwrap()
        .unwrap();
        assert_eq!(host.sounds[&2].to_text(), "BennyHill");
        assert!(MEMBER_STYLE_PROPS.contains(&"usehypertextstyles"));
        // Home sets a text member's rect and width.
        host.members.push(MemberInfo {
            lib: 5,
            number: 2,
            name: String::new(),
            kind: "text".into(),
            script: None,
            bitmap: None,
            text: Some("abc def".into()),
            styled: None,
            file_name: None,
            style: vec![],
        });
        let t = HostRef::Member { lib: 5, number: 2 };
        assert!(host
            .set_prop(&mut vm, &t, "rect", &Value::Rect(0.0, 0.0, 30.0, 40.0))
            .unwrap());
        assert_eq!(host.measure(5, 2).unwrap().1, 30);
        assert!(host
            .set_prop(&mut vm, &t, "width", &Value::Int(90))
            .unwrap());
        assert_eq!(host.measure(5, 2).unwrap().1, 90);
        // Extras sets a field's scrollTop.
        assert!(MEMBER_STYLE_PROPS.contains(&"scrolltop"));
        host.members.push(MemberInfo {
            lib: 5,
            number: 182,
            name: "ClipboardMember".into(),
            kind: "field".into(),
            script: None,
            bitmap: None,
            text: None,
            styled: None,
            file_name: None,
            style: vec![],
        });
        let m = HostRef::Member {
            lib: 5,
            number: 182,
        };
        let same = host.get_prop(&mut vm, &m, "member").unwrap().unwrap();
        assert_eq!(same.to_string(), Value::Host(m).to_string());
    }

    #[test]
    fn hyperlinks_of_member_chunks() {
        // Regression: Encyclopedia's "Hyperlink style" reads member
        // hyperlinks, chunk refs, hyperlinkRange/hyperlink/hyperlinkState
        // and pointToChar/pointInHyperlink (none implemented).
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        let mut vm = Vm::new(lingo_vm::Program::default());
        host.members.push(MemberInfo {
            lib: 1,
            number: 1,
            name: String::new(),
            kind: "text".into(),
            script: None,
            bitmap: None,
            text: Some(String::new()),
            styled: None,
            file_name: None,
            style: vec![("fontsize".into(), Value::Int(10))],
        });
        let m = HostRef::Member { lib: 1, number: 1 };
        let none = host.get_prop(&mut vm, &m, "hyperlinks").unwrap().unwrap();
        assert_eq!(none.to_string(), "[]");
        host.set_prop(
            &mut vm,
            &m,
            "html",
            &Value::str("ab <a href=\"x:1\">cd</a> e"),
        )
        .unwrap();
        let links = host.get_prop(&mut vm, &m, "hyperlinks").unwrap().unwrap();
        assert_eq!(links.to_string(), "[[4, 5]]");
        let args = [Value::sym("char"), Value::Int(4), Value::Int(5)];
        let Value::Host(c) = host
            .method(&mut vm, &m, "getPropRef", &args)
            .unwrap()
            .unwrap()
        else {
            panic!()
        };
        let get = |host: &mut PlayerHost, vm: &mut Vm, k: &str| {
            host.get_prop(vm, &c, k).unwrap().unwrap().to_string()
        };
        assert_eq!(
            get(&mut host, &mut vm, "ref"),
            Value::Host(c.clone()).to_string()
        );
        assert_eq!(get(&mut host, &mut vm, "hyperlinkRange"), "[4, 5]");
        assert_eq!(get(&mut host, &mut vm, "hyperlink"), "\"x:1\"");
        assert_eq!(get(&mut host, &mut vm, "hyperlinkState"), "#normal");
        host.set_prop(&mut vm, &c, "hyperlinkState", &Value::sym("visited"))
            .unwrap();
        assert_eq!(get(&mut host, &mut vm, "hyperlinkState"), "#visited");
        // A sprite showing the member at (100, 50): 'c' is the 4th char.
        host.set_prop(
            &mut vm,
            &HostRef::Sprite(3),
            "member",
            &Value::Host(m.clone()),
        )
        .unwrap();
        host.set_prop(
            &mut vm,
            &HostRef::Sprite(3),
            "rect",
            &Value::Rect(100.0, 50.0, 300.0, 80.0),
        )
        .unwrap();
        let s3 = Value::Host(HostRef::Sprite(3));
        let call = |host: &mut PlayerHost, vm: &mut Vm, f: &str, x: f64| {
            host.call(vm, f, &[s3.clone(), Value::Point(x, 52.0)])
                .unwrap()
                .unwrap()
                .to_int()
        };
        assert_eq!(call(&mut host, &mut vm, "pointToChar", 120.0), 4);
        assert_eq!(call(&mut host, &mut vm, "pointInHyperlink", 120.0), 1);
        assert_eq!(call(&mut host, &mut vm, "pointInHyperlink", 101.0), 0);
        assert_eq!(call(&mut host, &mut vm, "pointToChar", 50.0), -1);
    }

    #[test]
    fn timeouts_run_on_the_clock() {
        // Regression: the TreeView behaviors call
        // `timeout("Initialize").new(100, #BuildRealTree, me)`, then forget
        // it from the handler (timeouts were not implemented).
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        let mut vm = Vm::new(lingo_vm::Program::default());
        let t = host
            .call(&mut vm, "timeout", &[Value::str("Initialize")])
            .unwrap()
            .unwrap();
        let Value::Host(r) = t.clone() else { panic!() };
        let args = [Value::Int(100), Value::sym("BuildRealTree"), Value::Int(0)];
        host.method(&mut vm, &r, "new", &args).unwrap().unwrap();
        assert_eq!(
            host.get_prop(&mut vm, &r, "period")
                .unwrap()
                .unwrap()
                .to_int(),
            100
        );
        host.clock_ms += 66;
        assert!(host.due_timeouts().is_empty());
        host.clock_ms += 66;
        let due = host.due_timeouts();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].1, "BuildRealTree");
        // Same timeout by name, any case; forgetting stops it.
        let same = host
            .call(&mut vm, "timeout", &[Value::str("initialize")])
            .unwrap()
            .unwrap();
        assert_eq!(same.to_string(), t.to_string());
        host.method(&mut vm, &r, "forget", &[]).unwrap();
        host.clock_ms += 1000;
        assert!(host.due_timeouts().is_empty());
    }

    #[test]
    fn matte_sprites_take_the_mouse_only_where_drawn() {
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        // A 2×1 bitmap: white (cleared by matte), then black.
        host.bitmaps.borrow_mut().insert(
            (1, 1),
            Some(std::rc::Rc::new(director::Bitmap {
                width: 2,
                height: 1,
                rgba: vec![255, 255, 255, 255, 0, 0, 0, 255],
                reg_x: 0,
                reg_y: 0,
            })),
        );
        host.sprites.insert(
            5,
            crate::stage::SpriteState {
                member: Some((1, 1)),
                width: 20,
                height: 10,
                visible: true,
                ink: 8,
                ..Default::default()
            },
        );
        assert_eq!(host.sprite_under((5, 5), None), None);
        assert_eq!(host.sprite_under((15, 5), None), Some(5));
        // Copy ink: the whole rectangle.
        host.sprites.get_mut(&5).unwrap().ink = 0;
        assert_eq!(host.sprite_under((5, 5), None), Some(5));
    }

    #[test]
    fn keys_reach_the_focused_editable_field() {
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        let mut vm = Vm::new(lingo_vm::Program::default());
        host.members.push(MemberInfo {
            lib: 1,
            number: 1,
            name: String::new(),
            kind: "field".into(),
            script: None,
            bitmap: None,
            text: Some("Gn".into()),
            styled: None,
            file_name: None,
            style: vec![("editable".into(), Value::Int(1))],
        });
        host.sprites.insert(
            4,
            crate::stage::SpriteState {
                member: Some((1, 1)),
                visible: true,
                ..Default::default()
            },
        );
        host.settings
            .insert("keyboardfocussprite".into(), Value::Int(4));
        let k = |key: &str, code: i64| crate::stage::Key {
            down: true,
            key: key.into(),
            code,
            shift: true,
            ..Default::default()
        };
        for (key, code) in [(" ", 49), ("1", 18), ("\u{8}", 51), ("2", 19)] {
            crate::stage::key(&mut vm, &mut host, &k(key, code)).unwrap();
        }
        assert_eq!(
            host.member_info(1, 1).unwrap().text.as_deref(),
            Some("Gn 2")
        );
        assert_eq!(host.the_value("keycode").unwrap().to_int(), 19);
        assert_eq!(host.the_value("key").unwrap().to_text(), "2");
        assert_eq!(host.the_value("shiftdown").unwrap().to_int(), 1);
        assert_eq!(host.the_value("mouseup").unwrap().to_int(), 1);
        host.mouse_down = true;
        assert_eq!(host.the_value("mouseup").unwrap().to_int(), 0);
        host.mouse_down = false;
        // `dontPassEvent` from a handler keeps the key out of the field.
        host.call(&mut vm, "dontPassEvent", &[]).unwrap();
        assert_eq!(host.pass_event, Some(false));
        // Not editable: nothing typed.
        host.members[0].style.clear();
        crate::stage::key(&mut vm, &mut host, &k("x", 7)).unwrap();
        assert_eq!(
            host.member_info(1, 1).unwrap().text.as_deref(),
            Some("Gn 2")
        );
    }

    #[test]
    fn civil_dates_from_day_counts() {
        // Regression: `systemDate` stayed at 2026-01-01 (nobody set the
        // date), which froze anything the program dates, the trial included.
        assert_eq!(civil_from_unix_days(0), (1970, 1, 1));
        assert_eq!(civil_from_unix_days(20_758), (2026, 11, 1));
        assert_eq!(civil_from_unix_days(11_016), (2000, 2, 29));
        let (y, m, d) = system_today();
        assert!(y >= 2026 && (1..=12).contains(&m) && (1..=31).contains(&d));
    }

    #[test]
    fn member_line_chunks_take_styles() {
        let mut host = PlayerHost::new(Vfs::new(Box::new(NoDisc), Vec::<String>::new()));
        let mut vm = Vm::new(lingo_vm::Program::default());
        host.members.push(MemberInfo {
            lib: 1,
            number: 1,
            name: String::new(),
            kind: "text".into(),
            script: None,
            bitmap: None,
            text: Some("ab\rcd\ref".into()),
            styled: None,
            file_name: None,
            style: vec![],
        });
        let m = HostRef::Member { lib: 1, number: 1 };
        // Regression: Home's add-on rollover sets `member.line[n].color`
        // (getPropRef on a member was not implemented).
        let args = [Value::sym("line"), Value::Int(2)];
        let r = host
            .method(&mut vm, &m, "getPropRef", &args)
            .unwrap()
            .unwrap();
        let Value::Host(chunk) = r else {
            panic!("{r:?}")
        };
        assert_eq!(
            host.get_prop(&mut vm, &chunk, "text")
                .unwrap()
                .unwrap()
                .to_text(),
            "cd"
        );
        assert!(host
            .set_prop(&mut vm, &chunk, "color", &Value::Color(9, 8, 7))
            .unwrap());
        let st = host.styled_text(1, 1).unwrap();
        let color_at = |i: usize| {
            st.runs
                .iter()
                .find(|r| r.start <= i && i < r.end)
                .unwrap()
                .style
                .color
        };
        assert_eq!(
            (color_at(1), color_at(3), color_at(6)),
            ([0, 0, 0], [9, 8, 7], [0, 0, 0])
        );
        let t = host.method(&mut vm, &m, "getProp", &args).unwrap().unwrap();
        assert_eq!(t.to_text(), "cd");
        let n = host
            .method(&mut vm, &m, "count", &[Value::sym("paragraph")])
            .unwrap()
            .unwrap();
        assert_eq!(n.to_int(), 3);
        // Chunks of chunks, and setProp(#line, n, text) on the member.
        let first = [Value::sym("char"), Value::Int(1)];
        let c = host
            .method(&mut vm, &chunk, "getProp", &first)
            .unwrap()
            .unwrap();
        assert_eq!(c.to_text(), "c");
        let Value::Host(sub) = host
            .method(&mut vm, &chunk, "getPropRef", &first)
            .unwrap()
            .unwrap()
        else {
            panic!()
        };
        assert!(host
            .set_prop(&mut vm, &sub, "firstIndent", &Value::Int(20))
            .unwrap());
        let set = [Value::sym("line"), Value::Int(2), Value::str("XY")];
        host.method(&mut vm, &m, "setProp", &set).unwrap().unwrap();
        assert_eq!(
            host.member_info(1, 1).unwrap().text.as_deref(),
            Some("ab\rXY\ref")
        );
        // New text drops the chunk styles.
        host.set_prop(&mut vm, &m, "text", &Value::str("xy"))
            .unwrap();
        assert!(host.member_ranges.is_empty());
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
