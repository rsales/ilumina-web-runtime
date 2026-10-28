#!/bin/bash
# Container entrypoint.
#   run      (default) install on first start if needed, then stream iLumina
#   install  only perform the installation into /state
#   shell    interactive shell with the Wine environment set up
source /opt/ilumina/lib.sh

install_all() {
  if [[ -f "$INSTALL_MARKER" ]]; then
    log "already installed (delete the /state volume to reinstall)"
    return 0
  fi
  # Each step is skipped once done, so a restart after a failure resumes.
  [[ -f /state/.files-installed ]] || { /opt/ilumina/install-ilumina.sh && touch /state/.files-installed; }
  /opt/ilumina/install-quicktime.sh
  date -u +%FT%TZ > "$INSTALL_MARKER"
  log "installation complete"
}

run_stream() {
  install_all
  map_cd_drives   # drive symlinks are cheap; refresh in case media moved
  pkill -f 'Xvfb :90' 2>/dev/null || true

  export XDG_RUNTIME_DIR="/tmp/xdg-$(id -u)"
  mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"

  log "starting Xpra on :${XPRA_PORT} — open http://localhost:${XPRA_PORT}/"
  cd "$APP_DIR"
  exec xpra start :100 \
    --daemon=no \
    --bind-tcp="0.0.0.0:${XPRA_PORT}" \
    --html=on \
    --xvfb="Xvfb +extension Composite -screen 0 ${ILUMINA_RES}x24+32 -nolisten tcp -noreset" \
    --resize-display=no \
    --start-child=/opt/ilumina/run-ilumina.sh \
    --exit-with-children=yes \
    --pulseaudio=yes --speaker=on --microphone=off \
    --dbus-launch= --dbus-proxy=no --dbus-control=no \
    --mdns=no --notifications=no --system-tray=no --webcam=no \
    --printing=no --file-transfer=no --open-files=no --open-url=no \
    --clipboard=yes --sharing=no --env=WINEDEBUG="$WINEDEBUG" \
    --log-dir="$LOG_DIR"
}

# Quick environment check (useful on Apple Silicon / new-WoW64 experiments).
doctor() {
  set +e
  echo "== host"; uname -m; ls /proc/sys/fs/binfmt_misc 2>/dev/null | grep -v -E '^(register|status)$'
  echo "== wine"; command -v wine; wine --version; echo "WINEARCH=$WINEARCH"
  ls "$(dirname "$(dirname "$(readlink -f "$(command -v wine)")")")/lib/wine" 2>/dev/null
  echo "== prefix"
  [[ -d "$WINEPREFIX/drive_c" ]] || wineboot -i >"$LOG_DIR/wineboot.log" 2>&1
  ls -d "$WINEPREFIX/drive_c/windows/syswow64" 2>/dev/null && echo "win64 prefix (WoW64)" || echo "win32 prefix"
  echo "== 32-bit program test"
  local cmd32='C:\windows\system32\cmd.exe'
  [[ -d "$WINEPREFIX/drive_c/windows/syswow64" ]] && cmd32='C:\windows\syswow64\cmd.exe'
  timeout 120 wine "$cmd32" /c echo 32BIT-OK 2>&1 | grep -v -E '^(ALSA|fixme)' | tail -5
  echo "== fixed-base 0x400000 test (QuickTime installer)"
  if [[ -f "$APP_DIR/ThirdParty/QuickTime/QuickTimeInstaller.exe" ]]; then
    start_setup_display
    ( cd "$APP_DIR/ThirdParty/QuickTime" && timeout 60 wine QuickTimeInstaller.exe 2>&1 | grep -v ALSA | tail -5 ) &
    sleep 45
    if pgrep -f QuickTimeInstaller >/dev/null; then echo "QuickTimeInstaller is RUNNING (good)"; else echo "QuickTimeInstaller is not running (see output above)"; fi
    wineserver -k
  else
    echo "skipped: run 'install' once so the program files exist"
  fi
}

case "${1:-run}" in
  doctor)  doctor ;;
  run)     run_stream ;;
  install) install_all ;;
  shell)   exec bash ;;
  *)       exec "$@" ;;
esac
