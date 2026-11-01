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
| M3 Bible | Book tree and Gênesis 1 text, same as in Wine | ⚠️ started 2026-11-01: tree, introduction and Gênesis 1 render natively (below) |
| M4 multimedia | Animations and tours play; QTVR node switches and hotspots work | ⏳ |
| M5 rest | Parity with `compatibility.md`, including Mui registration/activation running the original Lingo | ⏳ |
| M6 polish | Crisp scaling, static deploy, timings on the Mac mini M4 | ⏳ |

## M3 — Bible (started 2026-11-01)

`bible_book_tree` disc test (a ratchet): Home → Bible, click "Gênesis" in
the book tree, then the first verse reference of the introduction.

1. ⚠️ TreeView Xtra drawing and callbacks: rows (icons from the cast, font
   and colour per node, the clicked node highlighted), the sprite's centre
   as its registration point, hit testing, `TreeViewSingleClick`/
   `treeViewMouseEnter`/`Exit` to the behaviors (called without `me`, as
   Xtras do: `Vm::call_unbound`). ✅ The 66 books draw; the click selects
   Gênesis and the program opens its introduction ("BookIntro"). ⏳ Row
   metrics and the scroll bar against Boxedwine; mouse wheel.
2. ✅ `the frame` follows `go` at once (the tree's link went to its label in
   a sprite's exitFrame, then the frame script's `go the frame` undid it).
3. ✅ Introduction text from Valentina (HTML): text members clip to their
   sprite and scroll with `scrollTop`; `&#150;`-style entities read as
   Windows-1252.
4. ✅ Verse reference click → chapter view ("ViewSingle") with Gênesis 1
   from the database as `member.rtf` (new RTF reader: fonts, sizes, bold,
   italic, underline, colours, alignment, `\u`, `\'hh`). Built-in
   `hyperlinkClicked` on mouseUp over a link; `pointToWord`/`Line`/`Item`/
   `Paragraph`, `the clickLoc`, `the rightMouseDown`, `member.selection`,
   `member.name =`, field `border`/`margin` defaults.
5. Owner's comparison with Boxedwine (2026-11-01, 34872be): the page
   shows the tree, the introduction and Gênesis 1 like the native run.
   Differences fixed (each with a test): the tree's named root row
   ("LIVROS BÍBLICOS", children indented) and row height (font line, the
   spacing does not add); "LIVROS BêBLICOS" (a Mac Roman literal tied with
   the Windows reading: a letter whose case differs from both neighbours
   now counts against a decoding); HTML styles (`<b>`, `<i>`, `<u>`,
   `<font face size color>`, headings) over Director's HTML default instead
   of the member's first run; RTF `\deff`, superscript/subscript (`\super`,
   `\up`, a baseline raise in the text style); `the stillDown` reports the
   release after 200 reads in one event (the step budget ran out while
   dragging); `hilite … of field` sets `member.selection`.
6. ✅ (2026-11-01) The "Referência" panel shows the notes of Gênesis 1.1,
   as in Boxedwine: `pl.ilk` on a property list without an #ilk key is
   #propList (Db.Comp tested `ls.ilk = #propList`; VM test). The passage
   pop-up (a one-row TreeView) sits in its box: setting a TreeView
   sprite's rect keeps the centre as its registration point (the Xtra is
   known per member now). 1×1 spacer icons are not drawn. `ILUMINA_SQL_LOG`
   prints each query with its row count and the first row's sizes.
7. ⏳ Open: the body text in Georgia where Boxedwine shows a sans (Wine
   likely lacks Georgia: check on Windows); "O TíTULO" (the IML small caps
   uppercase ASCII only, so Windows may show the same); the Referência
   pop-up's caption (OSMenu Xtra) is empty; the tree's scroll bar.

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

1. ✅ `lingo-vm` core (2026-10-31): values (VOID, int, float, string,
   symbol, lists and property lists by reference, point, rect, script,
   instance, host refs), stack, params/locals/globals, jumps and repeat,
   arithmetic and comparison with Lingo's coercions, chunk expressions
   (get/put/delete, `the last item of`), `localcall`/`extcall`/`objcall`
   across cast libraries, parent scripts with `new` and `ancestor`, D8
   chunk references (`s.word[1].delete()`), D4-style `the … of sprite/member`
   (property-name tables from public research), and a `Host` trait for
   everything the player provides. Errors carry the handler stack.
   - 24 unit tests (hand-assembled bytecode decoded by `rifx`).
   - Disc test: all 446 scripts load and every opcode in the 119,934
     instructions is implemented.
   - Disc test: Home's real `prepareMovie` and `startMovie` run nested
     handlers (`SharedPrepareMovie` → `init` → `PathMgr.new`; `Lite.new` →
     `dCheck.new` → `GetNameEmailData`) until they need the player:
     `the applicationPath`, `the moviePath`, BuddyAPI (`baReadIni`,
     `baSysFolder`), FileIO, Valentina init, prefs. That list is slice 2.
