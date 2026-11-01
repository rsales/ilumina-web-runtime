//! RTF as Lingo sets it on text members (`member.rtf = …`, the Bible's
//! chapters): the text with bold, italic, underline, font, size and colour
//! runs. Unknown control words and destinations are skipped.

use rifx::text::{Align, Run, StyledText, TextStyle};

#[derive(Clone)]
struct State {
    style: TextStyle,
    /// Inside a destination whose text is not shown (`\fonttbl`, `\*…`).
    skip: bool,
    align: Align,
}

/// Parse RTF into styled text (`\r` line breaks, as Director keeps text).
pub fn parse(rtf: &str) -> StyledText {
    let mut fonts: Vec<(i64, String)> = Vec::new();
    let mut colors: Vec<[u8; 3]> = Vec::new();
    let mut out = String::new();
    let mut runs: Vec<Run> = Vec::new();
    let mut paragraphs: Vec<(usize, Align)> = Vec::new();
    let mut stack: Vec<State> = Vec::new();
    let mut st = State {
        style: TextStyle::default(),
        skip: false,
        align: Align::Left,
    };
    // Font and colour tables are read while their group is open.
    let mut table: Option<&str> = None;
    let mut font_no: Option<i64> = None;
    let mut name = String::new();
    let mut rgb = [0u8; 3];
    let mut uc_skip = 0usize;
    let mut deff: Option<i64> = None;
    let mut base_size: u16 = TextStyle::default().size;
    let chars: Vec<char> = rtf.chars().collect();
    let mut i = 0;
    let push = |out: &mut String, runs: &mut Vec<Run>, st: &State, c: char| {
        let at = out.chars().count();
        out.push(c);
        match runs.last_mut() {
            Some(r) if r.end == at && r.style == st.style => r.end += 1,
            _ => runs.push(Run {
                start: at,
                end: at + 1,
                style: st.style.clone(),
            }),
        }
    };
    let mut para_start = true;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '{' => {
                stack.push(st.clone());
                i += 1;
            }
            '}' => {
                if table == Some("fonttbl") {
                    if let Some(n) = font_no.take() {
                        fonts.push((n, name.trim().trim_end_matches(';').to_string()));
                        name.clear();
                    }
                }
                let was_skip = st.skip;
                if let Some(s) = stack.pop() {
                    st = s;
                }
                if was_skip && !st.skip {
                    // The font table just closed: `\deffN` is the font
                    // until a `\f` says otherwise. Regression: the Bible's
                    // chapters drew in the default serif instead of Arial.
                    if table == Some("fonttbl") {
                        if let Some(f) = fonts.iter().find(|(n, _)| Some(*n) == deff) {
                            st.style.font = f.1.clone();
                        }
                    }
                    table = None;
                }
                i += 1;
            }
            '\\' => {
                i += 1;
                let Some(&n) = chars.get(i) else { break };
                if !n.is_ascii_alphabetic() {
                    i += 1;
                    match n {
                        '\'' => {
                            let h: String = chars
                                .get(i..i + 2)
                                .map(|s| s.iter().collect())
                                .unwrap_or_default();
                            i += 2;
                            if let Ok(b) = u8::from_str_radix(&h, 16) {
                                if uc_skip > 0 {
                                    uc_skip -= 1;
                                } else if !st.skip {
                                    for ch in crate::vfs::text_of(&[b]).chars() {
                                        push(&mut out, &mut runs, &st, ch);
                                    }
                                } else if table == Some("fonttbl") {
                                    name.push_str(&crate::vfs::text_of(&[b]));
                                }
                            }
                        }
                        '*' => st.skip = true,
                        '~' if !st.skip => push(&mut out, &mut runs, &st, '\u{a0}'),
                        '\\' | '{' | '}' if !st.skip => push(&mut out, &mut runs, &st, n),
                        _ => {}
                    }
                    continue;
                }
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let nstart = i;
                if i < chars.len() && (chars[i] == '-' || chars[i].is_ascii_digit()) {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let num: Option<i64> = chars[nstart..i].iter().collect::<String>().parse().ok();
                if i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
                let on = num.is_none_or(|v| v != 0);
                match word.as_str() {
                    "deff" => deff = num,
                    "fonttbl" => {
                        st.skip = true;
                        table = Some("fonttbl");
                    }
                    "colortbl" => {
                        st.skip = true;
                        table = Some("colortbl");
                    }
                    "stylesheet" | "info" | "pict" | "header" | "footer" | "listtable"
                    | "listoverridetable" | "generator" => st.skip = true,
                    "f" if table == Some("fonttbl") => {
                        if let Some(n) = font_no.take() {
                            fonts.push((n, name.trim().trim_end_matches(';').to_string()));
                            name.clear();
                        }
                        font_no = num;
                    }
                    "red" => rgb[0] = num.unwrap_or(0) as u8,
                    "green" => rgb[1] = num.unwrap_or(0) as u8,
                    "blue" => rgb[2] = num.unwrap_or(0) as u8,
                    "f" => {
                        if let Some(f) = fonts.iter().find(|(n, _)| Some(*n) == num) {
                            st.style.font = f.1.clone();
                        }
                    }
                    "fs" => {
                        st.style.size = (num.unwrap_or(24) / 2).clamp(1, 500) as u16;
                        base_size = st.style.size;
                    }
                    // Superscript (the Bible's verse numbers): smaller and
                    // raised. Regression: verse numbers were full size.
                    "super" => {
                        st.style.size = (base_size * 2 / 3).max(1);
                        st.style.raise = (base_size / 3) as i16;
                    }
                    "sub" => {
                        st.style.size = (base_size * 2 / 3).max(1);
                        st.style.raise = -((base_size / 6) as i16);
                    }
                    "nosupersub" => {
                        st.style.size = base_size;
                        st.style.raise = 0;
                    }
                    "up" => st.style.raise = (num.unwrap_or(6) / 2) as i16,
                    "dn" => st.style.raise = -((num.unwrap_or(6) / 2) as i16),
                    "b" => st.style.bold = on,
                    "i" => st.style.italic = on,
                    "ul" => st.style.underline = on,
                    "ulnone" => st.style.underline = false,
                    "cf" => {
                        st.style.color = colors
                            .get(num.unwrap_or(0) as usize)
                            .copied()
                            .unwrap_or([0; 3]);
                    }
                    "plain" => {
                        let font = st.style.font.clone();
                        st.style = TextStyle {
                            font,
                            ..TextStyle::default()
                        };
                    }
                    "ql" => st.align = Align::Left,
                    "qc" => st.align = Align::Center,
                    "qr" => st.align = Align::Right,
                    "pard" => st.align = Align::Left,
                    "par" | "line" if !st.skip => {
                        push(&mut out, &mut runs, &st, '\r');
                        para_start = true;
                    }
                    "tab" if !st.skip => push(&mut out, &mut runs, &st, '\t'),
                    "emdash" if !st.skip => push(&mut out, &mut runs, &st, '—'),
                    "endash" if !st.skip => push(&mut out, &mut runs, &st, '–'),
                    "lquote" if !st.skip => push(&mut out, &mut runs, &st, '‘'),
                    "rquote" if !st.skip => push(&mut out, &mut runs, &st, '’'),
                    "ldblquote" if !st.skip => push(&mut out, &mut runs, &st, '“'),
                    "rdblquote" if !st.skip => push(&mut out, &mut runs, &st, '”'),
                    "bullet" if !st.skip => push(&mut out, &mut runs, &st, '•'),
                    "u" if !st.skip => {
                        let v = num.unwrap_or(0);
                        let v = if v < 0 { v + 65536 } else { v };
                        if let Some(ch) = char::from_u32(v as u32) {
                            push(&mut out, &mut runs, &st, ch);
                        }
                        uc_skip = 1;
                    }
                    _ => {}
                }
            }
            '\r' | '\n' => i += 1,
            ';' if table == Some("colortbl") => {
                colors.push(rgb);
                rgb = [0; 3];
                i += 1;
            }
            _ => {
                if table == Some("fonttbl") && font_no.is_some() {
                    name.push(c);
                } else if !st.skip {
                    if uc_skip > 0 {
                        uc_skip -= 1;
                    } else {
                        if para_start {
                            let at = out.chars().count();
                            if paragraphs.last().is_none_or(|p| p.1 != st.align) {
                                paragraphs.push((at, st.align));
                            }
                            para_start = false;
                        }
                        push(&mut out, &mut runs, &st, c);
                    }
                }
                i += 1;
            }
        }
    }
    // A trailing paragraph mark ends the text, it does not add a line.
    if out.ends_with('\r') {
        out.pop();
        let n = out.chars().count();
        runs.retain(|r| r.start < n);
        if let Some(r) = runs.last_mut() {
            r.end = r.end.min(n);
        }
    }
    StyledText {
        text: out,
        runs,
        paragraphs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_styles() {
        // Regression: the Bible's chapters are set as `member.rtf`, which
        // was not implemented.
        let rtf = r"{\rtf1\ansi{\fonttbl{\f0\froman Times New Roman;}{\f1\fswiss Verdana;}}
{\colortbl;\red0\green0\blue255;}\f1\fs20 {\b 1}\~No princ\'edpio\par\cf1\i Deus\i0\cf0  disse\par}";
        let st = parse(rtf);
        assert_eq!(st.text, "1\u{a0}No princípio\rDeus disse");
        let at = |i: usize| {
            &st.runs
                .iter()
                .find(|r| r.start <= i && i < r.end)
                .unwrap()
                .style
        };
        assert!(at(0).bold && !at(2).bold);
        assert_eq!((at(2).font.as_str(), at(2).size), ("Verdana", 10));
        let d = st.text.chars().position(|c| c == 'D').unwrap();
        assert!(at(d).italic);
        assert_eq!(at(d).color, [0, 0, 255]);
        assert!(!at(d + 5).italic);
        assert_eq!(at(d + 5).color, [0, 0, 0]);
    }

    #[test]
    fn superscript_verse_numbers() {
        let st = parse(r"{\rtf1\fs24 {\super 1}No\super 2\nosupersub x}");
        let at = |i: usize| {
            &st.runs
                .iter()
                .find(|r| r.start <= i && i < r.end)
                .unwrap()
                .style
        };
        assert_eq!((at(0).size, at(0).raise), (8, 4));
        assert_eq!((at(1).size, at(1).raise), (12, 0));
        assert_eq!((at(3).size, at(3).raise), (8, 4));
        assert_eq!((at(4).size, at(4).raise), (12, 0));
    }

    #[test]
    fn default_font() {
        let st = parse(r"{\rtf1\deff1{\fonttbl{\f0 Georgia;}{\f1 Arial;}}x\f0 y}");
        assert_eq!(st.runs[0].style.font, "Arial");
        assert_eq!(st.runs.last().unwrap().style.font, "Georgia");
    }

    #[test]
    fn unicode_and_skipped_groups() {
        let st = parse(r"{\rtf1{\*\generator x;}\uc1\u8212?a\qc b}");
        assert_eq!(st.text, "—ab");
        assert_eq!(st.paragraphs.first().map(|p| p.1), Some(Align::Left));
    }
}
