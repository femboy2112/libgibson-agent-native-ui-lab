#!/usr/bin/env bash
# Build sanitized CONTROL and TREATMENT libgibson-reference/ trees for the
# AI visual-authoring holdout. Auditable + re-runnable.
set -euo pipefail

SRC="/home/leah/LibGibson"                       # upstream LibGibson repo
# Frozen treatment overlay source. Pinned to an EXACT commit (not a branch name)
# so the overlay is reproducible and cannot silently drift if the branch moves.
# treatment-v2 = a31b134 (atlas, degeneracy fixes); treatment-v1 was 58f3385.
TREAT_REF="${TREAT_REF:-a31b13447d3243ab1b7c4b34fb197b097c59e69d}"
V="v0.4.0"                                        # released substrate tag
OUT="${OUT:-/tmp/holdout-v0.4.0-reference}"      # throwaway build dir (overridable)

# Files pruned from BOTH arms (hypothesis record + model-naming narrative docs)
PRUNE_DIRS=( "docs/research" )
PRUNE_FILES=( "docs/HUMAN_MUSIC_RELEASE_ROAST.md" "docs/STATE_OF_LIBGIBSON.md" "docs/INTRODUCTORY_CINEMA.md" )

# The TREATMENT overlay (the ONLY difference between arms). treatment-v2:
# adds the atlas plan + three atlas recipes (skeleton/identity-transport/
# directed-atlas) over the v1 set. docs/research/* stays pruned from BOTH arms.
OVERLAY=(
  "AGENTS.md"
  "docs/AI_VISUAL_AUTHORING.md"
  "docs/AI_VISUAL_ATLAS_PLAN.md"
  "docs/UI_LAYER.md"
  "examples/recipe_hero_with_hud.rs"
  "examples/recipe_continuous_world.rs"
  "examples/recipe_semantic_zoom.rs"
  "examples/recipe_cinematic_overlay.rs"
  "examples/recipe_capability_safe_canvas.rs"
  "examples/recipe_atlas_skeleton.rs"
  "examples/recipe_identity_transport.rs"
  "examples/recipe_directed_atlas.rs"
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
echo -n "  atlas plan present in treatment?   "; [ -e "$OUT/treatment/docs/AI_VISUAL_ATLAS_PLAN.md" ] && echo "yes(good)" || echo "NO(BAD)"
echo -n "  atlas plan ABSENT in control?      "; [ -e "$OUT/control/docs/AI_VISUAL_ATLAS_PLAN.md" ] && echo "PRESENT(BAD)" || echo "absent(good)"
echo -n "  guide ABSENT in control?           "; [ -e "$OUT/control/docs/AI_VISUAL_AUTHORING.md" ] && echo "PRESENT(BAD)" || echo "absent(good)"
echo -n "  'panel farm' anywhere in control?  "; grep -rIl 'panel farm' "$OUT/control" >/dev/null 2>&1 && echo "YES(BAD)" || echo "no(good)"
# 'panel farm' is teaching vocabulary (the composition anti-pattern the treatment
# teaches you to avoid). It is legitimate ONLY in the treatment's teaching docs
# (holdout §13): the guide and the atlas plan. It must never appear in control,
# in a recipe, or anywhere that would name a model or the hypothesis.
echo -n "  'panel farm' in treatment (guide + atlas plan ok): "; grep -rIl 'panel farm' "$OUT/treatment" 2>/dev/null | sed "s#$OUT/treatment/##" | tr '\n' ' '; echo
echo "== done =="
