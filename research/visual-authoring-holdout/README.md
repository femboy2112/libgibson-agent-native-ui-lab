# AI visual-authoring holdout — v0.4.0

Blind CONTROL vs TREATMENT test of whether the LibGibson visual-authoring docs
change a fresh non-Sonnet agent's design behavior. Design authority: upstream
`docs/research/AI_VISUAL_AUTHORING_HOLDOUT.md` (+ §10 amendment). Operating
record: `PROTOCOL.md`. What each arm sees: `manifests/SANITIZATION.md`.

> **The experiment subjects do not run from this branch.** This branch is the
> coordinator record. Subjects run in throwaway worktrees off clean lab `main`
> and never see this dir, the protocol, the mapping, or each other.

## The working kit lives on disk (not in git)

The launch-ready kit is materialized at
`/home/leah/libgibson_lab/holdout-v0.4.0/` (big reference trees + per-run launch
scripts + the local mapping are deliberately **not** committed). This branch
carries the reproducible subset: prompts, manifests, the treatment overlay, the
build/capture/anonymize scripts, and the review template.

## Running it (maintainer / Leah — you hold the launch trigger)

Each launch is the **outward** step: it unleashes an untrusted external model
with auto-approved tools in an isolated worktree. (Claude Code's own safety
classifier refuses to spawn these, which is why this is hand-run.)

```bash
cd /home/leah/libgibson_lab/holdout-v0.4.0
./launch/R01.sh      # ... through R08.sh — any order; each is self-contained
```

Then, for every run that produced an app:

```bash
./scripts/capture.sh R01     # builds + captures 4 configs -> runs/R01/cap/*.png
# ... R02 .. R08
./scripts/anonymize.sh       # -> blind-review/candidate-A..H/ + BLIND_REVIEW.md
```

Open `blind-review/BLIND_REVIEW.md`, judge each candidate from the images only,
then **stop**. Unblinding is a separate later round.

## Invariants

- Subjects get the remote `v0.4.0` tag; the reference tree is lookup-only.
- No rescuing: ugly / panel / crash-from-own-code = DATA. Only infra failures
  invalidate a run.
- Nothing reveals arm or model to the blind packet; `MAPPING.local.txt` stays
  local and untracked until unblinding.
- Do not touch lab PRs #4–#7 or upstream issues #73–#79. Do not edit/merge
  PR #80 or implement any API from these results.
