//! The bytecode interpreter.
//!
//! Semantics follow public format research (ProjectorRays, ScummVM's
//! Director engine); no code is copied. Anything outside pure Lingo (player
//! functions, sprite/member properties, Xtras) goes to the [`Host`].

use crate::builtins;
use crate::chunk::{self, ChunkSpec};
use crate::program::Program;
use crate::value::{arith, HostRef, Instance, ObjectRef, Value, VarSlot};
use rifx::lingo::Literal;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A Lingo runtime error, with the handler stack where it happened.
#[derive(Debug, Clone)]
pub struct LingoError {
    pub message: String,
    pub trace: Vec<String>,
}

impl std::fmt::Display for LingoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        for t in &self.trace {
            write!(f, "\n  in {t}")?;
        }
        Ok(())
    }
}

pub type HostResult<T> = Result<T, String>;

/// Everything outside pure Lingo. Each method returns `Ok(None)` (or
/// `Ok(false)`) when it does not know the name, so the VM can report a
/// precise "not implemented" error.
pub trait Host {
    /// Global functions and commands (`go`, `sprite`, `member`, Xtra calls…).
    fn call(&mut self, vm: &mut Vm, name: &str, args: &[Value]) -> HostResult<Option<Value>> {
        let _ = (vm, name, args);
        Ok(None)
    }
    /// `target.name(args)` / `name(target, args)` on a host object.
    fn method(
        &mut self,
        vm: &mut Vm,
        target: &HostRef,
        name: &str,
        args: &[Value],
    ) -> HostResult<Option<Value>> {
        let _ = (vm, target, name, args);
        Ok(None)
    }
    fn get_prop(&mut self, vm: &mut Vm, target: &HostRef, name: &str) -> HostResult<Option<Value>> {
        let _ = (vm, target, name);
        Ok(None)
    }
    fn set_prop(
        &mut self,
        vm: &mut Vm,
        target: &HostRef,
        name: &str,
        value: &Value,
    ) -> HostResult<bool> {
        let _ = (vm, target, name, value);
        Ok(false)
    }
    /// `the name` (movie and system properties, `the frame`, `the milliseconds`…).
    fn the(&mut self, vm: &mut Vm, name: &str) -> HostResult<Option<Value>> {
        let _ = (vm, name);
        Ok(None)
    }
    fn set_the(&mut self, vm: &mut Vm, name: &str, value: &Value) -> HostResult<bool> {
        let _ = (vm, name, value);
        Ok(false)
    }
}

/// Opcodes the interpreter executes (base opcode, as `rifx` decodes it).
pub fn supports(op: u8) -> bool {
    matches!(
        op,
        0x01..=0x18 | 0x1b..=0x1f | 0x21 | 0x41..=0x46 | 0x48..=0x57 | 0x59..=0x5d | 0x5f..=0x67 | 0x6d..=0x72
    ) && op != 0x47
}

/// A host that knows nothing (pure-Lingo tests).
pub struct NoHost;
impl Host for NoHost {}

pub struct Vm {
    pub program: Program,
    /// Globals, keyed by lower-case name.
    pub globals: HashMap<String, Value>,
    pub item_delimiter: char,
    /// Lines written by `put` (the message window).
    pub output: Vec<String>,
    /// Instruction budget per top-level call (guards against loops that wait
    /// for events the host never sends).
    pub step_limit: u64,
    steps: u64,
    stack_trace: Vec<String>,
    /// Arguments each running handler received (`the paramCount`, `param(n)`).
    passed: Vec<Vec<Value>>,
    pending_trace: Option<Vec<String>>,
    rng: u64,
    /// When set, every handler entered is logged (indented by depth): a
    /// debugging aid for following what a click does.
    pub call_log: Option<Vec<String>>,
}

const MAX_DEPTH: usize = 256;

struct Frame {
    script: usize,
    handler: usize,
    receiver: Option<ObjectRef>,
    params: Vec<Value>,
    locals: Vec<Value>,
    stack: Vec<Value>,
    tell: Vec<Value>,
}

impl Frame {
    fn pop(&mut self) -> Result<Value, String> {
        self.stack
            .pop()
            .ok_or_else(|| "stack underflow".to_string())
    }
    fn pop_args(&mut self) -> Result<(Vec<Value>, bool), String> {
        match self.pop()? {
            Value::ArgList(v, noret) => Ok((v, noret)),
            other => Err(format!("expected an argument list, found {}", other.ilk())),
        }
    }
    fn pop_chunk(&mut self) -> Result<ChunkSpec, String> {
        let mut n = || self.pop().map(|v| v.to_int());
        let last_line = n()?;
        let first_line = n()?;
        let last_item = n()?;
        let first_item = n()?;
        let last_word = n()?;
        let first_word = n()?;
        let last_char = n()?;
        let first_char = n()?;
        Ok(ChunkSpec {
            char: (first_char, last_char),
            word: (first_word, last_word),
            item: (first_item, last_item),
            line: (first_line, last_line),
        })
    }
}

impl Vm {
    pub fn new(program: Program) -> Self {
        Vm {
            program,
            globals: HashMap::new(),
            item_delimiter: ',',
            output: Vec::new(),
            step_limit: 20_000_000,
            steps: 0,
            stack_trace: Vec::new(),
            passed: Vec::new(),
            pending_trace: None,
            rng: 0x2545_f491_4f6c_dd1d,
            call_log: None,
        }
    }

    pub fn global(&self, name: &str) -> Value {
        self.globals
            .get(&name.to_ascii_lowercase())
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_global(&mut self, name: &str, v: Value) {
        self.globals.insert(name.to_ascii_lowercase(), v);
    }

    /// Deterministic `random(n)` in 1..=n.
    pub fn random(&mut self, n: i64) -> i64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        if n <= 0 {
            0
        } else {
            (self.rng % n as u64) as i64 + 1
        }
    }

