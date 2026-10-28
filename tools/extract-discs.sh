#!/bin/bash
# Extract the four iLumina disc images (.iso / .nrg / .bin) into the folder
# layout the runtime expects:  OUT/cd1 … OUT/cd4, each containing N.txt.
# Usage: tools/extract-discs.sh OUT image1 image2 image3 image4
# Needs 7-Zip (7z or 7zz). CD 1 is a hybrid Mac/PC disc (Apple partition map
# + HFS+); 7-Zip exposes the HFS+ volume as a nested image, handled below.
set -euo pipefail
OUT="${1:?usage: $0 OUT img1 img2 img3 img4}"; shift
[[ $# -eq 4 ]] || { echo "need exactly 4 images, in disc order" >&2; exit 1; }
SZ="$(command -v 7zz || command -v 7z || true)"
[[ -n "$SZ" ]] || { echo "7-Zip not found (apt install 7zip / brew install sevenzip)" >&2; exit 1; }

n=0
for img in "$@"; do
  n=$((n + 1)); tmp="$OUT/.tmp-cd$n"
  rm -rf "$tmp" "$OUT/cd$n"; mkdir -p "$tmp"
  echo "== CD $n: $img"
  "$SZ" x -y -bso0 -bsp0 -o"$tmp" "$img" || true
  # Nested HFS / ISO volume (hybrid discs): extract it in place.
  while IFS= read -r -d '' inner; do
    "$SZ" x -y -bso0 -bsp0 -o"$tmp" "$inner" || true
    rm -f "$inner"
  done < <(find "$tmp" -maxdepth 2 -type f \( -iname '*.hfs' -o -iname '*.iso' \) -print0)
  marker="$(find "$tmp" -maxdepth 3 -type f -name "$n.txt" | head -1)"
  [[ -n "$marker" ]] || { echo "CD $n: $n.txt not found after extraction" >&2; exit 1; }
  mv "$(dirname "$marker")" "$OUT/cd$n"
  rm -rf "$tmp"
  echo "   -> $OUT/cd$n"
done
echo "Done. Next: tools/verify-discs.sh $OUT"
