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
    /// Position and size in the window (`locH`, `locV`, `width`, `height`).
    pub rect: (i64, i64, i64, i64),
}

/// One user action on a dialog (item positions are 1-based, as in Lingo).
#[derive(Clone, Debug, PartialEq)]
pub enum DialogAction {
    SetValue(usize, String),
    Click(usize),
    Close,
}

pub trait Responder {
    /// The user's next actions on a dialog titled `title`, as it looks now.
    /// `round` counts the times this `Run` already asked (0 = it just
    /// opened); the run ends at `Close` or when the program calls `stop`,
    /// otherwise the responder is asked again. `None` = the user has not
    /// answered yet (the page shows the dialog; the run is replayed).
    fn dialog(
        &mut self,
        title: &str,
        items: &[DialogItem],
        round: usize,
    ) -> Option<Vec<DialogAction>>;
    /// Button number (1-based) for a Mui alert.
    fn alert(&mut self, title: &str, message: &str, buttons: &str) -> usize;
}

/// A user who closes every dialog and takes the default alert button.
pub struct CloseAll;
impl Responder for CloseAll {
    fn dialog(&mut self, _: &str, _: &[DialogItem], _: usize) -> Option<Vec<DialogAction>> {
        Some(vec![DialogAction::Close])
    }
    fn alert(&mut self, _: &str, _: &str, _: &str) -> usize {
        1
    }
}

/// The owner typing their own data, like `auto-activate.sh` on Wine:
/// registration (first text field = name, second = e-mail, last = CEP, then
/// the first push button) and, with an activation code, the activation
/// dialogs ("Possuo o meu Código de Ativação", then the code field). Values
/// come from the owner (`ILUMINA_REG_*`, `ILUMINA_ACTIVATION_CODE`), never
/// generated; the program checks the code as it always does (ADR 0005).
#[derive(Clone)]
pub struct Owner {
    pub name: String,
    pub email: String,
    pub cep: String,
    /// The owner's own activation code (empty = they activate by hand).
    pub code: String,
}

/// Lower-case text of an item as shown (labels show their value).
fn shown(i: &DialogItem) -> String {
    let t = if i.kind.eq_ignore_ascii_case("label") {
        &i.value
    } else {
        &i.title
    };
    t.to_lowercase()
}

/// The button titled "Continuar", else the default / first push button.
/// The dialog's "Continuar" (or default) button; `disabled_too` also finds
/// one the program enables only after a field is typed in.
fn continue_button(items: &[DialogItem], disabled_too: bool) -> Option<usize> {
    let buttons = || {
        items.iter().enumerate().filter(|(_, i)| {
            i.kind.to_ascii_lowercase().ends_with("pushbutton") && (i.enabled || disabled_too)
        })
    };
    buttons()
        .find(|(_, i)| i.title.eq_ignore_ascii_case("continuar"))
        .or_else(|| buttons().find(|(_, i)| i.kind.eq_ignore_ascii_case("defaultPushButton")))
        .or_else(|| buttons().next())
        .map(|(k, _)| k + 1)
}

impl Owner {
    /// The registration dialog filled in; `None` for any other dialog.
    pub fn registration(&self, title: &str, items: &[DialogItem]) -> Option<Vec<DialogAction>> {
        if !title.to_lowercase().contains("registr") {
            return None;
        }
        Some(self.fill(items))
    }

    /// The owner's answer to a dialog they recognise: registration, or (with
    /// a code) choosing "I have my activation code" and typing it.
    pub fn answer(&self, title: &str, items: &[DialogItem]) -> Option<Vec<DialogAction>> {
        if let Some(a) = self.registration(title, items) {
            return Some(a);
        }
        if self.code.is_empty() {
            return None;
        }
        // "Possuo o meu Código de Ativação. Ativar agora."
        let radios: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.kind.eq_ignore_ascii_case("radioButton"))
            .map(|(k, _)| k)
            .collect();
        if let Some(&pick) = radios.iter().find(|&&k| {
            let t = shown(&items[k]);
            t.contains("digo de ativa") && t.contains("ativar agora")
        }) {
            let mut out: Vec<DialogAction> = radios
                .iter()
                .map(|&k| DialogAction::SetValue(k + 1, if k == pick { "1" } else { "0" }.into()))
                .collect();
            out.push(DialogAction::Click(continue_button(items, false)?));
            return Some(out);
        }
        // The code field: a label asking for the activation code.
        let asks = items
            .iter()
            .any(|i| i.kind.eq_ignore_ascii_case("label") && shown(i).contains("digo de ativa"));
        let field = items
            .iter()
            .position(|i| i.kind.eq_ignore_ascii_case("editText") && i.enabled);
        match (asks, field) {
            // "Continuar" stays disabled until the code is typed: the
            // program enables it on `itemChanged` (regression: the code step
            // was skipped because only enabled buttons were looked for).
            (true, Some(f)) => Some(vec![
                DialogAction::SetValue(f + 1, self.code.clone()),
                DialogAction::Click(continue_button(items, true)?),
            ]),
            _ => None,
        }
    }
}