    fn error(&mut self, message: impl Into<String>) -> LingoError {
        // An error that crossed a built-in (`new`, `call`) as text keeps the
        // trace from where it really happened.
        if let Some(trace) = self.pending_trace.take() {
            return LingoError {
                message: message.into(),
                trace,
            };
        }
        LingoError {
            message: message.into(),
            trace: self.stack_trace.iter().rev().cloned().collect(),
        }
    }

    /// Turn an inner error into a message for a built-in, keeping its trace.
    pub(crate) fn stash(&mut self, e: LingoError) -> String {
        self.pending_trace = Some(e.trace);
        e.message
    }

    /// Send a movie event (`prepareMovie`, `startMovie`…) to every movie
    /// script that handles it. Returns how many handlers ran.
    pub fn movie_event(
        &mut self,
        host: &mut dyn Host,
        name: &str,
        args: Vec<Value>,
    ) -> Result<usize, LingoError> {
        let targets = self.program.movie_handlers(name);
        for &(s, h) in &targets {
            if self.stack_trace.is_empty() {
                self.steps = 0;
            }
            self.run(host, s, h, None, args.clone())?;
        }
        Ok(targets.len())
    }

    /// Call a global handler by name, as `extcall` does.
    pub fn call(
        &mut self,
        host: &mut dyn Host,
        name: &str,
        args: Vec<Value>,
    ) -> Result<Value, LingoError> {
        // Only a top-level call gets a fresh budget: a host callback that
        // re-enters the VM (a dialog event) shares the caller's.
        if self.stack_trace.is_empty() {
            self.steps = 0;
        }
        self.call_global(host, name, args)
    }

    /// Call `name` on an object/list/host value, as `objcall` does.
    pub fn call_method(
        &mut self,
        host: &mut dyn Host,
        name: &str,
        args: Vec<Value>,
    ) -> Result<Value, LingoError> {
        let target = args.first().cloned().unwrap_or_default();
        match &target {
            Value::Object(o) => {
                if let Some((s, h)) = self.find_method(o, name) {
                    return self.run(host, s, h, Some(o.clone()), args);
                }
                if let Some(v) =
                    builtins::object_method(self, o, name, &args[1..]).map_err(|e| self.error(e))?
                {
                    return Ok(v);
                }
                let script = o.borrow().script;
                let sname = self.program.scripts[script].name.clone();
                Err(self.error(format!(
                    "handler #{name} not found in object of script {sname:?}"
                )))
            }
            Value::Script(i) => {
                let i = *i;
                if name.eq_ignore_ascii_case("new") {
                    return self.instantiate(host, i, args[1..].to_vec());
                }
                match self.program.scripts[i].handler(name) {
                    Some(h) => self.run(host, i, h, None, args),
                    None => Err(self.error(format!("handler #{name} not found in script {i}"))),
                }
            }
            Value::Host(r) => {
                let r = r.clone();
                match host
                    .method(self, &r, name, &args[1..])
                    .map_err(|e| self.error(e))?
                {
                    Some(v) => Ok(v),
                    None => {
                        Err(self.error(format!("not implemented: {}.{name}()", Value::Host(r))))
                    }
                }
            }
            _ => match builtins::call(self, host, name, &args).map_err(|e| self.error(e))? {
                Some(v) => Ok(v),
                None => Err(self.error(format!("handler #{name} not found for {}", target.ilk()))),
            },
        }
    }

    /// `new(script)`: create the instance, then run its `new` handler.
    pub fn instantiate(
        &mut self,
        host: &mut dyn Host,
        script: usize,
        args: Vec<Value>,
    ) -> Result<Value, LingoError> {
        let props = self.program.scripts[script]
            .props
            .iter()
            .map(|p| (p.clone(), Value::Void))
            .collect();
        let obj = Rc::new(RefCell::new(Instance { script, props }));
        let me = Value::Object(obj.clone());
        match self.program.scripts[script].handler("new") {
            Some(h) => {
                let mut a = vec![me];
                a.extend(args);
                self.run(host, script, h, Some(obj), a)
            }
            None => Ok(me),
        }
    }

    /// A behavior instance: the script's properties, then the author's
    /// settings from the score and `spriteNum`; its `new` is not run (⏳
    /// Director's behaviors initialise in `beginSprite`).
    pub fn new_behavior(
        &mut self,
        script: usize,
        params: &[(String, Value)],
        sprite: i64,
    ) -> Value {
        let mut props: Vec<(String, Value)> = self.program.scripts[script]
            .props
            .iter()
            .map(|p| (p.clone(), Value::Void))
            .collect();
        let mut inst = Instance {
            script,
            props: Vec::new(),
        };
        for (k, v) in params
            .iter()
            .cloned()
            .chain([("spriteNum".to_string(), Value::Int(sprite))])
        {
            match props.iter_mut().find(|(p, _)| p.eq_ignore_ascii_case(&k)) {
                Some(slot) => slot.1 = v,
                None => props.push((k, v)),
            }
        }
        inst.props = props;
        Value::Object(Rc::new(RefCell::new(inst)))
    }

    /// True when the object (or its ancestors) has handler `name`.
    /// Call `o`'s handler with `args` as its parameters, `o` being `me` for
    /// property access but not passed as the first parameter: how Xtras
    /// call back into behaviors (`on TreeViewSingleClick intRefCon, …`).
    pub fn call_unbound(
        &mut self,
        host: &mut dyn Host,
        o: &ObjectRef,
        name: &str,
        args: Vec<Value>,
    ) -> Result<Option<Value>, LingoError> {
        match self.find_method(o, name) {
            Some((s, h)) => self.run(host, s, h, Some(o.clone()), args).map(Some),
            None => Ok(None),
        }
    }

    pub fn has_method(&self, o: &ObjectRef, name: &str) -> bool {
        self.find_method(o, name).is_some()
    }

    /// A handler in the object's script or its `ancestor` chain.
    fn find_method(&self, o: &ObjectRef, name: &str) -> Option<(usize, usize)> {
        let mut cur = o.clone();
        for _ in 0..64 {
            let script = cur.borrow().script;
            if let Some(h) = self.program.scripts[script].handler(name) {
                return Some((script, h));
            }
            let next = match cur.borrow().get("ancestor") {
                Some(Value::Object(a)) => a.clone(),
                _ => return None,
            };
            cur = next;
        }
        None
    }

