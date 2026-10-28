#!/bin/bash
# Optional helper: types the owner's own registration data and activation
# code into iLumina's first-run dialogs, exactly as a user would.
# It only runs when the variables are set (see .env.example). The values
# are never written to the logs.
#
#   ILUMINA_REG_NAME / ILUMINA_REG_EMAIL / ILUMINA_REG_CEP  -> "Registre Sua Cópia"
#   ILUMINA_ACTIVATION_CODE                                -> "Ativação do iLúmina"
#
# Coordinates are absolute inside the 1024x768 Wine virtual desktop (the
# dialogs are always centred there).
source /opt/ilumina/lib.sh
export DISPLAY="${DISPLAY:-:100}"

type_text() { xdotool type --delay 60 -- "$1"; }

wait_window() {  # $1 = title regex, $2 = timeout s
  local _
  for _ in $(seq 1 "$2"); do
    xdotool search --name "$1" >/dev/null 2>&1 && return 0
    sleep 1
  done
  return 1
}

# Xpra only maps the windows (and delivers synthetic input) while a browser
# is connected, so wait for the first client.
wait_client() {
  until XDG_RUNTIME_DIR="/tmp/xdg-$(id -u)" xpra info :100 2>/dev/null | grep -qE '^clients=[1-9]'; do
    sleep 2
  done
  sleep 4
}

if [[ -z "${ILUMINA_REG_NAME:-}${ILUMINA_ACTIVATION_CODE:-}" ]]; then exit 0; fi
wait_client
log "auto-activate: browser connected"

if [[ -n "${ILUMINA_REG_NAME:-}" && -n "${ILUMINA_REG_EMAIL:-}" && -n "${ILUMINA_REG_CEP:-}" ]]; then
  if wait_window 'Registre Sua C' 90; then
    sleep 3
    log "auto-activate: filling registration dialog"
    slow_click 390 370; type_text "$ILUMINA_REG_NAME"
    slow_click 611 370; type_text "$ILUMINA_REG_EMAIL"
    slow_click 660 472; type_text "$ILUMINA_REG_CEP"
    sleep 1; slow_click 689 551
  fi
fi

[[ -n "${ILUMINA_ACTIVATION_CODE:-}" ]] || exit 0

if ! wait_window 'Ativa.*o do iL' 120; then
  log "auto-activate: no activation dialog (already activated?)"
  exit 0
fi
sleep 3
log "auto-activate: entering activation code"
slow_click 495 372; sleep 0.5      # "Possuo o meu Código de Ativação. Ativar agora."
slow_click 768 602; sleep 3        # Continuar
slow_click 700 278; sleep 0.5      # "Código de Ativação" field
type_text "$ILUMINA_ACTIVATION_CODE"
sleep 1
slow_click 768 602; sleep 5        # Continuar (stays disabled if the code is rejected)

if xdotool search --name 'Ativa.*o do iL' >/dev/null 2>&1; then
  log "auto-activate: dialog still open — code not accepted for this installation"
else
  log "auto-activate: activation dialog closed"
fi
