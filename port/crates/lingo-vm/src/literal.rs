//! `value(string)`: Director evaluates the text as Lingo. This covers the
//! expressions found in data: literals (numbers, strings, symbols, lists,
//! property lists), character constants (`QUOTE`, `RETURN`…), `&`/`&&`
//! concatenation, `sprite n`, `(member n of castLib m)`, `point(…)`,
//! `rect(…)` and `rgb(…)`. Score behavior settings are stored this way
//! (`"sprite 53: text " & QUOTE & "AddOns" & QUOTE`).
//!
//! Like Director, the value of the leading expression is returned when text
//! follows it (`value("sprite 53: text …")` is sprite 53; ⏳ confirm the
//! trailing-text rule on Wine). Unknown names give VOID.

use crate::value::{parse_number, HostRef, Value};

pub fn value_of(s: &str) -> Value {
    let mut p = Parser {
        s: s.chars().collect(),
        i: 0,
    };
    p.expr().unwrap_or_default()
}

struct Parser {
    s: Vec<char>,
    i: usize,
}

impl Parser {
    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.ws();
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn word(&mut self) -> Option<String> {
        self.ws();
        let start = self.i;
        while self
            .s
            .get(self.i)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            self.i += 1;
        }
        (self.i > start).then(|| self.s[start..self.i].iter().collect())
    }

    /// Next word without consuming it.
    fn peek_word(&mut self) -> Option<String> {
        let save = self.i;
        let w = self.word();
        self.i = save;
        w
    }

    fn expr(&mut self) -> Option<Value> {
        let mut v = self.term()?;
        loop {
            let save = self.i;
            if self.eat('&') {
                let spaced = self.s.get(self.i) == Some(&'&');
                if spaced {
                    self.i += 1;
                }
                let Some(r) = self.term() else {
                    self.i = save;
                    break;
                };
                let sep = if spaced { " " } else { "" };
                v = Value::Str(format!("{}{sep}{}", v.to_text(), r.to_text()));
            } else {
                break;
            }
        }
        Some(v)
    }

    fn args(&mut self) -> Option<Vec<Value>> {
        if !self.eat('(') {
            return None;
        }
        let mut out = Vec::new();
        if self.eat(')') {
            return Some(out);
        }
        loop {
            out.push(self.expr()?);
            if self.eat(')') {
                return Some(out);
            }
            if !self.eat(',') {
                return None;
            }
        }
    }

    fn number(&mut self) -> Option<Value> {
        self.ws();
        let start = self.i;
        if self.s.get(self.i) == Some(&'-') {
            self.i += 1;
        }
        while self
            .s
            .get(self.i)
            .is_some_and(|c| c.is_ascii_digit() || *c == '.')
        {
            self.i += 1;
        }
        let t: String = self.s[start..self.i].iter().collect();
        match parse_number(&t) {
            Some(n) => Some(n.value()),
            None => {
                self.i = start;
                None
            }
        }
    }

    fn term(&mut self) -> Option<Value> {
        let c = self.peek()?;
        if c.is_ascii_digit() || c == '-' || c == '.' {
            return self.number();
        }
        match c {
            '"' => {
                self.i += 1;
                let start = self.i;
                while self.s.get(self.i).is_some_and(|c| *c != '"') {
                    self.i += 1;
                }
                let t: String = self.s[start..self.i.min(self.s.len())].iter().collect();
                self.i += 1;
                Some(Value::Str(t))
            }
            '#' => {
                self.i += 1;
                Some(Value::Symbol(self.word()?))
            }
            '[' => self.list(),
            '(' => {
                self.i += 1;
                let v = self.expr()?;
                self.eat(')');
                Some(v)
            }
            _ => self.named(),
        }
    }

    fn named(&mut self) -> Option<Value> {
        let w = self.word()?;
        let int = |p: &mut Parser| p.term().map(|v| v.to_int());
        Some(match w.to_ascii_lowercase().as_str() {
            "quote" => Value::str("\""),
            "empty" => Value::str(""),
            "return" => Value::str("\r"),
            "enter" => Value::str("\u{3}"),
            "tab" => Value::str("\t"),
            "space" => Value::str(" "),
            "backspace" => Value::str("\u{8}"),
            "void" => Value::Void,
            "true" => Value::Int(1),
            "false" => Value::Int(0),
            "sprite" => match self.args() {
                Some(a) => Value::Host(HostRef::Sprite(a.first()?.to_int())),
                None => Value::Host(HostRef::Sprite(int(self)?)),
            },
            "castlib" => Value::Host(HostRef::CastLib(int(self)?)),
            "member" => {
                let n = int(self)?;
                let lib = if self
                    .peek_word()
                    .is_some_and(|w| w.eq_ignore_ascii_case("of"))
                {
                    self.word();
                    match self.word() {
                        Some(w) if w.eq_ignore_ascii_case("castlib") => int(self)?,
                        _ => return None,
                    }
                } else {
                    1
                };
                Value::Host(HostRef::Member { lib, number: n })
            }
            "point" => {
                let a = self.args()?;
                Value::Point(a.first()?.to_f64(), a.get(1)?.to_f64())
            }
            "rect" => {
                let a = self.args()?;
                Value::Rect(
                    a.first()?.to_f64(),
                    a.get(1)?.to_f64(),
                    a.get(2)?.to_f64(),
                    a.get(3)?.to_f64(),
                )
            }
            "rgb" => {
                let a = self.args()?;
                let c = |v: &Value| v.to_int().clamp(0, 255) as u8;
                Value::Color(c(a.first()?), c(a.get(1)?), c(a.get(2)?))
            }
            _ => return None,
        })
    }

    fn list(&mut self) -> Option<Value> {
        self.eat('[');
        if self.eat(']') {
            return Some(Value::list(vec![]));
        }
        if self.eat(':') {
            self.eat(']');
            return Some(Value::prop_list(vec![]));
        }
        let mut items = Vec::new();
        let mut props = Vec::new();
        loop {
            let v = self.expr()?;
            if self.eat(':') {
                props.push((v, self.expr()?));
            } else {
                items.push(v);
            }
            if self.eat(']') {
                break;
            }
            if !self.eat(',') {
                return None;
            }
        }
        Some(if !props.is_empty() && items.is_empty() {
            Value::prop_list(props)
        } else if props.is_empty() {
            Value::list(items)
        } else {
            return None;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals() {
        assert_eq!(
            value_of("[#a: 1, #b: [2, \"x\"]]").to_string(),
            "[#a: 1, #b: [2, \"x\"]]"
        );
        assert_eq!(value_of("3.5").to_string(), "3.5000");
        assert_eq!(value_of("-2").to_int(), -2);
        assert_eq!(value_of("#sym").to_string(), "#sym");
        assert_eq!(value_of("[:]").to_string(), "[:]");
        assert_eq!(value_of("[]").to_string(), "[]");
        assert!(value_of("foo bar").is_void());
        assert!(value_of("").is_void());
    }

    #[test]
    fn score_behavior_settings() {
        // Regression: "Single Scrollbar" settings hold expressions; the
        // literal-only parser returned VOID for the whole list.
        let v = value_of(
            "[#role: \"upArrow\", #el: \"sprite 53: text \" & QUOTE & \"AddOns\" & QUOTE & \"\", \
             #m: (member 30 of castLib 3), #d: 0.2500]",
        );
        assert_eq!(
            v.to_string(),
            "[#role: \"upArrow\", #el: \"sprite 53: text \\\"AddOns\\\"\", \
             #m: (member 30 of castLib 3), #d: 0.2500]"
                .replace("\\\"", "\"")
        );
        // value() of that setting: the leading expression, sprite 53.
        assert_eq!(
            value_of("sprite 53: text \"AddOns\"").to_string(),
            "(sprite 53)"
        );
        assert_eq!(value_of("\"a\" && \"b\"").to_text(), "a b");
        assert_eq!(value_of("rgb(1, 2, 3)").to_string(), "rgb( 1, 2, 3 )");
        assert_eq!(value_of("point(1, 2)").to_string(), "point(1, 2)");
    }
}
