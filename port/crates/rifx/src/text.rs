//! Text cast members: styled text (`XMED`, the D7+ `text` Xtra member) and
//! fields (`STXT`).
//!
//! `XMED` is a serialised Paige document. It is a sequence of sections, each
//! with a 20-character ASCII hex header (`key:4 bytes:8 type:4 decl:4`) and
//! a body of `bytes` bytes. Bodies hold "packed" values. A control byte with
//! bit 7 set repeats the last value (bit 6: the next byte is a repeat count).
//! Otherwise a hex number follows, where type 1 means 16-bit. `0x00` starts a
//! byte block: hex size, `,`, raw bytes. Sections used here:
//! * `0x0000` document header, whose first value is the document version;
//! * `0x0002` text, as one byte block (Windows-1252);
//! * `0x0004` style runs, as (position, style index) pairs;
//! * `0x0006` style table (font index, size, colours, bold/italic/underline);
//! * `0x0008` font table, where each entry has two 64-byte names plus properties.
//!
//! `STXT` (big-endian): `u32 header length (12), u32 text length, u32 style
//! length`, the text, then `u16 count` style runs of 20 bytes: `u32 start,
//! u16 height, u16 ascent, u16 font id, u8 style, u8 pad, u16 size,
//! u16 r, g, b`.
//!
//! The format notes follow public research (dirplayer-rs and Paige
//! documentation); no code is copied.

use crate::reader::{Endian, Reader};
use crate::Error;

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub font: String,
    pub size: u16,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: [u8; 3],
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            font: "Arial".into(),
            size: 12,
            bold: false,
            italic: false,
            underline: false,
            color: [0, 0, 0],
        }
    }
}

/// A styled run: `text[start..end]` uses `style`.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub start: usize,
    pub end: usize,
    pub style: TextStyle,
}

/// Paragraph alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyledText {
    /// Text with `\r` line breaks (Director convention), decoded to Unicode.
    pub text: String,
    pub runs: Vec<Run>,
    /// `(first char, alignment)` per paragraph run, sorted; empty = left.
    pub paragraphs: Vec<(usize, Align)>,
}

impl StyledText {
    /// Alignment of the paragraph that contains char `i`.
    pub fn align_at(&self, i: usize) -> Align {
        self.paragraphs
            .iter()
            .take_while(|(start, _)| *start <= i)
            .last()
            .map(|p| p.1)
            .unwrap_or_default()
    }
}

/// Windows-1252 to Unicode (only 0x80–0x9F differ from Latin-1).
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

/// Paige "packer" over one section body.
/// Mac OS Roman to Unicode (text members authored on a Mac).
pub fn mac_roman(b: &[u8]) -> String {
    const HI: [char; 128] = [
        'Ä', 'Å', 'Ç', 'É', 'Ñ', 'Ö', 'Ü', 'á', 'à', 'â', 'ä', 'ã', 'å', 'ç', 'é', 'è', 'ê', 'ë',
        'í', 'ì', 'î', 'ï', 'ñ', 'ó', 'ò', 'ô', 'ö', 'õ', 'ú', 'ù', 'û', 'ü', '†', '°', '¢', '£',
        '§', '•', '¶', 'ß', '®', '©', '™', '´', '¨', '≠', 'Æ', 'Ø', '∞', '±', '≤', '≥', '¥', 'µ',
        '∂', '∑', '∏', 'π', '∫', 'ª', 'º', 'Ω', 'æ', 'ø', '¿', '¡', '¬', '√', 'ƒ', '≈', '∆', '«',
        '»', '…', ' ', 'À', 'Ã', 'Õ', 'Œ', 'œ', '–', '—', '“', '”', '‘', '’', '÷', '◊', 'ÿ', 'Ÿ',
        '⁄', '€', '‹', '›', 'ﬁ', 'ﬂ', '‡', '·', '‚', '„', '‰', 'Â', 'Ê', 'Á', 'Ë', 'È', 'Í', 'Î',
        'Ï', 'Ì', 'Ó', 'Ô', '\u{f8ff}', 'Ò', 'Ú', 'Û', 'Ù', 'ı', 'ˆ', '˜', '¯', '˘', '˙', '˚', '¸',
        '˝', '˛', 'ˇ',
    ];
    b.iter()
        .map(|&c| {
            if c >= 0x80 {
                HI[(c - 0x80) as usize]
            } else {
                c as char
            }
        })
        .collect()
}

