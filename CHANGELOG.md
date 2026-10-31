# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Experiment branch `experiment/rust-director` (ADR 0008, which supersedes 0007
  with the outcome "redirect"): Rust Director MX player, WASM first. M0 so far:
  the `rifx` crate parses all 29 movies and disassembles all Lingo bytecode,
  `xtask inventory` exists, and the Valentina and QTVR spikes have started.
- `port/crates/valentina`: reader for the undocumented Valentina 1.x format
  (all 15 iLumina databases: KJV slot-heap text, RA/encyclopedia BLOBs) and
  `xtask valentina`, which dumps CSVs to `work/`.
- `port/crates/qtvr`: QuickTime VR reader (node graph, links, cubic view
  limits, `smc` hotspot maps; all 24 tours parse) and `xtask qtvr`, which
  writes JSON to `work/`.
- Port M1 (in progress): hash gate (`crates/discs`, `xtask verify`), bitmap
  decoding (all 1,288 `BITD`, palette from the user's projector), D6+ score
  and cast list, and a native compositor (`crates/director`, `xtask render`)
  that draws bitmaps and shapes for a score frame.
- Port M1 text: `rifx::text` parses styled text (`XMED`, Paige) and fields
  (`STXT` + `Fmap`), with per-document Mac Roman / Windows-1252 decoding;
  `director::text` lays out and draws it through a host `Fonts` trait;
  `xtask render` rasterises with fontdue over installed fonts (`ILUMINA_FONTS`
  adds folders) and `xtask texts` dumps text members to `work/texts/`.
- Port M2 slice 2 (in progress): `crates/player`, the host for the Lingo VM
  (virtual Windows file system like the original full install, movie and
  player properties, BuddyAPI, FileIO, prefs, dates). Home's `startMovie`
  runs to the end; `prepareMovie` reaches the Valentina database layer.
- Port M2 slice 1: `crates/lingo-vm`, a Lingo bytecode interpreter (values,
  lists, chunks, handlers across cast libs, parent scripts, `Host` trait).
  Every opcode on the discs is implemented; Home's movie events run until
  they need player functions.
- `xtask lingo`: disassembly per script and cast member (with script type)
  to `work/lingo/`, local only. M1 closed; M2 (Lingo VM) planned in slices.
- Port browser player: `crates/web` (stage behind a hand-written wasm ABI,
  fonts from the browser canvas), `port/web/` page and `xtask serve` (local,
  manifest-only file server). `discs::check_bytes` for fetched files.
- `discs::locate` / `discs::for_tests`: disc tests find the discs next to the
  repo without `ILUMINA_DISCS`, fail on a wrong value, and
  `ILUMINA_REQUIRE_DISCS=1` turns a skip into a failure. xtask checks its
  `<discs-dir>` argument and points at the right folder.
- Regression tests for each bug found so far: disc paths, the `.gitignore`
  rule that hid `crates/discs`, XMED encoding, Flash XMED, the XMED style
  count, and text-member clipping. AGENTS.md now asks for one per bug.
- `docs/history.md`: product history, 2005 system requirements and the
  publisher's media FAQ (from the Wayback Machine), and what follows for the runtime.
- Canonical documentation in `docs/`: use cases, architecture decision
  records, runbooks, and project memory (handoff + learnings).
- `AGENTS.md` (with `CLAUDE.md` pointing to it): objective, method, negative
  constraints and validation for AI agents and contributors.
- This changelog.

### Changed
- README rewritten around use cases.
- `docs/ARCHITECTURE.md` → `docs/architecture.md`, `docs/COMPATIBILITY.md` → `docs/compatibility.md`.
- `.gitignore` extended (secrets, OS/editor files, logs, state, disc images, app files).

### Fixed
- Build failed on the official `ubuntu:24.04` image (`useradd: UID 1000 is
  not unique`): the stock `ubuntu` user is removed first.
- The QuickTime installer timed out under emulation (Apple Silicon) with
  "wizard window not found (page 1)". The script now waits up to
  `QT_WAIT_SECONDS` (default 600), retries every page, and on failure prints
  the installer log and the running processes. Install steps resume after a
  restart instead of starting over.

## [0.1.0] - 2026-10-28

### Added
- Runtime image: Wine 9.0 (32-bit prefix), Xpra 3.1 HTML5 streaming with
  PulseAudio sound, all from the Ubuntu archive.
- Headless reproduction of the original iLumina full install, plus an
  unattended QuickTime 6.3 install from CD 1.
- `tools/verify-discs.sh` with the SHA-256 manifest of the 2.50 pt-BR disc set,
  and `tools/extract-discs.sh`.
- Optional `.env` auto-fill of the registration and activation dialogs
  (`ILUMINA_REG_*`, `ILUMINA_ACTIVATION_CODE`).
- Compatibility findings: Bible/Valentina, QuickTime video, QTVR and audio
  verified in the browser.
