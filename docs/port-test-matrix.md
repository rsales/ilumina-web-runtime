# Rust port test matrix (ADR 0008)

Each milestone has a time-box and a go/no-go gate. Mark ✅ / ⚠️ (works with
issues: say which) / ❌ (fails: paste the first error) / ⏳ (not run).
Evidence must be structural (counts, hashes, timings). Screenshots and dumps
stay in `work/`, never in git.

| Milestone | Gate | Status |
|---|---|---|
| M0 spike | `rifx` parses all 29 movies; inventory exists; Valentina and QTVR are readable | ✅ go (2026-10-30): parser, Valentina and QTVR done; license MIT, own engine |
| M1 static Home | First Home frame in the browser, close to the Wine reference (SSIM) | ✅ done (2026-10-31): every module renders its score frames in the browser (wasm, Mac M4), 2–10 ms per frame; pixel parity needs Lingo state, so SSIM moved to M2 |
| M2 interactive Home | Score + Lingo VM + events; module buttons navigate | ⏳ started 2026-10-31 (slices below) |
| M3 Bible | Book tree and Gênesis 1 text, same as in Wine | ⏳ |
| M4 multimedia | Animations and tours play; QTVR node switches and hotspots work | ⏳ |
| M5 rest | Parity with `compatibility.md`, including Mui registration/activation running the original Lingo | ⏳ |
| M6 polish | Crisp scaling, static deploy, timings on the Mac mini M4 | ⏳ |

## M2 — interactive Home (started 2026-10-31)

Time-box: 3 weeks. Stop rule (ADR 0008): if Home does not navigate by then
and the VM keeps growing without closing the gap, stop and record it.

Startup path (from `xtask lingo`, local only): `prepareMovie` →
`SharedPrepareMovie(#module)` (GCode movie scripts) → `startMovie` (creates
`script("Lite")`, the demo/activation object, run as-is) → behaviors'
`beginSprite`, `prepareFrame`, `exitFrame` (`go the frame` loops). Code lives
in `*Code.cxt`: GCode is shared (18 movie, 39 parent, 109 score scripts),
HOCode has Home's (2 movie, 5 parent, 15 score).

Slices, each with its own check:

1. ⏳ `lingo-vm` core: values (int, float, string, symbol, void, list,
   prop list, object), stack, locals/params/globals, jumps, arithmetic and
   comparison, `localcall`/`extcall` to handlers across scripts. Unit tests
   per opcode.
2. ⏳ Movie events: run `prepareMovie`/`startMovie` of the movie scripts
   across cast libs; log every builtin and Xtra call that is missing.
3. ⏳ Sprites and behaviors: `beginSprite`, `prepareFrame`, `exitFrame`,
   sprite properties (`member`, `loc`, `visible`, `ink`, `blend`), `go`,
   `the frame`, frame labels, puppets. Home frame 1 then shows Lingo's state.
4. ⏳ Events: mouse (`mouseUp`, `mouseEnter`, rollovers) and keyboard from the
   page; `go to movie` loads another module.
5. ⏳ Gate: Home navigates to each module; SSIM against Wine screenshots.

## M1 — static Home (started 2026-10-30, done 2026-10-31)

### Hash gate ✅

- `port/crates/discs` embeds `manifests/ilumina-gold-2.50-ptbr.sha256`. Each
  file the engine loads goes through `check_file`, so an unknown or modified
  file is refused.
- `xtask verify <discs>` prints `OK: 3297 files match ilumina-gold-2.50-ptbr`.
  It took 2.5 min in the sandbox, which is I/O-bound.

### Bitmaps ✅ (8-bit colours ⚠️)

- `rifx::bitmap` reads the member header (pitch, rect, reg point, depth,
  palette) and decodes `BITD`, raw or RLE. 16-bit is planar RGB555 and 32-bit
  is planar ARGB.
- **1,288 of 1,288 `BITD` decode.** By depth: 1,137 at 16 bits, 74 at 32, 68 at
  8, 9 at 1. The other 29 bitmap members are linked JPEGs under `Files/Media`
  or empty placeholders whose image Lingo sets at runtime. Checked by
  `cargo test -p rifx --test bitmaps`.
- **8-bit palette:** "System - Win" (-101) is read from the user's own
  `iLumina.exe`, where it sits as 256 BGRX entries, instead of being embedded.
  The index→colour mapping still needs to be confirmed against Wine.
