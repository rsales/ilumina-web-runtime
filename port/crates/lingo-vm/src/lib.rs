//! Lingo VM for the iLumina port (ADR 0008, milestone M2).
//!
//! Runs the original Lingo bytecode from the movies' casts. Pure Lingo
//! (values, lists, strings, chunks, control flow, handlers, parent scripts)
//! lives here; everything that touches the player goes through [`Host`].
//! No I/O, so it builds the same for native and `wasm32`.

pub mod builtins;
pub mod chunk;
pub mod doeval;
pub mod literal;
pub mod program;
pub mod value;
pub mod vm;

pub use program::{Program, ScriptKind};
pub use value::{HostRef, Value};
pub use vm::{Host, HostResult, LingoError, NoHost, Vm};

#[cfg(test)]
mod tests {
    use super::*;
    use program::{script_from_parts, HandlerCode};
    use rifx::bytecode::disassemble;
    use rifx::lingo::{Context, Literal};

    /// Assemble `(opcode, arg)` pairs the way Director encodes them.
    fn asm(code: &[(u8, i32)]) -> Vec<u8> {
        let mut out = Vec::new();
        for &(op, arg) in code {
            if op < 0x40 {
                out.push(op);
            } else if (0..=0xff).contains(&arg) && !(op == 0x41 && arg > 0x7f) {
                out.push(op);
                out.push(arg as u8);
            } else if (-0x8000..=0x7fff).contains(&arg) {
                out.push(0x80 | (op & 0x3f));
                out.extend_from_slice(&(arg as i16).to_be_bytes());
            } else {
                out.push(0xc0 | (op & 0x3f));
                out.extend_from_slice(&arg.to_be_bytes());
            }
        }
        out
    }

