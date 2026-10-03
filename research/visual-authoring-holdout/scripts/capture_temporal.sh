#!/usr/bin/env bash
# Temporal capture for ONE holdout run whose app has a seekable timeline.
# Companion to capture.sh (static ladder). Same honesty discipline: it never
# edits the app to make it prettier; if the app exposes no time-seek, or a phase
# cannot be rendered, it records NO_FRAME (valid data) and moves on. Raw
# deterministic .ansi frames are authoritative; PNGs are derived for review.
#
# The phase->time map is SUBJECT-SPECIFIC and lives in runs/$RID/TEMPORAL.plan,
# authored by the coordinator AFTER a dry run of the app (see TEMPORAL.plan.template).
# Times are observed from the app's own clock, never invented. If the app has no
# temporal transition, mark the plan NONE and run only the static capture.sh.
set -uo pipefail

RID="${1:?usage: capture_temporal.sh R0x}"
ROOT="${ROOT:-/home/leah/libgibson_lab/holdout-v0.4.0}"
WT="$ROOT/runs/$RID"
OUT="$WT/cap/temporal"
A2P="$ROOT/scripts/ansi2png.py"
PLAN="$WT/TEMPORAL.plan"
# The app's seek flag. Convention: `--capture WxH[:mono] --at T` (the recipe
# director convention, pure frame(t)). If a subject's app uses a different seek
# flag, set SEEK_FLAG per that app's README — do NOT change the app.
SEEK_FLAG="${SEEK_FLAG:---at}"
# Spec for temporal frames: full color + Mono, at the main 120x40 geometry.
SPECS=( "120x40" "120x40:mono" )

if [ ! -f "$PLAN" ]; then
  echo "$RID: no TEMPORAL.plan (static-only run) — nothing to do" ; exit 0
fi
if grep -qiE '^[[:space:]]*NONE[[:space:]]*$' "$PLAN"; then
  echo "$RID: TEMPORAL.plan = NONE (app has no temporal transition) — static-only" ; exit 0
fi

PKG="$(grep -rIl --include=Cargo.toml 'libgibson' "$WT" 2>/dev/null \
        | grep -v '/libgibson-reference/' | head -1)"
if [ -z "$PKG" ]; then
  echo "$RID: NO_FRAME (no app package found)" | tee "$OUT/STATUS.txt" ; exit 0
fi
APPDIR="$(dirname "$PKG")"
mkdir -p "$OUT"
echo "$RID: temporal capture, app at $APPDIR"

if ! ( cd "$APPDIR" && cargo build --release >/dev/null 2>"$OUT/build.err" ); then
  echo "$RID: NO_FRAME (build failed — DATA, kept)" | tee "$OUT/STATUS.txt" ; exit 0
fi

ok=0 ; tried=0
# Each non-comment, non-blank line is `phase=T` (T in the app's own time units).
while IFS='=' read -r phase t; do
  phase="$(echo "$phase" | tr -d '[:space:]')"
  t="$(echo "$t" | tr -d '[:space:]')"
  [ -z "$phase" ] && continue
  case "$phase" in \#*) continue ;; esac
  [ -z "$t" ] && { echo "  ! $phase: no time, skipped"; continue; }
  for spec in "${SPECS[@]}"; do
    tried=$((tried+1))
    tag="${spec/:/-}"                 # 120x40 / 120x40-mono
    base="${phase}.${tag}"
    ansi="$OUT/$base.ansi"
    ( cd "$APPDIR" && cargo run --release -q -- --capture "$spec" "$SEEK_FLAG" "$t" ) >"$ansi" 2>/dev/null
    if [ ! -s "$ansi" ]; then
      echo "  - $phase @ $t ($spec) -> empty (no seek path at $SEEK_FLAG; set per README)"
      rm -f "$ansi" ; continue
    fi
    wh="${spec%%:*}" ; W="${wh%x*}" ; H="${wh#*x}"
    if python3 "$A2P" --in "$ansi" --width "$W" --height "$H" --out "$OUT/$base.png"; then
      ok=$((ok+1)) ; echo "  + $phase @ $t ($spec)"
    fi
  done
done < "$PLAN"

if [ "$ok" -eq 0 ]; then
  echo "$RID: NO_FRAME (no temporal frame rendered of $tried attempts — record app's seek cmd, re-run)" | tee "$OUT/STATUS.txt"
else
  echo "$RID: CAPTURED $ok temporal frames ($tried attempted)" | tee "$OUT/STATUS.txt"
fi
