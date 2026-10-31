//! Lingo values and their coercion, comparison and arithmetic rules.
//!
//! Lists and property lists are shared by reference (as in Lingo: `b = a`
//! makes both names point at one list; `duplicate()` copies). Strings,
//! numbers, symbols, points and rects are values.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

pub type ListRef = Rc<RefCell<Vec<Value>>>;
pub type PropListRef = Rc<RefCell<Vec<(Value, Value)>>>;
pub type ObjectRef = Rc<RefCell<Instance>>;

/// Something the host owns (sprite, member, cast lib, Xtra, window…).
#[derive(Clone, Debug, PartialEq)]
pub enum HostRef {
    Sprite(i64),
    Member {
        lib: i64,
        number: i64,
    },
    CastLib(i64),
    /// An Xtra by name (`xtra("FileIO")`).
    Xtra(String),
    /// An object the host created (Xtra instance, window, timeout…).
    Object {
        kind: String,
        id: i64,
    },
}

/// A variable that `put`, `putchunk`, `deletechunk` and chunk references
/// write back to.
#[derive(Clone, Debug)]
pub enum VarSlot {
    Global(String),
    Prop(String),
    Param(usize),
    Local(usize),
    /// `field x of castLib y`
    Field(Box<Value>, Box<Value>),
}

/// A parent-script instance (`new(script "X")`).
#[derive(Debug)]
pub struct Instance {
    pub script: usize,
    pub props: Vec<(String, Value)>,
}

