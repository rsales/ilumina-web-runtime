//! Chunk expressions: `char 2 to 4 of word 3 of item 1 of line 5 of s`.
//!
//! The bytecode pushes eight numbers (first/last for char, word, item, line).
//! `first = 0` means the level is not used; `last = 0` means "same as first".
//! Levels apply from the outside in: line, item, word, char.

use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChunkSpec {
    pub char: (i64, i64),
    pub word: (i64, i64),
    pub item: (i64, i64),
    pub line: (i64, i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Char,
    Word,
    Item,
    Line,
}

impl Kind {
    /// Chunk type ids used by `the number of <chunks>` (`get 1`).
    pub fn from_id(id: i64) -> Option<Self> {
        Some(match id {
            1 => Kind::Char,
            2 => Kind::Word,
            3 => Kind::Item,
            4 => Kind::Line,
            _ => return None,
        })
    }
}

/// Byte ranges of each chunk of `kind` in `s` (always on char boundaries).
fn pieces(s: &str, kind: Kind, item_delim: char) -> Vec<Range<usize>> {
    match kind {
        Kind::Char => s.char_indices().map(|(i, c)| i..i + c.len_utf8()).collect(),
        Kind::Item | Kind::Line => {
            let d = if kind == Kind::Line { '\r' } else { item_delim };
            let mut out = Vec::new();
            let mut start = 0;
            for (i, c) in s.char_indices() {
                if c == d {
                    out.push(start..i);
                    start = i + c.len_utf8();
                }
            }
            out.push(start..s.len());
            out
        }
        Kind::Word => {
            let mut out = Vec::new();
            let mut start = None;
            for (i, c) in s.char_indices() {
                let space = c.is_whitespace();
                match (start, space) {
                    (None, false) => start = Some(i),
                    (Some(st), true) => {
                        out.push(st..i);
                        start = None;
                    }
                    _ => {}
                }
            }
            if let Some(st) = start {
                out.push(st..s.len());
            }
            out
        }
    }
}

/// Count of `kind` chunks (`the number of words in s`).
pub fn count(s: &str, kind: Kind, item_delim: char) -> usize {
    if s.is_empty() {
        return 0;
    }
    pieces(s, kind, item_delim).len()
}

/// Byte range of one level within `range` of `s`. Out-of-range chunks give an
/// empty range at the end (Lingo returns "" for them).
fn narrow(
    s: &str,
    range: Range<usize>,
    kind: Kind,
    first: i64,
    last: i64,
    item_delim: char,
) -> Range<usize> {
    if first == 0 {
        return range;
    }
    let sub = &s[range.clone()];
    let ps = pieces(sub, kind, item_delim);
    let last = if last == 0 { first } else { last };
    let n = ps.len() as i64;
    let (a, b) = (first.max(1), last.min(n));
    if first > n || a > b {
        return range.end..range.end;
    }
    let start = ps[(a - 1) as usize].start;
    let end = ps[(b - 1) as usize].end;
    range.start + start..range.start + end
}

/// Byte range selected by `spec` in `s`.
pub fn locate(s: &str, spec: &ChunkSpec, item_delim: char) -> Range<usize> {
    let mut r = 0..s.len();
    r = narrow(s, r, Kind::Line, spec.line.0, spec.line.1, item_delim);
    r = narrow(s, r, Kind::Item, spec.item.0, spec.item.1, item_delim);
    r = narrow(s, r, Kind::Word, spec.word.0, spec.word.1, item_delim);
    r = narrow(s, r, Kind::Char, spec.char.0, spec.char.1, item_delim);
    r
}

pub fn get(s: &str, spec: &ChunkSpec, item_delim: char) -> String {
    s[locate(s, spec, item_delim)].to_string()
}

/// `put v into/after/before chunk … of s`. `mode`: 1 into, 2 after, 3 before.
pub fn put(s: &str, spec: &ChunkSpec, v: &str, mode: u8, item_delim: char) -> String {
    let r = locate(s, spec, item_delim);
    let mut out = String::with_capacity(s.len() + v.len());
    match mode {
        2 => {
            out.push_str(&s[..r.end]);
            out.push_str(v);
            out.push_str(&s[r.end..]);
        }
        3 => {
            out.push_str(&s[..r.start]);
            out.push_str(v);
            out.push_str(&s[r.start..]);
        }
        _ => {
            out.push_str(&s[..r.start]);
            out.push_str(v);
            out.push_str(&s[r.end..]);
        }
    }
    out
}

/// `delete chunk … of s`: also removes one adjacent delimiter for items and
/// lines.
pub fn delete(s: &str, spec: &ChunkSpec, item_delim: char) -> String {
    let mut r = locate(s, spec, item_delim);
    let delim = if spec.char.0 != 0 || spec.word.0 != 0 {
        None
    } else if spec.item.0 != 0 {
        Some(item_delim)
    } else if spec.line.0 != 0 {
        Some('\r')
    } else {
        None
    };
    if let Some(d) = delim {
        if s[r.end..].starts_with(d) {
            r.end += d.len_utf8();
        } else if r.start > 0 && s[..r.start].ends_with(d) {
            r.start -= d.len_utf8();
        }
    }
    format!("{}{}", &s[..r.start], &s[r.end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(f: impl FnOnce(&mut ChunkSpec)) -> ChunkSpec {
        let mut s = ChunkSpec::default();
        f(&mut s);
        s
    }

    #[test]
    fn get_levels_and_ranges() {
        let s = "alpha beta,gamma\rdelta  eps";
        assert_eq!(get(s, &spec(|c| c.word = (2, 0)), ','), "beta,gamma");
        assert_eq!(get(s, &spec(|c| c.item = (2, 0)), ','), "gamma\rdelta  eps");
        assert_eq!(get(s, &spec(|c| c.line = (2, 0)), ','), "delta  eps");
        assert_eq!(
            get(
                s,
                &spec(|c| {
                    c.line = (2, 0);
                    c.word = (2, 0)
                }),
                ','
            ),
            "eps"
        );
        assert_eq!(get(s, &spec(|c| c.char = (2, 4)), ','), "lph");
        assert_eq!(get(s, &spec(|c| c.word = (9, 0)), ','), "");
        assert_eq!(get("a;b", &spec(|c| c.item = (2, 0)), ';'), "b");
    }

    #[test]
    fn counts() {
        assert_eq!(count("a b  c", Kind::Word, ','), 3);
        assert_eq!(count("a,b,", Kind::Item, ','), 3);
        assert_eq!(count("", Kind::Line, ','), 0);
        assert_eq!(count("é!", Kind::Char, ','), 2);
    }

    #[test]
    fn put_and_delete() {
        assert_eq!(
            put("a,b,c", &spec(|c| c.item = (2, 0)), "X", 1, ','),
            "a,X,c"
        );
        assert_eq!(
            put("a,b,c", &spec(|c| c.item = (2, 0)), "X", 2, ','),
            "a,bX,c"
        );
        assert_eq!(
            put("a,b,c", &spec(|c| c.item = (2, 0)), "X", 3, ','),
            "a,Xb,c"
        );
        assert_eq!(delete("a,b,c", &spec(|c| c.item = (2, 0)), ','), "a,c");
        assert_eq!(delete("a,b,c", &spec(|c| c.item = (3, 0)), ','), "a,b");
        assert_eq!(delete("l1\rl2", &spec(|c| c.line = (1, 0)), ','), "l2");
        assert_eq!(delete("hello", &spec(|c| c.char = (1, 2)), ','), "llo");
    }
}
