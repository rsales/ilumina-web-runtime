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
- Wine 11.0 as a **pure WoW64 build** (Kron4ek `wine-11.0-amd64-wow64.tar.xz`
  from GitHub, pinned by SHA-256 with `ADD --checksum`) instead of Ubuntu's Wine 9.
  No `i386` dpkg architecture at all. Ubuntu's `wine64` package is installed
  only to pull in the 64-bit runtime libraries.
  - First attempt: WineHQ `winehq-stable` 11.0 with `i386-unix` hidden. That
    failed on the Mac (`ShellExecuteEx failed`, `failed to open …syswow64\cmd.exe`).
    WineHQ's Ubuntu packages are an old-style build: 32-bit programs still need
    `i386-unix`.
- The install goes to **`C:\Program Files (x86)\iLuminaPO`** on the win64
  prefix. iLumina's Lingo looks up `iLuminaInfo.txt` under the 32-bit
  Program Files folder. With the win32 path it fails with "Error occurred
  reading file Bad file name" (traced with `WINEDEBUG=+file`).
- QuickTime wizard clicks are placed relative to the window's bottom edge:
  Wine 11 draws the wizard 25 px taller (552×333 vs 552×308).
- `WINEARCH=win64`. 32-bit system files land in `syswow64` (`sys32_dir()` in lib.sh).
- A separate image tag (`:wow64`) and volume (`ilumina-state-wow64`), so the
  win32 install from `main` is not touched.
- `entrypoint.sh doctor` prints the Wine layout, runs a 32-bit `cmd.exe` and
  starts `QuickTimeInstaller.exe` to see whether it gets past c0000018.

## Results so far

| Check | x86_64 (sandbox) | Apple Silicon |
|---|---|---|
| Build + layout check | ✅ | ⏳ |
| QuickTime 6.3 unattended install | ✅ (~80 s) | ⏳ |
| App starts, registration auto-fill | ✅ | ⏳ |
| Animation with sound in the browser | ✅ | ⏳ |

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
