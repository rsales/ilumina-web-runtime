# 0007 — Investigate Boxedwine WebAssembly as an alternative runtime

- Status: **Accepted for experimentation** (branch `experiment/boxedwine`)
- Date: 2026-10-28
- Relates to: 0001 (Wine, not recompilation), 0006 (Apple Silicon: no 32-bit
  mode in containers; branch `experiment/wine11-wow64` kept as history)

## Decision
Investigate **Boxedwine** as a browser-based x86 execution environment, to
find out whether the existing iLumina Windows runtime can run **without a
remote Linux server and without a native Wine install**.

This record does **not** claim that Boxedwine solves Apple Silicon or the
server requirement. Proving or disproving that is the point of the experiment.

## Why Boxedwine is worth testing
- It brings its own x86 CPU emulator (with a wasm JIT), Linux kernel emulation
  and a 32-bit Wine 11 file system, and compiles all of it to WebAssembly.
  The unmodified binaries run on top.
- Because it emulates the CPU itself, it doesn't depend on Rosetta or QEMU
  user-mode, the two things that blocked ADR 0006. This is a hypothesis to test.
- No iLumina code is rewritten, so the preservation goal (ADR 0001) holds.

## Alternatives (not chosen now)
- **Static recompilation in the style of nfsu2-web**: it would recompile
  Macromedia's and Apple's code (Director engine, ~30 Xtras, QuickTime 6.3),
  because iLumina itself is Lingo bytecode, i.e. data. USER32/GDI32 would also
  need reimplementing. Worst effort-to-result ratio.
- **Reimplement the Director player** (DirPlayer / ScummVM Director, with Xtras
  rewritten: Valentina → SQLite, QuickTime → `<video>`, QTVR → WebGL): the
  **long-term strategic alternative**. It is where effort goes if this
  experiment is abandoned.

## Method: progressive, layer by layer
Each layer isolates one part of the stack. If a layer fails, the problem is in
that layer. The test runs in isolation from Docker and from the current Wine
prefix: it uses Boxedwine's own Wine, and the browser holds all the state.

| Layer | Test | Isolates |
|---|---|---|
| L1 | `web/probes/hello32gui.exe` (minimal PE32 GUI) | **emulator** (CPU + kernel) |
| L2 | Wine's own notepad | **Boxedwine's Wine** |
| L3 | `iLumina.exe` started without QuickTime (`run-noqt.bat`) | **Director** projector start-up |
| L4 | Home opens, Bible → Gênesis shows text | **iLumina content** (`.dxr`/`.cxt` movies) |
| L5 | Book tree, Bible text, dialogs, volume | **Xtras** (TreeView, Valentina, Mui, …) |
| L6 | QuickTime 6.3 wizard, then Animações + Passeios Virtuais | **QuickTime, video, QTVR** |
| L7 | Time to Home, tab memory, video/QTVR smoothness on the Mac mini M4 | **performance** |

Results go in `docs/boxedwine-test-matrix.md`.

## Success criteria
- Windows x86 programs run in the browser (L1–L2).
- iLumina starts and loads real content (L3–L4).
- The essential Xtras work (L5).
- Video and QTVR play (L6).
- Performance is acceptable on a Mac mini M4 (L7).
- No Linux server is needed to execute the runtime. A static file server is fine.

## Required outcome
The experiment ends with an explicit decision, recorded by superseding this ADR:
**continue** (Boxedwine becomes the web runtime), **abandon**, or **redirect**
the effort to the Director player reimplementation.

## Progress

- 2026-10-29, native x86_64: L1–L6 pass, with two fixes (keymap patch, GDI
  renderer). See the test matrix. Next: native on the Mac M4 (arm64), then wasm.

## Known risks
- A full install is ~1.9 GB and is fetched whole into the browser. Fallbacks:
  `cdrom=` ISOs (one disc in memory at a time; `make-overlay.sh --isos`) or
  `storage=hostfolder` (the user picks the discs folder).
- CPU emulation speed for Director and QuickTime decoding.
- Activation is unchanged (ADR 0005).
