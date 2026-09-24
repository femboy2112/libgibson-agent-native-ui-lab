# External expressivity protocol

Preregistered before experiment implementation, 2026-09-24.

Substrate: `e5ede0a4ab8ff52caa567ee10d45824e42c1ebc6`, the ordinary merge
of flagship PR #14. Exact-main public CI `35960000454`: all five jobs passed.
Local Rust 1.98.1 baseline: 574 tests (226 unit / 348 integration), fmt,
strict Clippy/rustdoc, release/example builds, FX Lab and binding/sanitizer smoke passed.

This separate public repository tests external consumption. No path dependency,
private module access, raw ANSI, generated native code or substrate edits.
One finite semantic trace drives all three planned consumers. Explicit user
actions are recorded separately; presentation never changes semantic decisions.

Implementation order: manga first; semantic reactions second; validated
instruments third. Friction is recorded before reconciliation. Only repeated,
generic evidence can justify a separate LibGibson promotion PR.

Required matrix: 56×24, 80×24, 120×32, 160×40; TrueColor, ANSI16, Mono.
Tests distinguish semantic equality, replayed presentation, usable keyboard
focus under reflow, rejection paths and bounded resource policies. Visual
inspection is additional evidence, not a test-derived beauty claim.

After reconciliation, record exact substrate and helper/IR commits. Only then
ask a fresh context for five new concepts, without supplying the dossier's
examples. Screen proposals against dossier contamination, select three for
diversity, and assign fresh implementers. Same-model fresh contexts are not
independent model families. No core/helper/IR rescue during these holdouts.

Verdicts: PUBLIC API SUFFICIENT, ERGONOMIC GAP, GENERIC PRIMITIVE GAP,
DELIBERATE SAFETY BOUNDARY, EXPRESSIVE WALL. Three successful holdouts cannot
prove universal representability. Preserve failures and narrow claim boundaries.
