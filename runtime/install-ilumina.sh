#!/bin/bash
# Reproduces what the original iLuminaInstaller.exe does for a FULL install,
# without the GUI and without disc swapping.
#
# The original installer (verified by running it under Wine and diffing the
# result against the discs) does exactly this:
#   1. copies CD1:\Program\* (minus Mac-only files) to C:\Program Files\iLuminaPO
#   2. copies CD2..CD4:\Files\Media\* into ...\iLuminaPO\Files\Media
#   3. writes iLuminaInfo.txt (a Lingo property list read by GCode.cxt PathMgr)
#   4. adds an Uninstall registry entry and Start-menu shortcuts
#   5. launches ThirdParty\QuickTime\QuickTimeInstaller.exe (see install-quicktime.sh)
# The application creates Prefs\*.txt itself on first launch.
source /opt/ilumina/lib.sh

check_media

log "initialising 32-bit Wine prefix at $WINEPREFIX"
wineboot -i >"$LOG_DIR/wineboot.log" 2>&1
wineserver -w
map_cd_drives

log "copying program files from CD 1"
mkdir -p "$APP_DIR"
# Exclusions = files the Windows installer does not copy (Mac projector,
# resource forks, Mac Xtras, AppleDouble files).
tar -C "$(cd_dir 1)/Program" \
    --exclude='./iLumina' --exclude='./Uninstall' \
    --exclude='*:rsrc' --exclude='._*' --exclude='.DS_Store' \
    --exclude='./Xtras/Mac' \
    -cf - . | tar -C "$APP_DIR" -xf -

for n in 2 3 4; do
  log "merging media from CD $n"
  tar -C "$(cd_dir $n)/Files" --exclude='._*' --exclude='.DS_Store' \
      -cf - Media | tar -C "$APP_DIR/Files" -xf -
done
chmod -R u+w "$APP_DIR"

# Byte-for-byte what the original installer wrote for a full CD install
# (no trailing newline). #CDDrive points at CD 1 on D:.
printf '%s' '[#applicationPath: "C:\Program Files\iLuminaPO\", #Full: -1, #DVDOrCD: #cd, #DvDDrive: "", #CDDrive: "D:\", #edition: "Gold", #version: 2.50, #installRanFromApp: 0]' \
  > "$APP_DIR/iLuminaInfo.txt"

KEY='HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\{BF0F5955-FC76-4F85-A13D-C9A8A9A5E067}'
wine reg add "$KEY" /v DisplayName     /d 'iLumina Bible'       /f >/dev/null 2>&1
wine reg add "$KEY" /v DisplayVersion  /d '2.50'                /f >/dev/null 2>&1
wine reg add "$KEY" /v InstallLocation /d "$APP_DIR_WIN\\"      /f >/dev/null 2>&1
wine reg add "$KEY" /v Publisher       /d 'iLumina Brasil LTDA' /f >/dev/null 2>&1
wineserver -w

log "iLumina files installed ($(du -sh "$APP_DIR" | cut -f1))"
