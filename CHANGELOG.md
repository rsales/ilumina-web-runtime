# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
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
