# 0001 — Run the original under Wine instead of recompiling or rewriting it

- Status: Accepted; amended by 0008 (Wine stays as fallback and oracle; the Director-player path is now being tried)
- Date: 2026-10-28

## Problem
We want iLumina Gold to work in a browser. The obvious model,
[nfsu2-web](https://github.com/th0mk/nfsu2-web), lifts a game's x86
executable to C and then to WebAssembly. Does that fit here?

## Decision
No. Run the unmodified Windows build in Wine 9.0 (32-bit prefix) inside a
container, and stream it to the browser (see 0002).

## Alternatives considered
- **Static recompilation of `iLumina.exe`**: the exe is a generic Director
  MX projector. The application is Lingo bytecode in `.dxr`/`.cxt` plus 30+
  native Xtras (Valentina, BuddyAPI, QuickTime…). Recompiling the projector
  would leave all of that unsolved.
- **Director runtime in WASM (DirPlayer, ScummVM Director)**: the best
  long-term result, but neither supports Director MX plus these
  third-party Xtras today. Months of work. Kept as a future path.
- **Mac OS 9 build in a browser PPC emulator (Infinite Mac)**: fully
  client-side, but slow, and QTVR under emulation is unproven.
- **Rewrite as a web app**: out of scope, since it would not be the
  original (see use-cases).

## Consequences
- ✅ Fastest path to a faithful, playable archive.
- ❌ Needs a server process per user, plus an x86 host (Apple Silicon emulates).
- The compatibility findings carry over to the WASM path later.

## Evidence
Projector, Valentina, TreeView, QuickTime 6.3 video and QTVR all ran on
first try under Wine 9.0. See ../compatibility.md.
