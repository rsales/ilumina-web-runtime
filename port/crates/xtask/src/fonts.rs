//! Native [`director::Fonts`] over the host's installed fonts (fontdue).
//!
//! iLumina's members ask for Windows/Mac families (Verdana, Georgia, Times
//! New Roman, Arial, Geneva, MS Sans Serif…). Each family has a list of file
//! candidates: the real face first (macOS ships most of them under
//! `/System/Library/Fonts/Supplemental`), then a metric-compatible or similar
//! free face (Liberation, DejaVu). `ILUMINA_FONTS` adds directories to search
//! first (for example a folder with the owner's own Windows fonts).

use director::{Fonts, Glyph, LineMetrics};
use rifx::text::TextStyle;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

type Key = (String, bool, bool);

pub struct HostFonts {
    dirs: Vec<PathBuf>,
    faces: RefCell<HashMap<Key, Option<std::rc::Rc<fontdue::Font>>>>,
    glyphs: RefCell<HashMap<(Key, u16, char), Glyph>>,
    /// Requested family → file actually used (for the report).
    pub used: RefCell<HashMap<String, String>>,
}

/// File-name candidates per family and style suffix (regular, bold, italic,
/// bold italic). Real faces first, free substitutes after.
fn candidates(family: &str) -> Vec<[&'static str; 4]> {
    let f = family.to_ascii_lowercase();
    let verdana = [
        "Verdana.ttf",
        "Verdana Bold.ttf",
        "Verdana Italic.ttf",
        "Verdana Bold Italic.ttf",
    ];
    let georgia = [
        "Georgia.ttf",
        "Georgia Bold.ttf",
        "Georgia Italic.ttf",
        "Georgia Bold Italic.ttf",
    ];
    let tnr = [
        "Times New Roman.ttf",
        "Times New Roman Bold.ttf",
        "Times New Roman Italic.ttf",
        "Times New Roman Bold Italic.ttf",
    ];
    let arial = [
        "Arial.ttf",
        "Arial Bold.ttf",
        "Arial Italic.ttf",
        "Arial Bold Italic.ttf",
    ];
    let courier = [
        "Courier New.ttf",
        "Courier New Bold.ttf",
        "Courier New Italic.ttf",
        "Courier New Bold Italic.ttf",
    ];
    let lib_sans = [
        "LiberationSans-Regular.ttf",
        "LiberationSans-Bold.ttf",
        "LiberationSans-Italic.ttf",
        "LiberationSans-BoldItalic.ttf",
    ];
    let lib_serif = [
        "LiberationSerif-Regular.ttf",
        "LiberationSerif-Bold.ttf",
        "LiberationSerif-Italic.ttf",
        "LiberationSerif-BoldItalic.ttf",
    ];
    let lib_mono = [
        "LiberationMono-Regular.ttf",
        "LiberationMono-Bold.ttf",
        "LiberationMono-Italic.ttf",
        "LiberationMono-BoldItalic.ttf",
    ];
    let dejavu = [
        "DejaVuSans.ttf",
        "DejaVuSans-Bold.ttf",
        "DejaVuSans-Oblique.ttf",
        "DejaVuSans-BoldOblique.ttf",
    ];
    let dejavu_serif = [
        "DejaVuSerif.ttf",
        "DejaVuSerif-Bold.ttf",
        "DejaVuSerif-Italic.ttf",
        "DejaVuSerif-BoldItalic.ttf",
    ];
    let winfiles_verdana = [
        "verdana.ttf",
        "verdanab.ttf",
        "verdanai.ttf",
        "verdanaz.ttf",
    ];
    let winfiles_georgia = [
        "georgia.ttf",
        "georgiab.ttf",
        "georgiai.ttf",
        "georgiaz.ttf",
    ];
    let winfiles_tnr = ["times.ttf", "timesbd.ttf", "timesi.ttf", "timesbi.ttf"];
    let winfiles_arial = ["arial.ttf", "arialbd.ttf", "ariali.ttf", "arialbi.ttf"];
    let winfiles_courier = ["cour.ttf", "courbd.ttf", "couri.ttf", "courbi.ttf"];
    let geneva = ["Geneva.ttf", "Geneva.ttf", "Geneva.ttf", "Geneva.ttf"];
    let mss = [
        "Microsoft Sans Serif.ttf",
        "Microsoft Sans Serif.ttf",
        "Microsoft Sans Serif.ttf",
        "Microsoft Sans Serif.ttf",
    ];
    match f.as_str() {
        "verdana" => vec![verdana, winfiles_verdana, dejavu, lib_sans],
        "georgia" => vec![georgia, winfiles_georgia, dejavu_serif, lib_serif],
        "times new roman" | "times" => vec![tnr, winfiles_tnr, lib_serif],
        "courier new" | "courier" => vec![courier, winfiles_courier, lib_mono],
        "geneva" | "charcoal" => vec![geneva, verdana, winfiles_verdana, dejavu, lib_sans],
        "ms sans serif" => vec![mss, arial, winfiles_arial, lib_sans],
        _ => vec![arial, winfiles_arial, lib_sans, dejavu],
    }
}