    struct H<'a> {
        name: &'a str,
        args: &'a [&'a str],
        locals: usize,
        code: Vec<(u8, i32)>,
    }

    /// A program with one script holding `handlers`; `names` is the Lnam.
    fn program(
        kind: ScriptKind,
        names: &[&str],
        props: &[&str],
        literals: Vec<Literal>,
        handlers: Vec<H>,
    ) -> Program {
        let ctx = Context {
            section: 0,
            names: names.iter().map(|s| s.to_string()).collect(),
            scripts: Vec::new(),
        };
        let hs = handlers
            .into_iter()
            .map(|h| HandlerCode {
                name: h.name.to_string(),
                args: h.args.iter().map(|s| s.to_string()).collect(),
                locals: (0..h.locals).map(|i| format!("l{i}")).collect(),
                instrs: disassemble(&asm(&h.code)).unwrap(),
            })
            .collect();
        let mut p = Program::default();
        p.add_script(script_from_parts(
            "Main",
            kind,
            ctx,
            props.iter().map(|s| s.to_string()).collect(),
            hs,
            literals,
        ));
        p
    }

    fn run(p: Program, handler: &str, args: Vec<Value>) -> Result<Value, LingoError> {
        let mut vm = Vm::new(p);
        vm.call(&mut NoHost, handler, args)
    }

    // Opcodes
    const RET: u8 = 0x01;
    const PUSHZERO: u8 = 0x03;
    const MUL: u8 = 0x04;
    const ADD: u8 = 0x05;
    const SUB: u8 = 0x06;
    const DIV: u8 = 0x07;
    const JOIN: u8 = 0x0a;
    const JOINPAD: u8 = 0x0b;
    const LT: u8 = 0x0c;
    const EQ: u8 = 0x0f;
    const NOT: u8 = 0x14;
    const CONTAINS: u8 = 0x15;
    const GETCHUNK: u8 = 0x17;
    const PUSHLIST: u8 = 0x1e;
    const PUSHPROPLIST: u8 = 0x1f;
    const PUSHINT8: u8 = 0x41;
    const ARGLISTNORET: u8 = 0x42;
    const ARGLIST: u8 = 0x43;
    const PUSHCONS: u8 = 0x44;
    const PUSHSYMB: u8 = 0x45;
    const GETGLOBAL: u8 = 0x49;
    const GETPROP: u8 = 0x4a;
    const GETPARAM: u8 = 0x4b;
    const GETLOCAL: u8 = 0x4c;
    const SETGLOBAL: u8 = 0x4f;
    const SETPROP: u8 = 0x50;
    const SETLOCAL: u8 = 0x52;
    const JMP: u8 = 0x53;
    const ENDREPEAT: u8 = 0x54;
    const JMPIFZ: u8 = 0x55;
    const LOCALCALL: u8 = 0x56;
    const EXTCALL: u8 = 0x57;
    const PUT: u8 = 0x59;
    const GETOBJPROP: u8 = 0x61;
    const SETOBJPROP: u8 = 0x62;
    const PEEK: u8 = 0x64;
    const POP: u8 = 0x65;
    const OBJCALL: u8 = 0x67;
    const PUSHINT16: u8 = 0x6e;
    const PUSHFLOAT32: u8 = 0x71;

    #[test]
    fn arithmetic_and_return() {
        // return (2 + 3) * 4 - 10 / 3
        let p = program(
            ScriptKind::Movie,
            &["f"],
            &[],
            vec![],
            vec![H {
                name: "f",
                args: &[],
                locals: 0,
                code: vec![
                    (PUSHINT8, 2),
                    (PUSHINT8, 3),
                    (ADD, 0),
                    (PUSHINT8, 4),
                    (MUL, 0),
                    (PUSHINT8, 10),
                    (PUSHINT8, 3),
                    (DIV, 0),
                    (SUB, 0),
                    (RET, 0),
                ],
            }],
        );
        assert_eq!(run(p, "f", vec![]).unwrap().to_int(), 17);
    }

    #[test]
    fn do_runs_calls_built_from_strings() {
        // Regression: Poster.stepFrame does `do "PostCallback(\"…\")"`;
        // `do` was not implemented (Home's playback stopped).
        let p = program(
            ScriptKind::Movie,
            &["cb", "g"],
            &[],
            vec![],
            vec![H {
                name: "cb",
                args: &["a"],
                locals: 0,
                code: vec![(GETPARAM, 0), (SETGLOBAL, 1), (RET, 0)],
            }],
        );
        let mut vm = Vm::new(p);
        vm.do_text(&mut NoHost, "cb(\"ok\" & \"!\")").unwrap();
        assert_eq!(vm.global("g").to_text(), "ok!");
        vm.do_text(&mut NoHost, "cb 3 + 4").unwrap();
        assert_eq!(vm.global("g").to_int(), 7);
        vm.do_text(&mut NoHost, "global g\rg = [1, #a]").unwrap();
        assert_eq!(vm.global("g").to_string(), "[1, #a]");
        // Without `global`, an assignment stays local to the do.
        vm.do_text(&mut NoHost, "g = 5").unwrap();
        assert_eq!(vm.global("g").to_string(), "[1, #a]");
        vm.do_text(&mut NoHost, "").unwrap();
        assert!(vm.do_text(&mut NoHost, "cb(1) ]").is_err());
    }

    #[test]
    fn param_count_and_param() {
        // Regression: Utilities.sprintf uses `the paramCount` / param(n).
        // on f a, b, c: return the paramCount * 10 + param(2)
        const THEBUILTIN: u8 = 0x66;
        let p = program(
            ScriptKind::Movie,
            &["f", "paramCount", "param"],
            &[],
            vec![],
            vec![H {
                name: "f",
                args: &["a", "b", "c"],
                locals: 0,
                code: vec![
                    (ARGLIST, 0),
                    (THEBUILTIN, 1),
                    (PUSHINT8, 10),
                    (MUL, 0),
                    (PUSHINT8, 2),
                    (ARGLIST, 1),
                    (EXTCALL, 2),
                    (ADD, 0),
                    (RET, 0),
                ],
            }],
        );
        assert_eq!(
            run(p, "f", vec![Value::Int(5), Value::Int(7)])
                .unwrap()
                .to_int(),
            27
        );
    }

    #[test]
    fn repeat_loop_with_locals_and_params() {
        // on sum n: s = 0; i = 1; repeat while i <= n … ; return s
        // encoded as: loop: getlocal i; getparam n; lt-or-eq…  (uses lt with n+1)
        let code = vec![
            (PUSHZERO, 0),
            (SETLOCAL, 0), // 0,1: s = 0
            (PUSHINT8, 1),
            (SETLOCAL, 1), // 3,5: i = 1
            (GETLOCAL, 1),
            (GETPARAM, 0),
            (PUSHINT8, 1),
            (ADD, 0),
            (LT, 0),      // 7..14: i < n+1
            (JMPIFZ, 17), // 15: exit -> 32
            (GETLOCAL, 0),
            (GETLOCAL, 1),
            (ADD, 0),
            (SETLOCAL, 0), // 17..23
            (GETLOCAL, 1),
            (PUSHINT8, 1),
            (ADD, 0),
            (SETLOCAL, 1),   // 24..30
            (ENDREPEAT, 25), // 31 -> 7 (… see offsets)
            (GETLOCAL, 0),
            (RET, 0),
        ];
        // Compute offsets to make the jumps exact.
        let bytes = asm(&code);
        let ins = disassemble(&bytes).unwrap();
        let off = |i: usize| ins[i].offset as i32;
        let mut code = code;
        code[9].1 = off(19) - off(9); // jmpifz -> getlocal 0 after the loop
        code[18].1 = off(18) - off(4); // endrepeat -> loop head (getlocal i)
        let p = program(
            ScriptKind::Movie,
            &["sum", "n"],
            &[],
            vec![],
            vec![H {
                name: "sum",
                args: &["n"],
                locals: 2,
                code,
            }],
        );
        assert_eq!(run(p, "sum", vec![Value::Int(10)]).unwrap().to_int(), 55);
    }

    #[test]
    fn strings_literals_symbols_and_chunks() {
        let lits = vec![
            Literal::String("Hello".into()),
            Literal::String("world".into()),
            Literal::String("a,b,c".into()),
        ];
        let p = program(
            ScriptKind::Movie,
            &["f", "g", "h"],
            &[],
            lits,
            vec![
                H {
                    name: "f",
                    args: &[],
                    locals: 0,
                    code: vec![(PUSHCONS, 0), (PUSHCONS, 1), (JOINPAD, 0), (RET, 0)],
                },
                H {
                    name: "g",
                    args: &[],
                    locals: 0,
                    // item 2 of "a,b,c": chars(0,0) words(0,0) items(2,0) lines(0,0), string
                    code: vec![
                        (PUSHZERO, 0),
                        (PUSHZERO, 0),
                        (PUSHZERO, 0),
                        (PUSHZERO, 0),
                        (PUSHINT8, 2),
                        (PUSHZERO, 0),
                        (PUSHZERO, 0),
                        (PUSHZERO, 0),
                        (PUSHCONS, 2),
                        (GETCHUNK, 0),
                        (RET, 0),
                    ],
                },
                H {
                    name: "h",
                    args: &[],
                    locals: 0,
                    code: vec![
                        (PUSHCONS, 0),
                        (PUSHCONS, 1),
                        (JOIN, 0),
                        (PUSHCONS, 0),
                        (CONTAINS, 0),
                        (PUSHSYMB, 0),
                        (PUSHSYMB, 0),
                        (EQ, 0),
                        (JOIN, 0),
                        (RET, 0),
                    ],
                },
            ],
        );
        let mut vm = Vm::new(p);
        assert_eq!(
            vm.call(&mut NoHost, "f", vec![]).unwrap().to_text(),
            "Hello world"
        );
        assert_eq!(vm.call(&mut NoHost, "g", vec![]).unwrap().to_text(), "b");
        assert_eq!(vm.call(&mut NoHost, "h", vec![]).unwrap().to_text(), "11");
    }

    #[test]
    fn lists_proplists_and_dot_properties() {
        let p = program(
            ScriptKind::Movie,
            &["f", "count", "add", "a"],
            &[],
            vec![],
            vec![H {
                name: "f",
                args: &[],
                locals: 2,
                code: vec![
                    (PUSHINT8, 1),
                    (PUSHINT8, 2),
                    (ARGLIST, 2),
                    (PUSHLIST, 0),
                    (SETLOCAL, 0), // l = [1, 2]
                    (GETLOCAL, 0),
                    (PUSHINT8, 3),
                    (ARGLISTNORET, 2),
                    (OBJCALL, 2), // l.add(3)
                    (PUSHSYMB, 3),
                    (PUSHINT8, 9),
                    (ARGLIST, 2),
                    (PUSHPROPLIST, 0),
                    (SETLOCAL, 1), // p = [#a: 9]
                    (GETLOCAL, 1),
                    (PUSHINT8, 5),
                    (SETOBJPROP, 3), // p.a = 5
                    (GETLOCAL, 0),
                    (GETOBJPROP, 1), // l.count
                    (GETLOCAL, 1),
                    (GETOBJPROP, 3),
                    (ADD, 0),
                    (RET, 0), // + p.a
                ],
            }],
        );
        assert_eq!(run(p, "f", vec![]).unwrap().to_int(), 8);
    }

    #[test]
    fn globals_extcall_localcall_peek_pop() {
        let p = program(
            ScriptKind::Movie,
            &["f", "g", "gCount", "double"],
            &[],
            vec![],
            vec![
                H {
                    name: "f",
                    args: &[],
                    locals: 0,
                    code: vec![
                        (PUSHINT8, 21),
                        (SETGLOBAL, 2),
                        (GETGLOBAL, 2),
                        (ARGLIST, 1),
                        (EXTCALL, 3), // double(gCount) via extcall
                        (PUSHINT8, 1),
                        (ARGLIST, 1),
                        (LOCALCALL, 1), // g(1) via localcall
                        (ADD, 0),
                        (PEEK, 0),
                        (POP, 1),
                        (RET, 0),
                    ],
                },
                H {
                    name: "g",
                    args: &["x"],
                    locals: 0,
                    code: vec![(GETPARAM, 0), (NOT, 0), (RET, 0)],
                },
                H {
                    name: "double",
                    args: &["x"],
                    locals: 0,
                    code: vec![(GETPARAM, 0), (GETPARAM, 0), (ADD, 0), (RET, 0)],
                },
            ],
        );
        let mut vm = Vm::new(p);
        assert_eq!(vm.call(&mut NoHost, "f", vec![]).unwrap().to_int(), 42);
        assert_eq!(vm.global("GCOUNT").to_int(), 21);
    }

    #[test]
    fn parent_script_new_properties_and_ancestor() {
        // Parent script "Counter": property pCount, ancestor
        //   on new me: pCount = 10; return me
        //   on bump me, n: pCount = pCount + n; return pCount
        let mut p = program(
            ScriptKind::Movie,
            &["make", "Counter", "new", "bump", "pCount", "script"],
            &[],
            vec![Literal::String("Counter".into())],
            vec![H {
                name: "make",
                args: &[],
                locals: 1,
                code: vec![
                    (PUSHCONS, 0),
                    (ARGLIST, 1),
                    (EXTCALL, 5),
                    (ARGLIST, 1),
                    (EXTCALL, 2),
                    (SETLOCAL, 0), // o = new(script "Counter")
                    (GETLOCAL, 0),
                    (PUSHINT8, 5),
                    (ARGLIST, 2),
                    (OBJCALL, 3), // o.bump(5)
                    (RET, 0),
                ],
            }],
        );
        let ctx = Context {
            section: 0,
            names: vec![
                "new".into(),
                "bump".into(),
                "pCount".into(),
                "me".into(),
                "n".into(),
            ],
            scripts: vec![],
        };
        let h = |name: &str, args: &[&str], code: Vec<(u8, i32)>| HandlerCode {
            name: name.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            locals: vec![],
            instrs: disassemble(&asm(&code)).unwrap(),
        };
        p.add_script(script_from_parts(
            "Counter",
            ScriptKind::Parent,
            ctx,
            vec!["pCount".into(), "ancestor".into()],
            vec![
                h(
                    "new",
                    &["me"],
                    vec![(PUSHINT8, 10), (SETPROP, 2), (GETPARAM, 0), (RET, 0)],
                ),
                h(
                    "bump",
                    &["me", "n"],
                    vec![
                        (GETPROP, 2),
                        (GETPARAM, 1),
                        (ADD, 0),
                        (SETPROP, 2),
                        (GETPROP, 2),
                        (RET, 0),
                    ],
                ),
            ],
            vec![],
        ));
        assert_eq!(run(p, "make", vec![]).unwrap().to_int(), 15);
    }

    #[test]
    fn put_after_local_and_jumps() {
        // s = "a"; put "b" after s; if s = "ab" then return 1 else return 2
        let lits = vec![
            Literal::String("a".into()),
            Literal::String("b".into()),
            Literal::String("ab".into()),
        ];
        let code = vec![
            (PUSHCONS, 0),
            (SETLOCAL, 0),
            (PUSHCONS, 1),
            (PUSHINT8, 0),
            (PUT, 0x25),
            (GETLOCAL, 0),
            (PUSHCONS, 2),
            (EQ, 0),
            (JMPIFZ, 0),
            (PUSHINT8, 1),
            (RET, 0),
            (PUSHINT8, 2),
            (RET, 0),
        ];
        let ins = disassemble(&asm(&code)).unwrap();
        let mut code = code;
        code[8].1 = (ins[11].offset - ins[8].offset) as i32;
        let p = program(
            ScriptKind::Movie,
            &["f"],
            &[],
            lits,
            vec![H {
                name: "f",
                args: &[],
                locals: 1,
                code,
            }],
        );
        assert_eq!(run(p, "f", vec![]).unwrap().to_int(), 1);
    }

    // Real pattern from the discs: `s.word[1].delete()` then
    // `s.char[1].delete()` on a local, and `the last item of s`. Deleting a
    // word leaves the space after it (hence the char delete in the original).
    #[test]
    fn chunk_refs_delete_and_last_chunk() {
        const PUSHCHUNKVARREF: u8 = 0x6d;
        const GET: u8 = 0x5c;
        let lits = vec![
            Literal::String("ab cd ef".into()),
            Literal::String("x,y,z".into()),
        ];
        let names = &["f", "word", "getPropRef", "delete", "char", "g"];
        let code = vec![
            (PUSHCONS, 0),
            (SETLOCAL, 0),
            (PUSHZERO, 0),
            (PUSHCHUNKVARREF, 5),
            (PUSHSYMB, 1),
            (PUSHINT8, 1),
            (ARGLIST, 3),
            (OBJCALL, 2),
            (ARGLISTNORET, 1),
            (OBJCALL, 3),
            (PUSHZERO, 0),
            (PUSHCHUNKVARREF, 5),
            (PUSHSYMB, 4),
            (PUSHINT8, 1),
            (ARGLIST, 3),
            (OBJCALL, 2),
            (ARGLISTNORET, 1),
            (OBJCALL, 3),
            (GETLOCAL, 0),
            (RET, 0),
        ];
        let last = vec![(PUSHCONS, 1), (PUSHINT8, 14), (GET, 0), (RET, 0)];
        let p = program(
            ScriptKind::Movie,
            names,
            &[],
            lits,
            vec![
                H {
                    name: "f",
                    args: &[],
                    locals: 1,
                    code,
                },
                H {
                    name: "g",
                    args: &[],
                    locals: 0,
                    code: last,
                },
            ],
        );
        let mut vm = Vm::new(p);
        assert_eq!(
            vm.call(&mut NoHost, "f", vec![]).unwrap().to_text(),
            "cd ef"
        );
        assert_eq!(vm.call(&mut NoHost, "g", vec![]).unwrap().to_text(), "z");
    }

    // Regression: `return x` is `extcall return` (1,715 times on the discs),
    // often not followed by `ret`; it was sent to the host and execution
    // carried on into the else branch.
    #[test]
    fn extcall_return_leaves_the_handler() {
        // if p then return 1; return 2
        let code = vec![
            (GETPARAM, 0),
            (JMPIFZ, 0),
            (PUSHINT8, 1),
            (ARGLISTNORET, 1),
            (EXTCALL, 1),
            (PUSHINT8, 2),
            (ARGLISTNORET, 1),
            (EXTCALL, 1),
            (RET, 0),
        ];
        let ins = disassemble(&asm(&code)).unwrap();
        let mut code = code;
        code[1].1 = (ins[5].offset - ins[1].offset) as i32;
        let p = program(
            ScriptKind::Movie,
            &["f", "return"],
            &[],
            vec![],
            vec![H {
                name: "f",
                args: &["p"],
                locals: 0,
                code,
            }],
        );
        let mut vm = Vm::new(p);
        assert_eq!(
            vm.call(&mut NoHost, "f", vec![Value::Int(1)])
                .unwrap()
                .to_int(),
            1
        );
        assert_eq!(
            vm.call(&mut NoHost, "f", vec![Value::Int(0)])
                .unwrap()
                .to_int(),
            2
        );
    }

    // Regression: an error inside a `new` handler reached through the
    // `new()` built-in was turned into text and its trace printed twice.
    #[test]
    fn errors_through_new_keep_one_trace() {
        let mut p = program(
            ScriptKind::Movie,
            &["make", "new", "script"],
            &[],
            vec![Literal::String("Obj".into())],
            vec![H {
                name: "make",
                args: &[],
                locals: 0,
                code: vec![
                    (PUSHCONS, 0),
                    (ARGLIST, 1),
                    (EXTCALL, 2),
                    (ARGLIST, 1),
                    (EXTCALL, 1),
                    (RET, 0),
                ],
            }],
        );
        let ctx = Context {
            section: 0,
            names: vec!["new".into(), "nope".into()],
            scripts: vec![],
        };
        p.add_script(script_from_parts(
            "Obj",
            ScriptKind::Parent,
            ctx,
            vec![],
            vec![HandlerCode {
                name: "new".into(),
                args: vec!["me".into()],
                locals: vec![],
                instrs: disassemble(&asm(&[(ARGLISTNORET, 0), (EXTCALL, 1), (RET, 0)])).unwrap(),
            }],
            vec![],
        ));
        let e = run(p, "make", vec![]).unwrap_err();
        assert!(e.message.contains("#nope"), "{e}");
        assert_eq!(e.trace.len(), 2, "{:?}", e.trace);
        assert!(
            e.trace[0].starts_with("new ") && e.trace[1].starts_with("make "),
            "{:?}",
            e.trace
        );
    }

    // Regression: `(i - 1) mod keyLen` with an empty key (fresh profile)
    // stopped the startup with "division by zero".
    #[test]
    fn integer_division_by_zero_warns_and_continues() {
        const MOD: u8 = 0x08;
        let p = program(
            ScriptKind::Movie,
            &["f"],
            &[],
            vec![],
            vec![H {
                name: "f",
                args: &[],
                locals: 0,
                code: vec![
                    (PUSHINT8, 7),
                    (PUSHZERO, 0),
                    (MOD, 0),
                    (PUSHINT8, 7),
                    (PUSHZERO, 0),
                    (DIV, 0),
                    (ADD, 0),
                    (RET, 0),
                ],
            }],
        );
        let mut vm = Vm::new(p);
        assert_eq!(vm.call(&mut NoHost, "f", vec![]).unwrap().to_int(), 7);
        assert_eq!(vm.output.len(), 2);
    }

    // Regression: `GetRecordAsPropList(hRs)` inside a handler of the same name
    // compiles to `localcall`; with a V4MD cursor as first argument it must
    // reach the cursor's method, not recurse into the local handler.
    #[test]
    fn localcall_on_a_host_object_prefers_its_method() {
        struct Cursors;
        impl Host for Cursors {
            fn method(
                &mut self,
                _: &mut Vm,
                _: &HostRef,
                name: &str,
                _: &[Value],
            ) -> HostResult<Option<Value>> {
                Ok((name == "GetRecord").then(|| Value::str("from host")))
            }
        }
        let p = program(
            ScriptKind::Movie,
            &["GetRecord", "x"],
            &[],
            vec![],
            vec![H {
                name: "GetRecord",
                args: &["x"],
                locals: 0,
                code: vec![(GETPARAM, 0), (ARGLIST, 1), (LOCALCALL, 0), (RET, 0)],
            }],
        );
        let mut vm = Vm::new(p);
        let obj = Value::Host(HostRef::Object {
            kind: "VCursor".into(),
            id: 1,
        });
        assert_eq!(
            vm.call(&mut Cursors, "GetRecord", vec![obj])
                .unwrap()
                .to_text(),
            "from host"
        );
    }

    // Regression: a loop that re-enters the VM through the host (a dialog
    // shown again and again) reset the instruction budget on every
    // re-entry and never stopped.
    #[test]
    fn reentrant_calls_share_the_budget() {
        struct Reenter;
        impl Host for Reenter {
            fn call(&mut self, vm: &mut Vm, name: &str, _: &[Value]) -> HostResult<Option<Value>> {
                if name == "show" {
                    return vm.call(self, "cb", vec![]).map(Some).map_err(|e| e.message);
                }
                Ok(None)
            }
        }
        // f: repeat forever: show()   (show re-enters and runs cb)
        let p = program(
            ScriptKind::Movie,
            &["f", "show", "cb"],
            &[],
            vec![],
            vec![
                H {
                    name: "f",
                    args: &[],
                    locals: 0,
                    code: vec![(ARGLISTNORET, 0), (EXTCALL, 1), (ENDREPEAT, 4)],
                },
                H {
                    name: "cb",
                    args: &[],
                    locals: 0,
                    code: vec![(PUSHINT8, 1), (RET, 0)],
                },
            ],
        );
        let mut vm = Vm::new(p);
        vm.step_limit = 10_000;
        let e = vm.call(&mut Reenter, "f", vec![]).unwrap_err();
        assert!(e.message.contains("budget"), "{e}");
    }

    #[test]
    fn wide_ints_floats_and_jmp() {
        let code = vec![
            (JMP, 0),
            (PUSHINT8, 99),
            (RET, 0),
            (PUSHINT16, 1000),
            (PUSHFLOAT32, 0.5f32.to_bits() as i32),
            (ADD, 0),
            (RET, 0),
        ];
        let ins = disassemble(&asm(&code)).unwrap();
        let mut code = code;
        code[0].1 = (ins[3].offset - ins[0].offset) as i32;
        let p = program(
            ScriptKind::Movie,
            &["f"],
            &[],
            vec![],
            vec![H {
                name: "f",
                args: &[],
                locals: 0,
                code,
            }],
        );
        assert_eq!(run(p, "f", vec![]).unwrap().to_string(), "1000.5000");
    }

    #[test]
    fn missing_handler_reports_name_and_trace() {
        let p = program(
            ScriptKind::Movie,
            &["f", "goToFrame"],
            &[],
            vec![],
            vec![H {
                name: "f",
                args: &[],
                locals: 0,
                code: vec![(PUSHINT8, 1), (ARGLISTNORET, 1), (EXTCALL, 1), (RET, 0)],
            }],
        );
        let e = run(p, "f", vec![]).unwrap_err();
        assert!(e.message.contains("#goToFrame"), "{e}");
        assert!(e.trace[0].starts_with("f "), "{:?}", e.trace);
    }

    #[test]
    fn infinite_loop_hits_the_budget() {
        let p = program(
            ScriptKind::Movie,
            &["f"],
            &[],
            vec![],
            vec![H {
                name: "f",
                args: &[],
                locals: 0,
                code: vec![(PUSHINT8, 1), (POP, 1), (ENDREPEAT, 4)],
            }],
        );
        let mut vm = Vm::new(p);
        vm.step_limit = 10_000;
        assert!(vm
            .call(&mut NoHost, "f", vec![])
            .unwrap_err()
            .message
            .contains("budget"));
    }

    #[test]
    fn prop_list_ilk_unless_a_key() {
        let mut vm = Vm::new(Program::default());
        let pl = Value::prop_list(vec![(Value::sym("text"), Value::str("x"))]);
        let ilk = vm.get_property(&mut vm::NoHost, &pl, "ilk").unwrap();
        assert!(ilk.lingo_eq(&Value::sym("propList")));
        let own = Value::prop_list(vec![(Value::sym("ilk"), Value::Int(3))]);
        assert_eq!(
            vm.get_property(&mut vm::NoHost, &own, "ilk")
                .unwrap()
                .to_int(),
            3
        );
        assert!(vm
            .get_property(&mut vm::NoHost, &pl, "other")
            .unwrap()
            .is_void());
    }
}