    fn call_global(
        &mut self,
        host: &mut dyn Host,
        name: &str,
        args: Vec<Value>,
    ) -> Result<Value, LingoError> {
        if let Some((s, h)) = self.program.movie_handler(name) {
            return self.run(host, s, h, None, args);
        }
        // do "lingo text"
        if name.eq_ignore_ascii_case("do") {
            let text = args.first().map(|v| v.to_text()).unwrap_or_default();
            self.do_text(host, &text).map_err(|e| self.error(e))?;
            return Ok(Value::Void);
        }
        // param(n): the n-th argument the running handler received.
        if name.eq_ignore_ascii_case("param") {
            let n = args.first().map(|v| v.to_int()).unwrap_or(0);
            let v = self
                .passed
                .last()
                .and_then(|a| a.get((n - 1).max(0) as usize).filter(|_| n >= 1))
                .cloned()
                .unwrap_or_default();
            return Ok(v);
        }
        if let Some(v) = builtins::call(self, host, name, &args).map_err(|e| self.error(e))? {
            return Ok(v);
        }
        // `foo(obj, …)` reaches the object's handler when no global exists.
        if let Some(Value::Object(o)) = args.first() {
            if let Some((s, h)) = self.find_method(o, name) {
                let o = o.clone();
                return self.run(host, s, h, Some(o), args);
            }
        }
        if let Some(Value::Host(r)) = args.first() {
            let r = r.clone();
            if let Some(v) = host
                .method(self, &r, name, &args[1..])
                .map_err(|e| self.error(e))?
            {
                return Ok(v);
            }
        }
        match host.call(self, name, &args).map_err(|e| self.error(e))? {
            Some(v) => Ok(v),
            None => Err(self.error(format!("not implemented: handler #{name}"))),
        }
    }

    pub fn get_property(
        &mut self,
        host: &mut dyn Host,
        obj: &Value,
        name: &str,
    ) -> Result<Value, String> {
        let lname = name.to_ascii_lowercase();
        Ok(match obj {
            Value::List(l) => match lname.as_str() {
                "count" => Value::Int(l.borrow().len() as i64),
                "ilk" => Value::sym("list"),
                _ => return Err(format!("list has no property #{name}")),
            },
            Value::PropList(l) => {
                if lname == "count" {
                    return Ok(Value::Int(l.borrow().len() as i64));
                }
                let key = Value::sym(name);
                l.borrow()
                    .iter()
                    .find(|(k, _)| k.lingo_eq(&key))
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            }
            Value::Object(o) => match instance_prop(o, name) {
                Some(v) => v,
                None if lname == "ilk" => Value::sym("instance"),
                None => {
                    let s = o.borrow().script;
                    return Err(format!(
                        "property #{name} not found in object of script {:?}",
                        self.program.scripts[s].name
                    ));
                }
            },
            Value::Point(x, y) => match lname.as_str() {
                "loch" | "x" => num(*x),
                "locv" | "y" => num(*y),
                _ => return Err(format!("point has no property #{name}")),
            },
            Value::Rect(l, t, r, b) => match lname.as_str() {
                "left" => num(*l),
                "top" => num(*t),
                "right" => num(*r),
                "bottom" => num(*b),
                "width" => num(r - l),
                "height" => num(b - t),
                _ => return Err(format!("rect has no property #{name}")),
            },
            Value::Str(s) if lname == "length" => Value::Int(s.chars().count() as i64),
            Value::Color(r, g, b) => match lname.as_str() {
                "red" => Value::Int(*r as i64),
                "green" => Value::Int(*g as i64),
                "blue" => Value::Int(*b as i64),
                "colortype" => Value::sym("rgb"),
                "hexstring" => Value::Str(format!("#{r:02X}{g:02X}{b:02X}")),
                _ => return Err(format!("color has no property #{name}")),
            },
            Value::Date(d) => {
                let (y, m, dd) = crate::value::civil_from_days(*d);
                match lname.as_str() {
                    "year" => Value::Int(y),
                    "month" => Value::Int(m),
                    "day" => Value::Int(dd),
                    _ => return Err(format!("date has no property #{name}")),
                }
            }
            Value::Host(r) => {
                let r = r.clone();
                match host.get_prop(self, &r, name)? {
                    Some(v) => v,
                    None => return Err(format!("not implemented: {}.{name}", Value::Host(r))),
                }
            }
            other => return Err(format!("{} has no property #{name}", other.ilk())),
        })
    }

    pub fn set_property(
        &mut self,
        host: &mut dyn Host,
        obj: &Value,
        name: &str,
        v: Value,
    ) -> Result<(), String> {
        match obj {
            Value::PropList(l) => {
                let key = Value::sym(name);
                let mut l = l.borrow_mut();
                match l.iter_mut().find(|(k, _)| k.lingo_eq(&key)) {
                    Some(slot) => slot.1 = v,
                    None => l.push((key, v)),
                }
                Ok(())
            }
            Value::Object(o) => {
                set_instance_prop(o, name, v);
                Ok(())
            }
            Value::Host(r) => {
                let r = r.clone();
                if host.set_prop(self, &r, name, &v)? {
                    Ok(())
                } else {
                    Err(format!("not implemented: set {}.{name}", Value::Host(r)))
                }
            }
            other => Err(format!("cannot set #{name} of {}", other.ilk())),
        }
    }

    fn the(&mut self, host: &mut dyn Host, name: &str) -> Result<Value, String> {
        match name.to_ascii_lowercase().as_str() {
            "itemdelimiter" => return Ok(Value::str(self.item_delimiter.to_string())),
            "floatprecision" => return Ok(Value::Int(4)),
            // Arguments the running handler received (not its declared count).
            // Regression: Utilities.sprintf reads it; it was not implemented.
            "paramcount" => {
                return Ok(Value::Int(
                    self.passed.last().map(|a| a.len()).unwrap_or(0) as i64
                ))
            }
            _ => {}
        }
        match host.the(self, name)? {
            Some(v) => Ok(v),
            None => Err(format!("not implemented: the {name}")),
        }
    }

