# 0006 — Experiment: Wine 11 in new-WoW64 mode for Apple Silicon

- Status: Proposed (experiment on branch `experiment/wine11-wow64`)
- Date: 2026-10-28
- Relates to: 0002 (it adds a third-party apt repo: WineHQ)

## Problem
On Apple Silicon, Docker Desktop runs x86_64 code through Rosetta but
**32-bit i386 code through QEMU user-mode**. Ubuntu's Wine 9 uses the old
WoW scheme (real i386 processes), so every Windows program runs under QEMU.
`QuickTimeInstaller.exe` has no relocations and needs address 0x400000,
which QEMU can't provide: `status c0000018`. The same thing happens with
Rosetta on or off.

## Hypothesis
Wine 11's **new WoW64** runs 32-bit Windows code inside a **64-bit** Linux
process. Docker would then use Rosetta, not QEMU, and the 64-bit process
controls its own low address space (the preloader reserves 0x400000).

## What the branch changes
- Wine 11 from `dl.winehq.org` (`winehq-stable`) instead of Ubuntu's Wine 9.
- The build moves `lib/wine/i386-unix` aside, so 32-bit programs can only run
  through WoW64. It fails if `wow64.dll` / `wow64cpu.dll` / `i386-windows`
  are missing.
- `WINEARCH=win64`. 32-bit system files land in `syswow64` (`sys32_dir()` in lib.sh).
- A separate image tag (`:wow64`) and volume (`ilumina-state-wow64`), so the
  win32 install from `main` is not touched.
- `entrypoint.sh doctor` prints the Wine layout, runs a 32-bit `cmd.exe` and
  starts `QuickTimeInstaller.exe` to see whether it gets past c0000018.

## Success criteria
1. `docker compose build` passes the layout check.
2. `doctor` prints `32BIT-OK` and "QuickTimeInstaller is RUNNING".
3. `install` ends with "installation complete".
4. Home, Bible text, animation with sound and a QTVR tour work in the browser.

If everything passes on Apple Silicon **and** x86_64, consider merging behind
a build arg. If it fails, record why in `docs/memory/learnings.md` and close
this ADR as Rejected.

## Risks
- The WineHQ package layout may differ from what the build expects (the build
  fails, which is the cheap outcome).
- Director MX, the Xtras or QuickTime 6.3 may behave differently under WoW64 or Wine 11.
- Rosetta may still collide with 0x400000 (it has its own runtime in the process).
