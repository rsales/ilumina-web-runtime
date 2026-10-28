#!/bin/bash
# Launched by Xpra as the session's only child. When iLumina quits, Wine
# exits, Xpra exits (--exit-with-children) and the container restarts.
source /opt/ilumina/lib.sh
cd "$APP_DIR"
# Optional: fill registration / activation from .env (no-op when unset).
/opt/ilumina/auto-activate.sh >>"$LOG_DIR/auto-activate.log" 2>&1 &
# One Wine "virtual desktop" the size of the Director stage, so every dialog
# the app opens stays inside a single browser-visible window.
exec wine explorer "/desktop=iLumina,${ILUMINA_RES}" "$APP_DIR_WIN\\iLumina.exe" \
  >>"$LOG_DIR/ilumina.log" 2>&1