    fn set_the(&mut self, host: &mut dyn Host, name: &str, v: Value) -> Result<(), String> {
        if name.eq_ignore_ascii_case("itemDelimiter") {
            self.item_delimiter = v.to_text().chars().next().unwrap_or(',');
            return Ok(());
        }
        if host.set_the(self, name, &v)? {
            Ok(())
        } else {
            Err(format!("not implemented: set the {name}"))
        }
    }

    /// Run one handler to completion.
    pub fn run(
        &mut self,
        host: &mut dyn Host,
        script: usize,
        handler: usize,
        receiver: Option<ObjectRef>,
        args: Vec<Value>,
    ) -> Result<Value, LingoError> {
        if self.stack_trace.len() >= MAX_DEPTH {
            return Err(self.error("call stack too deep"));
        }
        let (n_args, n_locals, label) = {
            let s = &self.program.scripts[script];
            let h = &s.handlers[handler];
            (
                h.args.len(),
                h.locals.len(),
                format!(
                    "{} ({} #{} lib {})",
                    h.name,
                    script_label(s),
                    s.member,
                    s.lib
                ),
            )
        };
        self.passed.push(args.clone());
        let mut params = args;
        if params.len() < n_args {
            params.resize(n_args, Value::Void);
        }
        let mut f = Frame {
            script,
            handler,
            receiver,
            params,
            locals: vec![Value::Void; n_locals],
            stack: Vec::new(),
            tell: Vec::new(),
        };
        if let Some(log) = &mut self.call_log {
            log.push(format!("{}{label}", "  ".repeat(self.stack_trace.len())));
        }
        self.stack_trace.push(label);
        let r = self.exec(host, &mut f);
        let r = match r {
            Ok(v) => Ok(v),
            Err(Exit::Error(e)) => Err(e),
            Err(Exit::Msg(m)) => Err(self.error(m)),
        };
        self.stack_trace.pop();
        self.passed.pop();
        r
    }

