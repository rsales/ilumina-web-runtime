# Rust port test matrix (ADR 0008)

Each milestone has a time-box and a go/no-go gate. Mark ✅ / ⚠️ (works with
issues: say which) / ❌ (fails: paste the first error) / ⏳ (not run).
Evidence must be structural (counts, hashes, timings). Screenshots and dumps
stay in `work/`, never in git.

| Milestone | Gate | Status |
|---|---|---|
| M0 spike | `rifx` parses all 29 movies; inventory exists; Valentina and QTVR are readable | ⚠️ partly done, see below |
| M1 static Home | First Home frame in the browser, close to the Wine reference (SSIM) | ⏳ |
| M2 interactive Home | Score + Lingo VM + events; module buttons navigate | ⏳ |
| M3 Bible | Book tree and Gênesis 1 text, same as in Wine | ⏳ |
| M4 multimedia | Animations and tours play; QTVR node switches and hotspots work | ⏳ |
| M5 rest | Parity with `compatibility.md`, including Mui registration/activation running the original Lingo | ⏳ |
| M6 polish | Crisp scaling, static deploy, timings on the Mac mini M4 | ⏳ |

## M0 — spike (started 2026-10-30)

### Container and Lingo ✅

- `rifx` opens **29/29** movies on the first run: `XFIR` (little-endian),
  codec `MV93`, `imap` version `0x73A` (Director MX).
- **2,018 cast members** across 20 `CAS*` tables:
  - 1,317 bitmaps, 446 scripts, 126 Xtra members, 92 fields, 32 shapes, 5 sounds;
  - 1,288 `BITD`, 92 `STXT`, 90 `XMED` (styled text), 74 `ALFA`, 5 `snd`.
- **446 scripts and 2,751 handlers** (about 254 KB of bytecode) in 10 protected
  `LctX` contexts. All of it disassembles with **no unknown opcodes**: 66
  distinct opcodes are used, and every name id resolves.
- Code is concentrated in `GCode.cxt`: 166 scripts and 1,415 handlers.
- The `.dxr` movies hold only the score and links (`VWSC`, `MCsL`, `Sord`…).
  Their members live in the shared `*Code.cxt` / `*Assets.cxt` casts.
- Checked by: `cargo test -p rifx` with `ILUMINA_DISCS` set (asserts the
  counts above). It passes on x86_64 Linux and on the Mac mini M4 (2026-10-30).
- `cargo build -p rifx --target wasm32-unknown-unknown` ✅ on the Mac mini
  M4, so the parser compiles for the browser. The inventory is `cargo run -p xtask -- inventory <discs>`,
  written to `work/inventory/`.

### Engine surface (from the inventory)

- **Xtra cast members:** `text` (88), `OSmenu` (22), `quickTimeMedia` (8),
  `swa` (3), `TreeView` (2), `flash` (2), `Impressario` (1).
- **Most used calls:**
  - `sprite`, `member`, `sendSprite`, `addProp`, `GetMsg`, `alert`;
  - BuddyAPI: `baFileExists`, `baVersion`;
  - TreeView: `…XTV`;
  - Mui: `initDialog`, `ReleaseDialog`.
- **Movie properties:** 22, including `windowList` and `activeWindow`, so the
  app uses **MIAW windows**. Top-level `the` properties: 29.
- **Valentina API:**
  - setup: `ValentinaInit`, `OpenDatabase`;
  - queries: `ExecuteSql`, `GetCursor`, `GoFirst`, `GoNext`, `GetField`;
  - results: `GetRecordSetAsPropList` (61 calls), `GetRecordAsPropList`.
  - That is a cursor and SQL surface, not a raw table API.

### Valentina ⚠️ readable, not parsed yet

- Files are page-based, with a common header (`00 02 20 00 90 01 01 00`, then the
  page size: 4 KB `.vdb`, 32 KB `.dat/.ind/.blb`). They are **not encrypted**.
- The schema is stored as text records in the `.vdb`. For example table
  `KJV`: `Book`, `Chapter`, `Verse` (type 3), `Text`, `Title` (type 26, ASCII).
- KJV verse text is **plain** in `KJV.DAT`. Text pages carry a trailer of
  u16 offsets plus a count. Those offsets are not verse boundaries, so the
  record-to-text mapping is the next thing to work out.
- The Portuguese versions (RA…) keep their text in `.BLB`.
- **Fallback if the format resists:** export every table once through the
  original V4MD Xtra under the Wine runtime (oracle) into a local SQLite
  file, and have the port read that.

### QTVR ⚠️ faces OK, hotspots and graph pending

- `NZV.mov` (Nazaré) is a **cubic** QTVR. Streams:
  - one `qtvr` track (50 samples);
  - a pano track with **28 nodes**;
  - **168 MPEG-4 frames of 768×768**, which is 28 nodes × 6 cube faces;
  - a 168-frame `smc` hotspot track of 1500×1500.
- ffmpeg decodes all 168 faces in about 2 s on x86. It logs MB errors, but the
  frames look right. The `smc` hotspot frames fail to decode, and the node
  graph (`vrni`, links) isn't parsed yet.
- For comparison, under Boxedwine QuickTime took seconds per node switch.

### Not done in M0 yet ⏳

- dirplayer-rs test against `Home.dxr`, and the GPL decision.
- Hash gate against the manifest inside `xtask`.