impl Responder for Owner {
    fn dialog(
        &mut self,
        title: &str,
        items: &[DialogItem],
        round: usize,
    ) -> Option<Vec<DialogAction>> {
        // The owner answers the registration dialog only (once per run: a
        // second ask means the answer was not accepted, and they close it);
        // any other dialog (activation, about) is closed, like `CloseAll`.
        // Regression: the activation dialog would have got the name typed
        // into its code field.
        // The activation takes two rounds (choose, then type the code).
        match self.answer(title, items) {
            Some(a) if round <= 1 => Some(a),
            _ => Some(vec![DialogAction::Close]),
        }
    }
    fn alert(&mut self, _: &str, _: &str, _: &str) -> usize {
        1
    }
}

impl Owner {
    fn fill(&self, items: &[DialogItem]) -> Vec<DialogAction> {
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
}

/// The page's user: the owner's registration data fills the registration
/// dialog (once per run); every other dialog waits for the answers the page
/// collected (`answers`, consumed in order on each replay).
pub struct PageUser {
    pub owner: Option<Owner>,
    pub answers: std::collections::VecDeque<Vec<DialogAction>>,
    /// The owner's last answer: the same answer asked for again means the
    /// program did not accept it, and the page's user takes over.
    pub last: Option<Vec<DialogAction>>,
}

impl Responder for PageUser {
    fn dialog(
        &mut self,
        title: &str,
        items: &[DialogItem],
        round: usize,
    ) -> Option<Vec<DialogAction>> {
        // The owner answers registration and (with their own code)
        // activation, so the page never shows those dialogs. Regression: only
        // registration was answered and the activation dialog stayed up.
        let _ = round;
        if let Some(a) = self.owner.as_ref().and_then(|o| o.answer(title, items)) {
            if self.last.as_ref() != Some(&a) {
                self.last = Some(a.clone());
                return Some(a);
            }
        }
        self.answers.pop_front()
    }
    fn alert(&mut self, _: &str, _: &str, _: &str) -> usize {
        1
    }
}

/// The error that interrupts a run when a dialog waits for the page's user.
pub const DIALOG_PENDING: &str = "dialog waiting for the user";

/// What the page shows of a dialog waiting for an answer.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingDialog {
    pub title: String,
    /// Window width and height.
    pub size: (i64, i64),
    pub items: Vec<DialogItem>,
}

impl PendingDialog {
    /// Lines for the page: `title⇥width⇥height`, then per item
    /// `kind⇥x⇥y⇥w⇥h⇥enabled⇥value⇥title` (tabs and returns in texts become
    /// spaces / `\r`).
    pub fn to_lines(&self) -> String {
        let clean = |s: &str| s.replace('\t', " ").replace('\n', "\r");
        let mut out = format!("{}\t{}\t{}", clean(&self.title), self.size.0, self.size.1);
        for i in &self.items {
            out.push_str(&format!(
                "\n{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                i.kind,
                i.rect.0,
                i.rect.1,
                i.rect.2,
                i.rect.3,
                i.enabled as u8,
                clean(&i.value).replace('\r', "\\r"),
                clean(&i.title).replace('\r', "\\r"),
            ));
        }
        out
    }
}