    fn exec(&mut self, host: &mut dyn Host, f: &mut Frame) -> Result<Value, Exit> {
        let mut pc = 0usize;
        loop {
            let (op, arg, offset) = {
                let h = &self.program.scripts[f.script].handlers[f.handler];
                match h.instrs.get(pc) {
                    Some(i) => (i.op.0, i.arg, i.offset),
                    None => return Ok(f.stack.pop().unwrap_or_default()),
                }
            };
            self.steps += 1;
            if self.steps > self.step_limit {
                return Err(Exit::Msg(
                    "instruction budget exceeded (waiting for an event?)".into(),
                ));
            }
            pc += 1;
            let cur_script = f.script;
            let name = |vm: &Vm| vm.program.scripts[cur_script].name_of(arg).to_string();
            match op {
                0x01 | 0x02 => return Ok(f.stack.pop().unwrap_or_default()),
                0x03 => f.stack.push(Value::Int(0)),
                0x04..=0x08 => {
                    let b = f.pop()?;
                    let a = f.pop()?;
                    let c = ['*', '+', '-', '/', '%'][(op - 0x04) as usize];
                    // Integer division by zero: warn and divide by 1 (the
                    // original code reaches `x mod 0` on a fresh profile and
                    // no script error is known from Wine; unverified).
                    let b = if matches!(c, '/' | '%')
                        && matches!(b.number(), Some(crate::value::Num::Int(0)))
                    {
                        self.output
                            .push(format!("warning: {} {c} 0 treated as {c} 1", a));
                        Value::Int(1)
                    } else {
                        b
                    };
                    f.stack.push(arith(c, &a, &b)?);
                }
                0x09 => {
                    let a = f.pop()?;
                    f.stack.push(match a {
                        Value::Point(x, y) => Value::Point(-x, -y),
                        other => arith('-', &Value::Int(0), &other)?,
                    });
                }
                0x0a | 0x0b => {
                    let b = f.pop()?;
                    let a = f.pop()?;
                    let sep = if op == 0x0b { " " } else { "" };
                    f.stack
                        .push(Value::Str(format!("{}{sep}{}", a.to_text(), b.to_text())));
                }
                0x0c..=0x11 => {
                    let b = f.pop()?;
                    let a = f.pop()?;
                    use std::cmp::Ordering::*;
                    let r = match op {
                        0x0e => !a.lingo_eq(&b),
                        0x0f => a.lingo_eq(&b),
                        _ => {
                            let o = a.lingo_cmp(&b);
                            match op {
                                0x0c => o == Some(Less),
                                0x0d => matches!(o, Some(Less | Equal)),
                                0x10 => o == Some(Greater),
                                _ => matches!(o, Some(Greater | Equal)),
                            }
                        }
                    };
                    f.stack.push(Value::bool(r));
                }
                0x12 | 0x13 => {
                    let b = f.pop()?.truthy();
                    let a = f.pop()?.truthy();
                    f.stack
                        .push(Value::bool(if op == 0x12 { a && b } else { a || b }));
                }
                0x14 => {
                    let a = f.pop()?;
                    f.stack.push(Value::bool(!a.truthy()));
                }
                0x15 | 0x16 => {
                    let b = f.pop()?.to_text().to_lowercase();
                    let a = f.pop()?.to_text().to_lowercase();
                    f.stack.push(Value::bool(if op == 0x15 {
                        a.contains(&b)
                    } else {
                        a.starts_with(&b)
                    }));
                }
                0x17 => {
                    let s = f.pop()?.to_text();
                    let spec = f.pop_chunk()?;
                    f.stack
                        .push(Value::Str(chunk::get(&s, &spec, self.item_delimiter)));
                }
                0x1c => {
                    let t = f.pop()?;
                    f.tell.push(t);
                }
                0x1d => {
                    f.tell.pop();
                }
                0x1e => {
                    let (v, _) = f.pop_args()?;
                    f.stack.push(Value::list(v));
                }
                0x1f => {
                    let (v, _) = f.pop_args()?;
                    let pairs = v
                        .chunks(2)
                        .map(|p| (p[0].clone(), p.get(1).cloned().unwrap_or_default()))
                        .collect();
                    f.stack.push(Value::prop_list(pairs));
                }
                0x21 => {
                    let n = f.stack.len();
                    if n < 2 {
                        return Err(Exit::Msg("stack underflow".into()));
                    }
                    f.stack.swap(n - 1, n - 2);
                }
                0x41 | 0x6e | 0x6f => f.stack.push(Value::Int(arg as i64)),
                0x42 | 0x43 => {
                    let n = arg.max(0) as usize;
                    if f.stack.len() < n {
                        return Err(Exit::Msg("stack underflow".into()));
                    }
                    let v = f.stack.split_off(f.stack.len() - n);
                    f.stack.push(Value::ArgList(v, op == 0x42));
                }
                0x44 => {
                    let lit = self.program.scripts[f.script]
                        .literals
                        .get(arg.max(0) as usize)
                        .cloned();
                    f.stack.push(match lit {
                        Some(Literal::String(s)) => Value::Str(s),
                        Some(Literal::Int(i)) => Value::Int(i as i64),
                        Some(Literal::Float(x)) => Value::Float(x),
                        Some(Literal::Other { kind, .. }) => {
                            return Err(Exit::Msg(format!("unsupported literal kind {kind}")));
                        }
                        None => return Err(Exit::Msg(format!("literal {arg} out of range"))),
                    });
                }
                0x45 => f.stack.push(Value::Symbol(name(self))),
                0x46 => f.stack.push(Value::Str(name(self))),
                0x48 | 0x49 => {
                    let v = self.global(&name(self));
                    f.stack.push(v);
                }
                0x4a => {
                    let n = name(self);
                    let v = self.receiver_prop(f, &n)?;
                    f.stack.push(v);
                }
                0x4b => f
                    .stack
                    .push(f.params.get(arg as usize).cloned().unwrap_or_default()),
                0x4c => f
                    .stack
                    .push(f.locals.get(arg as usize).cloned().unwrap_or_default()),
                0x4e | 0x4f => {
                    let v = f.pop()?;
                    let n = name(self);
                    self.set_global(&n, v);
                }
                0x50 => {
                    let v = f.pop()?;
                    let n = name(self);
                    self.set_receiver_prop(f, &n, v)?;
                }
                0x51 => {
                    let v = f.pop()?;
                    let i = arg as usize;
                    if i >= f.params.len() {
                        f.params.resize(i + 1, Value::Void);
                    }
                    f.params[i] = v;
                }
                0x52 => {
                    let v = f.pop()?;
                    let i = arg as usize;
                    if i >= f.locals.len() {
                        f.locals.resize(i + 1, Value::Void);
                    }
                    f.locals[i] = v;
                }
                0x53..=0x55 => {
                    let target = if op == 0x54 {
                        offset as i64 - arg as i64
                    } else {
                        offset as i64 + arg as i64
                    };
                    let jump = op != 0x55 || !f.pop()?.truthy();
                    if jump {
                        let h = &self.program.scripts[f.script].handlers[f.handler];
                        let end = h.instrs.last().map(|i| i.offset + 1).unwrap_or(0);
                        pc = match h.index_of(target as usize) {
                            Some(i) => i,
                            // A jump to the end of the handler returns.
                            None if target as usize >= end => h.instrs.len(),
                            None => return Err(Exit::Msg(format!("bad jump target {target}"))),
                        };
                    }
                }
                0x56 => {
                    let (args, noret) = f.pop_args()?;
                    // A local call whose first argument is a host object that
                    // answers the method goes to the object: `Db.Base` calls
                    // `GetRecordAsPropList(hRs)` on a V4MD cursor from inside
                    // its own handler of the same name.
                    let via_host = match args.first() {
                        Some(Value::Host(r)) => {
                            let r = r.clone();
                            let hname = self.program.scripts[f.script].handlers[arg as usize]
                                .name
                                .clone();
                            host.method(self, &r, &hname, &args[1..])?
                        }
                        _ => None,
                    };
                    let r = match via_host {
                        Some(v) => v,
                        None => self
                            .run(host, f.script, arg as usize, f.receiver.clone(), args)
                            .map_err(Exit::Error)?,
                    };
                    if !noret {
                        f.stack.push(r);
                    }
                }
                0x57 => {
                    let (args, noret) = f.pop_args()?;
                    let n = name(self);
                    // `return x` compiles to `extcall return` and leaves the
                    // handler at once (code after it is the else branch).
                    if n.eq_ignore_ascii_case("return") {
                        return Ok(args.into_iter().next().unwrap_or_default());
                    }
                    let r = self.call_global(host, &n, args).map_err(Exit::Error)?;
                    if !noret {
                        f.stack.push(r);
                    }
                }
                0x67 => {
                    let (args, noret) = f.pop_args()?;
                    let n = name(self);
                    let r = if matches!(args.first(), Some(Value::VarRef(_) | Value::ChunkRef(..)))
                    {
                        self.ref_method(host, f, &n, &args)?
                    } else {
                        self.call_method(host, &n, args).map_err(Exit::Error)?
                    };
                    if !noret {
                        f.stack.push(r);
                    }
                }
                0x63 => {
                    let (args, noret) = f.pop_args()?;
                    let n = name(self);
                    let r = match f.tell.last().cloned() {
                        Some(Value::Host(t)) => match host.method(self, &t, &n, &args)? {
                            Some(v) => v,
                            None => {
                                return Err(Exit::Msg(format!(
                                    "not implemented: tell {} to {n}",
                                    Value::Host(t)
                                )))
                            }
                        },
                        _ => self.call_global(host, &n, args).map_err(Exit::Error)?,
                    };
                    if !noret {
                        f.stack.push(r);
                    }
                }
                0x59 => {
                    let put_type = ((arg >> 4) & 0xf) as u8;
                    let var = self.read_var(f, arg & 0xf)?;
                    let v = f.pop()?;
                    let cur = self.var_get(host, f, &var)?;
                    let new = match put_type {
                        2 => Value::Str(format!("{}{}", cur.to_text(), v.to_text())),
                        3 => Value::Str(format!("{}{}", v.to_text(), cur.to_text())),
                        _ => v,
                    };
                    self.var_set(host, f, &var, new)?;
                }
                0x5a => {
                    let put_type = ((arg >> 4) & 0xf) as u8;
                    let var = self.read_var(f, arg & 0xf)?;
                    let spec = f.pop_chunk()?;
                    let v = f.pop()?;
                    let cur = self.var_get(host, f, &var)?.to_text();
                    let new = chunk::put(&cur, &spec, &v.to_text(), put_type, self.item_delimiter);
                    self.var_set(host, f, &var, Value::Str(new))?;
                }
                0x5b => {
                    let var = self.read_var(f, arg & 0xf)?;
                    let spec = f.pop_chunk()?;
                    let cur = self.var_get(host, f, &var)?.to_text();
                    let new = chunk::delete(&cur, &spec, self.item_delimiter);
                    self.var_set(host, f, &var, Value::Str(new))?;
                }
                0x5c => {
                    let id = f.pop()?.to_int();
                    let v = self.v4_get(host, f, arg, id)?;
                    f.stack.push(v);
                }
                0x5d => {
                    let id = f.pop()?.to_int();
                    let v = f.pop()?;
                    self.v4_set(host, f, arg, id, v)?;
                }
                0x5f => {
                    let n = name(self);
                    let v = self.the(host, &n)?;
                    f.stack.push(v);
                }
                0x60 => {
                    let v = f.pop()?;
                    let n = name(self);
                    self.set_the(host, &n, v)?;
                }
                0x61 | 0x70 => {
                    let obj = f.pop()?;
                    let n = name(self);
                    let v = self.get_property(host, &obj, &n)?;
                    f.stack.push(v);
                }
                0x62 => {
                    let v = f.pop()?;
                    let obj = f.pop()?;
                    let n = name(self);
                    self.set_property(host, &obj, &n, v)?;
                }
                0x64 => {
                    let i = f
                        .stack
                        .len()
                        .checked_sub(1 + arg as usize)
                        .ok_or("stack underflow")?;
                    let v = f.stack[i].clone();
                    f.stack.push(v);
                }
                0x65 => {
                    for _ in 0..arg {
                        f.pop()?;
                    }
                }
                0x66 => {
                    f.pop_args()?;
                    let n = name(self);
                    let v = self.the(host, &n)?;
                    f.stack.push(v);
                }
                0x71 => f
                    .stack
                    .push(Value::Float(f32::from_bits(arg as u32) as f64)),
                0x6d => {
                    let var = self.read_var(f, arg & 0xf)?;
                    f.stack.push(Value::VarRef(Box::new(var)));
                }
                0x1b => {
                    let lib = f.pop()?;
                    let field = f.pop()?;
                    let v = self.field_text(host, &field, &lib)?;
                    f.stack.push(v);
                }
                0x18 => {
                    let lib = f.pop()?;
                    let field = f.pop()?;
                    let spec = f.pop_chunk()?;
                    let m = self.member_ref(host, &field, &lib)?;
                    let Value::Host(r) = m else {
                        return Err("hilite: not a member".into());
                    };
                    let a = [Value::Int(spec.char.0), Value::Int(spec.char.1)];
                    if host.method(self, &r, "hilite", &a)?.is_none() {
                        return Err(Exit::Msg("not implemented: hilite chunk of field".into()));
                    }
                }
                0x72 => {
                    let n = name(self);
                    let v = match host.the(self, &n)? {
                        Some(v) => v,
                        None => Value::Host(HostRef::Object { kind: n, id: 0 }),
                    };
                    f.stack.push(v);
                }
                _ => {
                    let op_name = rifx::bytecode::Op(op).name();
                    return Err(Exit::Msg(format!(
                        "opcode {op_name} (0x{op:02x}) not implemented"
                    )));
                }
            }
        }
    }

