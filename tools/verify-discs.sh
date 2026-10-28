#!/bin/bash
# Check that a disc folder (cd1..cd4) matches a known iLumina release.
# Usage: tools/verify-discs.sh /path/to/discs [manifest]
# The manifest holds only SHA-256 hashes and paths — no iLumina content.
set -euo pipefail
DISCS="${1:?usage: $0 /path/to/discs [manifest]}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="${2:-$HERE/manifests/ilumina-gold-2.50-ptbr.sha256}"
cd "$DISCS"
if sha256sum --quiet -c "$MANIFEST"; then
  echo "OK: $(wc -l < "$MANIFEST") files match $(basename "$MANIFEST")"
else
  echo "MISMATCH: see the lines above (missing or modified files)" >&2
  exit 1
fi