- `xtask bitmaps <discs>` exports every bitmap as PNG to `work/bitmaps/`.

### Score and cast list ✅

- `rifx::score` reads the D6+ `VWSC` frame stream (delta-compressed,
  48-byte records, 1,006 channels) and `MCsL` (cast libs → external `.cst`,
  which ship as `.cxt`).
- `Home.dxr` has 160 frames. Frame 1 has 49 sprites across five cast libs
  (Internal, LocalCode, LocalAssets, GlobalCode, GlobalAssets).
- `xtask score <movie.dxr> [frame]` lists the sprites.

### Text ✅ parsed, ⚠️ host fonts (2026-10-31)

- `rifx::text` parses **all 88 styled texts** (`XMED`, the `text` Xtra
  member, Paige document version 0x40001) and **all 92 fields** (`STXT`).
  Two more `XMED` chunks are Flash Asset members (SWF data) and are skipped.
  Checked by `cargo test -p rifx --test texts`.
  - Runs carry font, size, bold/italic/underline and colour. The style table
    has one leading value, then fixed 77-value records (count in the section
    header). Paragraph records are 54 values; the first is the justification
    (80 left, 8 right, 3 centred).
  - Field font ids (0x8001…) resolve through each file's `Fmap`: every id
    used by an `STXT` run is found.
  - **Encoding is per document.** The text-block flags (section 1) end in
    0x81 for Mac OS Roman (6 documents, where `0x92` is "í") and 0x84 for
    Windows-1252. Fields are Windows-1252.
  - Fonts used: Verdana (most), Georgia, Times New Roman, Geneva, Arial,
    Helvetica, MS Sans Serif, Courier (New), Charcoal; 8–26 pt.
- `director::text`: a `Fonts` trait (the host rasterises), layout with word
  wrap and `\r` breaks, alignment, underline, field border + margin and
  alignment from the member. Point size = pixels (72 dpi stage).
  - Text members are not clipped to the score height (they grow to fit);
    fields clip unless their box type is "adjust to fit".
- `xtask` rasterises with fontdue over host fonts: the real faces when
  installed (macOS has Verdana, Georgia, Times New Roman, Arial, Courier New,
  Geneva and Microsoft Sans Serif), then Liberation/DejaVu. `ILUMINA_FONTS`
  adds folders. The render prints which file each family used.
- `xtask texts <discs>` dumps every text member with its runs to
  `work/texts/` (local only).
- **Home frame 1 has no static text:** its text members (`txt_Editions`,
  `AddOns`, `txt_Clickable`) are empty in the cast and Lingo fills them. The
  pipeline was checked on frames that do hold text (Bible 30, MediaCenter 99):
  wrapping, accents, styles and right alignment look right by eye.
- **Mac M4 (owner, 2026-10-31):** `render ../../discs Bible.dxr 30` used the
  real faces (Verdana, Verdana Bold, Georgia, Microsoft Sans Serif from
  `/System/Library/Fonts/Supplemental`), and Mac Roman text decodes ("Isaías").
  Compared by eye with Boxedwine on 1 Samuel 3 (`work/reference/`), the
  static parts line up: the panel frames, positions, fonts and sizes. The
  differences are runtime state again:
  - Score frame 30 stacks every panel of the screen (index, photo, map,
    titles "Índice/Mapa/Cruz" on top of each other). Lingo shows one at a
    time, so sprite visibility and puppeting come first in M2.
  - The chapter text, the reference notes and the "1 samuel" header come from
    Valentina through Lingo (M3).
  - Windows draws small text without anti-aliasing; the port always smooths.
- ⚠️ In the sandbox Verdana falls back to DejaVu Sans (wider), so line breaks
  differ from Windows. On the Mac the real faces are used. Director's
  anti-aliasing threshold and kerning are not modelled.

### Browser player ✅ (Mac M4, owner, 2026-10-31)

- `cargo build --release -p web --target wasm32-unknown-unknown` and
  `xtask serve ../../discs`: the page loads every module (Home, Bible,
  Encyclopedia, TimeTravel, MediaCenter, Extras, MyStudyCenter) and renders any
  frame in 2–10 ms. Fonts come from macOS through the canvas.
