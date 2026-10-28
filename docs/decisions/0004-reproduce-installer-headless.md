# 0004 — Reproduce the installer with scripts instead of running its GUI

- Status: Accepted
- Date: 2026-10-28

## Problem
`iLuminaInstaller.exe` works under Wine, but it needs a human: a licence
page, four disc swaps, a failing Start-menu step and then the QuickTime
wizard. A container must install unattended and reproducibly.

## Decision
- iLumina: `runtime/install-ilumina.sh` performs the same file operations
  as the real installer, plus `iLuminaInfo.txt` byte for byte and the
  Uninstall key.
- QuickTime 6.3 has no reliable silent mode. `runtime/install-quicktime.sh`
  drives the original wizard with xdotool. Windows are matched by **size**
  (the wizard retitles itself on every page), and a screenshot of each
  step goes to `/state/logs/screens`.

## Alternatives considered
- **Drive the iLumina installer GUI too**: fragile (disc-swap timing) and
  adds nothing, since the result is a plain copy.
- **Snapshot a finished prefix**: that would mean distributing the
  QuickTime binaries (see 0003).

## Consequences
- ✅ About 2 minutes, no interaction, the same result every time.
- ❌ The button offsets are tied to Wine 9.0's rendering. A Wine upgrade
  may need re-measuring: runbooks/debugging-gui-automation.md.

## Evidence
We ran the real installer under Wine, then diffed the installed tree
against the discs: 3,216 files, the only extras being `iLuminaInfo.txt`
and the `Prefs/` files the app creates on first launch.
