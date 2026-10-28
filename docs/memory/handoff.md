# Handoff

_Last updated: 2026-10-28. Overwrite this file at the end of each session._

## State

- Working proof of concept: install → stream → Bible, animations, QTVR and
  audio verified in Chromium via Xpra.
- `.env` auto-fill for registration and activation works mechanically. A
  fake code is rejected as expected. No real code has been tested yet.

## Active experiment

Branch `experiment/wine11-wow64` (ADR 0006): Wine 11 new-WoW64, to get
around QEMU's c0000018 on Apple Silicon. **Untested**: the sandbox can't
reach dl.winehq.org and has no arm64 host. The owner tests on a Mac mini (M-series).

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
