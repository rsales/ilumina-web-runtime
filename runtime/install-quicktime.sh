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

QT_QTS="$WINEPREFIX/drive_c/windows/system32/QuickTime.qts"
if [[ -f "$QT_QTS" ]]; then log "QuickTime already installed"; exit 0; fi

start_setup_display
log "running QuickTime 6.3 installer (unattended)"
( cd "$APP_DIR/ThirdParty/QuickTime" && \
  wine QuickTimeInstaller.exe >"$LOG_DIR/quicktime-installer.log" 2>&1 ) &

# Window sizes and default-button offsets, measured on Wine 9.0.
WIZARD_SIZE=(552 308); WIZARD_BTN=(408 288)   # setup wizard: Next / Agree / Close
ASSIST_SIZE=(324 363); ASSIST_BTN=(273 347) # "QuickTime Settings" assistant: Next

click_default() {  # $1 = wizard|assist
  local -n size="${1^^}_SIZE" btn="${1^^}_BTN"
  local w id x y
  w="$(find_window_sized "${size[0]}" "${size[1]}")" || return 1
  read -r id x y <<<"$w"
  read -r x y <<<"$(ensure_onscreen "$id" "$x" "$y")"
  slow_click $((x + btn[0])) $((y + btn[1]))
}

# 1) Welcome, Welcome 2, Licence (Agree), Destination, Setup type,
#    Program folder, Pro registration (left empty) = 7 pages.
for _ in $(seq 1 60); do find_window_sized "${WIZARD_SIZE[@]}" >/dev/null && break; sleep 1; done
for page in 1 2 3 4 5 6 7; do
  sleep 4; shot "qt-page-$page"
  click_default wizard || die "QuickTime wizard window not found (page $page)"
done

# 2) Files are copied; then an Explorer window for the Start-menu folder and
#    the "QuickTime Settings" assistant pop up.
assist_seen=0
for _ in $(seq 1 90); do
  sleep 2
  for id in $(xdotool search --onlyvisible --name '^QuickTime$' 2>/dev/null); do
    xdotool windowclose "$id" 2>/dev/null || true
  done
  if find_window_sized "${ASSIST_SIZE[@]}" >/dev/null; then
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
