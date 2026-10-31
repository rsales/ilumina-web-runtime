//! The SQL subset the iLumina scripts send to Valentina (read side):
//!
//! ```text
//! SELECT [DISTINCT] * | item, … FROM table[, table] [WHERE cond]
//!        [GROUP BY col, …] [ORDER BY col [ASC|DESC], …]
//! item := col | table.col | MIN|MAX|COUNT|SUM|AVG ( col | * )
//! cond := cond OR cond | cond AND cond | NOT cond | ( cond )
//!       | operand (= | <> | != | < | <= | > | >= | LIKE) operand
//! ```
//!
//! Strings compare case-insensitively; `LIKE` uses `%` and `_`. A join is
//! the filtered product of the tables (they are small).

use crate::db::{Cell, Database};
use crate::schema::TableDef;
use crate::Error;
use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq)]
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Num(f64),
    Str(String),
    Sym(String),
}

fn tokens(sql: &str) -> Result<Vec<Tok>, Error> {
    let c: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '\'' || ch == '"' {
            let q = ch;
            let mut s = String::new();
            i += 1;
            while i < c.len() {
                if c[i] == q {
                    if c.get(i + 1) == Some(&q) {
                        s.push(q);
                        i += 2;
                        continue;
                    }
                    break;
                }
                s.push(c[i]);
                i += 1;
            }
            i += 1;
            out.push(Tok::Str(s));
        } else if ch.is_ascii_digit()
            || (ch == '-'
                && c.get(i + 1).is_some_and(|d| d.is_ascii_digit())
                && !matches!(out.last(), Some(Tok::Word(_) | Tok::Num(_) | Tok::Str(_))))
        {
            let start = i;
            i += 1;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            let s: String = c[start..i].iter().collect();
            out.push(Tok::Num(
                s.parse()
                    .map_err(|_| Error::Format(format!("bad number {s}")))?,
            ));
        } else if ch.is_alphanumeric() || ch == '_' {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Word(c[start..i].iter().collect()));
        } else {
            let two: String = c[i..(i + 2).min(c.len())].iter().collect();
            if ["<>", "!=", "<=", ">="].contains(&two.as_str()) {
                out.push(Tok::Sym(two));
                i += 2;
            } else {
                out.push(Tok::Sym(ch.to_string()));
                i += 1;
            }
        }
    }
    Ok(out)
}

#[derive(Clone, Debug)]
struct ColRef {
    table: Option<String>,
    name: String,
}

#[derive(Clone, Debug)]
enum Item {
    Star,
    Col(ColRef),
    Agg(String, Option<ColRef>),
}

#[derive(Clone, Debug)]
enum Operand {
    Col(ColRef),
    Lit(Cell),
}

#[derive(Clone, Debug)]
enum Cond {
    Or(Box<Cond>, Box<Cond>),
    And(Box<Cond>, Box<Cond>),
    Not(Box<Cond>),
    Cmp(Operand, String, Operand),
}