impl HostFonts {
    pub fn new() -> Self {
        let mut dirs: Vec<PathBuf> = std::env::var("ILUMINA_FONTS")
            .map(|v| std::env::split_paths(&v).collect())
            .unwrap_or_default();
        for d in [
            "/System/Library/Fonts/Supplemental",
            "/System/Library/Fonts",
            "/Library/Fonts",
            "C:\\Windows\\Fonts",
            "/usr/share/fonts/truetype/msttcorefonts",
            "/usr/share/fonts/truetype/liberation",
            "/usr/share/fonts/truetype/liberation2",
            "/usr/share/fonts/truetype/dejavu",
            "/usr/share/fonts/TTF",
        ] {
            dirs.push(PathBuf::from(d));
        }
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(Path::new(&home).join("Library/Fonts"));
            dirs.push(Path::new(&home).join(".fonts"));
        }
        HostFonts {
            dirs,
            faces: Default::default(),
            glyphs: Default::default(),
            used: Default::default(),
        }
    }

    fn face(&self, st: &TextStyle) -> Option<std::rc::Rc<fontdue::Font>> {
        let key = (st.font.to_ascii_lowercase(), st.bold, st.italic);
        if let Some(f) = self.faces.borrow().get(&key) {
            return f.clone();
        }
        let slot = st.bold as usize + 2 * st.italic as usize;
        let mut found = None;
        'outer: for set in candidates(&st.font) {
            // Prefer the exact style, then the regular face of the same family.
            for name in [set[slot], set[0]] {
                for dir in &self.dirs {
                    let p = dir.join(name);
                    if let Ok(bytes) = std::fs::read(&p) {
                        if let Ok(f) =
                            fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
                        {
                            self.used.borrow_mut().insert(
                                format!(
                                    "{}{}{}",
                                    st.font,
                                    if st.bold { " bold" } else { "" },
                                    if st.italic { " italic" } else { "" }
                                ),
                                p.display().to_string(),
                            );
                            found = Some(std::rc::Rc::new(f));
                            break 'outer;
                        }
                    }
                }
            }
        }
        self.faces.borrow_mut().insert(key, found.clone());
        found
    }
}

impl Fonts for HostFonts {
    fn line(&self, st: &TextStyle) -> LineMetrics {
        let px = st.size.max(1) as f32;
        match self.face(st).and_then(|f| f.horizontal_line_metrics(px)) {
            Some(m) => LineMetrics {
                ascent: m.ascent,
                descent: -m.descent,
                gap: m.line_gap,
            },
            None => LineMetrics {
                ascent: px * 0.8,
                descent: px * 0.2,
                gap: 0.0,
            },
        }
    }

    fn glyph(&self, st: &TextStyle, ch: char) -> Glyph {
        let key = (
            (st.font.to_ascii_lowercase(), st.bold, st.italic),
            st.size,
            ch,
        );
        if let Some(g) = self.glyphs.borrow().get(&key) {
            return g.clone();
        }
        let g = match self.face(st) {
            Some(f) => {
                let (m, coverage) = f.rasterize(ch, st.size.max(1) as f32);
                Glyph {
                    width: m.width,
                    height: m.height,
                    xmin: m.xmin,
                    ymin: m.ymin,
                    advance: m.advance_width,
                    coverage,
                }
            }
            None => Glyph {
                advance: st.size as f32 * 0.5,
                ..Default::default()
            },
        };
        self.glyphs.borrow_mut().insert(key, g.clone());
        g
    }
}