struct Packer<'a> {
    d: &'a [u8],
    pos: usize,
    last: i64,
    repeat: u32,
}

impl<'a> Packer<'a> {
    fn new(d: &'a [u8]) -> Self {
        Packer {
            d,
            pos: 0,
            last: 0,
            repeat: 0,
        }
    }

    fn more(&self) -> bool {
        self.repeat > 0 || (self.pos < self.d.len() && self.d[self.pos] != 0x03)
    }

    fn num(&mut self) -> i64 {
        if self.repeat > 0 {
            self.repeat -= 1;
            return self.last;
        }
        let Some(&ctrl) = self.d.get(self.pos) else {
            return 0;
        };
        self.pos += 1;
        if ctrl & 0x80 != 0 {
            if ctrl & 0x40 != 0 {
                if let Some(&n) = self.d.get(self.pos) {
                    self.repeat = (n as u32).saturating_sub(1);
                    self.pos += 1;
                }
            }
            return self.last;
        }
        let start = self.pos;
        while self.pos < self.d.len()
            && (self.d[self.pos].is_ascii_hexdigit() || self.d[self.pos] == b'-')
        {
            self.pos += 1;
        }
        let s = std::str::from_utf8(&self.d[start..self.pos]).unwrap_or("0");
        let (neg, digits) = match s.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, s),
        };
        let mut v = i64::from_str_radix(digits, 16).unwrap_or(0);
        if neg {
            v = -v;
        }
        if ctrl & 0x0f == 1 {
            v = v as u16 as i64;
        }
        self.last = v;
        v
    }

    /// `0x00`, hex size, `,`, raw bytes.
    fn bytes(&mut self) -> Option<&'a [u8]> {
        if self.d.get(self.pos) != Some(&0) {
            return None;
        }
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.d.len() && self.d[self.pos].is_ascii_hexdigit() {
            self.pos += 1;
        }
        let size =
            usize::from_str_radix(std::str::from_utf8(&self.d[start..self.pos]).ok()?, 16).ok()?;
        if self.d.get(self.pos) == Some(&b',') {
            self.pos += 1;
        }
        let end = (self.pos + size).min(self.d.len());
        let b = &self.d[self.pos..end];
        self.pos = end;
        Some(b)
    }
}

/// Split an `XMED` document into `(key, count, body)` sections. The header's
/// last eight hex digits are the number of records in the body.
fn sections(d: &[u8]) -> Vec<(u16, usize, &[u8])> {
    let hex = |b: &[u8]| {
        std::str::from_utf8(b)
            .ok()
            .and_then(|s| usize::from_str_radix(s, 16).ok())
    };
    let mut out = Vec::new();
    let mut at = 0;
    while at + 20 <= d.len() {
        let h = &d[at..at + 20];
        let (Some(key), Some(len), Some(count)) = (hex(&h[0..4]), hex(&h[4..12]), hex(&h[12..20]))
        else {
            break;
        };
        at += 20;
        let end = (at + len).min(d.len());
        out.push((key as u16, count, &d[at..end]));
        at = end;
    }
    out
}

/// Values per style record in the Paige version Director MX writes
/// (document version 0x40001), and the fields used from it.
const STYLE_LEN: usize = 77;
const ST_FONT: usize = 0;
const ST_POINT: usize = 3;
const ST_FORE: usize = 9;
const ST_SIZE_FIXED: usize = 18;
const ST_BOLD: usize = 39;
/// Values per paragraph record (section 7).
const PAR_LEN: usize = 54;

