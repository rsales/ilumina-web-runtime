#!/bin/bash
# Build the iLumina "overlay" for Boxedwine from the user's discs.
#
#   make-overlay.sh DISCS_DIR OUT          -> OUT/ilumina.zip   (web: ?overlay=ilumina)
#   make-overlay.sh DISCS_DIR OUT --dir    -> OUT/ (unpacked; native: merged into -root)
#   add --isos to also write OUT/cd1..cd4.iso (web: ?cdrom=d:cd1.iso;...)
#
# The layout mirrors runtime/install-ilumina.sh (the original installer's full
# install, verified by diff), but inside Boxedwine's guest file system:
#   home/username/.wine/drive_c/Program Files/iLuminaPO/...   (32-bit prefix)
#   home/username/run.bat                                      (launcher)
# Nothing is executed here: it's a plain file copy + zip.
set -euo pipefail
DISCS="${1:?usage: $0 DISCS_DIR OUT [--dir] [--isos]}"; OUT="${2:?}"; shift 2
MODE=zip; ISOS=0
for a in "$@"; do case "$a" in --dir) MODE=dir;; --isos) ISOS=1;; esac; done

for n in 1 2 3 4; do [[ -f "$DISCS/cd$n/$n.txt" ]] || { echo "missing $DISCS/cd$n/$n.txt" >&2; exit 1; }; done
mkdir -p "$OUT"
if [[ $MODE == dir ]]; then STAGE="$OUT"; else STAGE="$(mktemp -d)"; trap 'rm -rf "$STAGE"' EXIT; fi

APP="$STAGE/home/username/.wine/drive_c/Program Files/iLuminaPO"
mkdir -p "$APP"
echo "copying program files from CD 1"
tar -C "$DISCS/cd1/Program" \
    --exclude='./iLumina' --exclude='./Uninstall' --exclude='*:rsrc' \
    --exclude='._*' --exclude='.DS_Store' --exclude='./Xtras/Mac' --exclude='./Prefs' \
    -cf - . | tar -C "$APP" -xf -
for n in 2 3 4; do
  echo "merging media from CD $n"
  tar -C "$DISCS/cd$n/Files" --exclude='._*' --exclude='.DS_Store' -cf - Media | tar -C "$APP/Files" -xf -
done
chmod -R u+w "$APP"
# Same bytes as the original installer writes (see docs/compatibility.md).
printf '%s' '[#applicationPath: "C:\Program Files\iLuminaPO\", #Full: -1, #DVDOrCD: #cd, #DvDDrive: "", #CDDrive: "D:\", #edition: "Gold", #version: 2.50, #installRanFromApp: 0]' \
  > "$APP/iLuminaInfo.txt"

# DirectDraw through Wine's GDI renderer: QuickTime otherwise asks for an
# OpenGL window, which fails without GPU/GLX ("Couldn't find matching GLX
# visual") and the web build has no 3D anyway.
# First run: the original QuickTime 6.3 installer, clicked through once by the
# user (state persists: IndexedDB on the web, /state on native). Then iLumina.
printf '%s\r\n' \
  '@echo off' \
  'reg add "HKCU\Software\Wine\Direct3D" /v renderer /d gdi /f >nul' \
  'if not exist "C:\windows\system32\QuickTime.qts" (' \
  '  cd /d "C:\Program Files\iLuminaPO\ThirdParty\QuickTime"' \
  '  start /wait QuickTimeInstaller.exe' \
  ')' \
  'cd /d "C:\Program Files\iLuminaPO"' \
  'start /wait iLumina.exe' \
  > "$STAGE/home/username/run.bat"
# Layers 3-5 of the test plan: start iLumina WITHOUT installing QuickTime, to
# separate Director/Xtra problems from QuickTime problems.
printf '%s\r\n' '@echo off' 'reg add "HKCU\Software\Wine\Direct3D" /v renderer /d gdi /f >nul' 'cd /d "C:\Program Files\iLuminaPO"' 'start /wait iLumina.exe' \
  > "$STAGE/home/username/run-noqt.bat"
# Also at C:\ so they can be started by absolute path (wine's cmd doesn't
# start in /home/username): /bin/wine cmd /c C:\run.bat
cp "$STAGE/home/username/run.bat" "$STAGE/home/username/run-noqt.bat" "$STAGE/home/username/.wine/drive_c/"

if [[ $MODE == zip ]]; then
  echo "zipping overlay (stored, the media is already compressed)"
  rm -f "$OUT/ilumina.zip"
  (cd "$STAGE" && zip -q -r -0 -X "$OUT/ilumina.zip" home)
  ls -lh "$OUT/ilumina.zip"
fi

if (( ISOS )); then
  for n in 1 2 3 4; do
    echo "building cd$n.iso"
    xorriso -as mkisofs -quiet -J -joliet-long -R -V "ILUMINACD$n" -o "$OUT/cd$n.iso" "$DISCS/cd$n"
  done
fi
echo "done: $OUT"
