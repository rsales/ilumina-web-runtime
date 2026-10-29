#!/bin/bash
# Package the layer-1 probe as a Boxedwine "app" zip (mounted at C:\files on the web).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"; OUT="${1:?usage: $0 OUT_DIR}"
mkdir -p "$OUT"; rm -f "$OUT/probes.zip"
(cd "$HERE" && zip -q -X "$OUT/probes.zip" hello32gui.exe)
echo "$OUT/probes.zip"