/// Parse an `XMED` chunk into plain text plus styled runs.
pub fn parse_xmed(d: &[u8]) -> Result<StyledText, Error> {
    if d.get(..4) != Some(b"FFFF") {
        return Err(Error::Unsupported("XMED without FFFF header".into()));
    }
    let secs = sections(d);
    let get = |k: u16| secs.iter().filter(move |(key, _, _)| *key == k);

    let doc = get(0)
        .next()
        .map(|(_, _, b)| Packer::new(b).num())
        .unwrap_or(0);
    if doc != 0x40001 {
        return Err(Error::Unsupported(format!(
            "XMED document version {doc:#x}"
        )));
    }

    // Text (one or more 0x0002 sections).
    let mut raw = Vec::new();
    for (_, _, b) in get(2) {
        if let Some(t) = Packer::new(b).bytes() {
            raw.extend_from_slice(t);
        }
    }
    while raw.last() == Some(&0) {
        raw.pop();
    }
    // Text-block flags (section 1, third value): low byte 0x81 marks text
    // stored in Mac OS Roman, 0x84 in Windows-1252.
    let mac = get(1)
        .next()
        .map(|(_, _, b)| {
            let mut p = Packer::new(b);
            p.num();
            p.num();
            p.num() & 0xff == 0x81
        })
        .unwrap_or(false);
    let text = if mac { mac_roman(&raw) } else { cp1252(&raw) };

    // Fonts: two byte blocks (name, alternate name) then numeric properties.
    let mut fonts = Vec::new();
    if let Some((_, _, b)) = get(8).next() {
        let mut p = Packer::new(b);
        while p.d.get(p.pos) == Some(&0) {
            let name = p.bytes().map(|n| {
                let n = n.split(|&c| c == 0).next().unwrap_or(n);
                // Pascal-style length byte before the name.
                let n = if !n.is_empty() && n[0] as usize == n.len() - 1 {
                    &n[1..]
                } else {
                    n
                };
                cp1252(n)
            });
            let _alt = p.bytes();
            while p.more() && p.d.get(p.pos) != Some(&0) {
                p.num();
            }
            fonts.push(name.unwrap_or_default());
        }
    }

    // Styles: one leading value, then `count` records.
    let mut styles = Vec::new();
    if let Some((_, count, b)) = get(6).next() {
        let mut p = Packer::new(b);
        p.num();
        for _ in 0..(*count).min(4096) {
            if !p.more() {
                break;
            }
            let v: Vec<i64> = (0..STYLE_LEN).map(|_| p.num()).collect();
            let mut s = TextStyle::default();
            if let Some(f) = usize::try_from(v[ST_FONT]).ok().and_then(|i| fonts.get(i)) {
                if !f.is_empty() {
                    s.font = f.clone();
                }
            }
            let fixed = v[ST_SIZE_FIXED] >> 16;
            let size = if (1..=400).contains(&fixed) {
                fixed
            } else {
                v[ST_POINT]
            };
            if (1..=400).contains(&size) {
                s.size = size as u16;
            }
            s.color = [
                (v[ST_FORE] >> 8) as u8,
                (v[ST_FORE + 1] >> 8) as u8,
                (v[ST_FORE + 2] >> 8) as u8,
            ];
            s.bold = v[ST_BOLD] != 0;
            s.italic = v[ST_BOLD + 1] != 0;
            s.underline = v[ST_BOLD + 2] != 0;
            styles.push(s);
        }
    }

    // Style runs: (offset, style index) pairs; the last pair marks the end.
    let mut runs = Vec::new();
    let n = text.chars().count();
    if let Some((_, _, b)) = get(4).next() {
        let mut p = Packer::new(b);
        let mut pairs = Vec::new();
        while p.more() {
            let pos = p.num().max(0) as usize;
            let idx = p.num().max(0) as usize;
            pairs.push((pos, idx));
        }
        for (i, &(pos, idx)) in pairs.iter().enumerate() {
            let end = pairs.get(i + 1).map(|p| p.0).unwrap_or(n).min(n);
            if pos < end {
                runs.push(Run {
                    start: pos,
                    end,
                    style: styles.get(idx).cloned().unwrap_or_default(),
                });
            }
        }
    }
    if runs.is_empty() && (n > 0 || !styles.is_empty()) {
        // An empty member keeps its style as a zero-length run: Lingo fills
        // the text at run time and it takes this style.
        // Regression: empty members ("GOLD", the add-on list) lost their
        // authored font and were drawn in the default Arial 12.
        let idx = get(4)
            .next()
            .and_then(|(_, _, b)| {
                let mut p = Packer::new(b);
                p.num();
                p.more().then(|| p.num().max(0) as usize)
            })
            .unwrap_or(0);
        runs.push(Run {
            start: 0,
            end: n,
            style: styles
                .get(idx)
                .or(styles.first())
                .cloned()
                .unwrap_or_default(),
        });
    }

    // Paragraph formats (section 7, `PAR_LEN` values each; the first is the
    // justification) and paragraph runs (section 5, like section 4).
    let mut aligns = Vec::new();
    if let Some((_, count, b)) = get(7).next() {
        let mut p = Packer::new(b);
        for _ in 0..(*count).min(4096) {
            if !p.more() {
                break;
            }
            let v: Vec<i64> = (0..PAR_LEN).map(|_| p.num()).collect();
            aligns.push(match v[0] {
                1 => Align::Center,
                2 => Align::Right,
                3 => Align::Justify,
                _ => Align::Left,
            });
        }
    }
    let mut paragraphs = Vec::new();
    if let Some((_, _, b)) = get(5).next() {
        let mut p = Packer::new(b);
        while p.more() {
            let pos = p.num().max(0) as usize;
            let idx = p.num().max(0) as usize;
            if pos < n.max(1) {
                paragraphs.push((pos, aligns.get(idx).copied().unwrap_or_default()));
            }
        }
    }
    Ok(StyledText {
        text,
        runs,
        paragraphs,
    })
}

