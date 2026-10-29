# Boxedwine test matrix (ADR 0007)

Fill one column per environment. Use ✅ / ⚠️ (works with issues: describe) /
❌ (fails: paste the first error from the browser console) / ⏳ (not run).
Stop at the first ❌: the layers after it are not conclusive.

| Layer | What to check | native x86_64 | native Mac M4 (arm64) | wasm Chrome Mac M4 |
|---|---|---|---|---|
| L1 emulator | `hello32gui.exe` message box; `C:\boxedwine-probe.txt` written | ✅ | ✅ (implied by L3+) | ⏳ |
| L2 Boxedwine's Wine | notepad opens and accepts typing | ✅ | ✅ (implied) | ⏳ |
| L3 Director | `iLumina.exe` splash ("CARREGANDO…"), registration dialog | ✅ (with patch 0001) | ✅ | ⏳ |
| L4 content | Home screen, Bíblia → Gênesis text | ✅ | ✅ | ⏳ |
| L5 Xtras | book tree (TreeView), Bible text (Valentina), Mui dialogs | ✅ | ✅ | ⏳ |
| L6 QuickTime | 6.3 wizard completes; Animações plays with sound; QTVR pans | ⚠️ installs, video plays, QTVR pans; needs GDI renderer; sound not tested | ⚠️ installs (then crash 139, recovered by restart); video plays; **audio stutters** (patch 0002 ⏳); QTVR ⏳ | ⏳ |
| L7 performance | time to Home (s); tab memory (GB); video fps / QTVR smoothness | — | — | ⏳ |

## Environment details (fill in)

- Boxedwine commit: `8790ff20428c22daa64368af9553417e4f499c94`, target `multiThreadedJit`
- File system: `TinyCore15Wine11.0.zip` (Wine 11.0, 32-bit)
- Browser + version:
- Overlay size: ilumina.zip = 1.9 GB (full install layout)

## Notes / errors

### native Mac mini M4 (arm64 container, Docker Desktop), 2026-10-29

- The arm64 image builds (Boxedwine compiled natively in ~75 s) and runs
  **without Rosetta or QEMU**: Boxedwine's own CPU emulator executes the 32-bit
  x86 code that failed in ADR 0006.
- L1–L3 implied: Wine starts and the **QuickTime 6.3 wizard is shown and usable
  in the browser** (Xpra, audio stream active).
- At the end of the QuickTime install, the installer opens an Explorer window
  for the Start-menu folder (`IShellBrowser` fixmes), and Boxedwine
  **segfaults (exit 139)**. On x86_64 the same Explorer window froze the
  screen. Suspect: Wine's explorer/shell browser under Boxedwine, not iLumina.
  `QuickTime.qts` **was** written before the crash, so `restart: unless-stopped`
  brings iLumina back and `run.bat` skips the installer.
- **L4–L5**: Home, Extras, Bíblia → Gênesis 1 text render correctly.
- **L6**: Multimídia → Excursões Virtuais video ("José e Maria Vão para
  Belém") plays. **Audio cuts out and stalls often.** QTVR not checked yet.
- Audio analysis: on native builds Boxedwine queues at most 125 ms to SDL and
  reports every queued byte to Wine's OSS driver, which then keeps only one
  or two periods ahead; any emulator stall underruns. The web build already
  hides a 20 ms reserve from the guest for the same reason. Patch 0002 adds
  opt-in env knobs (`BOXEDWINE_AUDIO_BUFFER_MS`, `_RESERVE_MS`,
  `_PERIOD_FRAMES`); the native runtime defaults to 400/200/2048 plus
  `PULSE_LATENCY_MSEC=100` and Xpra `--speed=90 --quality=60` to cut encoder
  CPU. Trade-off: about 0.2 s of extra audio latency. Retest pending.

### native x86_64 (sandbox, 2 vCPU, Xvfb), 2026-10-29

- **L1–L2**: OK at the first try.
- **L3**: registration rejected every e-mail. Cause: Boxedwine's US keymap
  maps Shift+2 to `&`, so `@` can't be typed. Fixed with
  `web/patches/0001-xkeyboard-shift-2-is-at.patch` (to report upstream).
- **L4–L5**: Home, Bible book tree, Gênesis introduction text and the timeline
  all render correctly. About 2 minutes from launch to the registration dialog
  on this machine.
- **L6**: the original QuickTime 6.3 wizard installs (`QuickTime.qts` in
  system32). The fixed-base 0x400000 exe loads fine, unlike QEMU (ADR 0006).
  After installing, iLumina hung at "CARREGANDO…" with `Couldn't find
  matching GLX visual`. Fixed by forcing Wine's DirectDraw **GDI renderer**
  (`HKCU\Software\Wine\Direct3D\renderer=gdi`, now in `run.bat`). After that:
  the animation "Jesus Ressuscita dos Mortos" plays with captions advancing,
  and QTVR "Pináculo do Templo" pans by dragging.
- Known app issue reproduced: opening a top-bar dropdown menu hangs the app
  (with Wine 9 it showed "Index out of range"). Workaround: the Home/hub buttons.
- Sound was not tested (`-nosound` in the sandbox).

