//! The VM over the real code on the user's discs (found by
//! `discs::for_tests`). Asserts structure and prints counts, never code.

use lingo_vm::value::HostRef;
use lingo_vm::{Host, HostResult, Program, Value, Vm};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn files() -> Option<PathBuf> {
    let port = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    Some(discs::for_tests(&port)?.join("cd1/Program/Files"))
}

#[test]
fn every_opcode_on_the_discs_is_implemented() {
    let Some(dir) = files() else { return };
    let mut missing: BTreeMap<String, usize> = BTreeMap::new();
    let (mut scripts, mut handlers, mut instrs) = (0, 0, 0);
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let l = name.to_ascii_lowercase();
        if name.starts_with("._") || !(l.ends_with(".cxt") || l.ends_with(".dxr")) {
            continue;
        }
        let data = std::fs::read(e.path()).unwrap();
        let movie = rifx::Movie::parse(&data).unwrap();
        let mut p = Program::default();
        scripts += p.add_cast(&movie, 1).unwrap();
        for s in &p.scripts {
            for h in &s.handlers {
                handlers += 1;
                for i in &h.instrs {
                    instrs += 1;
                    if !lingo_vm::vm::supports(i.op.0) {
                        *missing.entry(i.op.name().to_string()).or_default() += 1;
                    }
                }
            }
        }
    }
    eprintln!("{scripts} scripts, {handlers} handlers, {instrs} instructions");
    assert_eq!(scripts, 446);
    assert!(missing.is_empty(), "opcodes not implemented: {missing:?}");
}

/// Answers VOID to everything the player would provide, and records what
/// was asked for.
#[derive(Default)]
struct Recorder {
    calls: BTreeMap<String, usize>,
}

impl Recorder {
    fn note(&mut self, what: String) {
        *self.calls.entry(what).or_default() += 1;
    }
}

impl Host for Recorder {
    fn call(&mut self, _: &mut Vm, name: &str, _: &[Value]) -> HostResult<Option<Value>> {
        self.note(format!("call {name}"));
        Ok(Some(Value::Void))
    }
    fn method(
        &mut self,
        _: &mut Vm,
        t: &HostRef,
        name: &str,
        _: &[Value],
    ) -> HostResult<Option<Value>> {
        self.note(format!("method {}.{name}", Value::Host(t.clone()).ilk()));
        Ok(Some(Value::Void))
    }
    fn get_prop(&mut self, _: &mut Vm, t: &HostRef, name: &str) -> HostResult<Option<Value>> {
        self.note(format!("get {}.{name}", Value::Host(t.clone()).ilk()));
        Ok(Some(Value::Void))
    }
    fn set_prop(&mut self, _: &mut Vm, t: &HostRef, name: &str, _: &Value) -> HostResult<bool> {
        self.note(format!("set {}.{name}", Value::Host(t.clone()).ilk()));
        Ok(true)
    }
    fn the(&mut self, _: &mut Vm, name: &str) -> HostResult<Option<Value>> {
        self.note(format!("the {name}"));
        Ok(Some(Value::Void))
    }
    fn set_the(&mut self, _: &mut Vm, name: &str, _: &Value) -> HostResult<bool> {
        self.note(format!("set the {name}"));
        Ok(true)
    }
}

/// Load Home's cast libraries in score order and send the movie events.
/// Slice 2 turns the recorded calls into real host functions; for now the
/// test only requires the VM itself not to fail on an unknown opcode or a
/// stack error.
#[test]
fn home_movie_events_run_against_a_recording_host() {
    let Some(dir) = files() else { return };
    let mut p = Program::default();
    for (lib, f) in [
        "Home.dxr",
        "HOCode.cxt",
        "HOAssets.cxt",
        "GCode.cxt",
        "GAssets.cxt",
    ]
    .iter()
    .enumerate()
    {
        let data = std::fs::read(dir.join(f)).unwrap();
        p.add_cast(&rifx::Movie::parse(&data).unwrap(), lib as u16 + 1)
            .unwrap();
    }
    let mut vm = Vm::new(p);
    let mut host = Recorder::default();
    for event in ["prepareMovie", "startMovie"] {
        let r = vm.movie_event(&mut host, event, vec![]);
        match &r {
            Ok(n) => eprintln!("{event}: {n} handler(s) ran"),
            Err(e) => eprintln!("{event}: stopped: {e}"),
        }
        if let Err(e) = r {
            assert!(
                !e.message.contains("opcode") && !e.message.contains("stack underflow"),
                "VM-level failure: {e}"
            );
        }
    }
    let mut v: Vec<_> = host.calls.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    eprintln!("{} distinct host requests:", v.len());
    for (k, n) in v.iter().take(40) {
        eprintln!("  {n:5}  {k}");
    }
}