    /// Methods on variable/chunk references (`s.word[1].delete()`,
    /// `s.char.count`, `getProp(ref, #item, 2)`).
    fn ref_method(
        &mut self,
        host: &mut dyn Host,
        f: &mut Frame,
        name: &str,
        args: &[Value],
    ) -> Result<Value, String> {
        let (slot, base) = match &args[0] {
            Value::VarRef(s) => ((**s).clone(), ChunkSpec::default()),
            Value::ChunkRef(s, c) => ((**s).clone(), *c),
            _ => unreachable!(),
        };
        let text = self.var_get(host, f, &slot)?.to_text();
        let delim = self.item_delimiter;
        let level = |v: &Value| -> Result<chunk::Kind, String> {
            Ok(match v.to_text().to_ascii_lowercase().as_str() {
                "char" => chunk::Kind::Char,
                "word" => chunk::Kind::Word,
                "item" => chunk::Kind::Item,
                "line" | "paragraph" => chunk::Kind::Line,
                other => return Err(format!("unknown chunk type #{other}")),
            })
        };
        let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
        match name.to_ascii_lowercase().as_str() {
            "getpropref" | "getprop" => {
                let kind = level(&arg(1))?;
                let range = (arg(2).to_int(), arg(3).to_int());
                let mut spec = base;
                match kind {
                    chunk::Kind::Char => spec.char = range,
                    chunk::Kind::Word => spec.word = range,
                    chunk::Kind::Item => spec.item = range,
                    chunk::Kind::Line => spec.line = range,
                }
                if name.eq_ignore_ascii_case("getpropref") {
                    Ok(Value::ChunkRef(Box::new(slot), spec))
                } else {
                    Ok(Value::Str(chunk::get(&text, &spec, delim)))
                }
            }
            "count" => {
                let sub = chunk::get(&text, &base, delim);
                Ok(Value::Int(chunk::count(&sub, level(&arg(1))?, delim) as i64))
            }
            "delete" => {
                let new = chunk::delete(&text, &base, delim);
                self.var_set(host, f, &slot, Value::Str(new))?;
                Ok(Value::Void)
            }
            other => Err(format!("chunk reference method #{other} not implemented")),
        }
    }