- Compared by eye with Boxedwine (`work/reference/`): Encyclopedia frame 10,
  TimeTravel frame 7 and MyStudyCenter frame 10 match in layout, photos and
  text. Bible frame 4 matches except the book tree. The remaining differences
  are runtime state:
  - the module menu bar (":: BÍBLIA :: ENCICLOPÉDIA …") and the search field
    texts, built by Lingo;
  - navbar icons in their roll/down state;
  - TreeView contents (book list, Extras tree) and Valentina text (M3);
  - MediaCenter's title position/scale and TimeTravel's header bitmap.
- Fonts: where the member asks for Georgia, the port draws Georgia; Boxedwine
  draws a sans-serif (Wine has no Georgia). The Wine screenshots are not a
  font reference.

#### Written 2026-10-31

- `port/crates/web`: the stage behind a hand-written wasm ABI (`alloc`,
  `file_add`, `needed_files`, `load`, `render`, `frame_ptr`…), so the only
  requirement is the `wasm32-unknown-unknown` target. Fonts are imported from
  the page (`js_glyph`, `js_line`), which draws each glyph once on a canvas.
- `port/web/index.html` + `app.js`: module picker, frame field/slider/keys,
  URL `#Movie/frame`, timings in the status line.
- `xtask serve <discs> [port]`: std-only server on 127.0.0.1 that serves the
  page, the wasm and only manifest-listed disc files.
- Checked in the sandbox (no wasm target here):
  - `App` end to end natively over the real discs: Home loads its 4 casts
    through the hash gate, 1024×768, 160 frames, frame 1 renders.
  - The `wasm32` branch type-checks (`cargo check` with the cfgs flipped).
  - A test keeps the page and the module in agreement on export/import names.
  - Server: page, `app.js` and a disc file served; traversal and non-manifest
    paths refused; missing wasm reported with the build command.
  - `js_glyph`/`js_line` in headless Chromium with a fake wasm memory:
    Verdana 10 gives plausible boxes ("g" descends 3 px, space has advance
    only), ascent 9 / descent 2.
- ⏳ First real run: build the wasm on the Mac and open the page.

### Compositing ⚠️ native only

- `port/crates/director`: `Stage::load` (through a `Files` provider; natively
  the hash-gated disc reader) and `render(frame)`.
  - Bitmaps are placed at loc − reg point and scaled to the sprite rect.
  - Inks: copy; white as transparent for inks 1, 8 and 36; 32-bit alpha; blend.
  - Shapes: rectangle, oval and line, in the sprite's RGB or palette colour.
  - Text and fields (see Text above).
- `xtask render <discs> Home.dxr 1` → `work/render/Home-1.png`.
  - Home frame 1: 33 sprites drawn, 16 skipped (linked/empty bitmaps, two
    sprites pointing at members that don't exist, Xtra members without text).
  - **MediaCenter frame 1, compared by eye with a Boxedwine screenshot:**
    the layout, the purple bars, the circles, the photos and the connecting
    lines match.
- **Missing:**
  - linked JPEGs, which Lingo loads at runtime;
  - some positions that Lingo changes at runtime (the "a" before "Bíblia");
  - the browser canvas (WASM);
  - SSIM against a Wine screenshot of Home (none has been captured yet).
- **Compared with Boxedwine screenshots of Home and Multimídia (owner,
  2026-10-31; kept in `work/reference/`):** most of the gaps are runtime
  state that **Lingo** sets after the movie starts. The score's frame 1
  doesn't contain it:
  - The navbar icons in the score are the `-roll`/`-down` members. Lingo
    swaps them to the normal/gray states.
  - The Home photo, the module panel ("BÍBLIA …") and the menu-bar buttons
    are linked JPEGs or empty bitmaps that Lingo fills in.
  - The menus, captions and "MAIS: Estudo Pessoal" are `text` Xtra members
    and fields whose text Lingo sets (empty in the cast).
  - In Multimídia, the title's position and scale differ from frame 1. Lingo
    or another frame/label changes them.
  - **Consequence:** pixel parity with Wine needs the Lingo VM (M2), not just
    better static rendering. M1 now aims at "everything the score and casts
    hold renders correctly" (bitmaps ✅, shapes ✅, text ✅). Comparison with
    Wine moves to M2.

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

- **License:** decided MIT with our own engine (ADR 0008). dirplayer-rs is a reference only.
- Hash gate against the manifest inside `xtask`.
