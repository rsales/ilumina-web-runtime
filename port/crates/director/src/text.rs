//! Text layout and drawing for text (`XMED`) and field (`STXT`) members.
//!
//! The crate does no I/O and ships no fonts: a host implements [`Fonts`]
//! (fontdue over the system's fonts natively, the browser's canvas in WASM).
//! Director treats point sizes as pixels (72 dpi stage), so a 10 pt run is
//! rasterised at a 10 px em.

use crate::Canvas;
use rifx::text::{Align, StyledText, TextStyle};

/// A rasterised glyph. `xmin`/`ymin` place the bitmap's bottom-left corner
/// relative to the pen position on the baseline (y grows upwards, as in
/// font units); `coverage` is `width × height` alpha, top row first.
#[derive(Clone, Debug, Default)]
pub struct Glyph {
    pub width: usize,
    pub height: usize,
    pub xmin: i32,
    pub ymin: i32,
    pub advance: f32,
    pub coverage: Vec<u8>,
}

/// Vertical metrics of a face at a size, in pixels (`descent` positive).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub gap: f32,
}

/// Font rasteriser supplied by the host.
pub trait Fonts {
    fn line(&self, style: &TextStyle) -> LineMetrics;
    fn glyph(&self, style: &TextStyle, ch: char) -> Glyph;
}

/// A character placed on a line.
#[derive(Clone, Debug)]
struct Placed {
    ch: char,
    idx: usize,
    run: usize,
    x: f32,
    advance: f32,
}

#[derive(Clone, Debug, Default)]
struct Line {
    items: Vec<Placed>,
    ascent: f32,
    height: f32,
}

/// Lay `text` out in a box `width` pixels wide: `\r`/`\n` break lines and
/// words wrap at spaces (a word wider than the box is broken anywhere).
fn layout(text: &StyledText, width: f32, fonts: &dyn Fonts) -> Vec<Line> {
    let chars: Vec<char> = text.text.chars().collect();
    let run_of = |i: usize| {
        text.runs
            .iter()
            .position(|r| r.start <= i && i < r.end)
            .unwrap_or(0)
    };
    let default = TextStyle::default();
    let style = |run: usize| text.runs.get(run).map(|r| &r.style).unwrap_or(&default);

    let mut lines = Vec::new();
    let mut line = Line::default();
    let mut x = 0f32;
    let mut last_space: Option<usize> = None; // index into line.items

    let finish = |line: &mut Line, lines: &mut Vec<Line>, fallback: &TextStyle| {
        if line.items.is_empty() && line.height == 0.0 {
            let m = fonts.line(fallback);
            line.ascent = m.ascent;
            line.height = m.ascent + m.descent + m.gap;
        }
        lines.push(std::mem::take(line));
    };
    let mut prev_style = style(0).clone();

    for (i, &ch) in chars.iter().enumerate() {
        let run = run_of(i);
        let st = style(run);
        if ch == '\r' || ch == '\n' {
            prev_style = st.clone();
            finish(&mut line, &mut lines, &prev_style);
            x = 0.0;
            last_space = None;
            continue;
        }
        let m = fonts.line(st);
        let adv = fonts.glyph(st, ch).advance;
        if x + adv > width && !line.items.is_empty() && ch != ' ' {
            // Wrap: move the current word (after the last space) to a new line.
            let carry: Vec<Placed> = match last_space {
                Some(s) => line.items.split_off(s + 1),
                None => Vec::new(),
            };
            finish(&mut line, &mut lines, st);
            x = 0.0;
            last_space = None;
            for mut p in carry {
                let ps = style(p.run);
                let pm = fonts.line(ps);
                p.x = x;
                x += p.advance;
                line.ascent = line.ascent.max(pm.ascent);
                line.height = line.height.max(pm.ascent + pm.descent + pm.gap);
                line.items.push(p);
            }
        }
        if ch == ' ' {
            last_space = Some(line.items.len());
        }
        line.ascent = line.ascent.max(m.ascent);
        line.height = line.height.max(m.ascent + m.descent + m.gap);
        line.items.push(Placed {
            ch,
            idx: i,
            run,
            x,
            advance: adv,
        });
        x += adv;
        prev_style = st.clone();
    }
    if !line.items.is_empty() {
        finish(&mut line, &mut lines, &prev_style);
    }
    lines
}

/// Text measurements for Lingo (`charPosToLoc`, `locToCharPos`, a text
/// member's height), from the same layout the renderer draws.
pub struct Measure {
    lines: Vec<Line>,
    chars: usize,
}

