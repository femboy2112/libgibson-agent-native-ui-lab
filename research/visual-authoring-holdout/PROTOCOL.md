# Visual-authoring holdout — operating protocol (lab side)

The **authority** for the design is upstream:
`libgibson/docs/research/AI_VISUAL_AUTHORING_HOLDOUT.md` (plus its §10 pre-contact
amendment, commit `58f3385`). This file is the concrete, lab-side operating
record: the pins, the matrix shape, and the rules whoever runs it must follow.

## Question

Does the documentation intervention (guide + recipes + `AGENTS.md` router +
`UI_LAYER` callout, now including the representation-atlas plan and its recipes)
materially change the design behavior of a fresh, non-Sonnet coding agent given
the same substrate and a comparably ambitious prompt?

## Arms (one variable)

| Arm | `libgibson-reference/` tree |
|---|---|
| CONTROL | sanitized released `v0.4.0` (see `manifests/SANITIZATION.md`) |
| TREATMENT | same tree **+** the 13-file treatment-v2 overlay, nothing else |

The overlay is **treatment-v2** (the representation atlas plus the v2 degeneracy
fixes), pinned to `research/ai-visual-authoring @ a31b134`; the 13 files and their
SHA256 are in `manifests/SANITIZATION.md`. (treatment-v1 was the 9-file overlay at
`58f3385`, preserved historically in the same manifest.)

Both arms: app depends on the remote tag
`libgibson = { git = "https://github.com/femboy2112/libgibson", tag = "v0.4.0" }`
— no path override, no patch, no research-branch dependency.

## Subjects (fresh, non-Sonnet, pinned — OVERRIDABLE, documented)

| key | subject | invocation |
|---|---|---|
| DS | DeepSeek | `opencode run --model ollama/deepseek-v4.1-flash:cloud --auto <prompt>` |
| AGY | agy | `agy --print --dangerously-skip-permissions --model gemini-3.8-flash-high <prompt>` |

Notes recorded honestly:
- The bare `deepseek-v4-flash:cloud` alias is **retired** (2026-09-25); the live
  model is `deepseek-v4.1-flash:cloud`.
- `agy` is a multi-model harness that *can* drive Claude; it is pinned here to a
  **non-Claude** model (`gemini-3.8-flash-high`). The exact model `agy` drove for
  the prior Hyperion work was not recoverable from the lab, so a fixed non-Claude
  model is used and held constant across agy's four runs (holdout §4 allows the
  configured default, documented). Change the pin in `scripts/gen_runs.sh` before
  launch if a different non-Claude model is wanted.
- The two subjects use different harnesses (opencode vs agy) because DeepSeek is
  not available inside agy — an inherent confound, matching the prior campaign.
  Recorded in threats-to-validity; the within-subject CONTROL↔TREATMENT contrast
  is unaffected.

## Matrix

2 subjects × 2 tasks (Strata, Estuary) × 2 arms = **8 fresh sessions**, opaque
ids `R01`–`R08`. Arm-first order is balanced across the four subject×task pairs.
Each run: a clean worktree from lab `main`, its own `holdout/R0x` branch, the
correct read-only `libgibson-reference/`, and the frozen prompt. No run can see
another, the protocol, the mapping, or any prior Hyperion branch.

## Hard rules

- **Do not rescue.** Ugly output, a panel/dashboard look, an unfinished app, a
  compile failure from the model's own code — all **DATA**. Never paste the
  guide, say "make the world bigger", or fix the model's API mistakes. Only pure
  infrastructure failure (process crash before real work, service down, disk
  full, broken tool invocation, coordinator-corrupted worktree) invalidates a run
  and allows a fresh repeat; record every invalidation.
- **No source/guide edits after first contact.** The treatment and prompts are
  frozen. Record if anything changed (it must be "NO").
- **No upstream issue spam.** Subjects record friction in their own report; the
  coordinator does not turn this into a bug-fix round. Do not touch existing
  lab PRs #4–#7 or upstream issues #73–#79.

## Capture (post-run, uniform)

**Static ladder (every valid run):** `120x40`, `80x24`, `42x15` color, and
`120x40` Mono, via the app's own `--capture WxH[:mono]` path, rendered to PNG by
the single shared pipeline `scripts/ansi2png.py` (pyte → Pillow, fixed cell
geometry, no crop, no post-processing). Run with `scripts/capture.sh R0x`. If an
app uses a different capture flag, set it per that app's README — do **not** edit
the app to look better.

**Temporal ladder (runs whose app has a seekable timeline only):** the atlas
treatment teaches a pure `frame(t)` director, so a treatment app may be seekable.
Where it is, capture the transition as frames, not just an end state, so the
review can judge whether a transition *reads as a transform* (held out as human
art direction, holdout §13). Run with `scripts/capture_temporal.sh R0x`, which
reads a per-run `runs/$RID/TEMPORAL.plan` (copy `scripts/TEMPORAL.plan.template`):

- Canonical phases: `establish`, `pre-transition`, `transition-midpoint`,
  `post-transition`, `reveal`, `hold`, `payoff` — keep only those the app has.
- When the app has a sharp transition at a boundary `b`, also bracket it with
  `b-ε`, `b`, `b+ε` (`transition_pre` / `transition` / `transition_post`).
- Each phase is rendered at `120x40` color and `120x40` Mono via the convention
  `--capture WxH[:mono] --at T`; set `SEEK_FLAG` per the app's README if it
  differs. **Raw deterministic `.ansi` frames are authoritative**; PNG (and any
  GIF/video a reviewer later derives) are convenience, not required.
- Times in `TEMPORAL.plan` are **observed from a dry run of the app, never
  invented**. If the app has no temporal transition, the plan is `NONE` and only
  the static ladder applies — a static-only piece is valid data, not a failure.

Both ladders obey the no-rescue rule: a missing frame is recorded `NO_FRAME`
(data), never faked.

## Blind review & stop condition

`scripts/anonymize.sh` shuffles valid runs to `candidate-A…H`, copies only the
four PNGs (metadata stripped), and writes `BLIND_REVIEW.md`. The run→arm and
run→candidate maps live only in `MAPPING.local.txt` (untracked). **STOP** there:
the maintainer renders the blind verdict first; unblinding and the §10C
failure-mode classification happen in a later round. Do not unblind, do not edit
or merge PR #80, do not implement any API from these results.
