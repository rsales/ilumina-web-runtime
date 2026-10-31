//! Pure-Lingo built-in functions: types and conversion, strings, lists and
//! property lists, math, `script`/`new`/`call`. Player functions (`go`,
//! `sprite`, `member`, `the frame`…) belong to the host.

use crate::value::{parse_number, HostRef, ObjectRef, Value};
use crate::vm::{instance_prop, set_instance_prop, Host, Vm};

type R = Result<Option<Value>, String>;

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or_default()
}

fn idx(v: &Value) -> usize {
    v.to_int().max(0) as usize
}

/// A built-in by name. `Ok(None)`: not a built-in.
pub fn call(vm: &mut Vm, host: &mut dyn Host, name: &str, args: &[Value]) -> R {
    let a0 = arg(args, 0);
    let lname = name.to_ascii_lowercase();
    let v = match lname.as_str() {
        // --- types ---
        "voidp" => Value::bool(a0.is_void()),
        "objectp" => Value::bool(matches!(
            a0,
            Value::Object(_)
                | Value::List(_)
                | Value::PropList(_)
                | Value::Script(_)
                | Value::Host(_)
        )),
        "listp" => Value::bool(matches!(
            a0,
            Value::List(_) | Value::PropList(_) | Value::Point(..) | Value::Rect(..)
        )),
        "integerp" => Value::bool(matches!(a0, Value::Int(_))),
        "floatp" => Value::bool(matches!(a0, Value::Float(_))),
        "stringp" => Value::bool(matches!(a0, Value::Str(_))),
        "symbolp" => Value::bool(matches!(a0, Value::Symbol(_))),
        "ilk" => {
            if args.len() > 1 {
                Value::bool(a0.ilk().eq_ignore_ascii_case(&arg(args, 1).to_text()))
            } else {
                Value::sym(a0.ilk())
            }
        }
        "void" => Value::Void,
        // --- conversion ---
        "integer" => match &a0 {
            Value::Str(s) => match parse_number(s) {
                Some(n) => Value::Int(n.value().to_int()),
                None => Value::Void,
            },
            Value::Float(x) => Value::Int(x.round() as i64),
            v => Value::Int(v.to_int()),
        },
        "float" => match &a0 {
            Value::Str(s) => match parse_number(s) {
                Some(n) => Value::Float(n.as_f64()),
                None => Value::Void,
            },
            v => Value::Float(v.to_f64()),
        },
        "string" => Value::Str(a0.to_text()),
        "symbol" => match &a0 {
            Value::Symbol(_) => a0.clone(),
            v => Value::Symbol(v.to_text()),
        },
        "value" => value_of(&a0.to_text()),
        // --- strings ---
        "length" => Value::Int(a0.to_text().chars().count() as i64),
        "chars" => {
            let s: Vec<char> = a0.to_text().chars().collect();
            let (a, b) = (idx(&arg(args, 1)).max(1), idx(&arg(args, 2)).min(s.len()));
            Value::Str(if a > b {
                String::new()
            } else {
                s[a - 1..b].iter().collect()
            })
        }
        "offset" => {
            let (needle, hay) = (
                a0.to_text().to_lowercase(),
                arg(args, 1).to_text().to_lowercase(),
            );
            Value::Int(match hay.find(&needle) {
                Some(b) if !needle.is_empty() => hay[..b].chars().count() as i64 + 1,
                _ => 0,
            })
        }
        "chartonum" => Value::Int(a0.to_text().chars().next().map(|c| c as i64).unwrap_or(0)),
        "numtochar" => Value::Str(
            char::from_u32(a0.to_int() as u32)
                .map(String::from)
                .unwrap_or_default(),
        ),
        // --- math ---
        "abs" => match a0 {
            Value::Float(x) => Value::Float(x.abs()),
            v => Value::Int(v.to_int().abs()),
        },
        "sqrt" => Value::Float(a0.to_f64().sqrt()),
        "sin" => Value::Float(a0.to_f64().sin()),
        "cos" => Value::Float(a0.to_f64().cos()),
        "atan" => Value::Float(a0.to_f64().atan()),
        "power" => Value::Float(a0.to_f64().powf(arg(args, 1).to_f64())),
        "random" => Value::Int(vm.random(a0.to_int())),
        "bitand" => Value::Int(a0.to_int() & arg(args, 1).to_int()),
        "bitor" => Value::Int(a0.to_int() | arg(args, 1).to_int()),
        "bitxor" => Value::Int(a0.to_int() ^ arg(args, 1).to_int()),
        "bitnot" => Value::Int(!a0.to_int()),
        "max" | "min" => {
            let items: Vec<Value> = match &a0 {
                Value::List(l) if args.len() == 1 => l.borrow().clone(),
                _ => args.to_vec(),
            };
            let mut best: Option<Value> = None;
            for v in items {
                let better = match &best {
                    None => true,
                    Some(b) => {
                        let o = v.lingo_cmp(b);
                        if lname == "max" {
                            o == Some(std::cmp::Ordering::Greater)
                        } else {
                            o == Some(std::cmp::Ordering::Less)
                        }
                    }
                };
                if better {
                    best = Some(v);
                }
            }
            best.unwrap_or_default()
        }
        "point" => Value::Point(a0.to_f64(), arg(args, 1).to_f64()),
        "rect" => match (&a0, &arg(args, 1)) {
            (Value::Point(l, t), Value::Point(r, b)) => Value::Rect(*l, *t, *r, *b),
            _ => Value::Rect(
                a0.to_f64(),
                arg(args, 1).to_f64(),
                arg(args, 2).to_f64(),
                arg(args, 3).to_f64(),
            ),
        },
        "inside" => match (&a0, &arg(args, 1)) {
            (Value::Point(x, y), Value::Rect(l, t, r, b)) => {
                Value::bool(x >= l && x < r && y >= t && y < b)
            }
            _ => Value::Int(0),
        },
        // --- lists ---
        "list" => Value::list(args.to_vec()),
        "count" => match &a0 {
            Value::List(l) => Value::Int(l.borrow().len() as i64),
            Value::PropList(l) => Value::Int(l.borrow().len() as i64),
            Value::Point(..) => Value::Int(2),
            Value::Rect(..) => Value::Int(4),
            Value::Object(o) => Value::Int(o.borrow().props.len() as i64),
            _ => return Ok(None),
        },
        "getat" => match &a0 {
            Value::List(l) => at(&l.borrow(), &arg(args, 1))?,
            Value::PropList(l) => {
                let i = idx(&arg(args, 1));
                l.borrow()
                    .get(i.wrapping_sub(1))
                    .map(|p| p.1.clone())
                    .ok_or(format!("index {i} out of range"))?
            }
            Value::Point(x, y) => match idx(&arg(args, 1)) {
                1 => Value::Float(*x),
                2 => Value::Float(*y),
                i => return Err(format!("index {i} out of range")),
            },
            Value::Rect(l, t, r, b) => match idx(&arg(args, 1)) {
                1 => Value::Float(*l),
                2 => Value::Float(*t),
                3 => Value::Float(*r),
                4 => Value::Float(*b),
                i => return Err(format!("index {i} out of range")),
            },
            _ => return Ok(None),
        },
        "getlast" => match &a0 {
            Value::List(l) => l.borrow().last().cloned().unwrap_or_default(),
            Value::PropList(l) => l.borrow().last().map(|p| p.1.clone()).unwrap_or_default(),
            _ => return Ok(None),
        },
        "setat" => match &a0 {
            Value::List(l) => {
                let i = idx(&arg(args, 1)).max(1);
                let mut l = l.borrow_mut();
                if i > l.len() {
                    l.resize(i, Value::Int(0));
                }
                l[i - 1] = arg(args, 2);
                Value::Void
            }
            Value::PropList(l) => {
                let i = idx(&arg(args, 1));
                let mut l = l.borrow_mut();
                let slot = l
                    .get_mut(i.wrapping_sub(1))
                    .ok_or(format!("index {i} out of range"))?;
                slot.1 = arg(args, 2);
                Value::Void
            }
            _ => return Ok(None),
        },
        "add" | "append" => match &a0 {
            Value::List(l) => {
                l.borrow_mut().push(arg(args, 1));
                Value::Void
            }
            _ => return Ok(None),
        },
        "addat" => match &a0 {
            Value::List(l) => {
                let mut l = l.borrow_mut();
                let i = idx(&arg(args, 1)).max(1);
                if i > l.len() {
                    l.resize(i - 1, Value::Int(0));
                    l.push(arg(args, 2));
                } else {
                    l.insert(i - 1, arg(args, 2));
                }
                Value::Void
            }
            _ => return Ok(None),
        },
        "deleteat" => match &a0 {
            Value::List(l) => {
                let i = idx(&arg(args, 1));
                let mut l = l.borrow_mut();
                if i == 0 || i > l.len() {
                    return Err(format!("index {i} out of range"));
                }
                l.remove(i - 1);
                Value::Void
            }
            Value::PropList(l) => {
                let i = idx(&arg(args, 1));
                let mut l = l.borrow_mut();
                if i == 0 || i > l.len() {
                    return Err(format!("index {i} out of range"));
                }
                l.remove(i - 1);
                Value::Void
            }
            _ => return Ok(None),
        },
        "deleteone" => match &a0 {
            Value::List(l) => {
                let mut l = l.borrow_mut();
                if let Some(p) = l.iter().position(|v| v.lingo_eq(&arg(args, 1))) {
                    l.remove(p);
                }
                Value::Void
            }
            Value::PropList(l) => {
                let mut l = l.borrow_mut();
                if let Some(p) = l.iter().position(|v| v.1.lingo_eq(&arg(args, 1))) {
                    l.remove(p);
                }
                Value::Void
            }
            _ => return Ok(None),
        },
        "getpos" | "getone" | "findpos" => match &a0 {
            Value::List(l) if lname != "findpos" => Value::Int(
                l.borrow()
                    .iter()
                    .position(|v| v.lingo_eq(&arg(args, 1)))
                    .map(|p| p as i64 + 1)
                    .unwrap_or(0),
            ),
            Value::PropList(l) => {
                let l = l.borrow();
                let target = arg(args, 1);
                match lname.as_str() {
                    "getone" => l
                        .iter()
                        .find(|p| p.1.lingo_eq(&target))
                        .map(|p| p.0.clone())
                        .unwrap_or(Value::Int(0)),
                    "getpos" => Value::Int(
                        l.iter()
                            .position(|p| p.1.lingo_eq(&target))
                            .map(|p| p as i64 + 1)
                            .unwrap_or(0),
                    ),
                    _ => match l.iter().position(|p| p.0.lingo_eq(&target)) {
                        Some(p) => Value::Int(p as i64 + 1),
                        None => Value::Void,
                    },
                }
            }
            _ => return Ok(None),
        },
        "addprop" => match &a0 {
            Value::PropList(l) => {
                l.borrow_mut().push((arg(args, 1), arg(args, 2)));
                Value::Void
            }
            _ => return Ok(None),
        },
        "getprop" | "getaprop" => match &a0 {
            Value::PropList(l) => {
                let key = arg(args, 1);
                match l.borrow().iter().find(|p| p.0.lingo_eq(&key)) {
                    Some(p) => p.1.clone(),
                    None if lname == "getaprop" => Value::Void,
                    None => return Err(format!("property {key} not found")),
                }
            }
            Value::List(l) if lname == "getaprop" => {
                at(&l.borrow(), &arg(args, 1)).unwrap_or_default()
            }
            Value::Object(o) => instance_prop(o, &arg(args, 1).to_text()).unwrap_or_default(),
            Value::Host(_) => {
                let p = arg(args, 1).to_text();
                vm.get_property(host, &a0, &p)?
            }
            _ => return Ok(None),
        },
        "setprop" | "setaprop" => match &a0 {
            Value::PropList(l) => {
                let key = arg(args, 1);
                let mut l = l.borrow_mut();
                match l.iter_mut().find(|p| p.0.lingo_eq(&key)) {
                    Some(slot) => slot.1 = arg(args, 2),
                    None if lname == "setaprop" => l.push((key, arg(args, 2))),
                    None => return Err(format!("property {key} not found")),
                }
                Value::Void
            }
            Value::Object(o) => {
                set_instance_prop(o, &arg(args, 1).to_text(), arg(args, 2));
                Value::Void
            }
            Value::Host(_) => {
                let p = arg(args, 1).to_text();
                vm.set_property(host, &a0, &p, arg(args, 2))?;
                Value::Void
            }
            _ => return Ok(None),
        },
        "deleteprop" => match &a0 {
            Value::PropList(l) => {
                let key = arg(args, 1);
                let mut l = l.borrow_mut();
                if let Some(p) = l.iter().position(|p| p.0.lingo_eq(&key)) {
                    l.remove(p);
                }
                Value::Void
            }
            _ => return Ok(None),
        },
        "getpropat" => match &a0 {
            Value::PropList(l) => {
                let i = idx(&arg(args, 1));
                l.borrow()
                    .get(i.wrapping_sub(1))
                    .map(|p| p.0.clone())
                    .ok_or(format!("index {i} out of range"))?
            }
            _ => return Ok(None),
        },
        "duplicate" => match &a0 {
            Value::List(l) => Value::list(l.borrow().iter().map(deep_copy).collect()),
            Value::PropList(l) => Value::prop_list(
                l.borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), deep_copy(v)))
                    .collect(),
            ),
            v => v.clone(),
        },
        "sort" => match &a0 {
            Value::List(l) => {
                l.borrow_mut()
                    .sort_by(|a, b| a.lingo_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                Value::Void
            }
            Value::PropList(l) => {
                l.borrow_mut()
                    .sort_by(|a, b| a.0.lingo_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                Value::Void
            }
            _ => return Ok(None),
        },
        // --- scripts and objects ---
        "script" => match &a0 {
            Value::Str(s) | Value::Symbol(s) => match vm.program.script_named(s) {
                Some(i) => Value::Script(i),
                None => return Err(format!("script {s:?} not found")),
            },
            Value::Script(_) => a0.clone(),
            Value::Host(HostRef::Member { .. }) => match host.call(vm, "__scriptOfMember", args)? {
                Some(v) => v,
                None => return Err("not implemented: script(member)".into()),
            },
            other => return Err(format!("script({}) not implemented", other.ilk())),
        },
        "new" => match &a0 {
            Value::Script(i) => {
                let i = *i;
                match vm.instantiate(host, i, args[1..].to_vec()) {
                    Ok(v) => v,
                    Err(e) => return Err(vm.stash(e)),
                }
            }
            _ => return Ok(None),
        },
        "call" => {
            // call(#handler, objectOrList, args…)
            let handler = a0.to_text();
            let rest = args.get(2..).unwrap_or(&[]).to_vec();
            let targets: Vec<Value> = match arg(args, 1) {
                Value::List(l) => l.borrow().clone(),
                v => vec![v],
            };
            let mut last = Value::Void;
            for t in targets {
                if matches!(t, Value::Void) {
                    continue;
                }
                let mut a = vec![t];
                a.extend(rest.iter().cloned());
                last = match vm.call_method(host, &handler, a) {
                    Ok(v) => v,
                    Err(e) => return Err(vm.stash(e)),
                };
            }
            last
        }
        "put" => {
            let line = args
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            vm.output.push(line);
            Value::Void
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}

/// Methods every instance answers even without a handler.
pub fn object_method(_vm: &mut Vm, o: &ObjectRef, name: &str, args: &[Value]) -> R {
    Ok(Some(match name.to_ascii_lowercase().as_str() {
        "handler" => Value::Int(0),
        "getaprop" | "getprop" => instance_prop(o, &arg(args, 0).to_text()).unwrap_or_default(),
        "setaprop" | "setprop" => {
            set_instance_prop(o, &arg(args, 0).to_text(), arg(args, 1));
            Value::Void
        }
        _ => return Ok(None),
    }))
}

fn at(l: &[Value], i: &Value) -> Result<Value, String> {
    let n = idx(i);
    l.get(n.wrapping_sub(1))
        .cloned()
        .ok_or(format!("index {n} out of range (count {})", l.len()))
}

fn deep_copy(v: &Value) -> Value {
    match v {
        Value::List(l) => Value::list(l.borrow().iter().map(deep_copy).collect()),
        Value::PropList(l) => Value::prop_list(
            l.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), deep_copy(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `value("…")` for literals: numbers, quoted strings, symbols, lists.
pub fn value_of(s: &str) -> Value {
    let t = s.trim();
    if let Some(n) = parse_number(t) {
        return n.value();
    }
    if let Some(sym) = t.strip_prefix('#') {
        if sym.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Value::sym(sym);
        }
    }
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        return Value::str(&t[1..t.len() - 1]);
    }
    if t.starts_with('[') && t.ends_with(']') {
        return parse_list(&t[1..t.len() - 1]);
    }
    Value::Void
}

fn parse_list(inner: &str) -> Value {
    let inner = inner.trim();
    if inner == ":" {
        return Value::prop_list(vec![]);
    }
    if inner.is_empty() {
        return Value::list(vec![]);
    }
    let parts = split_top(inner, ',');
    let is_prop = parts.iter().all(|p| split_top(p, ':').len() == 2);
    if is_prop {
        Value::prop_list(
            parts
                .iter()
                .map(|p| {
                    let kv = split_top(p, ':');
                    (value_of(&kv[0]), value_of(&kv[1]))
                })
                .collect(),
        )
    } else {
        Value::list(parts.iter().map(|p| value_of(p)).collect())
    }
}

/// Split at `sep` outside brackets and quotes.
fn split_top(s: &str, sep: char) -> Vec<String> {
    let (mut out, mut cur, mut depth, mut quoted) = (Vec::new(), String::new(), 0, false);
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            '[' if !quoted => depth += 1,
            ']' if !quoted => depth -= 1,
            _ => {}
        }
        if c == sep && depth == 0 && !quoted {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::Program;
    use crate::vm::NoHost;

    fn b(name: &str, args: &[Value]) -> Value {
        let mut vm = Vm::new(Program::default());
        call(&mut vm, &mut NoHost, name, args).unwrap().unwrap()
    }

    #[test]
    fn list_functions() {
        let l = Value::list(vec![Value::Int(3), Value::Int(1)]);
        b("add", &[l.clone(), Value::Int(2)]);
        assert_eq!(l.to_string(), "[3, 1, 2]");
        b("sort", std::slice::from_ref(&l));
        assert_eq!(l.to_string(), "[1, 2, 3]");
        assert_eq!(b("getPos", &[l.clone(), Value::Int(3)]).to_int(), 3);
        b("deleteAt", &[l.clone(), Value::Int(1)]);
        b("setAt", &[l.clone(), Value::Int(4), Value::Int(9)]);
        assert_eq!(l.to_string(), "[2, 3, 0, 9]");
        let d = b("duplicate", std::slice::from_ref(&l));
        b("add", &[d.clone(), Value::Int(5)]);
        assert_eq!(b("count", &[l]).to_int(), 4);
        assert_eq!(b("count", &[d]).to_int(), 5);
    }

    #[test]
    fn prop_list_functions() {
        let p = Value::prop_list(vec![]);
        b("addProp", &[p.clone(), Value::sym("a"), Value::Int(1)]);
        b("setaProp", &[p.clone(), Value::sym("b"), Value::Int(2)]);
        b("setaProp", &[p.clone(), Value::sym("A"), Value::Int(7)]);
        assert_eq!(p.to_string(), "[#a: 7, #b: 2]");
        assert!(b("getaProp", &[p.clone(), Value::sym("zz")]).is_void());
        assert_eq!(
            b("getPropAt", &[p.clone(), Value::Int(2)]).to_string(),
            "#b"
        );
        assert_eq!(b("findPos", &[p.clone(), Value::sym("b")]).to_int(), 2);
        b("deleteProp", &[p.clone(), Value::sym("a")]);
        assert_eq!(p.to_string(), "[#b: 2]");
    }

    #[test]
    fn conversions_and_strings() {
        assert_eq!(b("integer", &[Value::str("42")]).to_int(), 42);
        assert!(b("integer", &[Value::str("x")]).is_void());
        assert_eq!(b("string", &[Value::sym("abc")]).to_text(), "abc");
        assert_eq!(
            b("offset", &[Value::str("lo"), Value::str("Hello")]).to_int(),
            4
        );
        assert_eq!(
            b(
                "chars",
                &[Value::str("Hello"), Value::Int(2), Value::Int(3)]
            )
            .to_text(),
            "el"
        );
        assert_eq!(b("length", &[Value::str("ação")]).to_int(), 4);
        assert_eq!(
            b("max", &[Value::Int(3), Value::Int(9), Value::Int(1)]).to_int(),
            9
        );
    }

    #[test]
    fn value_parses_literals() {
        assert_eq!(
            value_of("[#a: 1, #b: [2, \"x\"]]").to_string(),
            "[#a: 1, #b: [2, \"x\"]]"
        );
        assert_eq!(value_of("3.5").to_string(), "3.5000");
        assert_eq!(value_of("#sym").to_string(), "#sym");
        assert!(value_of("foo bar").is_void());
    }
}
