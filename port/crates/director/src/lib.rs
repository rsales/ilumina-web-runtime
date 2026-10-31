//! Director MX stage for the iLumina port (ADR 0008, milestone M1).
//!
//! Loads a movie and its external casts through a [`Files`] provider (disk
//! natively, fetched bytes in the browser), resolves score sprites to cast
//! members and composites one frame into an RGBA buffer. Lingo, text and
//! shapes are later milestones; unsupported members are skipped and
//! reported.

use rifx::bitmap::{bitd_for, to_rgba, unpack, BitmapInfo, Palette};
use rifx::cast::{members, Member, MemberType};
use rifx::score::{cast_libs, CastLib, Score, Sprite};
use rifx::Movie;

/// Byte provider for the movie folder (`cd1/Program/Files` layout).
pub trait Files {
    fn read(&self, name: &str) -> Option<Vec<u8>>;
}

#[derive(Debug)]
pub enum Error {
    Missing(String),
    Format(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Missing(s) => write!(f, "missing file {s}"),
            Error::Format(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rifx::Error> for Error {
    fn from(e: rifx::Error) -> Self {
        Error::Format(e.to_string())
    }
}

/// Movie configuration (`DRCF`, big-endian): stage rect and colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub width: u16,
    pub height: u16,
    pub stage_color: [u8; 3],
}

pub fn config(movie: &Movie) -> Result<Config, Error> {
    let c = movie
        .first(rifx::FourCC::new(b"DRCF"))
        .ok_or(Error::Format("no DRCF".into()))?;
    let d = movie.chunk_data(c.id)?;
    let be = |i: usize| {
        d.get(i..i + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .unwrap_or(0)
    };
    let (top, left, bottom, right) = (be(4), be(6), be(8), be(10));
    let rgb = d.get(26).copied().unwrap_or(0) != 0;
    let stage_color = if rgb {
        [d[27], d[18], d[19]]
    } else {
        [255, 255, 255]
    };
    Ok(Config {
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
        stage_color,
    })
}

/// A cast library with its members, loaded from the movie or an external cast.
pub struct LoadedCast {
    pub lib: CastLib,
    pub file: String,
    data: Vec<u8>,
    pub members: Vec<Member>,
}

impl LoadedCast {
    pub fn member(&self, number: u16) -> Option<&Member> {
        self.members.iter().find(|m| m.number == number as u32)
    }
}

/// External cast file name for a cast-lib path such as
/// `F:\iLumina Portuguese\Files\HOCode.cst` (protected casts ship as `.cxt`).
pub fn cast_file_name(path: &str) -> Option<String> {
    let base = path.rsplit(['\\', ':', '/']).next()?;
    if base.is_empty() {
        return None;
    }
    Some(match base.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case("cst") => format!("{stem}.cxt"),
        _ => base.to_string(),
    })
}

pub struct Stage {
    pub config: Config,
    pub score: Option<Score>,
    pub casts: Vec<LoadedCast>,
    pub palette: Palette,
}

/// What happened to each sprite when rendering a frame.
#[derive(Debug, Default)]
pub struct FrameReport {
    pub drawn: Vec<u16>,
    pub skipped: Vec<(u16, String)>,
}

impl Stage {
    pub fn load(files: &dyn Files, movie_name: &str, palette: Palette) -> Result<Self, Error> {
        let data = files
            .read(movie_name)
            .ok_or_else(|| Error::Missing(movie_name.into()))?;
        let movie = Movie::parse(&data)?;
        let config = config(&movie)?;
        let score = Score::from_movie(&movie)?;
        let mut casts = Vec::new();
        for lib in cast_libs(&movie)? {
            let Some(file) = cast_file_name(&lib.path) else {
                continue;
            };
            let bytes = files
                .read(&file)
                .ok_or_else(|| Error::Missing(file.clone()))?;
            let m = Movie::parse(&bytes)?;
            let members = members(&m)?.into_iter().flatten().collect();
            casts.push(LoadedCast {
                lib,
                file,
                data: bytes,
                members,
            });
        }
        Ok(Stage {
            config,
            score,
            casts,
            palette,
        })
    }

    fn cast(&self, lib: u16) -> Option<&LoadedCast> {
        self.casts.iter().find(|c| c.lib.number == lib)
    }

    /// Decode a bitmap member to (width, height, rgba, reg_x, reg_y).
    pub fn bitmap(&self, lib: u16, member: u16) -> Result<Option<Bitmap>, Error> {
        let Some(cast) = self.cast(lib) else {
            return Ok(None);
        };
        let Some(m) = cast.member(member) else {
            return Ok(None);
        };
        if m.kind != MemberType::Bitmap {
            return Ok(None);
        }
        let movie = Movie::parse(&cast.data)?;
        let info = BitmapInfo::parse(&m.specific)?;
        let Some(bitd) = bitd_for(&movie, m.section) else {
            return Ok(None);
        };
        let rgba = to_rgba(&info, &unpack(&info, bitd?)?, Some(&self.palette))?;
        Ok(Some(Bitmap {
            width: info.width,
            height: info.height,
            rgba,
            reg_x: info.reg_x,
            reg_y: info.reg_y,
        }))
    }

    /// Composite score frame `frame` (1-based) into an RGBA buffer.
    pub fn render(&self, frame: usize) -> Result<(Vec<u8>, FrameReport), Error> {
        let (w, h) = (self.config.width as usize, self.config.height as usize);
        let mut canvas = Canvas::new(w, h, self.config.stage_color);
        let mut report = FrameReport::default();
        let Some(score) = &self.score else {
            return Ok((canvas.px, report));
        };
        for s in score.sprites(frame)? {
            let kind = self
                .cast(s.cast_lib)
                .and_then(|c| c.member(s.member))
                .map(|m| m.kind);
            if kind == Some(MemberType::Shape) {
                let m = self
                    .cast(s.cast_lib)
                    .and_then(|c| c.member(s.member))
                    .unwrap();
                match Shape::parse(&m.specific) {
                    Some(shape) => {
                        canvas.shape(&shape, &s, &self.palette);
                        report.drawn.push(s.number);
                    }
                    None => report.skipped.push((s.number, "unreadable shape".into())),
                }
                continue;
            }
            match self.bitmap(s.cast_lib, s.member)? {
                Some(b) => {
                    canvas.draw(&b, &s);
                    report.drawn.push(s.number);
                }
                None => report.skipped.push((
                    s.number,
                    match kind {
                        Some(MemberType::Bitmap) => {
                            "linked or empty bitmap (image set at runtime)".into()
                        }
                        Some(k) => format!("{k:?} not rendered yet"),
                        None => format!("lib {} member {} not found", s.cast_lib, s.member),
                    },
                )),
            }
        }
        Ok((canvas.px, report))
    }
}

/// Shape member (big-endian): `u16 type (1 rect, 2 round rect, 3 oval,
/// 4 line), rect, u16 pattern, u8 fore, u8 back, u8 fill, u8 line size,
/// u8 line direction`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    pub kind: u16,
    pub filled: bool,
    pub line: u8,
    pub direction: u8,
}

impl Shape {
    pub fn parse(d: &[u8]) -> Option<Self> {
        Some(Shape {
            kind: u16::from_be_bytes([*d.first()?, *d.get(1)?]),
            filled: *d.get(14)? != 0,
            line: *d.get(15)?,
            direction: *d.get(16)?,
        })
    }
}

/// A sprite colour: RGB when the sprite says so, else a palette index.
pub fn sprite_color(s: &Sprite, fore: bool, palette: &Palette) -> [u8; 3] {
    let (rgb, idx) = if fore {
        (s.fore_rgb, s.fore)
    } else {
        (s.back_rgb, s.back)
    };
    if s.color_flag & if fore { 1 } else { 2 } != 0 {
        rgb
    } else {
        palette[idx as usize]
    }
}

pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub reg_x: i16,
    pub reg_y: i16,
}

