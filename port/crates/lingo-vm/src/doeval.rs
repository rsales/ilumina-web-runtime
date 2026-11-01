//! `do "lingo"`: run Lingo text at run time. Director compiles it; the
//! movies only `do` short statements built from strings, so this evaluates
//! the forms they use:
//!
//! * calls: `Handler("arg", 2)`, `Handler "arg"`, `obj.method(args)`,
//!   `goCurrentModule.mSomething()`;
//! * `global a, b` and assignments (`x = expr`; only declared globals keep
//!   the value, as in Director, where `do` has its own locals);
//! * expressions: literals, `&`/`&&`/`+`/`-`/`*`/`/`, parentheses, lists,
//!   global variables, `obj.prop`, character constants.
//!
//! Lines are separated by returns. Anything else is an error naming the
//! text, so a new form shows up in the logs instead of being skipped.

use crate::value::{arith, Value};
use crate::vm::{Host, LingoError, Vm};

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Num(String),
    Str(String),
    Sym(String),
    P(char),
    AmpAmp,
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '"' {
            let start = i + 1;
            i += 1;
            while i < c.len() && c[i] != '"' {
                i += 1;
            }
            out.push(Tok::Str(c[start..i.min(c.len())].iter().collect()));
            i += 1;
        } else if ch == '#' {
            let start = i + 1;
            i += 1;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Sym(c[start..i].iter().collect()));
        } else if ch.is_ascii_digit() {
            let start = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            out.push(Tok::Num(c[start..i].iter().collect()));
        } else if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Ident(c[start..i].iter().collect()));
        } else if ch == '&' && c.get(i + 1) == Some(&'&') {
            out.push(Tok::AmpAmp);
            i += 2;
        } else if "()[],.=&+-*/:".contains(ch) {
            out.push(Tok::P(ch));
            i += 1;
        } else if ch == '-' && c.get(i + 1) == Some(&'-') {
            break; // comment
        } else {
            return Err(format!("do: unexpected {ch:?}"));
        }
    }
    Ok(out)
}

struct Ev<'a, 'b> {
    vm: &'a mut Vm,
    host: &'b mut dyn Host,
    toks: Vec<Tok>,
    i: usize,
    globals: &'a mut Vec<String>,
}

