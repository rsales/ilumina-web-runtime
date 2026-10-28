# Shared helpers for the iLumina runtime scripts. Sourced, not executed.

set -euo pipefail

: "${WINEPREFIX:=/state/prefix}"
: "${ILUMINA_MEDIA:=/media/ilumina}"
: "${ILUMINA_RES:=1024x768}"

export WINEPREFIX WINEARCH="${WINEARCH:-win32}" WINEDEBUG="${WINEDEBUG:--all}"

# 32-bit system DLLs live in system32 on a win32 prefix, but in syswow64 on a
# win64 (WoW64) prefix.
sys32_dir() {
  if [[ -d "$WINEPREFIX/drive_c/windows/syswow64" ]]; then
    echo "$WINEPREFIX/drive_c/windows/syswow64"
  else
    echo "$WINEPREFIX/drive_c/windows/system32"
  fi
}

APP_DIR_WIN='C:\Program Files\iLuminaPO'
APP_DIR="$WINEPREFIX/drive_c/Program Files/iLuminaPO"
INSTALL_MARKER="/state/.installed"
LOG_DIR="/state/logs"
SHOT_DIR="/state/logs/screens"

mkdir -p "$LOG_DIR" "$SHOT_DIR"

log() { printf '[ilumina %s] %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die() { log "ERROR: $*"; exit 1; }

# cd<N> must be the root of disc N (the folder that contains N.txt).
cd_dir() { printf '%s/cd%s' "$ILUMINA_MEDIA" "$1"; }

check_media() {
  local n d
  for n in 1 2 3 4; do
    d="$(cd_dir "$n")"
    [[ -d "$d" ]] || die "missing $d — mount the extracted contents of CD $n there"
    [[ -r "$d" && -x "$d" ]] || die "$d is not readable by uid $(id -u) — check permissions"
    [[ -f "$d/$n.txt" ]] || die "$d does not look like iLumina CD $n ($n.txt not found)"
  done
  [[ -f "$(cd_dir 1)/Program/iLumina.exe" ]] || die "CD 1 has no Program/iLumina.exe"
  log "media OK: 4 discs found under $ILUMINA_MEDIA"
}

# Map each disc to its own drive letter (D: … G:) typed as CD-ROM, so the
# Lingo code that enumerates CD drives (BuddyAPI baDiskList) finds them.
map_cd_drives() {
  local letters=(d e f g) i
  for i in 0 1 2 3; do
    ln -sfn "$(cd_dir $((i + 1)))" "$WINEPREFIX/dosdevices/${letters[$i]}:"
    wine reg add 'HKLM\Software\Wine\Drives' /v "${letters[$i]}:" /d cdrom /f >/dev/null 2>&1
  done
}

# Headless X server used only during unattended installation steps.
start_setup_display() {
  export DISPLAY=:90
  if ! xdpyinfo >/dev/null 2>&1; then
    Xvfb :90 -screen 0 "${ILUMINA_RES}x24" -nolisten tcp >"$LOG_DIR/xvfb-setup.log" 2>&1 &
    for _ in $(seq 1 50); do xdpyinfo >/dev/null 2>&1 && break; sleep 0.2; done
  fi
}

shot() { import -window root "$SHOT_DIR/$1.png" 2>/dev/null || true; }

# Director's MUI dialogs ignore xdotool's instantaneous clicks; press and
# release with a small gap instead.
slow_click() {
  xdotool mousemove "$1" "$2"; sleep 0.3
  xdotool mousedown 1; sleep 0.15; xdotool mouseup 1
}

# Print "id x y" for the first visible window of exactly $1 x $2 pixels.
# (Installer wizards retitle their window on every page, so size is a more
# stable key than the title.)
find_window_sized() {
  local id WINDOW X Y WIDTH HEIGHT SCREEN
  for id in $(xdotool search --onlyvisible --name '' 2>/dev/null); do
    eval "$(xdotool getwindowgeometry --shell "$id")"
    if (( WIDTH == $1 && HEIGHT == $2 )); then
      echo "$id $X $Y"; return 0
    fi
  done
  return 1
}

# Bring a window fully on-screen and print its new "x y".
ensure_onscreen() {
  local id=$1 x=$2 y=$3
  if (( x < 0 || y < 0 )); then
    xdotool windowmove "$id" 200 150; sleep 1; x=200; y=150
  fi
  echo "$x $y"
}
