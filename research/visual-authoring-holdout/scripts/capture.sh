#!/usr/bin/env bash
# Post-run capture for ONE holdout run. Best-effort, honest: it never edits the
# app to make it prettier; if it cannot find a working capture path it records
# NO_FRAME (valid data) and stops. Captures the 4 required configs and renders
# each to PNG with the SAME uniform pipeline (ansi2png.py).
set -uo pipefail

RID="${1:?usage: capture.sh R0x}"
ROOT="${ROOT:-/home/leah/libgibson_lab/holdout-v0.4.0}"
WT="$ROOT/runs/$RID"
OUT="$WT/cap"
A2P="$ROOT/scripts/ansi2png.py"
mkdir -p "$OUT"

# Locate the subject's app package (a Cargo.toml depending on libgibson,
# excluding the read-only reference tree).
PKG="$(grep -rIl --include=Cargo.toml 'libgibson' "$WT" 2>/dev/null \
        | grep -v '/libgibson-reference/' | head -1)"
if [ -z "$PKG" ]; then
  echo "$RID: NO_FRAME (no app package found)" | tee "$OUT/STATUS.txt"; exit 0
fi
APPDIR="$(dirname "$PKG")"
echo "$RID: app package at $APPDIR"

echo "$RID: building (release)"
if ! ( cd "$APPDIR" && cargo build --release >/dev/null 2>"$OUT/build.err" ); then
  echo "$RID: NO_FRAME (build failed — DATA, kept)" | tee "$OUT/STATUS.txt"
  echo "  (see $OUT/build.err)"; exit 0
fi

# spec -> output basename
declare -A SPECS=( ["120x40"]="120x40" ["80x24"]="80x24" ["42x15"]="42x15" ["120x40:mono"]="mono-120x40" )
ok=0
for spec in "120x40" "80x24" "42x15" "120x40:mono"; do
  base="${SPECS[$spec]}"
  ansi="$OUT/$base.ansi"
  # Try the recipe-convention capture flag. If the app uses another flag, edit
  # this line per that app's README (documented in PROTOCOL.md) — do NOT change
  # the app to suit us.
  ( cd "$APPDIR" && cargo run --release -q -- --capture "$spec" ) >"$ansi" 2>/dev/null
  if [ ! -s "$ansi" ]; then
    echo "$RID: $spec -> empty (capture path not found at --capture; needs manual spec)"
    continue
  fi
  wh="${spec%%:*}"; W="${wh%x*}"; H="${wh#*x}"
  if python3 "$A2P" --in "$ansi" --width "$W" --height "$H" --out "$OUT/$base.png"; then
    ok=$((ok+1))
  fi
done
if [ "$ok" -eq 0 ]; then
  echo "$RID: NO_FRAME (no capture spec worked — record app's real capture cmd, re-run)" | tee "$OUT/STATUS.txt"
else
  echo "$RID: CAPTURED $ok/4 frames" | tee "$OUT/STATUS.txt"
fi