impl Ev<'_, '_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::P(c)) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn lerr(&self, e: LingoError) -> String {
        e.message
    }

    fn args_until(&mut self, close: char) -> Result<Vec<Value>, String> {
        let mut out = Vec::new();
        if self.eat(close) {
            return Ok(out);
        }
        loop {
            out.push(self.expr()?);
            if self.eat(close) {
                return Ok(out);
            }
            if !self.eat(',') {
                return Err(format!("do: expected , or {close}"));
            }
        }
    }

    fn expr(&mut self) -> Result<Value, String> {
        let mut v = self.primary()?;
        loop {
            match self.peek() {
                Some(Tok::P('&')) => {
                    self.i += 1;
                    let r = self.primary()?;
                    v = Value::Str(format!("{}{}", v.to_text(), r.to_text()));
                }
                Some(Tok::AmpAmp) => {
                    self.i += 1;
                    let r = self.primary()?;
                    v = Value::Str(format!("{} {}", v.to_text(), r.to_text()));
                }
                Some(Tok::P(op @ ('+' | '-' | '*' | '/'))) => {
                    let op = *op;
                    self.i += 1;
                    let r = self.primary()?;
                    v = arith(op, &v, &r)?;
                }
                _ => return Ok(v),
            }
        }
    }

    fn call(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        let r = self.vm.call(self.host, name, args);
        r.map_err(|e| self.lerr(e))
    }

    fn primary(&mut self) -> Result<Value, String> {
        match self.next() {
            Some(Tok::Str(s)) => Ok(Value::Str(s)),
            Some(Tok::Sym(s)) => Ok(Value::Symbol(s)),
            Some(Tok::Num(n)) => Ok(crate::literal::value_of(&n)),
            Some(Tok::P('-')) => {
                let v = self.primary()?;
                arith('-', &Value::Int(0), &v)
            }
            Some(Tok::P('(')) => {
                let v = self.expr()?;
                self.eat(')');
                Ok(v)
            }
            Some(Tok::P('[')) => {
                if self.eat(':') {
                    self.eat(']');
                    return Ok(Value::prop_list(vec![]));
                }
                let mut items = Vec::new();
                let mut props = Vec::new();
                if !self.eat(']') {
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
                            return Err("do: bad list".into());
                        }
                    }
                }
                Ok(if props.is_empty() {
                    Value::list(items)
                } else {
                    Value::prop_list(props)
                })
            }
            Some(Tok::Ident(name)) => self.ident(name),
            t => Err(format!("do: unexpected {t:?}")),
        }
    }

    fn ident(&mut self, name: String) -> Result<Value, String> {
        let lname = name.to_ascii_lowercase();
        let constant = match lname.as_str() {
            "quote" => Some(Value::str("\"")),
            "empty" => Some(Value::str("")),
            "return" => Some(Value::str("\r")),
            "tab" => Some(Value::str("\t")),
            "space" => Some(Value::str(" ")),
            "void" => Some(Value::Void),
            "true" => Some(Value::Int(1)),
            "false" => Some(Value::Int(0)),
            _ => None,
        };
        if let Some(c) = constant {
            return Ok(c);
        }
        if self.eat('(') {
            let args = self.args_until(')')?;
            return self.call(&name, args);
        }
        let mut v = self.vm.global(&name);
        while self.eat('.') {
            let Some(Tok::Ident(m)) = self.next() else {
                return Err("do: expected a name after '.'".into());
            };
            if self.eat('(') {
                let mut args = vec![v.clone()];
                args.extend(self.args_until(')')?);
                let r = self.vm.call_method(self.host, &m, args);
                v = r.map_err(|e| self.lerr(e))?;
            } else {
                v = self.vm.get_property(self.host, &v, &m)?;
            }
        }
        Ok(v)
    }

    /// One statement (a line).
    fn statement(&mut self) -> Result<(), String> {
        let Some(Tok::Ident(first)) = self.peek().cloned() else {
            self.expr()?;
            return Ok(());
        };
        if first.eq_ignore_ascii_case("global") {
            self.i += 1;
            while let Some(Tok::Ident(n)) = self.next() {
                self.globals.push(n.to_ascii_lowercase());
                if !self.eat(',') {
                    break;
                }
            }
            return Ok(());
        }
        // name = expr
        if self.toks.get(self.i + 1) == Some(&Tok::P('=')) {
            self.i += 2;
            let v = self.expr()?;
            if self.globals.contains(&first.to_ascii_lowercase()) {
                self.vm.set_global(&first, v);
            }
            return Ok(());
        }
        // Command form: `Handler arg1, arg2` (no parentheses).
        let next = self.toks.get(self.i + 1).cloned();
        let command = matches!(next, Some(Tok::Str(_) | Tok::Num(_) | Tok::Sym(_)))
            || matches!(next, Some(Tok::Ident(_)));
        if command {
            self.i += 1;
            let mut args = Vec::new();
            loop {
                args.push(self.expr()?);
                if !self.eat(',') {
                    break;
                }
            }
            self.call(&first, args)?;
            return Ok(());
        }
        let r = self.expr();
        if r.is_ok() && self.i == 1 && self.toks.len() == 1 {
            // A bare handler name runs it with no arguments.
            if self.vm.program.movie_handler(&first).is_some() {
                self.call(&first, Vec::new())?;
            }
        }
        r.map(|_| ())
    }
}

impl Vm {
    /// `do text`: run each line of Lingo text (see the module notes).
    pub fn do_text(&mut self, host: &mut dyn Host, text: &str) -> Result<(), String> {
        let mut globals = Vec::new();
        for line in text.split(['\r', '\n']) {
            let toks = lex(line)?;
            if toks.is_empty() {
                continue;
            }
            let mut ev = Ev {
                vm: self,
                host,
                toks,
                i: 0,
                globals: &mut globals,
            };
            ev.statement()
                .map_err(|e| format!("{e} (in do {line:?})"))?;
            if ev.i < ev.toks.len() {
                return Err(format!("do: cannot run {line:?}"));
            }
        }
        Ok(())
    }
}
