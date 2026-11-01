//! The TreeView Xtra (`*XTV` sprite methods): a tree of nodes per sprite,
//! as Lingo builds it. Signatures come from the calls the program makes
//! (ADR 0008: behaviour, not the Xtra's code). Drawn rows, hit testing and
//! the `TreeViewSingleClick`/`treeViewMouseEnter`/`Exit` callbacks follow
//! what the scripts expect; ⏳ the exact metrics and the scroll bar are to
//! compare with the original (Boxedwine screenshots).

use lingo_vm::Value;
use std::collections::HashMap;

/// Pixels per row until the drawing exists (`setSpacingXTV` changes it).
const DEFAULT_SPACING: i64 = 16;

#[derive(Clone, Debug, Default)]
pub struct Node {
    pub parent: i64,
    pub children: Vec<i64>,
    pub name: String,
    pub expanded: bool,
    /// Icon id per state (`setIconIDXTV(node, state, id)`).
    pub icons: HashMap<i64, i64>,
    pub font: i64,
    pub color: Option<(i64, i64, i64)>,
    pub user_strings: HashMap<i64, String>,
    pub user_nums: HashMap<i64, i64>,
}

#[derive(Clone, Debug)]
pub struct Tree {
    /// Node 0 is unused; the root is node 1.
    pub nodes: Vec<Node>,
    /// Fonts and icons by id (0-based; id 0 is the Xtra's default, so
    /// Lingo treats 0 as "not added"): "name size style" and
    /// "castLib member flags width height", the words Lingo reads back.
    pub fonts: Vec<String>,
    pub icons: Vec<String>,
    pub spacing: i64,
    pub refcon: Value,
    /// Settings kept for reading back (colors, line mode, scroll bars…).
    pub settings: HashMap<String, Vec<Value>>,
    /// The node last clicked (drawn highlighted), 0 = none.
    pub selected: i64,
    /// The node under the mouse (for enter/exit callbacks), 0 = none.
    pub hover: i64,
    /// Pixels scrolled down.
    pub scroll: i64,
}

/// Where a point falls in the tree: the node and whether it is on the
/// node's icon (1) or its text (2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub node: i64,
    pub icon_or_text: i64,
}

/// Pixels of indent per level and between icon and text.
const INDENT: i64 = 14;
const GAP: i64 = 4;
const LEFT: i64 = 4;

impl Default for Tree {
    fn default() -> Self {
        Tree {
            nodes: vec![Node::default(), Node::default()],
            fonts: vec!["Arial 12 plain".into()],
            icons: vec!["0 0 0 0 0".into()],
            spacing: DEFAULT_SPACING,
            refcon: Value::Int(0),
            settings: HashMap::new(),
            selected: 0,
            hover: 0,
            scroll: 0,
        }
    }
}

/// A Director 16-bit-per-channel colour (`setTextHiliteColorXTV(52942, …)`)
/// or an 8-bit one, as RGB.
fn rgb16(v: &[Value]) -> Option<[u8; 3]> {
    let c: Vec<i64> = v.iter().take(3).map(|x| x.to_int()).collect();
    if c.len() < 3 {
        return None;
    }
    let big = c.iter().any(|&x| x > 255);
    Some([0, 1, 2].map(|i| {
        if big {
            (c[i] >> 8) as u8
        } else {
            c[i].clamp(0, 255) as u8
        }
    }))
}

impl Tree {
    /// The text style of font id `id` ("name size style" words).
    pub fn font_style(&self, id: i64) -> rifx::text::TextStyle {
        let f = self.fonts.get(id.max(0) as usize).or(self.fonts.first());
        let words: Vec<&str> = f.map(|s| s.split(' ').collect()).unwrap_or_default();
        // The name may have spaces: the size is the second word from the end.
        let n = words.len();
        let mut st = rifx::text::TextStyle::default();
        if n >= 3 {
            st.font = words[..n - 2].join(" ");
            st.size = words[n - 2].parse().unwrap_or(12);
            let style = words[n - 1].to_ascii_lowercase();
            st.bold = style.contains("bold") || style == "1";
            st.italic = style.contains("italic") || style == "2";
        }
        st
    }

