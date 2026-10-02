# Upstream actions (femboy2112/libgibson) — Project Chronoscope

All titled `[v0.4.0 consumer: Project Chronoscope] <mechanism>`; filed after searching the 30 existing issues.
Nothing in `femboy2112/libgibson` was modified.

| kind | # | title / subject | repro + recorded output |
|---|---|---|---|
| issue | [#76](https://github.com/femboy2112/libgibson/issues/76) | `Effect::Shake` and `Effect::Jitter` never settle: `eval` ignores their `duration` | `repro_shake_duration.rs`, `repro_shake_duration.out.txt` |
| issue | [#77](https://github.com/femboy2112/libgibson/issues/77) | `Scene` has no entity removal: hidden entities are still cloned and evaluated every frame | `repro_scene_no_remove.rs`, `repro_scene_no_remove.out.txt` |
| issue | [#78](https://github.com/femboy2112/libgibson/issues/78) | `HumanMusicSynth::render` with a non-contiguous `RenderCtx::start` fires every skipped event at once | `repro_synth_jumped_start.rs`, `repro_synth_jumped_start.out.txt` |
| issue | [#79](https://github.com/femboy2112/libgibson/issues/79) | `Context::run_once` never returns after the terminal hangs up if the process survived SIGHUP | `repro_hangup_spin.rs`, `repro_hangup_crossterm_only.rs`, `hangup_driver.py`, `hangup-observed.txt`, `hangup_probe.rs`, `hangup_probe_driver.py`, `hangup-probe-observed.txt` |
| comment | [#74](https://github.com/femboy2112/libgibson/issues/74#issuecomment-5948489460) | measurements from the interactive side: no cancel, RSS vs PCM budget, seek cost | `docs/evidence/audio.md`, `docs/evidence/sustained-12000-audio.md` |
| comment | [#15](https://github.com/femboy2112/libgibson/issues/15#issuecomment-5948489722) | positive second-consumer data point for queued input after resize (not attributed) | `resize-collision-observed.txt`, `tests/pty_resize_collision.rs` |

The repros are standalone `main` files; build them against
`libgibson = { git = "https://github.com/femboy2112/libgibson", tag = "v0.4.0" }` (the hang-up ones also need `crossterm = "0.29"`
and `libc`). The recorded outputs are from one host (Linux 7.0 x86_64, Rust 1.98.1).
