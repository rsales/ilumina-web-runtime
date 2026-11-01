//! The Mui Xtra (Director's dialog builder). A dialog is a window property
//! list plus a list of item property lists (`#windowBegin` … `#windowEnd`).
//! `Run` shows it modally and reports events to the Lingo callback named in
//! `#callback`: `callback(event, itemPosition, itemPropList)`.
//!
//! Who answers a dialog is a [`Responder`]: the page in the browser, or a
//! scripted user in tests. The program's own Lingo decides what the answers
//! mean (ADR 0005: no special handling of registration or activation).

use lingo_vm::Value;

/// What the responder sees of one item.
#[derive(Clone, Debug, PartialEq)]
pub struct DialogItem {
    /// Item type symbol name (`editText`, `pushButton`, `label`…).
    pub kind: String,
    pub title: String,
    pub value: String,
    pub enabled: bool,
}

/// One user action on a dialog (item positions are 1-based, as in Lingo).
#[derive(Clone, Debug, PartialEq)]
pub enum DialogAction {
    SetValue(usize, String),
    Click(usize),
    Close,
}

pub trait Responder {
    /// Actions for a dialog titled `title`; the run ends at `Close`, at the
    /// end of the list, or when the program calls `stop`.
    fn dialog(&mut self, title: &str, items: &[DialogItem]) -> Vec<DialogAction>;
    /// Button number (1-based) for a Mui alert.
    fn alert(&mut self, title: &str, message: &str, buttons: &str) -> usize;
}

/// A user who closes every dialog and takes the default alert button.
pub struct CloseAll;
impl Responder for CloseAll {
    fn dialog(&mut self, _: &str, _: &[DialogItem]) -> Vec<DialogAction> {
        vec![DialogAction::Close]
    }
    fn alert(&mut self, _: &str, _: &str, _: &str) -> usize {
        1
    }
}

/// The owner typing their own registration data, like `auto-activate.sh`
/// on Wine: first text field = name, second = e-mail, last = CEP; then the
/// first push button. Values come from the owner (`ILUMINA_REG_*`), never
/// generated.
pub struct Owner {
    pub name: String,
    pub email: String,
    pub cep: String,
}

impl Responder for Owner {
    fn dialog(&mut self, title: &str, items: &[DialogItem]) -> Vec<DialogAction> {
        // The owner answers the registration dialog only; any other dialog
        // (activation, about) is closed, like `CloseAll`.
        // Regression: the activation dialog would have got the name typed
        // into its code field.
        if !title.to_lowercase().contains("registr") {
            return vec![DialogAction::Close];
        }
        let texts: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.kind.eq_ignore_ascii_case("editText") && i.enabled)
            .map(|(k, _)| k + 1)
            .collect();
        let mut out = Vec::new();
        if let Some(&p) = texts.first() {
            out.push(DialogAction::SetValue(p, self.name.clone()));
        }
        if let Some(&p) = texts.get(1) {
            out.push(DialogAction::SetValue(p, self.email.clone()));
        }
        if texts.len() > 2 && !self.cep.is_empty() {
            out.push(DialogAction::SetValue(
                *texts.last().unwrap(),
                self.cep.clone(),
            ));
        }
        match items
            .iter()
            .position(|i| i.kind.to_ascii_lowercase().ends_with("pushbutton") && i.enabled)
        {
            Some(b) => out.push(DialogAction::Click(b + 1)),
            None => out.push(DialogAction::Close),
        }
        out
    }
    fn alert(&mut self, _: &str, _: &str, _: &str) -> usize {
        1
    }
}

/// State of one Mui instance.
#[derive(Default)]
pub struct Dialog {
    pub window: Value,
    pub items: Vec<Value>,
    pub stopped: bool,
}

fn sym(s: &str) -> Value {
    Value::sym(s)
}