impl Instance {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.props
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    }

    /// Set an existing property; returns false when it is not declared.
    pub fn set(&mut self, name: &str, v: Value) -> bool {
        match self
            .props
            .iter_mut()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
        {
            Some(slot) => {
                slot.1 = v;
                true
            }
            None => false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum Value {
    #[default]
    Void,
    Int(i64),
    Float(f64),
    Str(String),
    Symbol(String),
    List(ListRef),
    PropList(PropListRef),
    Point(f64, f64),
    Rect(f64, f64, f64, f64),
    /// A calendar date, as days since 1970-01-01 (proleptic Gregorian).
    Date(i64),
    /// A color: `rgb(r, g, b)` (Director 7+ color object).
    Color(u8, u8, u8),
    /// A script (`script "X"`), by index into the program.
    Script(usize),
    Object(ObjectRef),
    Host(HostRef),
    /// Arguments on the stack for a call; `true` = the result is discarded.
    ArgList(Vec<Value>, bool),
    /// A reference to a variable (`pushchunkvarref`), for `s.word[1].delete()`.
    VarRef(Box<VarSlot>),
    /// A chunk of a variable (`getPropRef(varRef, #word, 1)`).
    ChunkRef(Box<VarSlot>, crate::chunk::ChunkSpec),
}

impl Value {
    pub fn str(s: impl Into<String>) -> Self {
        Value::Str(s.into())
    }
    pub fn sym(s: impl Into<String>) -> Self {
        Value::Symbol(s.into())
    }
    pub fn list(v: Vec<Value>) -> Self {
        Value::List(Rc::new(RefCell::new(v)))
    }
    pub fn prop_list(v: Vec<(Value, Value)>) -> Self {
        Value::PropList(Rc::new(RefCell::new(v)))
    }
    pub fn bool(b: bool) -> Self {
        Value::Int(b as i64)
    }

    /// Lingo's `ilk` symbol name.
    pub fn ilk(&self) -> &'static str {
        match self {
            Value::Void => "void",
            Value::Int(_) => "integer",
            Value::Float(_) => "float",
            Value::Str(_) => "string",
            Value::Symbol(_) => "symbol",
            Value::List(_) => "list",
            Value::PropList(_) => "propList",
            Value::Point(..) => "point",
            Value::Rect(..) => "rect",
            Value::Date(_) => "date",
            Value::Color(..) => "color",
            Value::Script(_) => "script",
            Value::Object(_) => "instance",
            Value::Host(HostRef::Sprite(_)) => "sprite",
            Value::Host(HostRef::Member { .. }) => "member",
            Value::Host(HostRef::CastLib(_)) => "castLib",
            Value::Host(HostRef::Xtra(_)) => "xtra",
            Value::Host(HostRef::Object { .. }) => "instance",
            Value::ArgList(..) => "list",
            Value::VarRef(_) | Value::ChunkRef(..) => "string",
        }
    }

    pub fn is_void(&self) -> bool {
        matches!(self, Value::Void)
    }

    /// Condition test (`if x then`): zero and VOID are false.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Void => false,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => parse_number(s)
                .map(|n| n.as_f64() != 0.0)
                .unwrap_or(!s.is_empty()),
            _ => true,
        }
    }

    /// Numeric view: strings are parsed, VOID counts as 0.
    pub fn number(&self) -> Option<Num> {
        match self {
            Value::Int(i) => Some(Num::Int(*i)),
            Value::Float(f) => Some(Num::Float(*f)),
            Value::Void => Some(Num::Int(0)),
            Value::Str(s) => parse_number(s),
            _ => None,
        }
    }

    pub fn to_int(&self) -> i64 {
        match self.number() {
            Some(Num::Int(i)) => i,
            Some(Num::Float(f)) => f.round() as i64,
            None => 0,
        }
    }

    pub fn to_f64(&self) -> f64 {
        self.number().map(|n| n.as_f64()).unwrap_or(0.0)
    }

    /// `string(x)`: how Lingo turns a value into text.
    pub fn to_text(&self) -> String {
        match self {
            // `"a" & VOID` is "a" (only `put` shows <Void>).
            Value::Void => String::new(),
            Value::Str(s) => s.clone(),
            Value::Symbol(s) => s.clone(),
            other => other.to_string(),
        }
    }

    /// Identity for reference types, structural for the rest (Lingo `=`).
    pub fn lingo_eq(&self, other: &Value) -> bool {
        use Value::*;
        match (self, other) {
            (Void, Void) => true,
            (Void, _) | (_, Void) => false,
            (Str(a), Str(b)) => a.eq_ignore_ascii_case(b) || lower(a) == lower(b),
            (Symbol(a), Symbol(b)) => a.eq_ignore_ascii_case(b),
            (Symbol(a), Str(b)) | (Str(b), Symbol(a)) => a.eq_ignore_ascii_case(b),
            (Object(a), Object(b)) => Rc::ptr_eq(a, b),
            (Script(a), Script(b)) => a == b,
            (Color(a, b, c), Color(x, y, z)) => (a, b, c) == (x, y, z),
            (Host(a), Host(b)) => a == b,
            (List(a), List(b)) => {
                Rc::ptr_eq(a, b) || {
                    let (a, b) = (a.borrow(), b.borrow());
                    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.lingo_eq(y))
                }
            }
            (PropList(a), PropList(b)) => {
                Rc::ptr_eq(a, b) || {
                    let (a, b) = (a.borrow(), b.borrow());
                    a.len() == b.len()
                        && a.iter()
                            .zip(b.iter())
                            .all(|(x, y)| x.0.lingo_eq(&y.0) && x.1.lingo_eq(&y.1))
                }
            }
            (Point(a, b), Point(c, d)) => a == c && b == d,
            (Date(a), Date(b)) => a == b,
            (Rect(a, b, c, d), Rect(e, f, g, h)) => a == e && b == f && c == g && d == h,
            (a, b) => match (a.number_strict(), b.number_strict()) {
                (Some(x), Some(y)) => x.as_f64() == y.as_f64(),
                _ => false,
            },
        }
    }

    /// Ordering for `<`, `>`…: numbers numerically, strings case-insensitively.
    pub fn lingo_cmp(&self, other: &Value) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Value::Date(a), Value::Date(b)) => Some(a.cmp(b)),
            (Value::Str(a), Value::Str(b)) => Some(lower(a).cmp(&lower(b))),
            (Value::Symbol(a), Value::Symbol(b)) => Some(lower(a).cmp(&lower(b))),
            _ => {
                let (a, b) = (self.number()?, other.number()?);
                a.as_f64().partial_cmp(&b.as_f64())
            }
        }
    }

    /// Like `number`, but strings must look numeric and VOID is not 0.
    fn number_strict(&self) -> Option<Num> {
        match self {
            Value::Int(_) | Value::Float(_) => self.number(),
            Value::Str(s) => parse_number(s),
            _ => None,
        }
    }
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// Days since 1970-01-01 for a Gregorian date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// (year, month, day) for days since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    pub fn as_f64(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(f) => f,
        }
    }
    pub fn value(self) -> Value {
        match self {
            Num::Int(i) => Value::Int(i),
            Num::Float(f) => Value::Float(f),
        }
    }
}

