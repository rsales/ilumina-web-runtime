#!/bin/bash
# Unattended run of the QuickTime 6.3 installer that ships on CD 1
# (Program\ThirdParty\QuickTime\QuickTimeInstaller.exe, fully offline thanks
# to QuickTimeInstallCache.qdat).
#
# The installer has no reliable silent switch, so it is driven with xdotool.
# Every page of the wizard has its default button (Next / Agree / Close) at
# the same position relative to the window, which keeps this simple.
# Screenshots of each step land in /state/logs/screens for debugging.
source /opt/ilumina/lib.sh

QT_QTS="$(sys32_dir)/QuickTime.qts"
if [[ -f "$QT_QTS" ]]; then log "QuickTime already installed"; exit 0; fi

start_setup_display
log "running QuickTime 6.3 installer (unattended)"
( cd "$APP_DIR/ThirdParty/QuickTime" && \
  wine QuickTimeInstaller.exe >"$LOG_DIR/quicktime-installer.log" 2>&1 ) &
QT_PID=$!

# How long to wait for the first wizard page. The installer is a
# self-extracting archive: ~10 s on x86_64, several minutes under emulation
# (Apple Silicon). Override with QT_WAIT_SECONDS.
QT_WAIT_SECONDS="${QT_WAIT_SECONDS:-600}"

qt_fail() {
  shot "qt-failed"
  log "QuickTime installer log (last lines):"
  grep -v '^ALSA' "$LOG_DIR/quicktime-installer.log" 2>/dev/null | tail -15 >&2 || true
  log "processes:"; pgrep -a -f '\.exe' >&2 || true
  die "$1"
}

# Window width and default-button position: x from the left edge, y from the
# BOTTOM edge. Measured on Wine 9.0 (wizard 552x308) and Wine 11 WoW64
# (552x333): only the height changes, so the bottom offset holds for both.
WIZARD_W=552; WIZARD_BTN=(408 20)   # setup wizard: Next / Agree / Close
ASSIST_W=324; ASSIST_BTN=(273 16)   # "QuickTime Settings" assistant: Next

click_default() {  # $1 = wizard|assist
  local -n width="${1^^}_W" btn="${1^^}_BTN"
  local w id x y h
  w="$(find_window_w "$width")" || return 1
  read -r id x y h <<<"$w"
  read -r x y <<<"$(ensure_onscreen "$id" "$x" "$y")"
  slow_click $((x + btn[0])) $((y + h - btn[1]))
}

# 1) Welcome, Welcome 2, Licence (Agree), Destination, Setup type,
#    Program folder, Pro registration (left empty) = 7 pages.
log "waiting up to ${QT_WAIT_SECONDS}s for the QuickTime wizard"
for ((t = 0; t < QT_WAIT_SECONDS; t += 5)); do
  find_window_w "$WIZARD_W" >/dev/null && break
  kill -0 "$QT_PID" 2>/dev/null || pgrep -f QuickTimeInstaller >/dev/null || \
    qt_fail "QuickTime installer exited before showing its wizard"
  (( t % 60 == 0 )) && { log "  still waiting (${t}s)"; shot "qt-wait-$t"; }
  sleep 5
done
find_window_w "$WIZARD_W" >/dev/null || \
  qt_fail "QuickTime wizard did not appear within ${QT_WAIT_SECONDS}s"
log "wizard is up"

# Under emulation the pages also redraw slowly: wait per page as well.
PAGE_DELAY="${QT_PAGE_DELAY:-4}"
for page in 1 2 3 4 5 6 7; do
  sleep "$PAGE_DELAY"; shot "qt-page-$page"
  ok=0
  for _ in $(seq 1 30); do click_default wizard && { ok=1; break; }; sleep 2; done
  (( ok )) || qt_fail "QuickTime wizard window not found (page $page)"
done

# 2) Files are copied; then an Explorer window for the Start-menu folder and
#    the "QuickTime Settings" assistant pop up.
assist_seen=0
for _ in $(seq 1 300); do
  sleep 2
  for id in $(xdotool search --onlyvisible --name '^QuickTime$' 2>/dev/null); do
    xdotool windowclose "$id" 2>/dev/null || true
  done
  if find_window_w "$ASSIST_W" >/dev/null; then
    assist_seen=1; shot "qt-assistant"; click_default assist || true
  elif (( assist_seen )); then
    break
  fi
done

# 3) Final page: Close. (README / Player checkboxes may spawn extra
#    processes; they are terminated below.)
sleep 3; shot "qt-finish"; click_default wizard || true
sleep 5
wineserver -k || true
wineserver -w || true

[[ -f "$QT_QTS" ]] || die "QuickTime did not install — see $SHOT_DIR/qt-*.png"
log "QuickTime 6.3 installed"
