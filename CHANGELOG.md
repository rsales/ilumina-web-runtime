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