/// RGBA canvas with Director's most common inks.
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub px: Vec<u8>,
}

impl Canvas {
    pub fn new(width: usize, height: usize, bg: [u8; 3]) -> Self {
        let mut px = Vec::with_capacity(width * height * 4);
        for _ in 0..width * height {
            px.extend_from_slice(&[bg[0], bg[1], bg[2], 255]);
        }
        Canvas { width, height, px }
    }

    fn put(&mut self, x: i64, y: i64, c: [u8; 3], a: u32) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        let di = (y as usize * self.width + x as usize) * 4;
        for (k, &v) in c.iter().enumerate() {
            let d = self.px[di + k] as u32;
            self.px[di + k] = ((v as u32 * a + d * (255 - a)) / 255) as u8;
        }
    }

    /// Draw a shape sprite. The sprite rect is the shape's bounds.
    pub fn shape(&mut self, sh: &Shape, s: &Sprite, palette: &Palette) {
        let c = sprite_color(s, true, palette);
        let a = if s.blend > 0 && s.blend < 100 {
            255 * s.blend as u32 / 100
        } else {
            255
        };
        let (l, t, w, h) = (s.left as i64, s.top as i64, s.width as i64, s.height as i64);
        let lw = sh.line.max(1) as i64;
        match sh.kind {
            4 => {
                // Line from one corner to the opposite; direction 6 = rising.
                let n = w.max(h).max(1);
                for i in 0..=n {
                    let x = l + i * w / n;
                    let y = if sh.direction == 6 {
                        t + h - i * h / n
                    } else {
                        t + i * h / n
                    };
                    for k in 0..lw {
                        self.put(x, y + k, c, a);
                    }
                }
            }
            3 => {
                let (cx, cy, rx, ry) = (
                    l as f64 + w as f64 / 2.0,
                    t as f64 + h as f64 / 2.0,
                    w as f64 / 2.0,
                    h as f64 / 2.0,
                );
                for y in t..t + h {
                    for x in l..l + w {
                        let (dx, dy) = ((x as f64 + 0.5 - cx) / rx, (y as f64 + 0.5 - cy) / ry);
                        let d = dx * dx + dy * dy;
                        let inner = (1.0 - lw as f64 / rx.min(ry)).powi(2);
                        if d <= 1.0 && (sh.filled || d >= inner) {
                            self.put(x, y, c, a);
                        }
                    }
                }
            }
            _ => {
                for y in t..t + h {
                    for x in l..l + w {
                        let edge = x < l + lw || x >= l + w - lw || y < t + lw || y >= t + h - lw;
                        if sh.filled || edge {
                            self.put(x, y, c, a);
                        }
                    }
                }
            }
        }
    }

    /// Draw a bitmap at the sprite's location (registration point) and size.
    /// Inks: 0 copy, 1 transparent / 8 matte / 36 background transparent
    /// (white treated as transparent), 32-bit alpha always honoured.
    pub fn draw(&mut self, b: &Bitmap, s: &Sprite) {
        let (dw, dh) = (s.width.max(1) as i64, s.height.max(1) as i64);
        // Registration point scales with the sprite.
        let sx = dw as f64 / b.width.max(1) as f64;
        let sy = dh as f64 / b.height.max(1) as f64;
        let left = s.left as i64 - (b.reg_x as f64 * sx).round() as i64;
        let top = s.top as i64 - (b.reg_y as f64 * sy).round() as i64;
        let white_is_clear = matches!(s.ink, 1 | 8 | 36);
        for y in 0..dh {
            let cy = top + y;
            if cy < 0 || cy >= self.height as i64 {
                continue;
            }
            let src_y = ((y as f64 / sy) as usize).min(b.height.saturating_sub(1));
            for x in 0..dw {
                let cx = left + x;
                if cx < 0 || cx >= self.width as i64 {
                    continue;
                }
                let src_x = ((x as f64 / sx) as usize).min(b.width.saturating_sub(1));
                let si = (src_y * b.width + src_x) * 4;
                let Some(p) = b.rgba.get(si..si + 4) else {
                    continue;
                };
                if white_is_clear && p[0] == 255 && p[1] == 255 && p[2] == 255 {
                    continue;
                }
                let mut a = p[3] as u32;
                if s.blend > 0 && s.blend < 100 {
                    a = a * s.blend as u32 / 100;
                }
                let di = ((cy as usize) * self.width + cx as usize) * 4;
                for (c, &v) in p.iter().take(3).enumerate() {
                    let d = self.px[di + c] as u32;
                    self.px[di + c] = ((v as u32 * a + d * (255 - a)) / 255) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite(left: i16, top: i16, w: u16, h: u16, ink: u8) -> Sprite {
        Sprite {
            number: 1,
            kind: 1,
            ink,
            fore: 0,
            back: 0,
            cast_lib: 1,
            member: 1,
            top,
            left,
            height: h,
            width: w,
            blend: 0,
            color_flag: 0,
            fore_rgb: [0; 3],
            back_rgb: [0; 3],
        }
    }

    #[test]
    fn filled_rect_shape_uses_rgb_fore_colour() {
        let mut c = Canvas::new(4, 4, [255, 255, 255]);
        let mut s = sprite(1, 1, 2, 2, 0);
        s.color_flag = 1;
        s.fore_rgb = [100, 0, 100];
        let sh = Shape {
            kind: 1,
            filled: true,
            line: 1,
            direction: 0,
        };
        c.shape(&sh, &s, &[[0; 3]; 256]);
        assert_eq!(&c.px[(4 + 1) * 4..(4 + 1) * 4 + 3], &[100, 0, 100]);
        assert_eq!(&c.px[0..3], &[255, 255, 255]);
    }

    #[test]
    fn draws_at_registration_point_with_background_ink() {
        let mut c = Canvas::new(4, 4, [0, 0, 0]);
        // 2x2: red, white / white, red; reg point (1, 1)
        let b = Bitmap {
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 255,
            ],
            reg_x: 1,
            reg_y: 1,
        };
        c.draw(&b, &sprite(2, 2, 2, 2, 36));
        let at = |x: usize, y: usize| c.px[(y * 4 + x) * 4..(y * 4 + x) * 4 + 3].to_vec();
        assert_eq!(at(1, 1), vec![255, 0, 0]); // top-left = loc - reg
        assert_eq!(at(2, 1), vec![0, 0, 0]); // white skipped by ink 36
        assert_eq!(at(2, 2), vec![255, 0, 0]);
    }

    #[test]
    fn cast_paths_map_to_shipped_files() {
        assert_eq!(
            cast_file_name("F:\\iLumina Portuguese\\Files\\HOCode.cst").as_deref(),
            Some("HOCode.cxt")
        );
        assert_eq!(cast_file_name("").as_deref(), None);
    }
}
