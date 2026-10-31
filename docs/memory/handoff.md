# Handoff

_Last updated: 2026-10-30. Overwrite this file at the end of each session._

## State

- Working proof of concept: install → stream → Bible, animations, QTVR and
  audio verified in Chromium via Xpra.
- `.env` auto-fill for registration and activation works mechanically. A
  fake code is rejected as expected. No real code has been tested yet.

## Active experiment

Branch `experiment/rust-director` (ADR 0008): a Rust Director MX player, WASM
first. Earlier experiments are kept as history:

- `experiment/boxedwine` (ADR 0007, ended as **redirect**). It works on the
  Mac M4: audio was fixed, but the volume control hangs and QTVR is slow.
- `experiment/wine11-wow64` (ADR 0006).

M0 status:

- **Parser:** `port/crates/rifx` parses all 29 movies, and all 2,751 handlers
  disassemble. The inventory runs with `cargo run -p xtask -- inventory`; its
  output stays in `work/`.
- **Valentina ✅:** `port/crates/valentina` reads all 15 databases: fixed
  fields, the KJV slot-heap text and the RA/encyclopedia BLOBs. Checked by its
  disc tests; `xtask valentina` dumps CSVs to `work/`. Next: field names from
  the `.vdb` and the SQL subset the Lingo uses.
- **QTVR ✅:** `port/crates/qtvr` reads the node graph, links, cube view
  limits and the `smc` hotspot maps. All 24 tours parse; Nazaré is checked
  end to end. `xtask qtvr` writes JSON to `work/`.
- **M0 done (go).** dirplayer-rs was evaluated and stays a reference only; the license stays MIT (ADR 0008). Old note: and add a manifest hash gate. (The wasm build of `rifx` and the
  disc test pass on the Mac M4.)

See `docs/port-test-matrix.md`.

**Next: M1, static Home.** Decode `BITD` bitmaps, `CLUT` palettes and
`STXT`/`XMED` text. Parse frame 1 of the `VWSC` score and the `MCsL` cast
links. Then render the Home stage at 1024×768 into a canvas (WASM), and
compare it with a Wine screenshot locally (SSIM, in `work/`). Also add the
manifest hash gate.

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
