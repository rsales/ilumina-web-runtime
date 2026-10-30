# iLumina port — Director MX player in Rust (experiment)

Branch `experiment/rust-director`, [ADR 0008](../docs/decisions/0008-rust-director-port.md).

A Rust reimplementation of the Macromedia Director MX player that runs
iLumina's **original movies and Lingo bytecode** from the user's own discs,
in the browser (WASM first). The Wine runtime in the repository root stays
as the fallback and as the behaviour oracle.

## Status

Milestone **M0** (spike). See [docs/port-test-matrix.md](../docs/port-test-matrix.md).

| Crate | What | Status |
|---|---|---|
| `crates/rifx` | RIFX/XFIR container, cast members, Lingo contexts, bytecode disassembler | ✅ parses all 29 movies |
| `crates/xtask` | Local tooling (`inventory`), output to `work/` | ✅ |
| `lingo-vm`, `director`, `valentina`, `xtras`, `media-prep`, `web` | see ADR 0008 | created when their milestone starts |

## Commands

```sh
cd port
cargo test                                    # unit tests (no discs needed)
cargo build -p rifx --target wasm32-unknown-unknown   # wasm build check
ILUMINA_DISCS=../discs cargo test             # + structural test over the real movies
cargo run -p xtask -- inventory ../discs ../work/inventory
```

`ILUMINA_DISCS` points at the folder that holds `cd1`…`cd4` (the same layout as the Wine runtime). A relative path is resolved from `port/`.

## Rules

- Nothing derived from the discs is committed. That includes dumps,
  inventories, decompiled Lingo, screenshots and converted media. They go to
  `work/` or `ilumina-data/`, both git-ignored.
- The engine runs the original Lingo as-is, including registration, trial and
  activation (ADR 0005).