struct Select {
    distinct: bool,
    items: Vec<Item>,
    tables: Vec<String>,
    filter: Option<Cond>,
    group: Vec<ColRef>,
    order: Vec<(ColRef, bool)>,
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn kw(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Word(x)) if x.eq_ignore_ascii_case(w))
    }
    fn eat_kw(&mut self, w: &str) -> bool {
        if self.kw(w) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn sym(&self, s: &str) -> bool {
        matches!(self.peek(), Some(Tok::Sym(x)) if x == s)
    }
    fn eat_sym(&mut self, s: &str) -> bool {
        if self.sym(s) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn err<T>(&self, what: &str) -> Result<T, Error> {
        Err(Error::Format(format!(
            "SQL: expected {what} at token {} ({:?})",
            self.i,
            self.peek()
        )))
    }
    fn word(&mut self) -> Result<String, Error> {
        match self.peek().cloned() {
            Some(Tok::Word(w)) => {
                self.i += 1;
                Ok(w)
            }
            _ => self.err("a name"),
        }
    }
    fn col(&mut self) -> Result<ColRef, Error> {
        let a = self.word()?;
        if self.eat_sym(".") {
            Ok(ColRef {
                table: Some(a),
                name: self.word()?,
            })
        } else {
            Ok(ColRef {
                table: None,
                name: a,
            })
        }
    }

    fn select(&mut self) -> Result<Select, Error> {
        if !self.eat_kw("select") {
            return self.err("SELECT");
        }
        let distinct = self.eat_kw("distinct");
        let mut items = Vec::new();
        loop {
            if self.eat_sym("*") {
                items.push(Item::Star);
            } else if ["min", "max", "count", "sum", "avg"]
                .iter()
                .any(|f| self.kw(f))
                && matches!(self.t.get(self.i + 1), Some(Tok::Sym(s)) if s == "(")
            {
                let f = self.word()?.to_ascii_lowercase();
                self.eat_sym("(");
                let arg = if self.eat_sym("*") {
                    None
                } else {
                    Some(self.col()?)
                };
                if !self.eat_sym(")") {
                    return self.err(")");
                }
                items.push(Item::Agg(f, arg));
            } else {
                items.push(Item::Col(self.col()?));
            }
            if !self.eat_sym(",") {
                break;
            }
        }
        if !self.eat_kw("from") {
            return self.err("FROM");
        }
        let mut tables = vec![self.word()?];
        while self.eat_sym(",") {
            tables.push(self.word()?);
        }
        let filter = if self.eat_kw("where") {
            Some(self.or()?)
        } else {
            None
        };
        let mut group = Vec::new();
        if self.eat_kw("group") {
            self.eat_kw("by");
            loop {
                group.push(self.col()?);
                if !self.eat_sym(",") {
                    break;
                }
            }
        }
        let mut order = Vec::new();
        if self.eat_kw("order") {
            self.eat_kw("by");
            loop {
                let c = self.col()?;
                let desc = self.eat_kw("desc");
                if !desc {
                    self.eat_kw("asc");
                }
                order.push((c, desc));
                if !self.eat_sym(",") {
                    break;
                }
            }
        }
        self.eat_sym(";");
        if self.i < self.t.len() {
            return self.err("end of statement");
        }
        Ok(Select {
            distinct,
            items,
            tables,
            filter,
            group,
            order,
        })
    }

    fn or(&mut self) -> Result<Cond, Error> {
        let mut a = self.and()?;
        while self.eat_kw("or") {
            a = Cond::Or(Box::new(a), Box::new(self.and()?));
        }
        Ok(a)
    }
    fn and(&mut self) -> Result<Cond, Error> {
        let mut a = self.not()?;
        while self.eat_kw("and") {
            a = Cond::And(Box::new(a), Box::new(self.not()?));
        }
        Ok(a)
    }
    fn not(&mut self) -> Result<Cond, Error> {
        if self.eat_kw("not") {
            return Ok(Cond::Not(Box::new(self.not()?)));
        }
        if self.eat_sym("(") {
            let c = self.or()?;
            if !self.eat_sym(")") {
                return self.err(")");
            }
            return Ok(c);
        }
        let a = self.operand()?;
        let op = match self.peek().cloned() {
            Some(Tok::Sym(s)) if ["=", "<>", "!=", "<", "<=", ">", ">="].contains(&s.as_str()) => {
                self.i += 1;
                s
            }
            Some(Tok::Word(w)) if w.eq_ignore_ascii_case("like") => {
                self.i += 1;
                "like".into()
            }
            _ => return self.err("a comparison"),
        };
        let b = self.operand()?;
        // Valentina's `… = 'x' no_case`: comparisons are case-insensitive here anyway.
        self.eat_kw("no_case");
        Ok(Cond::Cmp(a, op, b))
    }
    fn operand(&mut self) -> Result<Operand, Error> {
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                self.i += 1;
                Ok(Operand::Lit(if n.fract() == 0.0 {
                    Cell::Int(n as i64)
                } else {
                    Cell::Float(n)
                }))
            }
            Some(Tok::Str(s)) => {
                self.i += 1;
                Ok(Operand::Lit(Cell::Text(s)))
            }
            Some(Tok::Word(_)) => Ok(Operand::Col(self.col()?)),
            _ => self.err("a value"),
        }
    }
}

/// Compare two cells: numbers numerically, text case-insensitively.
pub fn compare(a: &Cell, b: &Cell) -> Ordering {
    match (a, b) {
        (Cell::Null, Cell::Null) => Ordering::Equal,
        (Cell::Null, _) => Ordering::Less,
        (_, Cell::Null) => Ordering::Greater,
        (Cell::Text(x), Cell::Text(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
        _ => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            _ => a.as_text().to_lowercase().cmp(&b.as_text().to_lowercase()),
        },
    }
}

/// SQL `LIKE` with `%` and `_`, case-insensitive.
pub fn like(value: &str, pattern: &str) -> bool {
    let v: Vec<char> = value.to_lowercase().chars().collect();
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    fn m(v: &[char], p: &[char]) -> bool {
        match p.first() {
            None => v.is_empty(),
            Some('%') => m(v, &p[1..]) || (!v.is_empty() && m(&v[1..], p)),
            Some('_') => !v.is_empty() && m(&v[1..], &p[1..]),
            Some(c) => v.first() == Some(c) && m(&v[1..], &p[1..]),
        }
    }
    m(&v, &p)
}

struct Ctx<'a> {
    db: &'a Database,
    tables: Vec<&'a TableDef>,
}

