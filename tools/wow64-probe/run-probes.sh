#!/bin/bash
# Run inside the runtime container (see README.md in this folder).
# Uses a throwaway prefix in $HOME; /state is not touched.
set -u
P=/opt/ilumina-probe
export WINEPREFIX="$HOME/wow64-probe-prefix" WINEDEBUG=-all
echo "== 1. Linux: 32-bit compatibility mode inside a 64-bit process (no Wine)"
"$P/compat32"; echo "exit=$?"
echo "== 2. Wine: 64-bit Windows program"
wineboot -i >/dev/null 2>&1; echo "wineboot exit=$?"
wine cmd /c echo WINE64-OK 2>&1 | grep -v -E 'ALSA|fixme' | tail -3
echo "== 3. Wine: minimal 32-bit Windows program (hello32.exe, kernel32 only)"
wine "$P/hello32.exe" 2>&1 | grep -v -E 'ALSA|fixme' | tail -5; echo "exit=${PIPESTATUS[0]}"
