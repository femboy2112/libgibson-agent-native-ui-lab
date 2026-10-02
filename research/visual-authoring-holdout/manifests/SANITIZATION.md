# Sanitization manifest — reference trees & frozen inputs

This records exactly what each arm's read-only `libgibson-reference/` tree
contains, so the only difference between CONTROL and TREATMENT is the treatment
overlay. Built by `scripts/build_reference_trees.sh` (auditable, re-runnable).

## Control base (both arms start here)

- **Source:** released LibGibson `v0.4.0`
- **Commit:** `c2f6483d92fe2b351e6cd50936a97d8cdf73cb79`
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

## Treatment overlay (TREATMENT only — the single variable)

From `research/ai-visual-authoring` @ `58f3385899170fc665718aecef94cfa7c6fcc3d7`
(the overlay files are byte-identical to the frozen tip `e7028b4`; the later
commit only touched the holdout doc, which is **not** in the overlay):

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

`docs/UI_LAYER.md` is the treatment version (overwrites the v0.4.0 one).
`docs/AI_VISUAL_AUTHORING.md` is the only file where "panel farm" legitimately
appears (holdout §13); verified absent everywhere in control.

## Frozen task prompts (identical across arms)

```
986fb04ee549429b3555178437b403b6c925afd1342434589a30568a93e6acfe  task_A_strata.md
38cb1323846029a8955dbb87a339721319c3ab229235ffcd8d8654ce98fc3cbd  task_B_estuary.md
```

Screened clean of every forbidden teaching token (panel/dashboard/DOS/retro/
hero/camera/chrome/composition/metaphor/prior-model names). They state ambition
("unusually ambitious, visually striking … not a static report") but teach no
method.