    fn receiver_prop(&self, f: &Frame, name: &str) -> Result<Value, String> {
        match &f.receiver {
            Some(o) => Ok(instance_prop(o, name).unwrap_or_default()),
            None => Err(format!("property #{name} used without an object (me)")),
        }
    }

    fn set_receiver_prop(&self, f: &Frame, name: &str, v: Value) -> Result<(), String> {
        match &f.receiver {
            Some(o) => {
                set_instance_prop(o, name, v);
                Ok(())
            }
            None => Err(format!("property #{name} set without an object (me)")),
        }
    }

    fn read_var(&self, f: &mut Frame, var_type: i32) -> Result<VarSlot, String> {
        Ok(match var_type {
            1 | 2 => VarSlot::Global(f.pop()?.to_text()),
            3 => VarSlot::Prop(f.pop()?.to_text()),
            4 => VarSlot::Param(f.pop()?.to_int().max(0) as usize),
            5 => VarSlot::Local(f.pop()?.to_int().max(0) as usize),
            6 => {
                let lib = f.pop()?;
                let field = f.pop()?;
                VarSlot::Field(Box::new(field), Box::new(lib))
            }
            t => return Err(format!("variable type {t} not implemented")),
        })
    }

    fn var_get(&mut self, host: &mut dyn Host, f: &Frame, var: &VarSlot) -> Result<Value, String> {
        Ok(match var {
            VarSlot::Global(n) => self.global(n),
            VarSlot::Prop(n) => self.receiver_prop(f, n)?,
            VarSlot::Param(i) => f.params.get(*i).cloned().unwrap_or_default(),
            VarSlot::Local(i) => f.locals.get(*i).cloned().unwrap_or_default(),
            VarSlot::Field(field, lib) => self.field_text(host, field, lib)?,
        })
    }

    fn var_set(
        &mut self,
        host: &mut dyn Host,
        f: &mut Frame,
        var: &VarSlot,
        v: Value,
    ) -> Result<(), String> {
        match var {
            VarSlot::Global(n) => self.set_global(n, v),
            VarSlot::Prop(n) => self.set_receiver_prop(f, n, v)?,
            VarSlot::Param(i) => {
                if *i >= f.params.len() {
                    f.params.resize(i + 1, Value::Void);
                }
                f.params[*i] = v;
            }
            VarSlot::Local(i) => {
                if *i >= f.locals.len() {
                    f.locals.resize(i + 1, Value::Void);
                }
                f.locals[*i] = v;
            }
            VarSlot::Field(field, lib) => {
                let m = self.member_ref(host, field, lib)?;
                self.set_property(host, &m, "text", v)?;
            }
        }
        Ok(())
    }

    fn member_ref(
        &mut self,
        host: &mut dyn Host,
        member: &Value,
        lib: &Value,
    ) -> Result<Value, String> {
        if let Value::Host(HostRef::Member { .. }) = member {
            return Ok(member.clone());
        }
        let mut args = vec![member.clone()];
        if lib.to_int() != 0 || matches!(lib, Value::Str(_)) {
            args.push(lib.clone());
        }
        match host.call(self, "member", &args)? {
            Some(v @ Value::Host(_)) => Ok(v),
            Some(other) => Err(format!("member() returned {}", other.ilk())),
            None => Err("not implemented: handler #member".into()),
        }
    }

    fn field_text(
        &mut self,
        host: &mut dyn Host,
        field: &Value,
        lib: &Value,
    ) -> Result<Value, String> {
        let m = self.member_ref(host, field, lib)?;
        self.get_property(host, &m, "text")
    }

    /// `the <prop> of <target>` in the D4-style encoding (`get`/`set`).
    fn v4_target(
        &mut self,
        host: &mut dyn Host,
        f: &mut Frame,
        kind: i32,
        id: i64,
    ) -> Result<(Option<Value>, String), String> {
        Ok(match kind {
            0x00 if id <= 0x0b => (
                None,
                names::MOVIE
                    .get(id as usize)
                    .copied()
                    .unwrap_or("?")
                    .to_string(),
            ),
            0x06 => {
                let s = f.pop()?;
                let target = match s {
                    v @ Value::Host(HostRef::Sprite(_)) => v,
                    other => Value::Host(HostRef::Sprite(other.to_int())),
                };
                (Some(target), lookup(names::SPRITE, id, "sprite")?)
            }
            0x07 => (None, lookup(names::ANIMATION, id, "animation")?),
            0x09 => {
                let lib = f.pop()?;
                let member = f.pop()?;
                let m = self.member_ref(host, &member, &lib)?;
                (Some(m), lookup(names::MEMBER, id, "member")?)
            }
            k => return Err(format!("the-property type {k} id {id} not implemented")),
        })
    }

