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
- **Valentina:** plain and paged. Next is the record→text mapping in `.dat`,
  then the RA/RC/NTLH text in `.blb`. Fallback: export the tables once through
  V4MD under Wine.
- **QTVR:** `NZV.mov` is cubic, with 28 nodes × 6 faces of 768² MPEG-4.
  ffmpeg decodes the faces. The `smc` hotspot track and the node graph are pending.
- **Still to do:** try dirplayer-rs on `Home.dxr`, decide on GPL, and add a manifest hash gate. (The wasm build of `rifx` and the
  disc test pass on the Mac M4.)

See `docs/port-test-matrix.md`.

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
