#!/bin/bash
# Native Boxedwine runtime: install (file copy) on first start, then stream with Xpra.
set -euo pipefail
ROOT=/state/root
log() { printf '[ilumina-bw %s] %s\n' "$(date +%H:%M:%S)" "$*" >&2; }

[[ -f "$BOXEDWINE_FS" ]] || { log "ERROR: Boxedwine file system not found at $BOXEDWINE_FS (see web/README.md)"; exit 1; }

install() {
  [[ -f /state/.overlay-installed ]] && return 0
  mkdir -p "$ROOT"
  /opt/ilumina-web/make-overlay.sh "$ILUMINA_MEDIA" "$ROOT" --dir
  touch /state/.overlay-installed
}

boxed() {  # run iLumina's launcher inside Boxedwine
  # Audio goes Boxedwine -> SDL -> PulseAudio (Xpra) -> browser. Upstream
  # native defaults keep ~1 Wine period queued, which underruns under the
  # emulator's jitter; see patches/0002 and docs/boxedwine-test-matrix.md.
  export SDL_AUDIODRIVER="${SDL_AUDIODRIVER:-pulseaudio}"
  export BOXEDWINE_AUDIO_PERIOD_FRAMES="${BOXEDWINE_AUDIO_PERIOD_FRAMES:-2048}"
  export BOXEDWINE_AUDIO_BUFFER_MS="${BOXEDWINE_AUDIO_BUFFER_MS:-300}"
  export BOXEDWINE_AUDIO_RESERVE_MS="${BOXEDWINE_AUDIO_RESERVE_MS:-120}"
  export PULSE_LATENCY_MSEC="${PULSE_LATENCY_MSEC:-60}"
  # The guest's fixme spam (mountmgr ioctl, once per CD read) costs emulated
  # CPU and competes with audio; silence it unless WINEDEBUG is set.
  log "audio: period=${BOXEDWINE_AUDIO_PERIOD_FRAMES} buffer=${BOXEDWINE_AUDIO_BUFFER_MS}ms reserve=${BOXEDWINE_AUDIO_RESERVE_MS}ms pulse=${PULSE_LATENCY_MSEC}ms"
  exec boxedwine -root "$ROOT" -zip "$BOXEDWINE_FS" -resolution "$ILUMINA_RES" \
    -title iLumina -env "WINEDEBUG=${WINEDEBUG:--all}" /bin/wine cmd /c "C:\\run.bat"
}

case "${1:-run}" in
  install) install ;;
  boxed)   boxed ;;
  run)
    install
    export XDG_RUNTIME_DIR="/tmp/xdg-$(id -u)"; mkdir -p "$XDG_RUNTIME_DIR"; chmod 700 "$XDG_RUNTIME_DIR"
    log "starting Xpra on :${XPRA_PORT} — open http://localhost:${XPRA_PORT}/"
    exec xpra start :100 --daemon=no --bind-tcp="0.0.0.0:${XPRA_PORT}" --html=on \
      --xvfb="Xvfb +extension Composite -screen 0 ${ILUMINA_RES}x24+32 -nolisten tcp -noreset" \
      --resize-display=no --start-child="/opt/ilumina-web/native-entrypoint.sh boxed" \
      --exit-with-children=yes --pulseaudio=yes --speaker=on --microphone=off \
      --dbus-launch= --dbus-proxy=no --dbus-control=no --mdns=no --notifications=no \
      --system-tray=no --webcam=no --printing=no --file-transfer=no --sharing=no \
      ${XPRA_SPEAKER_CODEC:+--speaker-codec="$XPRA_SPEAKER_CODEC"} \
      --speed="${XPRA_SPEED:-90}" --quality="${XPRA_QUALITY:-60}" \
      --log-dir=/state ${XPRA_EXTRA_ARGS:-} ;;
  shell) exec bash ;;
  *) exec "$@" ;;
esac