impl Measure {
    pub fn new(text: &StyledText, width: f32, fonts: &dyn Fonts) -> Self {
        Measure {
            lines: layout(text, width.max(1.0), fonts),
            chars: text.text.chars().count(),
        }
    }

    /// Total height of the laid-out text.
    pub fn height(&self) -> f32 {
        self.lines.iter().map(|l| l.height).sum()
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// `charPosToLoc`: left edge of character `n` (1-based, clamped) and the
    /// bottom of its line (⏳ Director's exact vertical reference).
    pub fn char_loc(&self, n: usize) -> (f32, f32) {
        let idx = n.clamp(1, self.chars.max(1)) - 1;
        let mut top = 0.0;
        let mut last = (0.0, 0.0);
        for l in &self.lines {
            if let Some(p) = l.items.iter().find(|p| p.idx == idx) {
                return (p.x, top + l.height);
            }
            if let Some(p) = l.items.last() {
                last = (p.x + p.advance, top + l.height);
            }
            top += l.height;
        }
        last
    }

    /// `locToCharPos`: the character (1-based) nearest to point (x, y).
    pub fn char_at(&self, x: f32, y: f32) -> usize {
        let mut top = 0.0;
        for (i, l) in self.lines.iter().enumerate() {
            let last_line = i + 1 == self.lines.len();
            if y < top + l.height || last_line {
                return match l.items.iter().find(|p| x < p.x + p.advance / 2.0) {
                    Some(p) => p.idx + 1,
                    None => l
                        .items
                        .last()
                        .map(|p| p.idx + 2)
                        .unwrap_or(1)
                        .min(self.chars + 1),
                };
            }
            top += l.height;
        }
        1
    }

    /// `pointToChar`: the character (1-based) whose box holds (x, y), or
    /// `None` between or past characters.
    pub fn char_under(&self, x: f32, y: f32) -> Option<usize> {
        let mut top = 0.0;
        for l in &self.lines {
            if y >= top && y < top + l.height {
                return l
                    .items
                    .iter()
                    .find(|p| x >= p.x && x < p.x + p.advance)
                    .map(|p| p.idx + 1);
            }
            top += l.height;
        }
        None
    }

    /// `locVToLinePos`: the line (1-based) at distance `y` from the top;
    /// past the end, the last line (1 for empty text).
    pub fn line_at(&self, y: f32) -> usize {
        let mut top = 0.0;
        for (i, l) in self.lines.iter().enumerate() {
            if y < top + l.height {
                return i + 1;
            }
            top += l.height;
        }
        self.lines.len().max(1)
    }

    /// `linePosToLocV`: distance from the top to line `n`'s baseline.
    pub fn line_baseline(&self, n: usize) -> f32 {
        let mut top = 0.0;
        for (i, l) in self.lines.iter().enumerate() {
            if i + 1 == n {
                return top + l.ascent;
            }
            top += l.height;
        }
        top
    }
}

impl Canvas {
    /// Draw styled text into the box (`left`, `top`, `width`, `height`),
    /// clipped to it. `alpha` is the sprite blend (255 = opaque).
    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &mut self,
        text: &StyledText,
        left: i64,
        top: i64,
        width: i64,
        height: i64,
        alpha: u32,
        fonts: &dyn Fonts,
    ) {
        let default = TextStyle::default();
        let mut y = top as f32;
        for line in layout(text, width.max(1) as f32, fonts) {
            let baseline = (y + line.ascent).round() as i64;
            // Trailing spaces do not count towards the aligned width.
            let used = line
                .items
                .iter()
                .rev()
                .find(|p| p.ch != ' ')
                .map(|p| p.x + p.advance)
                .unwrap_or(0.0);
            let slack = (width as f32 - used).max(0.0);
            let dx = match line.items.first().map(|p| text.align_at(p.idx)) {
                Some(Align::Center) => (slack / 2.0).floor() as i64,
                Some(Align::Right) => slack.floor() as i64,
                _ => 0,
            };
            let pen = left + dx;
            for p in &line.items {
                if p.ch == ' ' {
                    continue;
                }
                let st = text.runs.get(p.run).map(|r| &r.style).unwrap_or(&default);
                let g = fonts.glyph(st, p.ch);
                let gx = pen + (p.x.round() as i64) + g.xmin as i64;
                let gy = baseline - (g.ymin as i64 + g.height as i64);
                for row in 0..g.height {
                    let cy = gy + row as i64;
                    if cy < top || cy >= top + height {
                        continue;
                    }
                    for col in 0..g.width {
                        let cx = gx + col as i64;
                        if cx < left || cx >= left + width {
                            continue;
                        }
                        let cov = g.coverage[row * g.width + col] as u32;
                        if cov > 0 {
                            self.put(cx, cy, st.color, cov * alpha / 255);
                        }
                    }
                }
                if st.underline {
                    for cx in gx..gx + g.advance.round() as i64 {
                        if cx >= left && cx < left + width && baseline + 1 < top + height {
                            self.put(cx, baseline + 1, st.color, alpha);
                        }
                    }
                }
            }
            y += line.height;
            if y as i64 >= top + height {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rifx::text::Run;

    /// Monospace test font: every glyph is a solid 4×6 box, advance 5.
    struct Boxes;
    impl Fonts for Boxes {
        fn line(&self, _: &TextStyle) -> LineMetrics {
            LineMetrics {
                ascent: 6.0,
                descent: 2.0,
                gap: 0.0,
            }
        }
        fn glyph(&self, _: &TextStyle, ch: char) -> Glyph {
            if ch == ' ' {
                return Glyph {
                    advance: 5.0,
                    ..Default::default()
                };
            }
            Glyph {
                width: 4,
                height: 6,
                xmin: 0,
                ymin: 0,
                advance: 5.0,
                coverage: vec![255; 24],
            }
        }
    }

    #[test]
    fn measurements_follow_the_layout() {
        // Width 22 fits four 5 px glyphs: "abc de" wraps after the space.
        let m = Measure::new(&plain("abc de"), 22.0, &Boxes);
        assert_eq!(m.line_count(), 2);
        assert_eq!(m.height(), 16.0);
        assert_eq!(m.char_loc(2), (5.0, 8.0));
        assert_eq!(m.char_loc(6), (5.0, 16.0));
        assert_eq!(m.char_loc(99), (5.0, 16.0)); // clamped to the last char
        assert_eq!(m.char_at(6.0, 1.0), 2);
        assert_eq!(m.char_at(0.0, 12.0), 5);
        assert_eq!(m.line_baseline(2), 14.0);
    }

    fn plain(s: &str) -> StyledText {
        StyledText {
            text: s.into(),
            runs: vec![Run {
                start: 0,
                end: s.chars().count(),
                style: TextStyle::default(),
            }],
            paragraphs: Vec::new(),
        }
    }

    #[test]
    fn right_and_centre_alignment() {
        let mut t = plain("ab");
        t.paragraphs = vec![(0, Align::Right)];
        let mut c = Canvas::new(20, 10, [255, 255, 255]);
        c.text(&t, 0, 0, 20, 10, 255, &Boxes);
        // "ab" advances 10 px: right-aligned it starts at x = 10.
        assert_eq!(c.px[9 * 4], 255);
        assert_eq!(c.px[10 * 4], 0);
        t.paragraphs = vec![(0, Align::Center)];
        let mut c = Canvas::new(20, 10, [255, 255, 255]);
        c.text(&t, 0, 0, 20, 10, 255, &Boxes);
        assert_eq!(c.px[4 * 4], 255);
        assert_eq!(c.px[5 * 4], 0);
    }

    #[test]
    fn wraps_at_spaces_and_breaks_lines() {
        // Box 22 px wide fits 4 glyphs (20 px): "ab cd" wraps before "cd".
        let lines = layout(&plain("ab cd\re"), 22.0, &Boxes);
        let text: Vec<String> = lines
            .iter()
            .map(|l| l.items.iter().map(|p| p.ch).collect())
            .collect();
        assert_eq!(text, ["ab ", "cd", "e"]);
        assert_eq!(lines[1].items[0].x, 0.0);
    }

    #[test]
    fn draws_glyphs_on_baseline_clipped_to_box() {
        let at = |c: &Canvas, x: usize, y: usize| c.px[(y * 20 + x) * 4];
        // 7 px box: "b" wraps to the second line (line height 8).
        let mut c = Canvas::new(20, 20, [255, 255, 255]);
        c.text(&plain("ab"), 2, 3, 7, 20, 255, &Boxes);
        assert_eq!(at(&c, 2, 3), 0); // first glyph top-left
        assert_eq!(at(&c, 5, 8), 0); // its bottom-right
        assert_eq!(at(&c, 2, 9), 255); // below the glyph (descent)
        assert_eq!(at(&c, 7, 3), 255); // nothing to its right
        assert_eq!(at(&c, 2, 11), 0); // "b" on line 2
                                      // A box 10 px tall clips line 2 after its first two rows.
        let mut c = Canvas::new(20, 20, [255, 255, 255]);
        c.text(&plain("ab"), 2, 3, 7, 10, 255, &Boxes);
        assert_eq!(at(&c, 2, 12), 0);
        assert_eq!(at(&c, 2, 13), 255);
    }
}