    /// Height of one row: the tallest font's line plus the spacing.
    pub fn row_height(&self, fonts: &dyn director::text::Fonts) -> i64 {
        let mut h = 0.0f32;
        for i in 0..self.fonts.len() as i64 {
            let m = fonts.line(&self.font_style(i));
            h = h.max(m.ascent + m.descent);
        }
        (h.ceil() as i64).max(1) + self.spacing
    }

    fn depth(&self, mut id: i64) -> i64 {
        let mut d = 0;
        while let Some(n) = self.node(id) {
            if n.parent <= ROOT {
                break;
            }
            id = n.parent;
            d += 1;
        }
        d
    }

    /// The icon id a node shows: 2/3 collapsed/expanded for parents, 0 for
    /// leaves (1 when selected).
    pub fn icon_of(&self, id: i64) -> i64 {
        let Some(n) = self.node(id) else { return 0 };
        let state = if n.children.is_empty() {
            (id == self.selected) as i64
        } else if n.expanded {
            3
        } else {
            2
        };
        n.icons
            .get(&state)
            .or(n.icons.get(&0))
            .copied()
            .unwrap_or(0)
    }

    /// (cast lib, member, width, height) of icon id `i`.
    pub fn icon(&self, i: i64) -> Option<(i64, i64, i64, i64)> {
        let w: Vec<i64> = self
            .icons
            .get(i.max(0) as usize)?
            .split(' ')
            .map(|x| x.parse().unwrap_or(0))
            .collect();
        match w.as_slice() {
            [l, m, _, iw, ih, ..] if *m > 0 => Some((*l, *m, *iw, *ih)),
            _ => None,
        }
    }

    /// The node and part under (x, y), relative to the sprite.
    pub fn hit(&self, x: i64, y: i64, fonts: &dyn director::text::Fonts) -> Option<Hit> {
        let row = (y + self.scroll).div_euclid(self.row_height(fonts));
        let id = *self.visible().get(usize::try_from(row).ok()?)?;
        let left = LEFT + self.depth(id) * INDENT;
        let iw = self.icon(self.icon_of(id)).map_or(0, |i| i.2);
        let on_icon = x >= left && x < left + iw.max(1) + GAP / 2;
        Some(Hit {
            node: id,
            icon_or_text: if on_icon { 1 } else { 2 },
        })
    }

    /// Draw the rows into a `w`×`h` bitmap (transparent background).
    pub fn render(
        &self,
        w: i64,
        h: i64,
        fonts: &dyn director::text::Fonts,
        icon_bitmap: &dyn Fn(i64, i64) -> Option<std::rc::Rc<director::Bitmap>>,
    ) -> director::Bitmap {
        let (w, h) = (w.max(1) as usize, h.max(1) as usize);
        let mut canvas = director::Canvas::new(w, h, [255, 255, 255]);
        let rh = self.row_height(fonts);
        let hilite = self
            .settings
            .get("settexthilitecolor")
            .and_then(|v| rgb16(v))
            .unwrap_or([206, 214, 222]);
        for (row, id) in self.visible().into_iter().enumerate() {
            let top = row as i64 * rh - self.scroll;
            if top + rh < 0 {
                continue;
            }
            if top >= h as i64 {
                break;
            }
            let n = &self.nodes[id as usize];
            let mut x = LEFT + self.depth(id) * INDENT;
            if let Some((lib, member, iw, ih)) = self.icon(self.icon_of(id)) {
                if let Some(b) = icon_bitmap(lib, member) {
                    let s = rifx::score::Sprite {
                        number: 0,
                        kind: 1,
                        ink: 36,
                        fore: 255,
                        back: 0,
                        cast_lib: lib as u16,
                        member: member as u16,
                        top: (top + (rh - ih) / 2 + b.reg_y as i64) as i16,
                        left: (x + b.reg_x as i64) as i16,
                        height: ih.max(1) as u16,
                        width: iw.max(1) as u16,
                        blend: 0,
                        stretch: false,
                        color_flag: 0,
                        fore_rgb: [0; 3],
                        back_rgb: [0; 3],
                    };
                    canvas.draw(&b, &s);
                }
                x += iw + GAP;
            }
            let mut style = self.font_style(n.font);
            if let Some((r, g, b)) = n.color {
                style.color =
                    rgb16(&[Value::Int(r), Value::Int(g), Value::Int(b)]).unwrap_or([0; 3]);
            }
            let text = rifx::text::StyledText {
                text: n.name.clone(),
                runs: vec![rifx::text::Run {
                    start: 0,
                    end: n.name.chars().count(),
                    style,
                }],
                paragraphs: Vec::new(),
            };
            if id == self.selected {
                let tw: f32 = n
                    .name
                    .chars()
                    .map(|c| fonts.glyph(&text.runs[0].style, c).advance)
                    .sum();
                fill(
                    &mut canvas,
                    x - 2,
                    top + 1,
                    tw.ceil() as i64 + 4,
                    rh - 2,
                    hilite,
                );
            }
            let m = fonts.line(&text.runs[0].style);
            let ty = top + ((rh as f32 - m.ascent - m.descent) / 2.0) as i64;
            canvas.text(&text, x, ty, w as i64 - x, rh, 255, fonts);
        }
        director::Bitmap {
            width: w,
            height: h,
            rgba: canvas.px,
            // Drawn at the sprite's location, its centre (see sprite rect).
            reg_x: (w / 2) as i16,
            reg_y: (h / 2) as i16,
        }
    }
}