impl Ctx<'_> {
    /// (table index, field) for a column reference.
    fn resolve(&self, c: &ColRef) -> Result<(usize, &crate::schema::FieldDef), Error> {
        for (i, t) in self.tables.iter().enumerate() {
            if c.table
                .as_ref()
                .is_some_and(|n| !n.eq_ignore_ascii_case(&t.name))
            {
                continue;
            }
            if let Some(f) = t.field(&c.name) {
                return Ok((i, f));
            }
        }
        Err(Error::Format(format!("SQL: unknown column {}", c.name)))
    }

    fn value(&self, c: &ColRef, row: &[usize]) -> Result<Cell, Error> {
        let (ti, f) = self.resolve(c)?;
        let col = self.db.column(self.tables[ti], f)?;
        Ok(col.get(row[ti]).cloned().unwrap_or(Cell::Null))
    }

    fn operand(&self, o: &Operand, row: &[usize]) -> Result<Cell, Error> {
        match o {
            Operand::Col(c) => self.value(c, row),
            Operand::Lit(v) => Ok(v.clone()),
        }
    }

    fn test(&self, c: &Cond, row: &[usize]) -> Result<bool, Error> {
        Ok(match c {
            Cond::Or(a, b) => self.test(a, row)? || self.test(b, row)?,
            Cond::And(a, b) => self.test(a, row)? && self.test(b, row)?,
            Cond::Not(a) => !self.test(a, row)?,
            Cond::Cmp(a, op, b) => {
                let (x, y) = (self.operand(a, row)?, self.operand(b, row)?);
                if op == "like" {
                    like(&x.as_text(), &y.as_text())
                } else {
                    let o = compare(&x, &y);
                    match op.as_str() {
                        "=" => o == Ordering::Equal,
                        "<>" | "!=" => o != Ordering::Equal,
                        "<" => o == Ordering::Less,
                        "<=" => o != Ordering::Greater,
                        ">" => o == Ordering::Greater,
                        _ => o != Ordering::Less,
                    }
                }
            }
        })
    }
}

fn aggregate(f: &str, vals: &[Cell]) -> Cell {
    let non_null: Vec<&Cell> = vals.iter().filter(|v| **v != Cell::Null).collect();
    match f {
        "count" => Cell::Int(vals.len() as i64),
        "min" => non_null
            .iter()
            .min_by(|a, b| compare(a, b))
            .map(|c| (*c).clone())
            .unwrap_or(Cell::Null),
        "max" => non_null
            .iter()
            .max_by(|a, b| compare(a, b))
            .map(|c| (*c).clone())
            .unwrap_or(Cell::Null),
        "sum" | "avg" => {
            let nums: Vec<f64> = non_null.iter().filter_map(|c| c.as_f64()).collect();
            if nums.is_empty() {
                return Cell::Null;
            }
            let s: f64 = nums.iter().sum();
            let v = if f == "avg" { s / nums.len() as f64 } else { s };
            if v.fract() == 0.0 {
                Cell::Int(v as i64)
            } else {
                Cell::Float(v)
            }
        }
        _ => Cell::Null,
    }
}

