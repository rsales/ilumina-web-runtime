# 0008 — Reimplement the Director player in Rust (WASM-first)

- Status: **Accepted for experimentation** (branch `experiment/rust-director`)
- Date: 2026-10-30
- Supersedes: 0007 (outcome: **redirect** to the Director player reimplementation)
- Amends: 0001 (Wine stays as fallback and behaviour oracle), 0003 (rule for derived local data)
- Keeps: 0005 (no activation bypass)

## Problem
Wine (0001) and Boxedwine (0007) run the original, unmodified program, and
both work on a Mac mini M4. But the experience has limits that can't be
fixed on those paths:
- latency: x86 emulation plus Xpra in between, and audio that arrives in bursts;
- blur: Xpra compresses frames to JPEG/WebP and scales them;
- slow QTVR: each node switch decodes 1000×1000 MPEG-4 tiles inside the emulator;
- a server process or a heavy container is always needed.

## Decision
Write a **Director MX player in Rust** that compiles to WebAssembly. It reads
the user's original `.dxr`/`.cxt` movies and runs **their Lingo bytecode**.
The approach is the same as ScummVM's Director engine and dirplayer-rs. The
target is the player, not the content.

- **Xtras** are reimplemented by behaviour behind one `Xtra` trait. That covers
  V4MD/Valentina, TreeView, vList, Mui Dialog, BuddyAPI, FileIO, OSControl and
  QT3Asset. Non-essential Xtras are logged stubs.
- **QuickTime media** is converted **locally, on the user's machine**, by
  `media-prep` when they install from their discs:
  - Sorenson and MPEG-4 → WebM/MP4;
  - QTVR → per-node images, plus a JSON node graph with hotspots.
  The browser plays the result with native decoders.
- **The browser comes first.** A native desktop build may follow from the same core.
- **Short, time-boxed vertical slices with go/no-go gates:** M0 spike → M1
  static Home → M2 interactive Home → M3 Bible → M4 multimedia → M5 rest →
  M6 polish. See `docs/port-test-matrix.md`.

**Stop rule** (the lesson from the Colin McRae port): if a milestone's
time-box runs out while the repository keeps growing and the distance to a
usable result doesn't shrink, stop. Record why here, and fall back to the
Wine runtime.

## Rules this decision adds
- **Derived data stays local.** Decompiled Lingo, dumps, inventories and
  screenshots live in `work/`. Converted media lives in `ilumina-data/`. Both
  are git-ignored. The repo holds code, hashes and synthetic fixtures only.
  This extends 0003.
- **Activation is untouched (0005).** The engine runs the original
  registration, trial and activation Lingo unchanged. It never special-cases
  those handlers. The owner enters their own code, as in Wine.
- **Hash-gated input.** Discs and movies must match
  `manifests/ilumina-gold-2.50-ptbr.sha256`. Unknown inputs are refused, not
  "best-effort" parsed.

## Alternatives considered
- **Keep tuning Wine/Boxedwine**: it can't remove the streaming hop or the emulation cost.
- **Fork dirplayer-rs (GPL-3)**: it targets Shockwave `.dcr` games. Whether it
  opens Director MX projector movies is checked in M0. Reusing its code would
  make the port GPL-3; that is decided in M0.
- **Remaster (extract the data, build a new UI)**: it isn't the original
  program, and it would skip the original activation. Out of scope (see use-cases).
- **Static recompilation (nfsu2-web style)**: see 0001 and 0007. It would
  recompile Macromedia's player rather than iLumina.

## Consequences
- ✅ Native rendering at device resolution (no blur), no server, and no emulator.
- ✅ The format knowledge is reusable for other Director MX titles (UC5).
- ❌ Large surface: a Lingo VM, score playback, about 10 Xtras, and an
  undocumented database format (Valentina). That is the main risk; M0 tests it first.
- ❌ Fidelity has to be proven screen by screen against the Wine runtime.

## Evidence
- Inventory of the discs (2026-10-30): 29 Director MX movies (about 14 MB, protected, no
  Lingo source). The Valentina databases are about 200 MB. Most media is already JPG/PDF/MP4;
  the rest is QuickTime (Sorenson animations, MPEG-4 tours, multi-node QTVR).
- M0 results: see `docs/port-test-matrix.md`.
