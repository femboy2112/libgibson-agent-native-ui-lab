# Project Cathedral — Friction

First-contact friction recorded while consuming the released **LibGibson `v0.4.0`** public
Rust API. Classifications use the laboratory's allowed set: **ERGONOMIC INCONVENIENCE**,
**HARNESS-SPECIFIC**, **DELIBERATE SAFETY BOUNDARY**, **GENERIC PRIMITIVE GAP**, **EXPRESSIVE
WALL**. Every entry records the desired behaviour, the public route attempted, the observed
result and the local workaround. Friction is recorded before any reconciliation.

No LibGibson source, patch, vendor copy or path dependency was used. `Cargo.lock` resolves
`tag = "v0.4.0"` to `c2f6483d92fe2b351e6cd50936a97d8cdf73cb79`.

| ID | Desired behaviour / attempted public route | Result / workaround | Classification |
|---|---|---|---|
| F-CATH-1 | A mistyped story start beat fails loudly. `Story::new(id).start()` — the obvious entry point. | `start()` returns an already-**finished** director for an unknown id: no error, `current_beat()` echoes the bad id. `validate()` does name the problem but nothing routes `start()` through it. Project Cathedral's scenario silently ran **zero beats**. Workaround: call `.validate()` in a unit test that pins the behaviour and keep the correct id. | ERGONOMIC INCONVENIENCE |
| F-CATH-2 | One-shot story intents should fire once. `StoryAction::set_text` + beat transitions. | Facts persist for the director's life, so a beat that does not set a new action re-reads the previous beat's fact and re-emits the directive (we got a duplicate `inject-fault`). Workaround: clear the one-shot fact (`facts_mut().set_text(key, "")`) immediately after dispatch. Documented ("facts persist"), but the one-shot idiom is app-owned glue. | ERGONOMIC INCONVENIENCE |
| F-CATH-3 | Render a long checked HumanMusic score to disk without holding it all in memory. `HumanMusicSynth::total_samples()` + `OfflineRenderer::render(..)` + `write_wav_i16(..)`. | Both the renderer result and the WAV writer hold the entire PCM buffer. A 10,000-frame take (~100 MB WAV) peaked at ~614 MiB RSS; there is no streaming/incremental render or write path. Workaround: accept the memory cost, or export a shorter representative take. | ERGONOMIC INCONVENIENCE |
| F-CATH-4 | A live score should not grow its build cost without bound. `SongMap::build` + `perform_checked` on a trace whose length tracks the frame horizon. | `total_beats` grows with the incident horizon, so a naive live rebuild becomes superlinear on a 10,000-frame run. Workaround: the consumer bounds the score horizon (`MUSIC_HORIZON_FRAMES = 1400`), which is also a reasonable live-score semantic. | HARNESS-SPECIFIC |
| F-CATH-5 | Per-frame render accounting read directly. `Context::stats()`. | `dirty_cells`, `frame_bytes` and `history_insertions` are **absolute** counters; a consumer must diff successive snapshots. This is documented on `RenderStats`, so it is a naming/ergonomics wrinkle, not a gap. Workaround: keep `prev_stats` and saturating-subtract. | ERGONOMIC INCONVENIENCE |
| F-CATH-6 | Read a frame as plain text. `Surface::to_visible_lines()`. | Works; this is how `--text` and the capability matrix inspect the surface without parsing ANSI. No workaround needed. | DELIBERATE SAFETY BOUNDARY |
| F-CATH-7 | Verify application input delivery from receipts. Application-side telemetry. | The consumer app (by our design) emits no per-key input receipt, so a PTY probe can only observe action *side effects*. Workaround: assert on committed `◇ ACTION` scrollback lines. This is a harness gap, not a LibGibson gap. | HARNESS-SPECIFIC |

## Notes on the classifications

- **No `GENERIC PRIMITIVE GAP` and no `EXPRESSIVE WALL` was demonstrated.** The custom tower/
  bridge/echo geometry, the nine-phase incident state and the checked-BAND score were all
  expressible through the public `Surface`/`Node`/`story`/`audio` surface. Where the app does
  more work (focus policy, one-shot intents, horizon bound, per-frame diffs), the route exists
  and the glue is small.
- **No `DELIBERATE SAFETY BOUNDARY` was hit as friction.** Facts persisting, counters being
  absolute and offline rendering staying whole-buffer all read as deliberate, documented
  contracts; they are recorded so a future consumer knows they are intentional.
- These entries come from a single implementation context. They are not independent evidence
  families and do not by themselves justify an upstream change. F-CATH-1 and F-CATH-3 were
  filed as focused v0.4.0-consumer issues; F-CATH-2 and F-CATH-5 are recorded here without a
  separate issue because the documented contract or a small local idiom resolves them.

## Issues and comments filed

| Ref | Title / subject | Mechanism |
|---|---|---|
| [libgibson#73](https://github.com/femboy2112/libgibson/issues/73) | `[v0.4.0 consumer: Project Cathedral] Story::start() silently returns a finished director for an unknown start beat` | F-CATH-1 |
| [libgibson#74](https://github.com/femboy2112/libgibson/issues/74) | `[v0.4.0 consumer: Project Cathedral] Offline HumanMusic rendering materializes the full PCM buffer (no streaming render/write path)` | F-CATH-3 |
| comment on [libgibson#15](https://github.com/femboy2112/libgibson/issues/15#issuecomment-5947166188) | `Investigate queued input after resize under graphical PTY backpressure` | Honest negative/partial evidence; **not** claimed fixed |

Both issues were filed after searching the open+closed issue list; neither mechanism was already
covered (the only open issue was #15).