/// Run a SELECT against `db`.
pub fn query(db: &Database, sql: &str) -> Result<ResultSet, Error> {
    let mut p = Parser {
        t: tokens(sql)?,
        i: 0,
    };
    let s = p.select()?;
    let tables: Vec<&TableDef> = s
        .tables
        .iter()
        .map(|n| {
            db.table(n)
                .ok_or(Error::Format(format!("SQL: unknown table {n}")))
        })
        .collect::<Result<_, _>>()?;
    let ctx = Ctx { db, tables };

    // Matching row tuples (one record index per table).
    let counts: Vec<usize> = ctx
        .tables
        .iter()
        .map(|t| db.row_count(t))
        .collect::<Result<_, _>>()?;
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut cur = vec![0usize; counts.len()];
    if counts.iter().all(|&n| n > 0) {
        'outer: loop {
            let keep = match &s.filter {
                Some(c) => ctx.test(c, &cur)?,
                None => true,
            };
            if keep {
                rows.push(cur.clone());
            }
            for k in (0..cur.len()).rev() {
                cur[k] += 1;
                if cur[k] < counts[k] {
                    continue 'outer;
                }
                cur[k] = 0;
            }
            break;
        }
    }

    // Output columns.
    let mut columns = Vec::new();
    let mut exprs: Vec<Item> = Vec::new();
    for it in &s.items {
        match it {
            Item::Star => {
                for t in &ctx.tables {
                    for f in &t.fields {
                        columns.push(f.name.clone());
                        exprs.push(Item::Col(ColRef {
                            table: Some(t.name.clone()),
                            name: f.name.clone(),
                        }));
                    }
                }
            }
            Item::Col(c) => {
                columns.push(ctx.resolve(c)?.1.name.clone());
                exprs.push(it.clone());
            }
            Item::Agg(f, c) => {
                columns.push(match c {
                    Some(c) => format!("{f}({})", c.name),
                    None => format!("{f}(*)"),
                });
                exprs.push(it.clone());
            }
        }
    }

    let has_agg = exprs.iter().any(|e| matches!(e, Item::Agg(..)));
    let mut out: Vec<Vec<Cell>> = Vec::new();
    if has_agg || !s.group.is_empty() {
        // Group rows by the GROUP BY key (one group when there is none).
        let mut groups: Vec<(Vec<Cell>, Vec<Vec<usize>>)> = Vec::new();
        for r in rows {
            let key: Vec<Cell> = s
                .group
                .iter()
                .map(|c| ctx.value(c, &r))
                .collect::<Result<_, _>>()?;
            match groups.iter_mut().find(|g| g.0 == key) {
                Some(g) => g.1.push(r),
                None => groups.push((key, vec![r])),
            }
        }
        if groups.is_empty() && s.group.is_empty() {
            groups.push((Vec::new(), Vec::new()));
        }
        for (_, members) in groups {
            let mut row = Vec::new();
            for e in &exprs {
                row.push(match e {
                    Item::Agg(f, Some(c)) => {
                        let vals: Vec<Cell> = members
                            .iter()
                            .map(|r| ctx.value(c, r))
                            .collect::<Result<_, _>>()?;
                        aggregate(f, &vals)
                    }
                    Item::Agg(f, None) => aggregate(f, &vec![Cell::Int(1); members.len()]),
                    Item::Col(c) => match members.first() {
                        Some(r) => ctx.value(c, r)?,
                        None => Cell::Null,
                    },
                    Item::Star => unreachable!(),
                });
            }
            out.push(row);
        }
    } else {
        if !s.order.is_empty() {
            let mut keyed: Vec<(Vec<Cell>, Vec<usize>)> = rows
                .into_iter()
                .map(|r| {
                    Ok((
                        s.order
                            .iter()
                            .map(|(c, _)| ctx.value(c, &r))
                            .collect::<Result<Vec<_>, Error>>()?,
                        r,
                    ))
                })
                .collect::<Result<_, Error>>()?;
            keyed.sort_by(|a, b| {
                for (k, (_, desc)) in s.order.iter().enumerate() {
                    let o = compare(&a.0[k], &b.0[k]);
                    if o != Ordering::Equal {
                        return if *desc { o.reverse() } else { o };
                    }
                }
                Ordering::Equal
            });
            rows = keyed.into_iter().map(|k| k.1).collect();
        }
        for r in rows {
            let row: Vec<Cell> = exprs
                .iter()
                .map(|e| match e {
                    Item::Col(c) => ctx.value(c, &r),
                    _ => unreachable!(),
                })
                .collect::<Result<_, _>>()?;
            if s.distinct && out.contains(&row) {
                continue;
            }
            out.push(row);
        }
    }
    Ok(ResultSet { columns, rows: out })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_and_like() {
        let t = tokens("select ID,TITLE from COMP where CATEGORY=-3 and NAME like 'Gê%'").unwrap();
        assert!(t.contains(&Tok::Num(-3.0)));
        assert!(t.contains(&Tok::Str("Gê%".into())));
        assert!(like("Gênesis", "gê%") && like("abc", "a_c") && !like("abc", "b%"));
        assert_eq!(
            compare(&Cell::Text("b".into()), &Cell::Text("A".into())),
            Ordering::Greater
        );
        assert_eq!(
            compare(&Cell::Int(10), &Cell::Text("9".into())),
            Ordering::Greater
        );
    }

    #[test]
    fn parser_accepts_the_shapes_the_scripts_use() {
        for sql in [
            "select ID,TITLE,SEQ from COMP where CATEGORY=3 and BOOK = 1 order by SEQ",
            "select min(ID) from OVERVIEWS where CATEGORY=2",
            "select distinct PHOTO from MEDIAWINDOW where MODULE=1 and ID >= 5 and ID <= 9",
            "select SCRIPTS.ID,TITLE,TEXT from SCRIPTS,ANIMATIONS where SCRIPTS.ID=ANIMATIONS.ID",
            "select * from MEDIAWINDOW where MODULE=1 and ID=2",
            "select TOPIC,TITLE from VERSEFINDER where TOPIC like 'a%' or (ID > 3 and not ID = 5)",
            "select NAME, count(*) from T group by NAME order by NAME desc",
            // Regression: Valentina's case-insensitive modifier (StringTable lookups).
            "select MSG from STRINGTABLE where WORD='x' no_case",
        ] {
            let mut p = Parser {
                t: tokens(sql).unwrap(),
                i: 0,
            };
            p.select().unwrap_or_else(|e| panic!("{sql}: {e}"));
        }
    }
}
