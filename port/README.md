# iLumina port — Director MX player in Rust (experiment)

Branch `experiment/rust-director`, [ADR 0008](../docs/decisions/0008-rust-director-port.md).

A Rust reimplementation of the Macromedia Director MX player that runs
iLumina's **original movies and Lingo bytecode** from the user's own discs,
in the browser (WASM first). The Wine runtime in the repository root stays
as the fallback and as the behaviour oracle.

## Status

Milestone **M1** (static Home) in progress: bitmaps, shapes, score and text render natively; the browser canvas is next. M0 done. See [docs/port-test-matrix.md](../docs/port-test-matrix.md).

| Crate | What | Status |
|---|---|---|
| `crates/rifx` | RIFX/XFIR container, cast members, Lingo contexts, bytecode disassembler, bitmaps, score, styled text and fields | ✅ parses all 29 movies |
| `crates/valentina` | Valentina 1.x reader: pages, catalog, tables, fixed fields, slot-heap text, BLOBs | ✅ reads all 15 databases |
| `crates/qtvr` | QuickTime VR: sample tables, node graph and links, cube view limits, `smc` hotspot maps | ✅ parses all 24 tours |
| `crates/discs` | Hash gate: embedded manifest, `verify`, `check_file` | ✅ 3,297 files match |
| `crates/director` | Stage: cast libs, score frames, compositing (bitmaps, shapes, inks, text through a host `Fonts` trait) | ⚠️ native render; browser canvas pending |
| `crates/xtask` | Local tooling (`verify`, `inventory`, `valentina`, `qtvr`, `bitmaps`, `texts`, `score`, `render`), output to `work/`; fontdue over host fonts | ✅ |
| `crates/web` | Browser player: the stage behind a hand-written wasm ABI (no wasm-bindgen); page in `port/web/`, fonts from the browser canvas | ⚠️ built on the Mac only |
| `crates/lingo-vm` | Lingo bytecode interpreter: values, lists, chunks, handlers, parent scripts, `Host` trait for the player | ✅ core (M2 slice 1); host functions in progress |
| `crates/player` | Host for the VM: virtual Windows file system (full-install layout), movie/player properties, BuddyAPI, FileIO, prefs | ⚠️ in progress (M2 slice 2) |
| `xtras`, `media-prep` | see ADR 0008 | created when their milestone starts |

## Commands

```sh
cd port
cargo test                                    # unit tests (no discs needed)
cargo build -p rifx --target wasm32-unknown-unknown   # wasm build check
ILUMINA_REQUIRE_DISCS=1 cargo test            # fail instead of skipping when no discs are found
cargo run -p xtask -- verify ../discs
cargo run -p xtask -- inventory ../discs ../work/inventory
cargo run -p xtask -- valentina ../discs/cd1/Program/Files/Databases/KJV.DAT ../work/valentina
cargo run -p xtask -- qtvr ../discs/cd2/Files/Media/QTVRs/NZV.mov ../work/qtvr
cargo run -p xtask -- bitmaps ../discs ../work/bitmaps
cargo run -p xtask -- score ../discs/cd1/Program/Files/Home.dxr 1
cargo run -p xtask -- texts ../discs ../work/texts
cargo run -p xtask -- lingo ../discs ../work/lingo        # disassembly per script
cargo run --release -p xtask -- render ../discs Home.dxr 1 ../work/render/Home-1.png
cargo run --release -p xtask -- render ../discs Bible.dxr 30 ../work/render/Bible-30.png
```

### Browser

```sh
rustup target add wasm32-unknown-unknown            # once
cargo build --release -p web --target wasm32-unknown-unknown
cargo run --release -p xtask -- serve ../../discs   # → http://localhost:8800/
```

With **Lingo** checked (the default) the page runs the movie's startup with
its original Lingo and draws the stage as Lingo left it. The run asks for the
disc files it reads (databases, linked images); the page fetches them and runs
again until nothing is missing (Home: about 60 files, 176 MB on the first
run). On first run the program asks for registration: the page shows a form
for the owner's own name, e-mail and CEP, kept only in that browser
(`sessionStorage`, or `localStorage` with "lembrar"), never generated.
Unchecked, the page renders any score frame (← → keys, slider) and keeps the
movie and frame in the URL (`#Bible/30`).

`port/.cargo/config.toml` gives the wasm module a 64 MB stack: the Lingo VM
recurses natively. The server listens on
127.0.0.1 only and serves the page, the wasm module and the disc files listed
in the manifest; the wasm side checks every file's hash again. Fonts come from
the browser (on macOS: Verdana, Georgia… from the system).

Text uses the fonts installed on the machine. On macOS the original faces
(Verdana, Georgia, Times New Roman, Arial…) are found under
`/System/Library/Fonts/Supplemental`; elsewhere Liberation/DejaVu stand in.
`ILUMINA_FONTS=/path/to/fonts` adds folders to search first.

Disc tests and `xtask` need the folder that holds `cd1`…`cd4`. Tests find it
at `<repo>/discs` or next to the repo (`<repo>/../discs`), or wherever
`ILUMINA_DISCS` points (relative to `port/`). A wrong `ILUMINA_DISCS` fails
the tests instead of skipping them. `xtask` checks its `<discs-dir>` argument
and prints where the discs are if the path is wrong. From `port/` with the
discs next to the repo, that is `../../discs`.

## Rules

- Nothing derived from the discs is committed. That includes dumps,
  inventories, decompiled Lingo, screenshots and converted media. They go to
  `work/` or `ilumina-data/`, both git-ignored.
- The engine runs the original Lingo as-is, including registration, trial and
  activation (ADR 0005).