/// `GetWindowPropList()` defaults (Director's Mui documentation).
pub fn window_defaults() -> Value {
    Value::prop_list(vec![
        (sym("type"), sym("normal")),
        (sym("name"), Value::str("window")),
        (sym("callback"), Value::str("nothing")),
        (sym("mode"), sym("data")),
        (sym("xPosition"), Value::Int(100)),
        (sym("yPosition"), Value::Int(120)),
        (sym("width"), Value::Int(200)),
        (sym("height"), Value::Int(210)),
        (sym("modal"), Value::Int(1)),
        (sym("toolTips"), Value::Int(0)),
        (sym("closeBox"), Value::Int(1)),
        (sym("canZoom"), Value::Int(0)),
    ])
}

/// `GetItemPropList()` defaults.
pub fn item_defaults() -> Value {
    Value::prop_list(vec![
        (sym("value"), Value::Int(0)),
        (sym("type"), sym("checkBox")),
        (sym("attributes"), Value::list(vec![])),
        (sym("title"), Value::str("title")),
        (sym("tip"), Value::str("tip")),
        (sym("locH"), Value::Int(20)),
        (sym("locV"), Value::Int(24)),
        (sym("width"), Value::Int(200)),
        (sym("height"), Value::Int(210)),
        (sym("enabled"), Value::Int(1)),
    ])
}

/// A property of a property list (symbol keys, case-insensitive).
pub fn prop(pl: &Value, key: &str) -> Value {
    match pl {
        Value::PropList(l) => l
            .borrow()
            .iter()
            .find(|(k, _)| k.lingo_eq(&Value::sym(key)))
            .map(|(_, v)| v.clone())
            .unwrap_or_default(),
        _ => Value::Void,
    }
}

pub fn set_prop(pl: &Value, key: &str, v: Value) {
    if let Value::PropList(l) = pl {
        let mut l = l.borrow_mut();
        match l.iter_mut().find(|(k, _)| k.lingo_eq(&Value::sym(key))) {
            Some(slot) => slot.1 = v,
            None => l.push((Value::sym(key), v)),
        }
    }
}

/// A deep copy (Initialize keeps its own lists, like the Xtra).
pub fn copy(v: &Value) -> Value {
    match v {
        Value::List(l) => Value::list(l.borrow().iter().map(copy).collect()),
        Value::PropList(l) => Value::prop_list(
            l.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), copy(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

impl Dialog {
    pub fn view(&self) -> (String, Vec<DialogItem>) {
        let title = prop(&self.window, "name").to_text();
        let items = self
            .items
            .iter()
            .map(|i| DialogItem {
                kind: prop(i, "type").to_text(),
                title: prop(i, "title").to_text(),
                value: prop(i, "value").to_text(),
                enabled: prop(i, "enabled").truthy() || prop(i, "enabled").is_void(),
            })
            .collect();
        (title, items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, enabled: bool) -> DialogItem {
        DialogItem {
            kind: kind.into(),
            title: String::new(),
            value: String::new(),
            enabled,
        }
    }

    #[test]
    fn owner_fills_text_fields_in_order_then_clicks() {
        let items = vec![
            item("windowBegin", true),
            item("editText", true),
            item("editText", true),
            item("editText", true),
            item("label", false),
            // Regression: the registration dialog's button is a
            // `defaultPushButton`, which the first version did not click.
            item("defaultPushButton", true),
            item("windowEnd", true),
        ];
        let mut o = Owner {
            name: "N".into(),
            email: "E".into(),
            cep: "C".into(),
        };
        assert_eq!(
            o.dialog("Registre Sua Cópia", &items),
            vec![
                DialogAction::SetValue(2, "N".into()),
                DialogAction::SetValue(3, "E".into()),
                DialogAction::SetValue(4, "C".into()),
                DialogAction::Click(6),
            ]
        );
        assert_eq!(CloseAll.dialog("t", &items), vec![DialogAction::Close]);
        assert_eq!(
            o.dialog("Ativação do iLúmina", &items),
            vec![DialogAction::Close]
        );
    }

    #[test]
    fn defaults_and_props() {
        let w = window_defaults();
        assert_eq!(prop(&w, "CALLBACK").to_text(), "nothing");
        set_prop(&w, "callback", Value::str("MyCallback"));
        assert_eq!(prop(&w, "callback").to_text(), "MyCallback");
        let c = copy(&w);
        set_prop(&c, "name", Value::str("x"));
        assert_eq!(prop(&w, "name").to_text(), "window");
    }
}
