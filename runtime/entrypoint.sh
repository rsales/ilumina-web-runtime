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
  /opt/ilumina/install-ilumina.sh
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

case "${1:-run}" in
  run)     run_stream ;;
  install) install_all ;;
  shell)   exec bash ;;
  *)       exec "$@" ;;
esac
