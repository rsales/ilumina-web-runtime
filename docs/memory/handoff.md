# Handoff

_Last updated: 2026-10-28. Overwrite this file at the end of each session._

## State

- Working proof of concept: install → stream → Bible, animations, QTVR and
  audio verified in Chromium via Xpra.
- `.env` auto-fill for registration and activation works mechanically. A
  fake code is rejected as expected. No real code has been tested yet.

## Active experiment

Branch `experiment/boxedwine` (ADR 0007, accepted for experimentation):
Boxedwine (x86 emulator + Wine 11 in WebAssembly) as a browser runtime.
Scaffold in `web/`: wasm build, native test bench, overlay packer, L1 probe,
layered launcher. Native image builds on x86_64. **Not run yet:** needs
`TinyCore15Wine11.0.zip` (boxedwine.org, which the sandbox can't reach).
Next: fill `docs/boxedwine-test-matrix.md` L1 → L7.
`experiment/wine11-wow64` is kept as history (ADR 0006).

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