2. ⚠️ Movie events (in progress, 2026-10-31): `crates/player` provides the
   host: a virtual Windows file system laid out like the original full
   install (`C:\Program Files\iLuminaPO\` = CD 1 `Program` + media of CDs
   2–4, the installer's `iLuminaInfo.txt`, `D:\` = CD 1, a writable overlay),
   movie/player properties, `the stage`, `systemDate` (real clock), member
   lookup, Director prefs, BuddyAPI (files, folders, INI, dialogs recorded)
   and FileIO.
   - Home's `startMovie` (HOCode) now runs to the end, including `Lite`
     and `dCheck` (the program's own demo/registration code, untouched).
   - `prepareMovie` runs PathMgr, Framework, SoundMgr, Nav, PaletteMgr,
     SpriteMgr, HelpMgr, IMLMgr, the database-update scan, opens
     `AppTables.vdb` through V4MD, passes the resolution check, and shows the
     **first-run registration dialog** "Registre Sua Cópia do iLúmina" (Mui:
     7 text fields, a default push button) — the dialog a fresh Wine profile
     shows. A user who only closes it is told "Ponha seu nome, por favor."
     and gets the dialog again (registration is required): the run stops
     after three identical dialogs. The shared `startMovie` fails only
     because objects created later in `init` (`goMenuMgr`) do not exist yet.
   - `player::mui`: the Mui Xtra (window/item property lists, `Initialize`,
     `Run` with events to the Lingo callback, `stop`, `ItemUpdate`, `Alert`)
     and a `Responder` for the user: `CloseAll` by default; `Owner` types the
     owner's own `ILUMINA_REG_NAME`/`EMAIL`/`CEP` (the Wine runtime's
     variables) into the text fields in order and presses the button, never
     generated values. The disc test masks those values in its output.
   - ✅ Owner run on the Mac (2026-10-31, values masked): the registration
     dialog accepts the owner's data, and `init` continues to `Poster.POST`,
     which needed `the actorList`. Now implemented, with offline NetLingo
     (`postNetText` … `netError` = 4146, as on Wine; only host names are
     recorded). The owner-mode test asserts it never stops at the
     registration dialog or `the actorList` again.
   - ✅ Post-registration managers (component disc test
     `home_managers_build`, no owner data needed): all 14 objects `init`
     creates after `POST` build (AddOnMgr, the five `Db.*` databases, Bible,
     Find, MenuMgr, Anims, Photos, Maps, Audio, Db.TT), then
     `UnlockAnimations`/`UnlockGTVs` and `new(script "Home Module")` run.
     Fixed on the way: `member.text = …` for fields/text members (texts are
     now read from the casts), `rgb()` colors, the BinaryIO Xtra (writes are
     byte patches in the virtual FS; originals untouched) and
     `register(xtra …)` (an Xtra vendor's licence string; a no-op here).
   - CMediaLock: at every start the program rewrites the first 25 bytes of
     83 media files (`Media.vdb`/`Tours.vdb` `bin` column). The patched files
     are valid QuickTime/MP4 headers (asserted on three). The disc copies
     already start with valid headers. ⏳ Check whether the patch bytes
     equal the disc bytes, which matters for `media-prep`.
   - ✅ Owner rerun (2026-10-31): `prepareMovie` completes with the owner's
     data (one registration dialog, no alerts); `startMovie` stopped at
     `sendSprite` → slice 3.
   - The disc test is a ratchet: it asserts where each event stops and fails
     if the run regresses or gets further without the table being updated.
   - The host reports a 1024×768 desktop like the Wine reference
     (`ILUMINA_RES`); at 1280×1024 the program offered to change the display.
   - On a fresh profile the program shows "Unable to read registration"
     (no `C:\WINDOWS\irkvbp.txt` yet), as a new Windows install would.
   - ⏳ Unverified: integer `x mod 0` (reached by the original code on a fresh
     profile) is treated as `x mod 1` with a warning; confirm on Wine that no
     script error appears at first launch.
3. ⚠️ Sprites and behaviors (in progress, 2026-10-31):
   - `rifx::score` reads sprite intervals and their behaviors (cast member +
     the author's settings, a Lingo property-list string). Home: 63
     intervals, 102 behaviors.
   - `player::stage`: sprite channels from the score, behavior instances
     (script properties + settings + `spriteNum`; `new` is not run, ⏳),
     `beginSprite`, frame events (behaviors → frame script → movie scripts),
     `sendSprite`/`sendAllSprites` (unhandled → movie scripts → ignored),
     and `start_movie` in Director's order (prepareMovie, beginSprite,
     prepareFrame, startMovie, enterFrame, exitFrame).
   - Sprite properties (member, loc, rect, size, visible, ink, blend,
     colors, `scriptInstanceList`; unknown ones go to the sprite's behaviors,
     then to Xtra sprites), `rollover()`, `the rollOver`, mouse position
     (off stage until the page sends one), `the stageLeft…`, member
     `fileName` (linked media under the program folder, ⏳), `html`, `media`,
     style properties, `regPoint`, `count(#char)`; `value()` now evaluates
     the expressions found in settings (`"…" & QUOTE`, `(member n of castLib
     m)`, `sprite n`); rect functions; point/rect arithmetic with lists.
   - **Bug found on the way:** cast member numbers ignored the cast's first
     member (`MCsL` min member, read in the wrong byte order). GlobalAssets
     starts at 2, so every sprite and score reference into it was one member
     off, in M1 rendering too. Fixed in `rifx` (`members_from`), the VM,
     the host and the compositor.
   - ✅ Owner rerun (2026-10-31): with the owner's data the startup stopped
     exactly where the component test did (`charPosToLoc`).
   - Text measuring: `director::text::Measure` (the renderer's own layout)
     gives `charPosToLoc`, `locToCharPos`, `linePosToLocV` and a text
     member's `height`/`lineCount`; the player measures with the page's
     fonts or `ApproxFonts` (Arial-like proportions, ⚠️ line breaks may
     differ from Director until real fonts are wired in). `updateStage`
     and `nothing` added.
   - ✅ Component test: **the whole startup completes** — beginSprite,
     prepareFrame, startMovie, enterFrame, exitFrame of frame 1 (2026-10-31).
     ⏳ Owner run expected to do the same.
   - ✅ Owner run (2026-10-31): all six startup steps OK with the owner's
     data, same as the component test.
   - ✅ Run-time rendering (2026-10-31): `PlayerHost::draws()` turns the
     sprite state into a draw list for `director::Stage::render_draws`
     (member, loc, size, ink, colours, run-time text with Lingo's
     font/size/style/colour, linked media). Linked media decode in Rust
     (JPEG via zune-jpeg, uncompressed BMP). Sprites show their member at
     its own size unless the score marks them stretched (ink byte 0x80).
     `xtask play <discs> Home.dxr` runs the startup (owner's data from
     `ILUMINA_REG_*`) and writes the PNG; the component test writes one with
     `ILUMINA_RENDER=<png>`. Home now shows the welcome picture, the module
     menu bar (BMP buttons), "Estudo Pessoal" under MAIS, the clickable text
     and the right icon states, close to the Wine reference.
   - Fixed on the way: score `left/top` is the registration point (was
     treated as the corner); shapes store line size + 1 (the invisible
     "MouseRollBlocker" was drawn as a box).
   - ✅ Compared with the owner's Boxedwine screenshot of the same state
     (2026-10-31), three bugs fixed:
     - `the last char of s` (index -30000) selected the first char: the
       program's own `SearchAndReplace` left "BClique" in the welcome text
       and a trailing return in the add-on list. Fixed in `chunk`; also an
       explicit empty range (`s.char[1..0]`) is "".
     - Empty text members lost their authored style ("GOLD" in Verdana 9
       bold grey, the add-on list): XMED now keeps a zero-length run.
     - Uncompressed 16/32-bit bitmaps are interleaved, not planar: the
       scroll bar and the minimize/close icons were coloured noise.
   - ✅ Second comparison (owner's run vs Boxedwine): fixed the scroll-bar
     track (a stretched sprite's rect used the unscaled reg point), the
     add-on list's right alignment (lost when Lingo replaced the text), and
     hyperlinks (`member.hyperlinks` from `<a>`, drawn underlined).
   - ⚠️ Still different: the welcome text wraps to two lines (font metrics;
     the Wine reference substitutes Verdana), and the module
     slideshow/rollover (frame loop + mouse).
   - The startup reads 58 disc files, 176 MB (every database opened whole,
     BLOBs included). For the browser: open Valentina BLOBs lazily.
   - ⏳ Browser (2026-10-31, built on the Mac only): `web::App::play` runs
     the startup over the files the page added; files the run reads that are
     missing come back as a list, the page fetches them and runs again from
     the start (Lingo is deterministic). `play_render` draws `draws()`,
     fetching linked media the same way. Registration form for the owner's
     data in the page. Native test `play_fetch_loop_over_the_discs` runs the
     same loop (no owner data: stops at the registration dialog). 64 MB wasm
     stack (`port/.cargo/config.toml`).
   - Before: wire `player` into the browser page (fetch the files the run
     needs, run the startup, draw `draws()`), then the frame loop (`go`,
     labels, endSprite, stepFrame) and mouse events.
   - ✅ Browser run on the Mac (2026-10-31): the page ran the whole startup
     with the owner's data and drew Home like `xtask play` (92 ms after the
     files were cached).
   - ✅ Frame loop (2026-10-31): `stage::step` moves the playhead (`go` to a
     frame or label, else the next frame), ends/begins sprites by score
     interval (`endSprite`/`beginSprite`; puppets stay), sends `stepFrame`
     to the actorList, then prepareFrame/enterFrame/exitFrame. Frame labels
     (`VWLB`), `label()`, `marker()`, `the frameLabel`, `the lastFrame`;
     the frame script now comes from the script channel's interval (the old
     record read was garbage). `the paramCount`/`param(n)` in the VM.
     Component test: Home holds on frame 2 ("trigger loop" calls
     `oLite.TriggerLoop`, then `go the frame`) for 45 steps without errors;
     the Lite object shows the "Ativação do iLúmina" dialog once (closed by
     the responder: the trial path).
   - Lingo string literals mix Mac Roman and Windows-1252 (even within a
     cast): bytes 0x80–0x9F without Windows accents ≥ 0xC0 decode as Mac
     Roman ("Ativação", "Código", "ícone"). ⏳ confirm on Wine.
   - The `Owner` responder answers only the registration dialog; other
     dialogs are closed.
   - Browser: `play_step` at 15 fps (Director's default tempo), files a step
     reads are fetched and the session replayed (step log); `go to movie`
     is reported, not followed yet.
   - Owner's browser run of every module (2026-10-31): Home, TimeTravel and
     MediaCenter render their first screens; playback stopped at `do`.
     Bible, Encyclopedia, Extras and MyStudyCenter stopped at
     `set the keyboardFocusSprite` in the registration callback (each movie
     opened fresh asks to register again). Both fixed.
4. ⚠️ Events and movies (2026-10-31):
   - `do "lingo"` (`lingo_vm::doeval`): calls, `obj.method()`, `global` +
     assignments, expressions. Poster's offline callback now runs.
   - Mouse (`stage::mouse`): mouseEnter/Leave/Within as the pointer
     crosses sprites, mouseDown to the sprite under it, mouseUp (or
     mouseUpOutside) to the clickOn; behaviors, then frame script, then
     movie scripts. `the mouseDown`, `the clickOn`, `the mouseMember`,
     `cursor()`. Component test: the pointer over a module highlights it
     (rollover), the click goes to the Encyclopedia movie.
   - `go to movie` (`player::switch_movie`): endSprite + stopMovie, the new
     movie's casts are appended to the same program (objects in globals
     keep their scripts; name lookups see only the current movie) and the
     same host (globals, actorList, files, clock). Encyclopedia's startup
     completes after Home's.
   - Dialogs wait for the user: the responder returns no answer, the run
     stops with the dialog's items and geometry, the page draws it over the
     stage, and the answer replays the session (`PageUser`, answers in
     order). The owner's saved data still fill the registration dialog.
   - Page: mouse events on the canvas, Mui dialogs drawn over the stage,
     movie changes followed (the selector updates). Session log (frames,
     mouse, movies) replayed after fetching files or answering.
   - Owner's browser run (2026-10-31): the activation dialog appeared as a
     page dialog. Fixed after it: Mui labels show their value; radio buttons
     hold 1/0 and are exclusive; choosing a radio no longer submits; `quit`
     ends the run with a message; Mac Roman literals with uppercase accents
     ("ATIVAÇÃO", "CÓDIGO") decode by letter score.
   - The owner's activation code (`ILUMINA_ACTIVATION_CODE`, or the page's
     form) is typed like `auto-activate.sh` does: choose "Possuo o meu Código
     de Ativação", then the code field. The program checks it as always
     (ADR 0005). The phone numbers #1/#2 come from `EncodePhoneData` over the
     registration name/e-mail, so a code issued for the same data should
     validate. ⏳ owner to confirm.
   - Owner's browser run (2026-11-01): typing the code by hand left
     "Continuar" disabled; the 15-day trial path and side navigation worked.
     Fixed: the page sends text edits live (`itemChanged` after a 500 ms
     pause, dialog redrawn), disabled items take no clicks, and the page's
     owner now answers both activation steps too (only registration before),
     so with the owner's code the page shows no activation dialog. A step
     asked again (code not accepted) goes to the page. ✅ owner's browser
     run (2026-11-01, a4fb143): no activation dialog with the owner's code;
     Home rollovers (Multimídia, Viagem no Tempo) work. Seen in the status
     line: `not implemented: handler #puppetSound` (sound is next).
   - Hit testing (2026-11-01): matte-ink (8) sprites take the mouse only
     on drawn pixels (cast bitmaps decoded on demand in the host, same
     white-is-clear rule as the renderer; ⏳ Director's edge-connected
     matte). Other inks use the rectangle, as in Director. Unit test.
   - Files the program writes (2026-11-01): `Vfs::export_state`/
     `import_state`; the page keeps them in sessionStorage (localStorage
     with "lembrar"), next to the owner's data, and every run starts with
     them, as a Windows install finds its files again. Nothing is reset
     or altered; the dialogs are never skipped (ADR 0005). Native test
     (state round trip) and web test (kept file survives a new session).
     ⏳ owner: reload the page after activating.
   - Keyboard (2026-11-01): `stage::key` (keyDown/keyUp to the focus
     sprite, frame script, movie scripts; `pass`/`dontPassEvent`/
     `stopEvent`; typing into an editable field, ⏳ caret/selection);
     `the key`, `the keyCode`, modifiers. Page sends Mac key codes and
     Director key characters while playing. Unit test. ⏳ owner.
   - Performance (2026-11-01): disc bytes are shared across replays (the
     page keeps fetched files as `Rc<[u8]>`), `.blb` files are read on the
     first query that needs a Text field, BinaryIO reads 4 KB windows. The
     startup reads 52 MB instead of 176 MB (native count); the web
     play-loop test went from ~37 s to ~3 s. The page still fetches whole
     files (the manifest hash checks whole files only).
5. ⚠️ Gate: Home navigates to each module (2026-11-01, `home_modules`
   disc test, a ratchet): a fresh Home per module, click in the side
   navigation, `go to movie` followed, startup and a second of playback.
   ✅ Bible (book tree: 66 nodes), Encyclopedia, TimeTravel, MediaCenter,
   Extras (tree: 7 nodes), MyStudyCenter. Fixed on the way, each with a
   unit test: `locVToLinePos`, member chunk refs (`member.line[n].color`,
   chunks of chunks, `setProp(#line…)`), `sprite.updateControl`,
   `sprite.quad`, `hexString`, member `hyperlinks`/chunk `hyperlink`/
   `hyperlinkRange`/`hyperlinkState`, `pointToChar`/`pointInHyperlink`,
   `#paragraph` chunks, `member.member`, `scrollTop`, `useHypertextStyles`,
   `puppetSound` (remembered, silent until M4), timeouts, and the TreeView
   Xtra model (`*XTV`: nodes, names, icons, fonts, user data; ⏳ drawing
   and its mouse callbacks, M3). ⏳ SSIM against Wine screenshots.
   - Owner's browser run (2026-11-01, 281eb9e): Encyclopedia, Multimídia,
     Viagem no Tempo and Extras open and draw. ❌ The activation dialog came
     back saying 0 days left, and the owner's code was not typed in (the
     radio buttons drew on top of each other). ❌ Encyclopedia:
     `not implemented: the mouseUp`.
   - Found: `systemDate` was fixed at 2026-01-01 (nobody set the date);
     the page now passes its date and `xtask play` the system date. Test.
     The trial logic itself is not inspected or changed (ADR 0005).
   - `the mouseUp` added (Encyclopedia). Test.
   - Owner's `xtask play` with the dialog log (2026-11-01): registration
     accepted; the activation dialog shows "Restam 15 dias"; the owner chose
     "Possuo o meu Código…" but the dialog closed without the code step.
     Cause: the answer set every radio button, so the last `itemChanged`
     named "Não quero ativar agora" (the trial path). Fixed in the owner's
     answer and in the page (only the chosen radio is reported). Tests.
   - ✅ Owner's `xtask play` (2026-11-01, 89fdc28): registration, then
     "Possuo o meu Código…", then the code typed into the field next to
     "Código de Ativação:"; the original Lingo accepted the owner's own code
     and showed the alert "Obrigado por ativar o iLúmina!". Home keeps
     playing (frame 2). First real activation in the Rust port (ADR 0005:
     the program checked the code; nothing bypassed).
   - ✅ Owner's browser run (2026-11-01, 0e65ccd): with the owner's data in
     the page form, registration and activation ran with no dialog shown;
     after a reload Home opened directly (the program found its own files);
     Encyclopedia and Viagem no Tempo opened. ❌ Typing into the page's
     registration dialog did not work (each pause replayed the whole run,
     the fields were disabled meanwhile; dark inputs). ❌ Status:
     `set member.rect` and `set member.width` on text members.
   - Fixed: no live replay while typing; edits go out with the button click
     and disabled buttons stay clickable (the player drops the click if the
     program left the button disabled after `itemChanged`); white inputs.
     Text members take `rect`/`width`/`height` (box width for measuring).
     Tests (host; page logic by review). ⏳ owner.
   - Owner's browser runs (2026-11-01, ccba603): fields in the page's
     registration dialog take typing (white); OK did nothing (incognito).
     Cause: Mui gives dividers a height of 210, so the divider at y 305 lay
     over the OK button and took the click. Dividers now draw as a line and
     labels/dividers ignore the mouse. The Bible module opens (start page,
     book panel empty until the TreeView draws). `textStyle`/`textFont`/
     `textSize` on members added (Bible). Test for the member props; the
     page fix by review. ⏳ owner.
   - ✅ Owner's browser run (2026-11-01, d2404bb, incognito): registration
     typed into the page's dialog, OK, then "DIGITE O SEU CÓDIGO DE
     ATIVAÇÃO", the owner's code typed and accepted by the program; Home,
     Bible, Multimídia and Encyclopedia then opened. ❌ Picking Home again
     in the page asked for registration again (the new session started from
     the files of the page load). Fixed: a new session keeps the files the
     last one wrote. Web test.
   - `xtask play` prints each dialog (items, masked owner values) and the
     answer, to see why the owner's answer is not taken.

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

### Valentina ✅ schema, records and SQL subset (2026-10-31)

- `valentina::schema` reads table and field names from the `.vdb` (itself a
  Valentina file: system table `0x7ffffffe` holds property rows — object
  kind/property id, text value, owner row). Tables: 34 name, 36 id; fields:
  42 name, 44 type, 56 max length; fields are numbered from 2 in the `.dat`.
  Checked: 15 databases, 49 tables, 266 fields, each with its `.dat`
  segment (the `.dat` catalog carries the table id at offset 16).
- Types by measured width: 3 byte, 4 signed short, 5 unsigned short, 7 long,
  9 float (no data on the discs; assumed 4 bytes), 10 String (`max+1`
  bytes, NUL-terminated), 17 Text (pointer into the `.blb`), 26 VarChar
  (slot heap).
- `valentina::db` reads every column of every table: 1,166,375 cells and
  317,535 non-empty texts across the 15 databases.
- `valentina::sql`: the SELECT subset the scripts build (DISTINCT, `*`,
  MIN/MAX/COUNT/SUM/AVG, WHERE with comparisons, LIKE, AND/OR/NOT,
  parentheses, Valentina's `no_case`, two-table joins, GROUP BY, ORDER BY).
- `player::v4md`: `VDatabase`, `VCursor` and the `Valentina*` functions;
  records come back as property lists keyed by field-name symbols. Writes
  (`SqlExecute`) are recorded, not applied yet.

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