/// Parse an `Fmap` chunk (font id → name), used by `STXT` runs. Layout
/// (big-endian): `u32 map length, u32 names length`; the map (from byte 8):
/// a 28-byte header (`u32 0, u32 0, u32 used, u32 total, 12 × 0`), `used`
/// entries of `u32 name offset, u16 platform, u16 font id`, padding; then the
/// names block, where offsets point at `u32 length` + bytes.
pub fn parse_fmap(d: &[u8]) -> Result<std::collections::HashMap<u16, String>, Error> {
    let mut r = Reader::new(d, Endian::Big);
    let map_len = r.u32()? as usize;
    let names = 8 + map_len;
    r.seek(16)?;
    let used = r.u32()? as usize;
    r.seek(36)?;
    let mut out = std::collections::HashMap::new();
    for i in 0..used {
        r.seek(36 + i * 8)?;
        let off = r.u32()? as usize;
        let _platform = r.u16()?;
        let id = r.u16()?;
        let mut n = Reader::new(d, Endian::Big);
        n.seek(names + off)?;
        let len = n.u32()? as usize;
        out.insert(id, cp1252(n.bytes(len)?));
    }
    Ok(out)
}

/// Parse an `STXT` field chunk. `font_name` maps a font id to a name.
pub fn parse_stxt(
    d: &[u8],
    font_name: &dyn Fn(u16) -> Option<String>,
) -> Result<StyledText, Error> {
    let mut r = Reader::new(d, Endian::Big);
    let header = r.u32()? as usize;
    let text_len = r.u32()? as usize;
    let _style_len = r.u32()?;
    r.seek(header)?;
    let text = cp1252(r.bytes(text_len)?);
    let n = text.chars().count();
    let mut runs = Vec::new();
    if r.remaining() >= 2 {
        let count = r.u16()? as usize;
        let mut starts = Vec::new();
        for _ in 0..count {
            let start = r.u32()? as usize;
            let _height = r.u16()?;
            let _ascent = r.u16()?;
            let font = r.u16()?;
            let style = r.u8()?;
            let _pad = r.u8()?;
            let size = r.u16()?;
            let (cr, cg, cb) = (r.u16()?, r.u16()?, r.u16()?);
            starts.push((
                start,
                TextStyle {
                    font: font_name(font).unwrap_or_else(|| "Arial".into()),
                    size,
                    bold: style & 1 != 0,
                    italic: style & 2 != 0,
                    underline: style & 4 != 0,
                    color: [(cr >> 8) as u8, (cg >> 8) as u8, (cb >> 8) as u8],
                },
            ));
        }
        for (i, (start, style)) in starts.iter().enumerate() {
            let end = starts.get(i + 1).map(|s| s.0).unwrap_or(n).min(n);
            if *start < end {
                runs.push(Run {
                    start: *start,
                    end,
                    style: style.clone(),
                });
            }
        }
    }
    if runs.is_empty() && n > 0 {
        runs.push(Run {
            start: 0,
            end: n,
            style: TextStyle::default(),
        });
    }
    Ok(StyledText {
        text,
        runs,
        paragraphs: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Append an XMED section (header + body).
    fn sec(d: &mut Vec<u8>, key: u16, count: usize, body: &[u8]) {
        d.extend_from_slice(format!("{key:04X}{:08X}{count:08X}", body.len()).as_bytes());
        d.extend_from_slice(body);
    }

    /// A minimal XMED document: header, version, optional text-block flags,
    /// and one text block.
    fn doc(flags: Option<&str>, text: &[u8]) -> Vec<u8> {
        let mut d = Vec::new();
        sec(&mut d, 0xffff, 1, b"\x0177AA\x03");
        sec(&mut d, 0, 0, b"\x0240001\x03");
        if let Some(f) = flags {
            sec(&mut d, 1, 1, format!("\x020\x020\x01{f}\x03").as_bytes());
        }
        let mut t = format!("\x00{:X},", text.len()).into_bytes();
        t.extend_from_slice(text);
        t.push(3);
        sec(&mut d, 2, 0, &t);
        d
    }

    // Regression: Mac-authored documents showed "Isa'as" for "Isaías".
    // The text-block flags pick the encoding; bytes alone cannot (0x92 is
    // also the Windows apostrophe).
    #[test]
    fn xmed_encoding_follows_text_block_flags() {
        assert_eq!(
            parse_xmed(&doc(Some("4081"), b"V\x92deo")).unwrap().text,
            "Vídeo"
        );
        assert_eq!(
            parse_xmed(&doc(Some("4084"), b"it\x92s")).unwrap().text,
            "it’s"
        );
        assert_eq!(parse_xmed(&doc(None, b"it\x92s")).unwrap().text, "it’s");
    }

    // Regression: Flash Asset members also carry an XMED chunk (a SWF).
    #[test]
    fn xmed_rejects_flash_data() {
        let mut swf = vec![0, 3, 0xb7, 0xc0, 0, 0, 0, 1, 0, 3, 0xb7, 0xb4];
        swf.extend_from_slice(b"FWS\x06");
        assert!(parse_xmed(&swf).is_err());
    }

    // Regression: the style count is in the section header and "R n" repeats
    // count in total. Reading the first value as the count shifted every
    // record, so all runs came out as the default Arial 12.
    #[test]
    fn xmed_style_count_comes_from_the_header() {
        let mut v = vec![0i64; STYLE_LEN];
        v[ST_FONT] = 0;
        v[ST_POINT] = 14;
        let mut styles = b"\x025".to_vec(); // leading value, not a count
        for x in &v {
            styles.extend_from_slice(format!("\x02{x:X}").as_bytes());
        }
        styles.push(3);
        let mut fonts = vec![0u8];
        fonts.extend_from_slice(b"7,\x06Geneva\x00");
        fonts.extend_from_slice(b"\x00\x31,\x00\x03");
        let mut d = doc(None, b"ab");
        sec(&mut d, 4, 2, b"\x020\x010\x022\x010\x03");
        sec(&mut d, 6, 1, &styles);
        sec(&mut d, 8, 1, &fonts);
        let t = parse_xmed(&d).unwrap();
        assert_eq!(
            (t.runs[0].style.font.as_str(), t.runs[0].style.size),
            ("Geneva", 14)
        );
    }

    #[test]
    fn empty_xmed_keeps_its_style() {
        let mut v = vec![0i64; STYLE_LEN];
        v[ST_POINT] = 9;
        let mut styles = b"\x020".to_vec();
        for x in &v {
            styles.extend_from_slice(format!("\x02{x:X}").as_bytes());
        }
        styles.push(3);
        let mut fonts = vec![0u8];
        fonts.extend_from_slice(b"7,\x06Geneva\x00");
        fonts.extend_from_slice(b"\x00\x31,\x00\x03");
        let mut d = doc(None, b"");
        sec(&mut d, 6, 1, &styles);
        sec(&mut d, 8, 1, &fonts);
        let t = parse_xmed(&d).unwrap();
        assert_eq!(t.text, "");
        assert_eq!(t.runs.len(), 1);
        assert_eq!(
            (
                t.runs[0].end,
                t.runs[0].style.font.as_str(),
                t.runs[0].style.size
            ),
            (0, "Geneva", 9)
        );
    }

    #[test]
    fn packer_numbers_repeats_and_bytes() {
        let mut p = Packer::new(b"\x02A\x02-3\x81\xc1\x03\x00C,hello world!\x03");
        assert_eq!(p.num(), 10);
        assert_eq!(p.num(), -3);
        assert_eq!(p.num(), -3); // 0x81 repeat
        assert_eq!(p.num(), -3); // 0xC1 + count 3 → this and 2 more
        assert_eq!(p.num(), -3);
        assert_eq!(p.num(), -3);
        assert_eq!(p.bytes(), Some(&b"hello world!"[..]));
    }

    #[test]
    fn xmed_text_and_runs() {
        fn style(font: i64, size: i64, bold: bool, red: bool) -> Vec<u8> {
            let mut v = vec![0i64; STYLE_LEN];
            v[ST_FONT] = font;
            v[ST_POINT] = size;
            v[ST_SIZE_FIXED] = size << 16;
            v[ST_FORE] = if red { 0xff00 } else { 0 };
            v[ST_BOLD] = if bold { -1 } else { 0 };
            let hex = |x: i64| {
                if x < 0 {
                    format!("-{:X}", -x)
                } else {
                    format!("{x:X}")
                }
            };
            v.iter()
                .flat_map(|&x| format!("\x02{}", hex(x)).into_bytes())
                .collect()
        }
        let mut styles = b"\x020".to_vec();
        styles.extend(style(1, 12, false, false));
        styles.extend(style(0, 9, true, true));
        styles.push(3);
        let mut fonts = Vec::new();
        for name in ["Geneva", "Verdana"] {
            let mut n = vec![name.len() as u8];
            n.extend_from_slice(name.as_bytes());
            n.resize(64, 0);
            fonts.extend_from_slice(b"\x0040,");
            fonts.extend_from_slice(&n);
            fonts.extend_from_slice(b"\x0040,");
            fonts.extend_from_slice(&[0; 64]);
            fonts.extend_from_slice(b"\x0110A\x020");
        }
        fonts.push(3);

        let mut d = Vec::new();
        sec(&mut d, 0xffff, 1, b"\x0177AA\x03");
        sec(&mut d, 0, 0, b"\x0240001\x03");
        sec(&mut d, 2, 0, b"\x00D,Abcd efghijk.\x03");
        sec(&mut d, 4, 3, b"\x020\x010\x024\x011\x02D\x010\x03");
        sec(&mut d, 6, 2, &styles);
        sec(&mut d, 8, 2, &fonts);
        let t = parse_xmed(&d).unwrap();
        assert_eq!(t.text, "Abcd efghijk.");
        assert_eq!(
            t.runs.iter().map(|r| (r.start, r.end)).collect::<Vec<_>>(),
            [(0, 4), (4, 13)]
        );
        let (a, b) = (&t.runs[0].style, &t.runs[1].style);
        assert_eq!(
            (a.font.as_str(), a.size, a.bold, a.color),
            ("Verdana", 12, false, [0, 0, 0])
        );
        assert_eq!(
            (b.font.as_str(), b.size, b.bold, b.color),
            ("Geneva", 9, true, [255, 0, 0])
        );
    }

    #[test]
    fn fmap_ids_to_names() {
        let mut d = Vec::new();
        let names: &[u8] = b"\0\0\0\x06Geneva\0\0\0\x07Verdana";
        d.extend_from_slice(&(28u32 + 16).to_be_bytes());
        d.extend_from_slice(&(names.len() as u32).to_be_bytes());
        d.extend_from_slice(&[0; 4]);
        for v in [0u32, 2, 2, 0, 0, 0] {
            d.extend_from_slice(&v.to_be_bytes());
        }
        for (off, id) in [(0u32, 0x8001u16), (10, 0x8002)] {
            d.extend_from_slice(&off.to_be_bytes());
            d.extend_from_slice(&2u16.to_be_bytes());
            d.extend_from_slice(&id.to_be_bytes());
        }
        d.extend_from_slice(names);
        let m = parse_fmap(&d).unwrap();
        assert_eq!(m[&0x8001], "Geneva");
        assert_eq!(m[&0x8002], "Verdana");
    }

    #[test]
    fn mac_roman_accents() {
        assert_eq!(
            mac_roman(b"V\x92deo m\x92dia a\x8d\x8bo \xd2x\xd3"),
            "Vídeo mídia ação “x”"
        );
    }

    #[test]
    fn cp1252_high_range() {
        assert_eq!(cp1252(&[0x93, b'a', 0x94, 0xe9]), "“a”é");
    }
}
