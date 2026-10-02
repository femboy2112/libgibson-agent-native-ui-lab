#!/usr/bin/env bash
# Assemble the BLIND packet. Shuffles valid runs to opaque candidate ids,
# copies ONLY the 4 rendered PNGs per candidate (metadata stripped), and writes
# the review form. The run->candidate map is appended to MAPPING.local.txt and
# NEVER committed. No source, logs, arm, model, or metrics enter the packet.
set -euo pipefail

ROOT="${ROOT:-/home/leah/libgibson_lab/holdout-v0.4.0}"
PACKET="$ROOT/blind-review"
MAP="$ROOT/MAPPING.local.txt"
FORM_SRC="$ROOT/blind-review-template/BLIND_REVIEW.md"
rm -rf "$PACKET"; mkdir -p "$PACKET"

# valid runs = those with at least one rendered PNG
mapfile -t VALID < <(for d in "$ROOT"/runs/R*/cap; do
  rid="$(basename "$(dirname "$d")")"
  ls "$d"/*.png >/dev/null 2>&1 && echo "$rid"
done)
if [ "${#VALID[@]}" -eq 0 ]; then echo "no runs produced frames; nothing to anonymize"; exit 0; fi

# unpredictable shuffle to candidate letters
mapfile -t SHUF < <(printf '%s\n' "${VALID[@]}" | shuf)
letters=( A B C D E F G H )

{ echo; echo "# CANDIDATE MAP (local only, do not reveal before unblinding) — $(date -u +%FT%TZ)"; } >> "$MAP"
i=0
for rid in "${SHUF[@]}"; do
  cand="candidate-${letters[$i]}"
  mkdir -p "$PACKET/$cand"
  for base in 120x40 80x24 42x15 mono-120x40; do
    src="$ROOT/runs/$rid/cap/$base.png"
    [ -f "$src" ] && python3 - "$src" "$PACKET/$cand/$base.png" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")   # re-save drops all metadata
im.save(sys.argv[2])
PY
  done
  cp "$FORM_SRC" "$PACKET/$cand/REVIEW.md" 2>/dev/null || true
  echo "candidate-${letters[$i]} <= $rid" >> "$MAP"
  i=$((i+1))
done
cp "$FORM_SRC" "$PACKET/BLIND_REVIEW.md" 2>/dev/null || true
echo "blind packet: $PACKET  (${#SHUF[@]} candidates)"
echo "candidate map appended (local): $MAP"
ls "$PACKET"
