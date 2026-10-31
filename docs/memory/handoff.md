# Handoff

_Last updated: 2026-10-31. Overwrite this file at the end of each session._

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

**M2 slice 2 in progress (2026-10-31):** `crates/player` (virtual file
system, properties, BuddyAPI, FileIO, prefs). Home's `startMovie` runs to the
end; `prepareMovie` stops at `VDatabase.OpenDatabase` (AppTables.vdb). Next:
the V4MD layer over `crates/valentina` — field names from `.vdb` and the SQL
subset the Db.* scripts use.

**M2 slice 1 done (2026-10-31):** `crates/lingo-vm` runs the original
bytecode; Home's `prepareMovie`/`startMovie` stop at the first player
function (`the applicationPath`, BuddyAPI, FileIO, Valentina, prefs). Next:
slice 2, a host that answers those, driven by the recorded list.

**M1 done (2026-10-31):** the browser player runs on the Mac (wasm) and renders
every module's score frames. **M2 started**: slices in `docs/port-test-matrix.md`;
`xtask lingo` dumps the disassembly per script to `work/lingo/`.

Earlier M1 notes: hash gate ✅, bitmaps ✅, score ✅, text ✅, native
compositor ⚠️. `xtask render <discs> Home.dxr 1` writes a PNG (now with text,
fonts from the host). All 88 XMED and 92 STXT parse; encoding is per document
(Mac Roman vs Windows-1252, flag in XMED section 1). Home frame 1 has no
static text: its text members are empty and Lingo fills them, like the icon
swaps and linked JPEGs. Owner's Boxedwine screenshots of Home and Multimídia
are in `work/reference/`; pixel parity needs M2. The browser player
(`port/crates/web`, `port/web/`, `xtask serve`) is written and tested natively;
its first wasm run happens on the Mac (no wasm target in the sandbox). Next:
the Lingo VM (M2), starting with sprite visibility/puppeting and the Home
scripts. 8-bit palette indices are still to be confirmed.

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
