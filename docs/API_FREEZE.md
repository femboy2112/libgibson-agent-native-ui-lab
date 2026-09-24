# API freeze before surprise selection

Freeze instant (UTC): 2026-09-24T05:59:50.744624+00:00.

- **FROZEN_LIBGIBSON_SHA:** `e5ede0a4ab8ff52caa567ee10d45824e42c1ebc6`
- **FROZEN_LAB_HELPER_IR_SHA:** `1d1d27286b50c752527021697c14fd8a32ccd0ea`
- No generic core primitive earned promotion. The dependency has never changed.
- LibGibson main `3d9117d4bfa45f7d890bff12d40879a8d2bb6a47` additionally
  contains the merged non-binding dossier; implementation is identical.

[Machine-readable receipt](FREEZE.json) hashes 21 files: public dependency
manifest/lock, runner, fixture, all planned consumers/IR and their tests, and
verification tools. `python3 scripts/verify_freeze.py` compares both working
files and their bytes at the frozen commit. Additive `src/lib.rs` module exports
are allowed solely to compile new independent holdout modules.

No holdout concepts were requested or selected before this committed freeze.
A fresh context next receives only the neutral public API guide and may inspect
public library source. It will propose five ideas; root screens the proposals
against the dossier before selecting three. Fresh contexts share a model family
and filesystem; this is context separation, not a security-isolated blind trial.
No holdout can modify core, helpers, dependency versions or the IR to pass.
Consumer-specific types and direct public Surface/canvas code remain allowed.
A genuine API wall is retained as a failed holdout, not rescued.
