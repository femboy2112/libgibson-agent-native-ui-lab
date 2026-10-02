#!/usr/bin/env bash
# Build sanitized CONTROL and TREATMENT libgibson-reference/ trees for the
# AI visual-authoring holdout. Auditable + re-runnable.
set -euo pipefail

SRC="/home/leah/LibGibson"                       # upstream LibGibson repo
TREAT_REF="research/ai-visual-authoring"         # frozen treatment branch (overlay source)
V="v0.4.0"                                        # released substrate tag
OUT="/tmp/claude-1000/-home-leah-LibGibson/6f14caba-f217-4ba6-8074-7d83438e52ce/scratchpad/holdout/reference"

# Files pruned from BOTH arms (hypothesis record + model-naming narrative docs)
PRUNE_DIRS=( "docs/research" )
PRUNE_FILES=( "docs/HUMAN_MUSIC_RELEASE_ROAST.md" "docs/STATE_OF_LIBGIBSON.md" "docs/INTRODUCTORY_CINEMA.md" )

# The TREATMENT overlay (the ONLY difference between arms)
OVERLAY=(
  "AGENTS.md"
  "docs/AI_VISUAL_AUTHORING.md"
  "docs/UI_LAYER.md"
  "examples/recipe_hero_with_hud.rs"
  "examples/recipe_continuous_world.rs"
  "examples/recipe_semantic_zoom.rs"
  "examples/recipe_cinematic_overlay.rs"
  "examples/recipe_capability_safe_canvas.rs"
  "examples/recipes_support/mod.rs"
)

rm -rf "$OUT"; mkdir -p "$OUT/control" "$OUT/treatment"

echo "== exporting released $V tree -> control =="
git -C "$SRC" archive "$V" | tar -x -C "$OUT/control"

echo "== pruning hypothesis/narrative docs from control =="
for d in "${PRUNE_DIRS[@]}"; do rm -rf "$OUT/control/$d"; echo "  - removed dir $d"; done
for f in "${PRUNE_FILES[@]}"; do rm -f "$OUT/control/$f"; echo "  - removed file $f"; done

echo "== treatment = copy of sanitized control =="
cp -a "$OUT/control/." "$OUT/treatment/"

echo "== overlaying frozen treatment files onto treatment =="
TREAT_SHA="$(git -C "$SRC" rev-parse "$TREAT_REF")"
for p in "${OVERLAY[@]}"; do
  mkdir -p "$OUT/treatment/$(dirname "$p")"
  git -C "$SRC" show "$TREAT_REF:$p" > "$OUT/treatment/$p"
  echo "  + overlaid $p"
done

echo
echo "== MANIFEST: control base =="
CONTROL_SHA="$(git -C "$SRC" rev-parse "$V^{commit}")"
echo "control_source_tag=$V"
echo "control_source_commit=$CONTROL_SHA"
echo "control_files=$(cd "$OUT/control" && find . -type f | wc -l)"
echo
echo "== MANIFEST: treatment overlay SHA256 (from $TREAT_REF @ $TREAT_SHA) =="
( cd "$OUT/treatment" && for p in "${OVERLAY[@]}"; do sha256sum "$p"; done )
echo
echo "== sanity: treatment must NOT contain hypothesis research files =="
echo -n "  AI_VISUAL_AUTHORING_AUDIT present? "; [ -e "$OUT/treatment/docs/research/AI_VISUAL_AUTHORING_AUDIT.md" ] && echo "YES(BAD)" || echo "no(good)"
echo -n "  docs/research/ present?            "; [ -d "$OUT/treatment/docs/research" ] && echo "YES(BAD)" || echo "no(good)"
echo -n "  guide present in treatment?        "; [ -e "$OUT/treatment/docs/AI_VISUAL_AUTHORING.md" ] && echo "yes(good)" || echo "NO(BAD)"
echo -n "  guide ABSENT in control?           "; [ -e "$OUT/control/docs/AI_VISUAL_AUTHORING.md" ] && echo "PRESENT(BAD)" || echo "absent(good)"
echo -n "  'panel farm' anywhere in control?  "; grep -rIl 'panel farm' "$OUT/control" >/dev/null 2>&1 && echo "YES(BAD)" || echo "no(good)"
echo -n "  'panel farm' in treatment (only guide ok): "; grep -rIl 'panel farm' "$OUT/treatment" 2>/dev/null | sed "s#$OUT/treatment/##" | tr '\n' ' '; echo
echo "== done =="
