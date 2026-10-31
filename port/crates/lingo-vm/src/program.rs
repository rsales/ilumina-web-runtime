//! The loaded code: every script of every cast library, with handlers looked
//! up by name and bytecode decoded once.

use rifx::bytecode::{disassemble, Instr};
use rifx::cast::{members, ScriptType};
use rifx::lingo::{contexts, Context, Literal};
use rifx::Movie;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptKind {
    Movie,
    Score,
    Parent,
    Other,
}

pub struct HandlerCode {
    pub name: String,
    pub args: Vec<String>,
    pub locals: Vec<String>,
    pub instrs: Vec<Instr>,
}

impl HandlerCode {
    /// Index of the instruction at byte `offset` (jump targets).
    pub fn index_of(&self, offset: usize) -> Option<usize> {
        self.instrs.binary_search_by_key(&offset, |i| i.offset).ok()
    }
}

pub struct ScriptCode {
    /// Cast library number (1-based, as in Lingo).
    pub lib: u16,
    pub member: u32,
    pub name: String,
    pub kind: ScriptKind,
    pub props: Vec<String>,
    pub handlers: Vec<HandlerCode>,
    by_name: HashMap<String, usize>,
    pub literals: Vec<Literal>,
    pub ctx: Rc<Context>,
}

impl ScriptCode {
    pub fn handler(&self, name: &str) -> Option<usize> {
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }

    /// A name from the script's context (`Lnam`).
    pub fn name_of(&self, id: i32) -> &str {
        self.ctx.name(id)
    }
}

#[derive(Default)]
pub struct Program {
    pub scripts: Vec<ScriptCode>,
}

impl Program {
    /// Add every script in `movie` (a `.dxr` or `.cxt`) as cast library `lib`.
    pub fn add_cast(&mut self, movie: &Movie, lib: u16) -> Result<usize, rifx::Error> {
        let all = members(movie)?;
        let mut added = 0;
        for ctx in contexts(movie)? {
            let ctx = Rc::new(ctx);
            for s in &ctx.scripts {
                let m = all.iter().flatten().find(|m| m.script_id == s.index);
                let kind = match m.and_then(|m| m.script_type) {
                    Some(ScriptType::Movie) => ScriptKind::Movie,
                    Some(ScriptType::Score) => ScriptKind::Score,
                    Some(ScriptType::Parent) => ScriptKind::Parent,
                    _ => ScriptKind::Other,
                };
                let mut handlers = Vec::new();
                let mut by_name = HashMap::new();
                for h in &s.handlers {
                    let name = ctx.name(h.name_id).to_string();
                    by_name
                        .entry(name.to_ascii_lowercase())
                        .or_insert(handlers.len());
                    handlers.push(HandlerCode {
                        name,
                        args: h
                            .argument_ids
                            .iter()
                            .map(|&i| ctx.name(i).to_string())
                            .collect(),
                        locals: h
                            .local_ids
                            .iter()
                            .map(|&i| ctx.name(i).to_string())
                            .collect(),
                        instrs: disassemble(&h.bytecode)?,
                    });
                }
                self.scripts.push(ScriptCode {
                    lib,
                    member: m.map(|m| m.number).unwrap_or(0),
                    name: m.map(|m| m.name.clone()).unwrap_or_default(),
                    kind,
                    props: s
                        .property_ids
                        .iter()
                        .map(|&i| ctx.name(i).to_string())
                        .collect(),
                    handlers,
                    by_name,
                    literals: s.literals.clone(),
                    ctx: ctx.clone(),
                });
                added += 1;
            }
        }
        Ok(added)
    }

    /// Add a script built in code (tests).
    pub fn add_script(&mut self, script: ScriptCode) -> usize {
        self.scripts.push(script);
        self.scripts.len() - 1
    }

    /// `script "name"`: a script member by name (case-insensitive).
    pub fn script_named(&self, name: &str) -> Option<usize> {
        self.scripts
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Movie-script handler by name, searching cast libraries in order.
    pub fn movie_handler(&self, name: &str) -> Option<(usize, usize)> {
        self.scripts
            .iter()
            .enumerate()
            .filter(|(_, s)| s.kind == ScriptKind::Movie)
            .find_map(|(i, s)| s.handler(name).map(|h| (i, h)))
    }

    /// Every movie script that defines `name` (events go to all of them).
    pub fn movie_handlers(&self, name: &str) -> Vec<(usize, usize)> {
        self.scripts
            .iter()
            .enumerate()
            .filter(|(_, s)| s.kind == ScriptKind::Movie)
            .filter_map(|(i, s)| s.handler(name).map(|h| (i, h)))
            .collect()
    }
}

/// Build a script from a context and decoded handlers (used by tests to run
/// hand-assembled bytecode).
pub fn script_from_parts(
    name: &str,
    kind: ScriptKind,
    ctx: Context,
    props: Vec<String>,
    handlers: Vec<HandlerCode>,
    literals: Vec<Literal>,
) -> ScriptCode {
    let by_name = handlers
        .iter()
        .enumerate()
        .map(|(i, h)| (h.name.to_ascii_lowercase(), i))
        .collect();
    ScriptCode {
        lib: 1,
        member: 0,
        name: name.to_string(),
        kind,
        props,
        handlers,
        by_name,
        literals,
        ctx: Rc::new(ctx),
    }
}
