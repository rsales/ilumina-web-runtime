# iLumina port — Director MX player in Rust (experiment)

Branch `experiment/rust-director`, [ADR 0008](../docs/decisions/0008-rust-director-port.md).

A Rust reimplementation of the Macromedia Director MX player that runs
iLumina's **original movies and Lingo bytecode** from the user's own discs,
in the browser (WASM first). The Wine runtime in the repository root stays
as the fallback and as the behaviour oracle.

## Status

Milestone **M1** (static Home) in progress; M0 done. See [docs/port-test-matrix.md](../docs/port-test-matrix.md).

| Crate | What | Status |
|---|---|---|
| `crates/rifx` | RIFX/XFIR container, cast members, Lingo contexts, bytecode disassembler | ✅ parses all 29 movies |
| `crates/valentina` | Valentina 1.x reader: pages, catalog, tables, fixed fields, slot-heap text, BLOBs | ✅ reads all 15 databases |
| `crates/qtvr` | QuickTime VR: sample tables, node graph and links, cube view limits, `smc` hotspot maps | ✅ parses all 24 tours |
| `crates/discs` | Hash gate: embedded manifest, `verify`, `check_file` | ✅ 3,297 files match |
| `crates/director` | Stage: cast libs, score frames, compositing (bitmaps, shapes, inks) | ⚠️ native render; text pending |
| `crates/xtask` | Local tooling (`verify`, `inventory`, `valentina`, `qtvr`, `bitmaps`, `score`, `render`), output to `work/` | ✅ |
| `lingo-vm`, `xtras`, `media-prep`, `web` | see ADR 0008 | created when their milestone starts |

## Commands

```sh
cd port
cargo test                                    # unit tests (no discs needed)
cargo build -p rifx --target wasm32-unknown-unknown   # wasm build check
ILUMINA_DISCS=../discs cargo test             # + structural test over the real movies
cargo run -p xtask -- verify ../discs
cargo run -p xtask -- inventory ../discs ../work/inventory
cargo run -p xtask -- valentina ../discs/cd1/Program/Files/Databases/KJV.DAT ../work/valentina
cargo run -p xtask -- qtvr ../discs/cd2/Files/Media/QTVRs/NZV.mov ../work/qtvr
cargo run -p xtask -- bitmaps ../discs ../work/bitmaps
cargo run -p xtask -- score ../discs/cd1/Program/Files/Home.dxr 1
cargo run --release -p xtask -- render ../discs Home.dxr 1 ../work/render/Home-1.png
```

`ILUMINA_DISCS` points at the folder that holds `cd1`…`cd4` (the same layout as the Wine runtime). A relative path is resolved from `port/`.

## Rules

- Nothing derived from the discs is committed. That includes dumps,
  inventories, decompiled Lingo, screenshots and converted media. They go to
  `work/` or `ilumina-data/`, both git-ignored.
- The engine runs the original Lingo as-is, including registration, trial and
  activation (ADR 0005).
