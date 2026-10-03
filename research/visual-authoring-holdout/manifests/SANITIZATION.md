# Sanitization manifest — reference trees & frozen inputs

This records exactly what each arm's read-only `libgibson-reference/` tree
contains, so the only difference between CONTROL and TREATMENT is the treatment
overlay. Built by `scripts/build_reference_trees.sh` (auditable, re-runnable).

## Control base (both arms start here)

- **Source:** released LibGibson `v0.4.0`
- **Commit:** `c2f6483d92fe2b351e6cd50936a97d8cdf73cb79`
- **Control file count:** 1285
- Exported with `git archive v0.4.0`, then the following were **removed from
  both arms** because they carry the hypothesis or name prior models (would
  contaminate the contrast and violate holdout §10A):
  - `docs/research/` (entire dir) — the Sept-24 expressivity/campaign record:
    `LIBGIBSON_EXPRESSIVITY_API_AUDIT`, `AGENT_NATIVE_UI_CAMPAIGN_RESULTS`,
    `AGENT_NATIVE_UI_EXPERIMENT_PLAN`, `AGENT_NATIVE_DYNAMIC_UI_RESEARCH_PROGRAM`,
    `AGENT_NATIVE_UI_ZERO_CONTEXT_HANDOFF` (hypothesis + model comparisons).
  - `docs/HUMAN_MUSIC_RELEASE_ROAST.md` — narrative, names prior models.
  - `docs/STATE_OF_LIBGIBSON.md` — narrative, names model/branch work.
  - `docs/INTRODUCTORY_CINEMA.md` — narrative, names prior model branches.
- Everything else (API/feature docs, `src/`, `examples/`) is kept verbatim as
  shipped. This matches the preregistration's CONTROL spec (README, UI_LAYER,
  FEATURES, GLYPHS, API docs, existing examples).

### Known residual (accepted, documented)

- `examples/polished_agent.rs` contains an inert demo-fixture string
  `deepseek/visual-fx-frontier` (a fake git branch rendered inside a showcase).
  It is released example source; editing example source would deviate from the
  released substrate. It reveals no visual hypothesis and no failure verdict.
  Present in **both** arms, so it is not a differential confound.
- `examples/libgibson_intro/` (the released cinematic example) is kept verbatim
  in **both** arms. The treatment's teaching docs point at it by name as shipped
  reference code; because it is identical in both arms it is not a differential
  confound (same category as `polished_agent.rs`). Only the *narrative* doc
  `docs/INTRODUCTORY_CINEMA.md` is pruned.

## Treatment overlay v2 — ACTIVE (TREATMENT only, the single variable)

From `research/ai-visual-authoring` @
`a31b13447d3243ab1b7c4b34fb197b097c59e69d` (treatment-v2: the representation
atlas plus the v2 degeneracy fixes). The overlay adds the atlas plan and three
atlas recipes over v1; `docs/research/*` is **not** in the overlay.

```
b406cafd5c50762c46676bca3928f49b9291695dc5b054a502e93120b3ddcf4b  AGENTS.md
9633b52035467fffbc06fa5e55e7cd6bd2a396f30286d449edb36713e7d0d8e4  docs/AI_VISUAL_AUTHORING.md
0be1710386e6f2981d24971ceab12262e838fcab1f0891837647b1e26d1f54e1  docs/AI_VISUAL_ATLAS_PLAN.md
289e9da6189c2c1b9e9f9a06e2e4eb6a92dc387cd91970e51c0697a132486f8d  docs/UI_LAYER.md
002b302fb3c9cb9b64dc6d3a3c2637d8bd5e7418c02739612fdcb8aab23767d5  examples/recipe_hero_with_hud.rs
0fb3c8a6ad18b23c992e3ded0500dcf7af6f3520f626445a693bda9cd07861d9  examples/recipe_continuous_world.rs
6b52184f8abd63f07b97b6120e796331675e3c4f9b841bc5dfbb9efb0605f43c  examples/recipe_semantic_zoom.rs
035bcd625156a8b3769b6c71028b53459f80626c78d264a04a9b4941ef997fd6  examples/recipe_cinematic_overlay.rs
deae9b39a512bcb6c4a2a08a424ce7b353653ce3b556bc00a01e10b3b09e76e5  examples/recipe_capability_safe_canvas.rs
9462317cdf21d452fe772f111e46fab58d63d908b1f85e5b1cd63f12e8ee2e7c  examples/recipe_atlas_skeleton.rs
2886afe946c0800fc6df09fceaaab247217439107a0b07da98865b15f4ad1f1d  examples/recipe_identity_transport.rs
530baf755966ece86641f99315b2bcad5790157e94e9e71ae180ab4f8308b5f9  examples/recipe_directed_atlas.rs
22b7e59d3ad562ebc0f74b4138f4800faf5124230d726e6570d614c891ffd9dc  examples/recipes_support/mod.rs
```