/// Parse the page's answer lines: `set⇥pos⇥text`, `click⇥pos`, `close`.
pub fn parse_actions(s: &str) -> Vec<DialogAction> {
    s.lines()
        .filter_map(|l| {
            let mut f = l.splitn(3, '\t');
            match f.next()? {
                "set" => Some(DialogAction::SetValue(
                    f.next()?.parse().ok()?,
                    f.next().unwrap_or("").to_string(),
                )),
                "click" => Some(DialogAction::Click(f.next()?.parse().ok()?)),
                "close" => Some(DialogAction::Close),
                _ => None,
            }
        })
        .collect()
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
                rect: (
                    prop(i, "locH").to_int(),
                    prop(i, "locV").to_int(),
                    prop(i, "width").to_int(),
                    prop(i, "height").to_int(),
                ),
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
            rect: (0, 0, 0, 0),
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
            code: String::new(),
        };
        assert_eq!(
            o.dialog("Registre Sua Cópia", &items, 0).unwrap(),
            vec![
                DialogAction::SetValue(2, "N".into()),
                DialogAction::SetValue(3, "E".into()),
                DialogAction::SetValue(4, "C".into()),
                DialogAction::Click(6),
            ]
        );
        assert_eq!(
            CloseAll.dialog("t", &items, 0).unwrap(),
            vec![DialogAction::Close]
        );
        assert_eq!(
            o.dialog("Ativação do iLúmina", &items, 0).unwrap(),
            vec![DialogAction::Close]
        );
        // Asked again in the same run: the answer was not accepted; close.
        assert_eq!(
            o.dialog("Registre Sua Cópia", &items, 2).unwrap(),
            vec![DialogAction::Close]
        );
        // The page's user: registration from the owner, others from answers.
        let mut u = PageUser {
            owner: Some(o),
            answers: [vec![DialogAction::Click(6)]].into(),
            last: None,
        };
        assert_eq!(u.dialog("Registre Sua Cópia", &items, 0).unwrap().len(), 4);
        assert_eq!(
            u.dialog("Ativação", &items, 0),
            Some(vec![DialogAction::Click(6)])
        );
        assert_eq!(u.dialog("Ativação", &items, 1), None);
    }

    #[test]
    fn owner_activates_with_their_own_code() {
        let it = |kind: &str, title: &str, value: &str| DialogItem {
            kind: kind.into(),
            title: title.into(),
            value: value.into(),
            enabled: true,
            rect: (0, 0, 0, 0),
        };
        let o = Owner {
            name: "N".into(),
            email: "E".into(),
            cep: "C".into(),
            code: "CODE".into(),
        };
        let choose = vec![
            it(
                "radioButton",
                "Possuo o Número de Série mas não o de Ativação.",
                "1",
            ),
            it(
                "radioButton",
                "Possuo o meu Código de Ativação. Ativar agora.",
                "0",
            ),
            it("radioButton", "Não quero ativar agora.", "0"),
            it("pushButton", "Continuar", "0"),
        ];
        assert_eq!(
            o.answer("Ativação do iLúmina", &choose).unwrap(),
            vec![
                DialogAction::SetValue(1, "0".into()),
                DialogAction::SetValue(2, "1".into()),
                DialogAction::SetValue(3, "0".into()),
                DialogAction::Click(4),
            ]
        );
        // "Continuar" is disabled until the code is typed (regression).
        let mut entry = vec![
            it("label", "title", "Código de Ativação:"),
            it("editText", "title", ""),
            it("pushButton", "Voltar", "0"),
            it("pushButton", "Continuar", "0"),
        ];
        entry[3].enabled = false;
        assert_eq!(
            o.answer("Ativação do iLúmina", &entry).unwrap(),
            vec![
                DialogAction::SetValue(2, "CODE".into()),
                DialogAction::Click(4)
            ]
        );
        // Without a code the owner leaves activation to the page's user.
        let no_code = Owner {
            code: String::new(),
            ..o.clone()
        };
        assert_eq!(no_code.answer("Ativação do iLúmina", &entry), None);
        // The page's user never sees the two activation steps; a step asked
        // again (code not accepted) goes to the page.
        let mut u = PageUser {
            owner: Some(o),
            answers: Default::default(),
            last: None,
        };
        assert_eq!(
            u.dialog("Ativação do iLúmina", &choose, 0).unwrap().len(),
            4
        );
        assert_eq!(u.dialog("Ativação do iLúmina", &entry, 1).unwrap().len(), 2);
        assert_eq!(u.dialog("Ativação do iLúmina", &entry, 2), None);
    }

    #[test]
    fn page_answers_parse() {
        assert_eq!(
            parse_actions("set\t2\tabc\tdef\nclick\t5\nbogus\nclose"),
            vec![
                DialogAction::SetValue(2, "abc\tdef".into()),
                DialogAction::Click(5),
                DialogAction::Close
            ]
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
