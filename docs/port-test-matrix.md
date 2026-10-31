# Rust port test matrix (ADR 0008)

Each milestone has a time-box and a go/no-go gate. Mark ✅ / ⚠️ (works with
issues: say which) / ❌ (fails: paste the first error) / ⏳ (not run).
Evidence must be structural (counts, hashes, timings). Screenshots and dumps
stay in `work/`, never in git.

| Milestone | Gate | Status |
|---|---|---|
| M0 spike | `rifx` parses all 29 movies; inventory exists; Valentina and QTVR are readable | ⚠️ parser ✅, Valentina ✅, QTVR ✅; dirplayer-rs pending |
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

### Valentina ✅ readable (2026-10-30)

The format was worked out from iLumina's own files, since there is no public
documentation. The reader is `port/crates/valentina`, and its layout notes are
in the crate docs.

- **Container:** a 1 KB file header, then 32 KB pages with 1-based ids.
  - Page 1 is the page-chain table.
  - Page 2 is the catalog of segments: `id, pages, size, kind, field`. A
    segment's id is also the id of its first page.
  - Every table starts with a record-bitmap segment (field 0), so a file
    with several tables splits at those.
- **Field storage:**
  - Fixed-width fields have `size / records` bytes each; booleans are packed one bit per record.
  - **VarChar-like text** (KJV) is a slot heap of 1 KB slots in the `.dat`.
    Each slot's trailer holds the offsets of the values it contains, and an
    index segment (kind 12) gives the first record of each slot.
  - **Text/BLOB** (RA, RC, NTLH, encyclopedia) is a 4-byte pointer to a chain
    of 128-byte segments in the `.blb`. Each segment has a 32-byte header:
    `next, first, part, len, total`.
- **Evidence:** `ILUMINA_DISCS=… cargo test -p valentina`, with 4 tests passing:
  - **KJV:** all 31,102 verses with text. Record 1 is (1,1,1).
    Per-book verse counts are canonical (Genesis 1,533, Psalms 2,461,
    Revelation 404), and (book, chapter, verse) is unique.
  - **RA:** every one of the 31,104 records has a BLOB. Gênesis 1:1 is 43
    bytes, the same as the published text.
  - **Ency:** splits into tables; Articles has 1,774 records.
  - **All 15 `.dat` files** open.
  - `xtask valentina <file.dat>` dumps every table of all 15 databases to CSV
    in `work/valentina/` without errors (36 tables).
  - The same tests and the RA dump also pass on the Mac mini M4 (2026-10-30).
- Records are 1-based and stored in insertion order, not canonical order.
- Encyclopedia text uses HTML entities plus iLumina markup (`+~…`). That
  matches the `ImlToHtml` handler in the Lingo.
- **Still to do:**
  - field names and types from the `.vdb` schema;
  - honouring the deleted-record bitmap;
  - `.ind` indexes, which are optional because tables are small enough to scan in memory;
  - the SQL subset that the Lingo sends through `ExecuteSql` and `GetCursor`.

### QTVR ✅ graph and hotspots readable (2026-10-30)

The reader is `port/crates/qtvr`. It parses the movie's sample tables and
the QTVR 2.x atom containers, and includes an `smc ` decoder.

- **Tracks in `NZV.mov` (Nazaré):**
  - `qtvr`: 50 node samples, which are 28 panoramas plus 22 object nodes;
  - `pano`: 28 samples (`pdat` + `cuvw`, type `cube`);
  - `mp4v`: the 768² faces, 6 per node;
  - `smc `: the 1000² hotspot maps, 6 per node;
  - `obje` + `jpeg`: the object nodes;
  - 13 sprite tracks.
- **Nodes:** each has a name (`vrsg`, e.g. `NZV_01`), an id (`ndhd`) and
  hotspots (`hots` with `hsin` type `link`/`undf`). A link carries the target
  node and its pan/tilt/FOV.
- **Hotspot maps:** pixel value = hotspot id. The smc decoder uses exactly
  every byte of each sample.
- **Evidence:** `cargo test -p qtvr` with the discs:
  - In Nazaré, every link points to an existing node.
  - For all 28 panoramas, the ids found in the 6 hotspot maps are a subset of
    that node's declared hotspots.
  - **24 of the 25 `.mov` files in `QTVRs/` are tours, and all of them parse.**
    `Default.mov` is a plain movie.
- `xtask qtvr <tour.mov>` writes the graph as JSON to `work/qtvr/`.
- The disc tests and the JSON dump also pass on the Mac mini M4 (2026-10-30).
- **Faces:** ffmpeg decodes the MPEG-4 faces in about 2 s per tour. It logs MB
  errors, but the frames look right, so this belongs in `media-prep` (M4).
- **Next:**
  - object nodes (`obje`);
  - what the sprite tracks do (wired actions?);
  - a WebGL cube viewer that switches nodes from the hotspot maps (M4).

### dirplayer-rs ⚠️ parses the casts; the player was not tried (2026-10-30)

- Tested commit `68376fb` (GPL-3.0-only, Rust edition 2024, built for
  wasm-pack). The harness was **local only and not committed**: a throwaway
  crate that calls `read_director_file_bytes` natively on each of the 29 files.
- **All 20 `.cxt` casts load.** Their member counts are identical to `rifx`
  (for example BIAssets 322, GCode 166, MCAssets 349), so two independent
  parsers agree.
- **8 of 9 `.dxr` projector movies crash natively** inside
  `ScoreChunk::analyze_behavior_attachment_entries`. It evaluates the Lingo
  of behaviour parameters through the global player (`PLAYER_OPT`), which
  only exists after the wasm player has initialised. This is an API limit of
  the native harness, not a format failure.
- The decisive test, playing `Home.dxr` in the dirplayer web app or the
  standalone app, needs a browser build. The sandbox can't do it; it could be
  done by hand on the Mac.
- **What it would bring:** a Lingo VM, score playback and a renderer that
  already run several D7–D10 Shockwave titles.
- **What it doesn't have:** none of iLumina's Xtras (V4MD, TreeView,
  BuddyAPI, Mui, OSControl, QT3Asset), no QTVR, and no projector/MIAW focus.
  It targets `.dcr` web games.

### Not done in M0 yet ⏳

- **License decision:** MIT with our own engine, or a GPL-3 fork of dirplayer-rs. The owner decides.
- Hash gate against the manifest inside `xtask`.
