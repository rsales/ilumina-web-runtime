# Handoff

_Last updated: 2026-11-01. Overwrite this file at the end of each session._

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

**Activation in the page (2026-11-01):** owner's run: trial path and side
navigation work; typing the code left "Continuar" disabled. Fixed: live
`itemChanged` from text fields, disabled items drop clicks, the page's owner
answers both activation steps with their own code (no dialog shown). Owner
confirmed in the browser (no dialog; rollovers OK). Next: `puppetSound`. The dialog is never skipped or removed (ADR 0005);
next, keep the program's written files in browser storage so an activated
install stays activated across page loads.

**M2 slice 4 (2026-10-31):** mouse events, `do`, real dialogs in the page
(answers replay the session) and `go to movie` keeping globals. Component
test: rollover on Home, click → Encyclopedia starts. Owner: test in the
browser (move/click modules; the activation dialog should appear as a
dialog). Next: persist the file overlay (registration/prefs) in the
browser, mask hit testing, the remaining module startups.

**M2 slice 3, frame loop (2026-10-31):** the browser page works on the Mac
(whole startup, Home drawn). Frame loop in (`stage::step`, labels, go,
endSprite, stepFrame); Home holds on frame 2 calling `oLite.TriggerLoop`,
which shows the activation dialog once. Page plays at 15 fps. Next: mouse
events (rollover/click → module navigation) and an interactive dialog UI in
the page (activation needs the owner's code), then `go to movie` keeping
globals.

**M2 slice 3, browser (2026-10-31):** the page runs the player (Lingo
checkbox, default on): fetch loop for the files the run reads, registration
form (owner's data, browser-only), draws the run-time stage. Native loop test
passes; the wasm build is untested here (no wasm target in the sandbox) —
owner builds and opens it on the Mac. Known cost: each round reopens every
database (176 MB copies); lazy BLOBs next.

**M2 slice 3, render (2026-10-31):** owner run: whole startup OK. The
run-time stage renders (`host.draws()` → `Stage::render_draws`), incl.
linked JPEG/BMP; `xtask play <discs> Home.dxr` (owner's `ILUMINA_REG_*`)
writes `work/play/Home.png`. Home looks close to the Wine reference. Next:
the browser page runs the player (needs ~176 MB of files: lazy BLOBs), then
the frame loop and mouse.

**M2 slice 3, latest (2026-10-31):** text measuring (`director::text::Measure`,
`player::fonts::ApproxFonts`) unblocked "Single Scrollbar"; in the component
test Home's whole startup (frame 1: beginSprite → exitFrame) completes.
Next: the frame loop (`go`, frame labels, endSprite, stepFrame for the
actorList → Poster's offline POST), then render Home from the run-time
sprite state in the browser. Owner: rerun the disc test.

**M2 slice 3 started (2026-10-31):** owner rerun: `prepareMovie` completes.
Score behaviors, `player::stage` (startup order, frame events,
`sendSprite`) and many sprite/member properties are in; the cast
first-member bug (GlobalAssets starts at 2; also affected M1 rendering) is
fixed. Component test: all frame-1 `beginSprite` run; next stop
`prepareFrame` → `charPosToLoc` (text metrics in the player). Owner: rerun
the disc test (expected: past beginSprite, stop at `charPosToLoc`) and,
optionally, re-render Home in the browser (lib 5 sprites change).

**M2 slice 2, latest (2026-10-31):** the owner's run passed registration
and stopped at `the actorList` in `Poster.POST`; that, offline NetLingo,
BinaryIO (media unlock as byte patches), `rgb()` and `member.text` are now
implemented. The component test `home_managers_build` builds all 14
post-registration managers and runs `UnlockAnimations`/`UnlockGTVs` and
`Home Module`; `startMovie` then stops at `sendSprite` = slice 3 (sprites
and behaviors). Next: owner reruns the disc test with `ILUMINA_REG_*`; then
slice 3.

**M2 slice 2 in progress (2026-10-31):** `crates/player` (virtual file
system, properties, BuddyAPI, FileIO, prefs, V4MD over `crates/valentina`,
Mui dialogs with a `Responder`). Home's `prepareMovie` shows the first-run
registration dialog and, without an answer, keeps asking (as the original
does). Next: the owner runs the disc test with their own `ILUMINA_REG_*`
values on the Mac (masked output) to see how far the startup goes and set
the next ratchet step.

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
