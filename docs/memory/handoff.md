# Handoff

_Last updated: 2026-10-29. Overwrite this file at the end of each session._

## State

- Working proof of concept: install → stream → Bible, animations, QTVR and
  audio verified in Chromium via Xpra.
- `.env` auto-fill for registration and activation works mechanically. A
  fake code is rejected as expected. No real code has been tested yet.

## Active experiment

Branch `experiment/boxedwine` (ADR 0007, accepted for experimentation):
Boxedwine (x86 emulator + Wine 11) as a runtime. **Native arm64 on the Mac
mini M4 runs iLumina** (Home, Extras, Bíblia, Excursões Virtuais video) with
no Rosetta/QEMU. Audio stutter **fixed** on the M4: Boxedwine patch 0002
(audio queue + reserve) plus an Xpra HTML5 client patch capping lag at
2000/800 ms. Open: iLumina's volume control flashes a popup and hangs (needs
a `WINEDEBUG=err+all,+seh` log). Then: QTVR on M4, wasm build + browser test (uses AudioWorklet, no Xpra hop),
L7 metrics, ADR 0007 decision. `experiment/wine11-wow64` kept as history.

## Next steps (in priority order)

1. Test the owner's real activation code (expect a rejection if it was
   issued for another install's "#1").
2. Test the untested modules: Enciclopédia, Viagem no Tempo, Extras,
   Estudo Pessoal, PDFs (Impressario) → update compatibility.md.
3. Reproduce the intermittent `Index out of range` outside Xpra (bare Wine,
   no virtual desktop) to isolate the cause.
4. Run `tools/extract-discs.sh` end-to-end on the real `.nrg` images.
5. Custom landing page around the Xpra client (no toolbar, scaling).

## Open questions

- Is the "#1" install code derived from anything deterministic? (Only
  observe it from the UI. See decision 0005.)
- Is it worth a newer Xpra (third-party repo) for H.264?