fn fill(c: &mut director::Canvas, x: i64, y: i64, w: i64, h: i64, rgb: [u8; 3]) {
    for yy in y.max(0)..(y + h).min(c.height as i64) {
        for xx in x.max(0)..(x + w).min(c.width as i64) {
            let i = (yy as usize * c.width + xx as usize) * 4;
            c.px[i..i + 3].copy_from_slice(&rgb);
            c.px[i + 3] = 255;
        }
    }
}

pub const ROOT: i64 = 1;

/// Arguments as space-separated words (symbols without `#`).
fn words(args: &[Value]) -> String {
    args.iter()
        .map(|v| v.to_text())
        .collect::<Vec<_>>()
        .join(" ")
}

impl Tree {
    fn node(&self, id: i64) -> Option<&Node> {
        (id >= ROOT).then(|| self.nodes.get(id as usize)).flatten()
    }

    fn node_mut(&mut self, id: i64) -> Option<&mut Node> {
        (id >= ROOT)
            .then(|| self.nodes.get_mut(id as usize))
            .flatten()
    }

    /// Rows shown: the root's descendants under expanded nodes, in order.
    pub fn visible(&self) -> Vec<i64> {
        fn walk(t: &Tree, id: i64, out: &mut Vec<i64>) {
            if let Some(n) = t.node(id) {
                for &c in &n.children {
                    out.push(c);
                    if t.node(c).is_some_and(|c| c.expanded) {
                        walk(t, c, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(self, ROOT, &mut out);
        out
    }

    fn remove_children(&mut self, id: i64) {
        // Detached nodes stay in the table (ids are never reused).
        if let Some(n) = self.node_mut(id) {
            n.children.clear();
        }
    }

    /// One `*XTV` method (lower-case name, arguments after the sprite).
    /// `None` = not a TreeView method.
    pub fn method(&mut self, name: &str, args: &[Value]) -> Option<Value> {
        let a = |i: usize| args.get(i).cloned().unwrap_or_default();
        let n = |i: usize| a(i).to_int();
        let base = name.strip_suffix("xtv")?;
        Some(match base {
            // The Xtra's own registration key: accepted, nothing to check.
            "setmagic" => Value::Void,
            "setrefcon" => {
                self.refcon = a(0);
                Value::Void
            }
            "getrootnode" => Value::Int(ROOT),
            "addchild" => {
                let parent = n(0);
                if self.node(parent).is_none() {
                    return Some(Value::Int(0));
                }
                let id = self.nodes.len() as i64;
                self.nodes.push(Node {
                    parent,
                    ..Default::default()
                });
                self.node_mut(parent).unwrap().children.push(id);
                Value::Int(id)
            }
            "removeallchildren" => {
                self.remove_children(n(0));
                Value::Void
            }
            "getnumchildren" => Value::Int(self.node(n(0)).map_or(0, |x| x.children.len()) as i64),
            "getnthchild" => Value::Int(
                self.node(n(0))
                    .and_then(|x| x.children.get((n(1) - 1).max(0) as usize).copied())
                    .unwrap_or(0),
            ),
            "getparent" => Value::Int(self.node(n(0)).map_or(0, |x| x.parent)),
            "setname" => {
                if let Some(x) = self.node_mut(n(0)) {
                    x.name = a(1).to_text();
                }
                Value::Void
            }
            "getname" => Value::Str(self.node(n(0)).map(|x| x.name.clone()).unwrap_or_default()),
            "setexpanded" => {
                if let Some(x) = self.node_mut(n(0)) {
                    x.expanded = a(1).truthy();
                }
                Value::Void
            }
            "getexpanded" => Value::Int(self.node(n(0)).is_some_and(|x| x.expanded) as i64),
            "seticonid" => {
                if let Some(x) = self.node_mut(n(0)) {
                    x.icons.insert(n(1), n(2));
                }
                Value::Void
            }
            "geticonid" => Value::Int(
                self.node(n(0))
                    .and_then(|x| x.icons.get(&n(1)).copied())
                    .unwrap_or(0),
            ),
            "setfontid" => {
                if let Some(x) = self.node_mut(n(0)) {
                    x.font = n(1);
                }
                Value::Void
            }
            "setfontcolor" => {
                if let Some(x) = self.node_mut(n(0)) {
                    x.color = Some((n(1), n(2), n(3)));
                }
                Value::Void
            }
            "setuserstring" => {
                if let Some(x) = self.node_mut(n(0)) {
                    x.user_strings.insert(n(1), a(2).to_text());
                }
                Value::Void
            }
            "getuserstring" => Value::Str(
                self.node(n(0))
                    .and_then(|x| x.user_strings.get(&n(1)).cloned())
                    .unwrap_or_default(),
            ),
            "setusernum" => {
                if let Some(x) = self.node_mut(n(0)) {
                    x.user_nums.insert(n(1), n(2));
                }
                Value::Void
            }
            "getusernum" => Value::Int(
                self.node(n(0))
                    .and_then(|x| x.user_nums.get(&n(1)).copied())
                    .unwrap_or(0),
            ),
            // Regression: the ids are 0-based and read back as words
            // (`word 1 of getFontXTV(id)`); a list broke `getIconID`.
            "addfont" => {
                self.fonts.push(words(args));
                Value::Int(self.fonts.len() as i64 - 1)
            }
            "getnumfonts" => Value::Int(self.fonts.len() as i64),
            "getfont" => Value::Str(
                self.fonts
                    .get(n(0).max(0) as usize)
                    .cloned()
                    .unwrap_or_default(),
            ),
            "setfont" => {
                if let Some(f) = self.fonts.get_mut(n(0).max(0) as usize) {
                    *f = words(args.get(1..).unwrap_or_default());
                }
                Value::Void
            }
            "addicon" => {
                self.icons.push(words(args));
                Value::Int(self.icons.len() as i64 - 1)
            }
            "getnumicons" => Value::Int(self.icons.len() as i64),
            "geticon" => Value::Str(
                self.icons
                    .get(n(0).max(0) as usize)
                    .cloned()
                    .unwrap_or_default(),
            ),
            "seticon" => {
                if let Some(i) = self.icons.get_mut(n(0).max(0) as usize) {
                    *i = words(args.get(1..).unwrap_or_default());
                }
                Value::Void
            }
            "setspacing" => {
                self.spacing = n(0).max(1);
                Value::Void
            }
            // "x y" of a node's row, relative to the sprite (⏳ indent).
            "getnodelocation" => {
                let row = self.visible().iter().position(|&v| v == n(0));
                match row {
                    Some(r) => Value::Str(format!("0 {}", r as i64 * self.spacing)),
                    None => Value::Str("0 0".into()),
                }
            }
            // "width height" of all shown rows (⏳ width).
            "getcontentsize" => {
                Value::Str(format!("0 {}", self.visible().len() as i64 * self.spacing))
            }
            // Display settings: kept, drawn later.
            _ if base.starts_with("set") || base.starts_with("reset") => {
                self.settings.insert(base.to_string(), args.to_vec());
                Value::Void
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(t: &mut Tree, name: &str, args: &[Value]) -> Value {
        t.method(&name.to_ascii_lowercase(), args).unwrap()
    }

    #[test]
    fn builds_and_reads_a_tree() {
        // Regression: Bible's and Extras' trees stopped at setMagicXTV (the
        // TreeView Xtra was not implemented).
        let mut t = Tree::default();
        let i = Value::Int;
        assert!(matches!(
            call(&mut t, "setMagicXTV", &[Value::str("key")]),
            Value::Void
        ));
        let root = call(&mut t, "getRootNodeXTV", &[]).to_int();
        let a = call(&mut t, "addChildXTV", &[i(root), i(1), i(0)]).to_int();
        let b = call(&mut t, "addChildXTV", &[i(root), i(1), i(0)]).to_int();
        let a1 = call(&mut t, "addChildXTV", &[i(a), i(1), i(0)]).to_int();
        call(&mut t, "setNameXTV", &[i(a), Value::str("Gênesis")]);
        call(
            &mut t,
            "SetUserStringXTV",
            &[i(a1), i(0), Value::str("Gn 1")],
        );
        call(&mut t, "SetUserNumXTV", &[i(a1), i(0), i(7)]);
        assert_eq!(call(&mut t, "getNameXTV", &[i(a)]).to_text(), "Gênesis");
        assert_eq!(call(&mut t, "getNumChildrenXTV", &[i(root)]).to_int(), 2);
        assert_eq!(call(&mut t, "getNthChildXTV", &[i(root), i(2)]).to_int(), b);
        assert_eq!(call(&mut t, "GetParentXTV", &[i(a1)]).to_int(), a);
        assert_eq!(
            call(&mut t, "GetUserStringXTV", &[i(a1), i(0)]).to_text(),
            "Gn 1"
        );
        assert_eq!(call(&mut t, "GetUserNumXTV", &[i(a1), i(0)]).to_int(), 7);
        assert_eq!(t.visible(), vec![a, b]);
        call(&mut t, "setExpandedXTV", &[i(a), i(1)]);
        assert_eq!(call(&mut t, "getExpandedXTV", &[i(a)]).to_int(), 1);
        assert_eq!(t.visible(), vec![a, a1, b]);
        call(&mut t, "setSpacingXTV", &[i(20)]);
        assert_eq!(call(&mut t, "getContentSizeXTV", &[]).to_text(), "0 60");
        assert_eq!(
            call(&mut t, "GetNodeLocationXTV", &[i(b)]).to_text(),
            "0 40"
        );
        call(&mut t, "removeAllChildrenXTV", &[i(root)]);
        assert!(t.visible().is_empty());
        let f = call(
            &mut t,
            "addFontXTV",
            &[Value::str("Verdana"), i(10), Value::sym("bold")],
        );
        assert_eq!(f.to_int(), 1);
        assert_eq!(call(&mut t, "getNumFontsXTV", &[]).to_int(), 2);
        assert_eq!(
            call(&mut t, "getFontXTV", &[i(1)]).to_text(),
            "Verdana 10 bold"
        );
        let ic = call(&mut t, "addIconXTV", &[i(3), i(12), i(0), i(9), i(9)]).to_int();
        assert_eq!(call(&mut t, "getIconXTV", &[i(ic)]).to_text(), "3 12 0 9 9");
        call(
            &mut t,
            "setIconXTV",
            &[i(ic), i(3), i(13), i(0), i(9), i(9)],
        );
        assert_eq!(call(&mut t, "getIconXTV", &[i(ic)]).to_text(), "3 13 0 9 9");
        assert!(t.method("notatreeviewcall", &[]).is_none());
        assert!(matches!(
            call(&mut t, "setLineModeXTV", &[i(1)]),
            Value::Void
        ));
    }
}