/// Parse a Lingo number literal from text (`"12"`, `" -3.5 "`).
pub fn parse_number(s: &str) -> Option<Num> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(i) = t.parse::<i64>() {
        return Some(Num::Int(i));
    }
    if t.chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))
    {
        if let Ok(f) = t.parse::<f64>() {
            return Some(Num::Float(f));
        }
    }
    None
}

/// Binary arithmetic. `op`: '+', '-', '*', '/', '%'. Points, rects and
/// lists combine element by element (or with a scalar).
pub fn arith(op: char, a: &Value, b: &Value) -> Result<Value, String> {
    use Value::*;
    match (a, b) {
        (Date(x), Date(y)) if op == '-' => Ok(Int(x - y)),
        (Date(x), n) if (op == '+' || op == '-') && n.number().is_some() => {
            let d = n.to_int();
            Ok(Date(if op == '+' { x + d } else { x - d }))
        }
        (Point(x1, y1), Point(x2, y2)) => Ok(Point(f(op, *x1, *x2)?, f(op, *y1, *y2)?)),
        (Point(x, y), s) if s.number().is_some() => {
            let n = s.to_f64();
            Ok(Point(f(op, *x, n)?, f(op, *y, n)?))
        }
        (Rect(a1, b1, c1, d1), Rect(a2, b2, c2, d2)) => Ok(Rect(
            f(op, *a1, *a2)?,
            f(op, *b1, *b2)?,
            f(op, *c1, *c2)?,
            f(op, *d1, *d2)?,
        )),
        (Rect(a1, b1, c1, d1), s) if s.number().is_some() => {
            let n = s.to_f64();
            Ok(Rect(
                f(op, *a1, n)?,
                f(op, *b1, n)?,
                f(op, *c1, n)?,
                f(op, *d1, n)?,
            ))
        }
        (List(l), other) => {
            let items: Result<Vec<_>, _> = l.borrow().iter().map(|x| arith(op, x, other)).collect();
            Ok(Value::list(items?))
        }
        _ => {
            let (x, y) = (
                a.number()
                    .ok_or_else(|| format!("{op}: {} is not a number", a.ilk()))?,
                b.number()
                    .ok_or_else(|| format!("{op}: {} is not a number", b.ilk()))?,
            );
            match (x, y) {
                (Num::Int(x), Num::Int(y)) => Ok(Int(match op {
                    '+' => x.wrapping_add(y),
                    '-' => x.wrapping_sub(y),
                    '*' => x.wrapping_mul(y),
                    '/' => {
                        if y == 0 {
                            return Err("division by zero".into());
                        }
                        x / y
                    }
                    '%' => {
                        if y == 0 {
                            return Err("division by zero".into());
                        }
                        x % y
                    }
                    _ => unreachable!(),
                })),
                _ => Ok(Float(f(op, x.as_f64(), y.as_f64())?)),
            }
        }
    }
}

fn f(op: char, x: f64, y: f64) -> Result<f64, String> {
    Ok(match op {
        '+' => x + y,
        '-' => x - y,
        '*' => x * y,
        '/' => x / y,
        '%' => x % y,
        _ => return Err(format!("bad operator {op}")),
    })
}

/// Lingo's float formatting (`the floatPrecision` 4 by default).
fn float_text(x: f64) -> String {
    format!("{x:.4}")
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Void => write!(f, "<Void>"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{}", float_text(*x)),
            Value::Str(s) => write!(f, "\"{s}\""),
            Value::Symbol(s) => write!(f, "#{s}"),
            Value::List(l) => {
                write!(f, "[")?;
                for (i, v) in l.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::PropList(l) => {
                let l = l.borrow();
                if l.is_empty() {
                    return write!(f, "[:]");
                }
                write!(f, "[")?;
                for (i, (k, v)) in l.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "]")
            }
            Value::Point(x, y) => write!(f, "point({}, {})", num_text(*x), num_text(*y)),
            Value::Rect(a, b, c, d) => write!(
                f,
                "rect({}, {}, {}, {})",
                num_text(*a),
                num_text(*b),
                num_text(*c),
                num_text(*d)
            ),
            Value::Date(d) => {
                let (y, m, dd) = civil_from_days(*d);
                write!(f, "date( {y}, {m}, {dd} )")
            }
            Value::Color(r, g, b) => write!(f, "rgb( {r}, {g}, {b} )"),
            Value::Script(i) => write!(f, "<script {i}>"),
            Value::Object(o) => write!(f, "<offspring script {}>", o.borrow().script),
            Value::Host(h) => match h {
                HostRef::Sprite(n) => write!(f, "(sprite {n})"),
                HostRef::Member { lib, number } => write!(f, "(member {number} of castLib {lib})"),
                HostRef::CastLib(n) => write!(f, "(castLib {n})"),
                HostRef::Xtra(n) => write!(f, "<Xtra \"{n}\">"),
                HostRef::Object { kind, id } => write!(f, "<{kind} {id}>"),
            },
            Value::ArgList(v, _) => write!(f, "<args {}>", v.len()),
            Value::VarRef(v) => write!(f, "<ref {v:?}>"),
            Value::ChunkRef(v, c) => write!(f, "<chunk {c:?} of {v:?}>"),
        }
    }
}