`docs/UI_LAYER.md` is the treatment version (overwrites the v0.4.0 one).
`docs/AI_VISUAL_AUTHORING.md` and `docs/AI_VISUAL_ATLAS_PLAN.md` are the two
teaching docs where "panel farm" legitimately appears (holdout §13, teaching
vocabulary for the composition anti-pattern); verified absent everywhere in
control and in every recipe.

### v2 blind-integrity screen (no new contamination over v1)

The three new overlay files and the rewritten guide were screened for
model-naming / hypothesis / holdout leak tokens. Findings, all benign:

- `libgibson_intro` is named in `AGENTS.md`, the guide, and
  `recipe_directed_atlas.rs` — it is a **released example present in both arms**
  (see residual above), so naming shipped reference code teaches method without
  revealing the contrast.
- `Chronoscope (external)` in the guide is a **pre-existing v1 residual**
  (v1 guide line 212), unchanged by v2 — an abstract "a full external
  realization exists" existence proof that names no holdout subject
  (DeepSeek / agy) and no verdict.
- The genuinely new surface — `AI_VISUAL_ATLAS_PLAN.md`,
  `recipe_atlas_skeleton.rs`, `recipe_identity_transport.rs` — introduces **no**
  model name, hypothesis, or holdout reference.

## Treatment overlay v1 — HISTORICAL (superseded by v2, kept for audit)

The first-contact overlay, pinned to `research/ai-visual-authoring` @
`58f3385899170fc665718aecef94cfa7c6fcc3d7` (byte-identical to the frozen tip
`e7028b4`; the later commit only touched the holdout doc, not in the overlay).
This is the overlay that was live at lab commit `2c58fe3`; preserved here as the
historical manifest. The full v1 overlay file bodies remain in git history at
that commit.

```
c534a1dca68f06e8032e51f8673ed4cd54d7f25f0ae34cee0c44c06f397a0dcf  AGENTS.md
fab7590416c17ae0fe5dd1f2a6fc9bb3c3493a752c8aa0e0bf8cb03e59d14bab  docs/AI_VISUAL_AUTHORING.md
289e9da6189c2c1b9e9f9a06e2e4eb6a92dc387cd91970e51c0697a132486f8d  docs/UI_LAYER.md
002b302fb3c9cb9b64dc6d3a3c2637d8bd5e7418c02739612fdcb8aab23767d5  examples/recipe_hero_with_hud.rs
0fb3c8a6ad18b23c992e3ded0500dcf7af6f3520f626445a693bda9cd07861d9  examples/recipe_continuous_world.rs
6b52184f8abd63f07b97b6120e796331675e3c4f9b841bc5dfbb9efb0605f43c  examples/recipe_semantic_zoom.rs
035bcd625156a8b3769b6c71028b53459f80626c78d264a04a9b4941ef997fd6  examples/recipe_cinematic_overlay.rs
deae9b39a512bcb6c4a2a08a424ce7b353653ce3b556bc00a01e10b3b09e76e5  examples/recipe_capability_safe_canvas.rs
22b7e59d3ad562ebc0f74b4138f4800faf5124230d726e6570d614c891ffd9dc  examples/recipes_support/mod.rs
```

(UI_LAYER.md, the five shared recipes, and `recipes_support/mod.rs` are
byte-identical between v1 and v2 — same hashes. The v1→v2 delta is exactly:
`AGENTS.md` and the guide rewritten, and four files added — the atlas plan,
`recipe_atlas_skeleton.rs`, `recipe_identity_transport.rs`,
`recipe_directed_atlas.rs`.)

## Frozen task prompts (identical across arms)

```
986fb04ee549429b3555178437b403b6c925afd1342434589a30568a93e6acfe  task_A_strata.md
38cb1323846029a8955dbb87a339721319c3ab229235ffcd8d8654ce98fc3cbd  task_B_estuary.md
```

Screened clean of every forbidden teaching token (panel/dashboard/DOS/retro/
hero/camera/chrome/composition/metaphor/prior-model names). They state ambition
("unusually ambitious, visually striking … not a static report") but teach no
method.