    fn v4_get(
        &mut self,
        host: &mut dyn Host,
        f: &mut Frame,
        kind: i32,
        id: i64,
    ) -> Result<Value, String> {
        match kind {
            // `the last char/word/item/line of s`
            0x00 if id > 0x0b => {
                let s = f.pop()?.to_text();
                let k = chunk::Kind::from_id(id - 0x0b).ok_or(format!("last-chunk type {id}"))?;
                let n = chunk::count(&s, k, self.item_delimiter) as i64;
                let mut spec = ChunkSpec::default();
                match k {
                    chunk::Kind::Char => spec.char = (n, 0),
                    chunk::Kind::Word => spec.word = (n, 0),
                    chunk::Kind::Item => spec.item = (n, 0),
                    chunk::Kind::Line => spec.line = (n, 0),
                }
                return Ok(Value::Str(chunk::get(&s, &spec, self.item_delimiter)));
            }
            0x01 => {
                let s = f.pop()?.to_text();
                let k = chunk::Kind::from_id(id).ok_or(format!("chunk type {id}"))?;
                return Ok(Value::Int(chunk::count(&s, k, self.item_delimiter) as i64));
            }
            0x08 => {
                let name = lookup(names::ANIMATION2, id, "animation2")?;
                if id == 0x02 {
                    let lib = f.pop()?;
                    return match host.call(self, "__numberOfMembers", &[lib])? {
                        Some(v) => Ok(v),
                        None => Err("not implemented: the number of castMembers".into()),
                    };
                }
                return self.the(host, &name);
            }
            _ => {}
        }
        let (target, prop) = self.v4_target(host, f, kind, id)?;
        match target {
            Some(t) => self.get_property(host, &t, &prop),
            None => self.the(host, &prop),
        }
    }

    fn v4_set(
        &mut self,
        host: &mut dyn Host,
        f: &mut Frame,
        kind: i32,
        id: i64,
        v: Value,
    ) -> Result<(), String> {
        let (target, prop) = self.v4_target(host, f, kind, id)?;
        match target {
            Some(t) => self.set_property(host, &t, &prop, v),
            None => self.set_the(host, &prop, v),
        }
    }
}

enum Exit {
    Error(LingoError),
    Msg(String),
}

impl From<String> for Exit {
    fn from(s: String) -> Self {
        Exit::Msg(s)
    }
}

impl From<&str> for Exit {
    fn from(s: &str) -> Self {
        Exit::Msg(s.to_string())
    }
}

fn script_label(s: &crate::program::ScriptCode) -> String {
    if s.name.is_empty() {
        "script".into()
    } else {
        format!("{:?}", s.name)
    }
}

fn num(x: f64) -> Value {
    if x.fract() == 0.0 {
        Value::Int(x as i64)
    } else {
        Value::Float(x)
    }
}

fn lookup(table: &[&str], id: i64, what: &str) -> Result<String, String> {
    table
        .get(id as usize)
        .filter(|n| !n.is_empty())
        .map(|n| n.to_string())
        .ok_or_else(|| format!("{what} property id {id} unknown"))
}

/// Instance property, walking the `ancestor` chain.
pub fn instance_prop(o: &ObjectRef, name: &str) -> Option<Value> {
    let mut cur = o.clone();
    for _ in 0..64 {
        if let Some(v) = cur.borrow().get(name) {
            return Some(v.clone());
        }
        let next = match cur.borrow().get("ancestor") {
            Some(Value::Object(a)) => a.clone(),
            _ => return None,
        };
        cur = next;
    }
    None
}

/// Set on the first object in the chain that declares it, else add it to
/// the instance itself.
pub fn set_instance_prop(o: &ObjectRef, name: &str, v: Value) {
    let mut cur = o.clone();
    for _ in 0..64 {
        if cur.borrow_mut().set(name, v.clone()) {
            return;
        }
        let next = match cur.borrow().get("ancestor") {
            Some(Value::Object(a)) => a.clone(),
            _ => break,
        };
        cur = next;
    }
    o.borrow_mut().props.push((name.to_string(), v));
}

/// Property names of the D4-style `the` encoding (from public research).
pub mod names {
    pub const MOVIE: &[&str] = &[
        "floatPrecision",
        "mouseDownScript",
        "mouseUpScript",
        "keyDownScript",
        "keyUpScript",
        "timeoutScript",
        "short time",
        "abbr time",
        "long time",
        "short date",
        "abbr date",
        "long date",
    ];
    pub const SPRITE: &[&str] = &[
        "",
        "type",
        "backColor",
        "bottom",
        "castNum",
        "constraint",
        "cursor",
        "foreColor",
        "height",
        "immediate",
        "ink",
        "left",
        "lineSize",
        "locH",
        "locV",
        "movieRate",
        "movieTime",
        "pattern",
        "puppet",
        "right",
        "startTime",
        "stopTime",
        "stretch",
        "top",
        "trails",
        "visible",
        "volume",
        "width",
        "blend",
        "scriptNum",
        "moveableSprite",
        "editableText",
        "scoreColor",
        "loc",
        "rect",
        "memberNum",
        "castLibNum",
        "member",
        "scriptInstanceList",
        "currentTime",
        "mostRecentCuePoint",
        "tweened",
        "name",
    ];
    pub const MEMBER: &[&str] = &[
        "",
        "name",
        "text",
        "textStyle",
        "textFont",
        "textHeight",
        "textAlign",
        "textSize",
        "picture",
        "hilite",
        "number",
        "size",
        "loop",
        "duration",
        "controller",
        "directToStage",
        "sound",
        "foreColor",
        "backColor",
        "type",
    ];
    pub const ANIMATION: &[&str] = &[
        "",
        "beepOn",
        "buttonStyle",
        "centerStage",
        "checkBoxAccess",
        "checkboxType",
        "colorDepth",
        "colorQD",
        "exitLock",
        "fixStageSize",
        "fullColorPermit",
        "imageDirect",
        "doubleClick",
        "key",
        "lastClick",
        "lastEvent",
        "keyCode",
        "lastKey",
        "lastRoll",
        "timeoutLapsed",
        "multiSound",
        "pauseState",
        "quickTimePresent",
        "selEnd",
        "selStart",
        "soundEnabled",
        "soundLevel",
        "stageColor",
        "",
        "switchColorDepth",
        "timeoutKeyDown",
        "timeoutLength",
        "timeoutMouse",
        "timeoutPlay",
        "timer",
        "preLoadRAM",
        "videoForWindowsPresent",
        "netPresent",
        "safePlayer",
        "soundKeepDevice",
        "soundMixMedia",
    ];
    pub const ANIMATION2: &[&str] = &[
        "",
        "perFrameHook",
        "number of castMembers",
        "number of menus",
        "number of castLibs",
        "number of xtras",
    ];
}