fn num_text(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        float_text(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_division_truncates_and_mixed_gives_float() {
        assert!(matches!(
            arith('/', &Value::Int(7), &Value::Int(2)),
            Ok(Value::Int(3))
        ));
        assert!(
            matches!(arith('/', &Value::Int(7), &Value::Float(2.0)), Ok(Value::Float(f)) if f == 3.5)
        );
        assert!(arith('/', &Value::Int(1), &Value::Int(0)).is_err());
        assert!(matches!(
            arith('+', &Value::str("2"), &Value::Int(3)),
            Ok(Value::Int(5))
        ));
        assert!(matches!(
            arith('+', &Value::Void, &Value::Int(3)),
            Ok(Value::Int(3))
        ));
    }

    #[test]
    fn points_and_rects_are_elementwise() {
        let p = arith('+', &Value::Point(1.0, 2.0), &Value::Point(10.0, 20.0)).unwrap();
        assert_eq!(p.to_string(), "point(11, 22)");
        let r = arith('*', &Value::Rect(1.0, 2.0, 3.0, 4.0), &Value::Int(2)).unwrap();
        assert_eq!(r.to_string(), "rect(2, 4, 6, 8)");
    }

    #[test]
    fn equality_rules() {
        assert!(Value::str("ABC").lingo_eq(&Value::str("abc")));
        assert!(Value::sym("Foo").lingo_eq(&Value::sym("foo")));
        assert!(Value::Int(2).lingo_eq(&Value::Float(2.0)));
        assert!(Value::str("2").lingo_eq(&Value::Int(2)));
        assert!(!Value::Void.lingo_eq(&Value::Int(0)));
        assert!(Value::list(vec![Value::Int(1)]).lingo_eq(&Value::list(vec![Value::Int(1)])));
        assert_eq!(
            Value::str("b").lingo_cmp(&Value::str("A")),
            Some(std::cmp::Ordering::Greater)
        );
    }

    #[test]
    fn dates_compare_and_subtract_in_days() {
        let a = Value::Date(days_from_civil(2005, 3, 1));
        let b = Value::Date(days_from_civil(2004, 12, 31));
        assert!(matches!(arith('-', &a, &b), Ok(Value::Int(60))));
        assert_eq!(
            arith('+', &b, &Value::Int(1)).unwrap().to_string(),
            "date( 2005, 1, 1 )"
        );
        assert_eq!(a.lingo_cmp(&b), Some(std::cmp::Ordering::Greater));
        assert_eq!(
            civil_from_days(days_from_civil(2026, 10, 31)),
            (2026, 10, 31)
        );
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    // Regression: a dialog title built with a VOID part read " <Void>Registre…".
    #[test]
    fn void_concatenates_as_empty_text() {
        assert_eq!(Value::Void.to_text(), "");
        assert_eq!(Value::Void.to_string(), "<Void>");
    }

    #[test]
    fn display_like_lingo() {
        let l = Value::prop_list(vec![
            (Value::sym("a"), Value::Int(1)),
            (Value::sym("b"), Value::str("x")),
        ]);
        assert_eq!(l.to_string(), "[#a: 1, #b: \"x\"]");
        assert_eq!(Value::prop_list(vec![]).to_string(), "[:]");
        assert_eq!(Value::Float(0.5).to_string(), "0.5000");
        assert!(!Value::Int(0).truthy() && !Value::Void.truthy() && Value::Int(-1).truthy());
    }
}
