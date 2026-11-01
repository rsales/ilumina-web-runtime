//! The TreeView Xtra (`*XTV` sprite methods): a tree of nodes per sprite,
//! as Lingo builds it. Signatures come from the calls the program makes
//! (ADR 0008: behaviour, not the Xtra's code). ⏳ Drawing and the
//! `treeView*` mouse callbacks come with the Bible milestone (M3).

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
}

impl Default for Tree {
    fn default() -> Self {
        Tree {
            nodes: vec![Node::default(), Node::default()],
            fonts: vec!["Arial 12 plain".into()],
            icons: vec!["0 0 0 0 0".into()],
            spacing: DEFAULT_SPACING,
            refcon: Value::Int(0),
            settings: HashMap::new(),
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
