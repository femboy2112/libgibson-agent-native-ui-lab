# FRICTION — Project Chronoscope against LibGibson v0.4.0

Time travel is hostile to APIs designed for forward progress. This file records what hurt, where it
hurt, and *whose* it is. Every entry names the evidence (a test, a measurement or a source line);
every claim carries a status:

* **Proven** — a deterministic reproduction exists in this repo or `docs/upstream/`, and (for defects) the
  documented contract or the source says otherwise.
* **Observed** — seen in a real run, not isolated.
* **Boundary** — documented, deliberate; recorded so nobody claims otherwise.

Categories: APPLICATION BUG · ERGONOMIC INCONVENIENCE · GENERIC ERGONOMIC GAP · GENERIC PRIMITIVE GAP ·
PROBABLE LIBGIBSON DEFECT · PROVEN LIBGIBSON DEFECT · DELIBERATE SAFETY BOUNDARY · EXPRESSIVE WALL.
Upstream references are to `femboy2112/libgibson` issues; "filed" entries have a minimal reproduction under
`docs/upstream/` and link the issue (see the table at the end).

---

## The shape of it: easy forward, painful backward

| going forward (cheap) | going backward (the price) |
|---|---|
| `Context::run_once` + `set_root` + `UiRuntime::frame` | history time ≠ UI time: `UiRuntime` clamps time **backwards** (by design), so two clocks are mandatory |
| `StoryDirector::update(dt, events)` | no seek, no snapshot API, no inverse: rewind = **clone a checkpoint + replay** (works; ~11–18 µs/clone) |
| `Scene::add` | **no `remove`**: a retired entity is evaluated and cloned every frame forever |
| `Effect::*` for finite flourishes | `Shake`/`Jitter` never settle, so a rewound-and-replayed atmosphere would shake forever |
| `compose` + `HumanMusicSynth` + `OfflineRenderer` | synth is **forward-only**; `compose` is **not prefix-causal**; a jumped `RenderCtx::start` is silently wrong; renders cannot be cancelled |
| keyed focus, modals | survives rebuilding historical trees ✓; does **not** survive a control disappearing and returning |
| `Context::last_frame_lines()` | text only: styles/colours of the composed frame need a VT emulator |

---

## APPLICATION BUGS (mine; all found by the tests/measurements below; all fixed unless stated)

| # | bug | how it was found | status |
|---|---|---|---|
| A1 | `Model` asked for HumanMusic performances on every fork/switch/compare **even when audio was off**, rendering ~4 s of DSP each, and never enforced the audio byte budget: first 1,500-frame trial = 72 performances, **744 MiB resident, RSS 890 MB** | first `--sustained` trial (RSS series 18 → 554 → 890 MB) | **fixed**: `AudioMode::{Off,Silent,Play}`, `want_audio()`, `enforce_budget` in `tick` (40.6 MB RSS after 12,000 frames without audio; with audio see A12) |
| A2 | The catastrophe beat used `Effect::Shake`; it **never stops** (→ P1 below). The meltdown banner would have shaken for the rest of the beat | `tests/story.rs` settle assertion failed | **fixed**: `settling_shake` composed from `Effect::translate` ending at (0,0) |
| A3 | Arrow keys did nothing in the real PTY (but worked headless): with any keyed control on screen the UI layer spends arrows on focus traversal | PTY session; headless rig had no side panel at 80×24 | **fixed**: the viewport is a keyed `on_event` sink (see E3) |
| A4 | Quitting while a HumanMusic performance was rendering **blocked until the render finished** (`Drop` joined the worker; `OfflineRenderer::render` has no cancel — G4) | `tests/pty.rs::quit_does_not_wait_for_an_in_flight_music_render` design | **fixed**: worker is detached on drop |
| A5 | Epoch classifier flickered **Contradiction** for 5 steps every ~40: CREDITS/LEDGER are legitimately two instructions apart | epoch spans dump of the first fixture | **fixed**: mismatch must persist 14 steps |
| A6 | The guided demo rewound to step 51 and the "change one input" fork was refused (the input lives at step **50**) | storyboard stopped after 4 frames | **fixed** |
| A7 | PTY harness raced: resizing the emulator *after* the PTY made frames for the new size parse at the old one (phantom "renderer corruption") | `tests/pty.rs` | **fixed**: emulator first, then PTY. (Palimpsest met the same trap class, #48 E-02.) |
| A8 | After the terminal vanished the app **spun at ~70–90% CPU forever** (22+ minutes observed on three leaked test children) | `ps` after a failed PTY test | **fixed with a watchdog thread**; root cause is not mine (P3 below) |
| A9 | `StoryDirector` checkpoints grew without bound | code review during the sustained run | **fixed**: capped (`MAX_STORY_CKPTS` = 600) with least-recently-used eviction (the first cap dropped the *oldest-branch* checkpoints, which are exactly the ones a rewound user revisits) |
| A10 | **RSS grows ≈ linearly with branch count** (≈ 11 KB per retained branch skeleton — an estimate from two RSS readings; +27 MB from frame 2k to 60k as branches went ≈ 90 → 2,606) and p90 frame time rose 4.9 → 17.4 ms at 60k (rehydration churn + O(branches) scans). Fossilization bounds resident *records*, not branches | `docs/evidence/sustained-60000.md` | **open**: only `max_branches = 4096` bounds it; skeleton eviction and a parent index are the obvious fixes, not done |
| A11 | If the external player exited at once (a box with `pw-play` installed and no sound server) the app **rewrote a ~20 MB WAV and spawned a process every frame (≈30×/s)** | red-team review of the player path; reproduced with a fake player that exits immediately | **fixed**: a player that dies within 1.5 s disables audio with a toast (`m` re-arms); `pty_audio::an_instantly_exiting_player_…` |
| A12 | **With audio, RSS far exceeds the PCM budget**: `--sustained --audio-every=40` peaked at **488 MB** (HWM 495 MB, 278 MB at the end) against a 96 MiB resident-PCM budget; 162 performances were built for 510 branches because a child's past is its ancestors' PCM. The earlier run of the same command reported 12 performances / 192 MB; code changed in between and the cause of the difference was **not isolated** | `docs/evidence/sustained-12000-audio.md` | **open**: the budget bounds resident PCM, not the transient `StereoBlock` of `OfflineRenderer::render` (≈ 39 MB per 100 s, LibGibson #74) nor allocator retention |
| A13 | The machine did not mark the step cap terminal itself: the cap was *discovered by the next call*, so replaying exactly `end` steps (and checkpoint clones at position `MAX_STEPS`) was non-terminal | debug-build assertion in `core`/`vm` tests; review | **fixed** in `Machine::step`; verified with `cargo test --test core --test vm` in **debug** and release |
| A14 | Input/decision **landmarks sat one step too late**: `g` then `f` stood *after* the step that applies the input, so the fork modal could not drop/replace/insert that very input (the same off-by-one class as A6) | review of A6 | **fixed**: landmarks are at `pos = s`; `core` landmark tests |
| A15 | A decision override *at the fork step itself* was dropped by an unrelated edit (the filter used `<` where inputs apply before scheduling) | review | **fixed** (`i.at <= at`); `core` test |
| A16 | The 1× playback rate was hard-wired to 88 BPM while the world (and thus the audio clock) is selectable: **Vapor95/SwissSignal drifted from their own audio** | review | **fixed**: tempo-locked (`ui::playback_rate_is_locked_to_the_selected_worlds_tempo`) |
| A17 | The camera spring was semi-implicit Euler with ω = 9: **unstable above ≈ 92 ms per frame**, so a slow terminal/SSH link could fling the camera out of the recorded range | review of the integrator | **fixed**: closed-form critically damped step, `dt` clamped, camera clamped to the recorded range; `ui::the_camera_spring_is_stable_for_any_frame_time` |
| A18 | A hang-up `_exit` (the watchdog) skipped `Drop`: the **player and its WAV directory outlived the app** (also on SIGKILL); a library `eprintln!` or a panic message from the background composer **painted over the TUI** | review | **fixed**: `PR_SET_PDEATHSIG`, WAV directory removed by the watchdog, fd 2 redirected to a file and reported on exit; `pty_audio` ×3 |
| A19 | The timeline strip could draw a fossilized branch's rows **without making it resident**; a refused fork from the modal was **silent**; branch ids could overflow `u16` at the budget cap | review | **fixed** (`ensure_chain`, toast, `min(BranchId::MAX)`); `ui` tests |
| A20 | The **COMPARE label blinked out** on any step that carries an input: it shared the banner entity, and the input flash's reveal starts at 0 opacity | `tests/pty.rs` timed out waiting for it after the landmark fix (A14) moved the test's cursor onto an input step; deterministic 6/6 | **fixed**: its own scene entity; `ui::the_compare_label_cannot_be_blinked_out_by_an_input_flash` (not run against the pre-fix code: the PTY test is the discriminator that failed before and passes after) |
| A21 | Audio budget enforcement could evict an *ancestor* of a branch being played, although a child's past is its parent's PCM (a child is unplayable without it) | review | **fixed earlier in the session**: `request()` recurses over parents, `enforce_budget` protects the lineage; `audio::byte_budget_evicts_lru_but_not_protected`, `audio::a_child_is_unplayable_without_its_parent_…` |

## ERGONOMIC INCONVENIENCES (cost paid by a consumer; nothing is *wrong*)

* **E1 — two different `Tone`/`Emphasis`/`Density`/`Elevation` families.** `gibson::ui` and
  `gibson::audio::human_music::semantic` both export them. Importing both preludes is `E0659: ambiguous`;
  the project aliases everywhere. (Proven: `tests/capability.rs` compile error history.)
* **E2 — `OfflineRenderer::render(&mut synth, synth.total_samples())` does not compile** (two borrows);
  every consumer binds the length first. (Proven; one line.)
* **E3 — Space, Enter and the arrows belong to whatever keyed control has focus.** The UI layer is clear
  about this (docs: Space activates; arrow keys traverse "as a fallback"), but a time scrubber needs arrows
  *globally*. The working pattern — a keyed `presented/raw` viewport with `.on_event(...)` — is only
  documented for scroll viewports. Proven: `tests/ui.rs::space_and_arrows_follow_focus_*`.
* **E4 — `Effect::reveal(0.0 → 1.0)` is invisible at t = 0** (visibility threshold `> 0.0`), so a banner
  triggered *at* a history position is not visible *at* that position, only a frame later. Reasonable, but it
  made a rewound frame and a played frame differ by one step (found as a PTY timeout). Observed.
* **E5 — `Camera::project` rejects any point outside the frustum**, so it cannot project the endpoints of
  a line that crosses the screen edge; wireframe consumers re-derive the view math from the public `Camera`
  fields (~40 lines). Proven (`src/view3d.rs::Proj`).
* **E6 — `Scene::evaluate` clones every entity's `Node` every frame.** Fine for `Node::surface(Arc<Surface>)`
  (cheap) and a trap for `Node::raster(Surface)` (deep copy). Observed from the source.
* **E7 — `StoryDirector::update` follows at most one transition per call** ("one-arrow law"). Correct and
  well documented; it means an epoch change *and* a fork at the same step need two updates or a reaction.
  The project uses reactions. Observed.

## GENERIC ERGONOMIC GAPS

* **G-focus — focus memory is dropped when a keyed control disappears and not restored when it returns.**
  Responsive layouts remove the history list below 100 columns; after widening again focus is **not** restored to
  the control the user had (it stays on another control). UI_LAYER.md says "an absent key is not retained forever" — a
  choice, but there is no opt-in "remember last focus for this scope". Proven (`tests/ui.rs::focus_is_lost_when_its_keyed_control_vanishes_and_can_be_restored_by_set_focus`); the app can
  restore it with `UiRuntime::set_focus`. Not filed (documented design).
* **G-inspect — the composed frame's *styles* are not inspectable.** `last_frame_lines()` (text),
  `last_frame_report()`, `stats()` and `last_dirty_cells()` are good; foreground/background/attributes are not
  readable, so colour-depth assertions parse the wire with a VT emulator. Proven (`tests/capability.rs`).
  (#47/#48 asked for text and exact deltas; both landed. This is the next rung.)
* **G-restore — no restore/seek on `StoryDirector`; `facts_mut`/`jump_to` are unrecorded and not replayable.**
  Documented by LibGibson itself; demonstrated in `tests/story.rs::jump_to_and_facts_mut_*`. The checkpoint +
  replay workaround is cheap (clone ≈ 11 µs fresh / 18 µs after 640 updates) and exact
  (`director_at(p) == replay-from-scratch` for every sampled position on root and fork).
* **G-world-state — replacing a *whole* semantic world state** (the brief's question): the application owns it
  entirely, so there is nothing to ask of LibGibson *except* that `UiRuntime` presentation state (focus, active
  motions) has no `reset`/`snapshot` and that a new `UiRuntime` loses focus. For this app it did not matter:
  keyed identity carried focus across every rebuild. Observed.

## GENERIC PRIMITIVE GAPS

* **P-scene — `Scene` cannot remove an entity.** Hidden entities are still `evaluate`d every frame. Measured:
  10 / 400 / 2,000 / 5,000 entities → 2 µs / 148 µs / 3.3 ms / 14.8 ms per `to_node`. A time-travel UI that adds an
  entity per branch grows without bound. Proven. **Filed** (see table).
* **P-music-continue — no incremental or continuation composition.** `compose(trace, world, seed)` plans the
  *whole* trace; two traces identical for 48 beats differ in audio from beat **0** (probe: first 8 beats
  differ; and the raw whole-trace performances of a parent and its fork, fork at 8.5 s, first differ at **frame 0** —
  `docs/evidence/audio.md`). There is no "continue this score", no stable prefix, no checkpointable plan. Proven
  (`tests/audio.rs::probe_compose_is_not_prefix_causal`). Not a defect (the docs describe a global planning
  pipeline); it is what forced the architecture.
* **P-music-seek — `HumanMusicSynth` has no seek or checkpoint.** Reaching 20 s costs ≈ 0.34–0.38 s of
  render-and-discard (two runs); the end of the 63 s root performance costs 1.1–1.3 s (`docs/evidence/audio.md`). Proven.
* **P-cancel — no cooperative cancellation for `OfflineRenderer::render`.** A 100 s performance is one
  uninterruptible ~4 s call. Quitting had to detach the worker (A4). Related to, but distinct from, #74 (streaming).
  Commented on #74.
* **P-braille — no depth-tested Braille line rasterizer.** `Rasterizer::line` writes half-block RGB (1×2 per
  cell); wireframe at 2×4 needs a consumer-written subpixel raster (~120 lines, `SubRaster`). `to_mono_surface`
  exists for fills, not lines. Observed.

## PROBABLE / PROVEN LIBGIBSON DEFECTS

* **P1 — `Effect::Shake` and `Effect::Jitter` ignore their `duration`: a finite effect never settles.**
  The variant docs say Shake overrides position "for `duration`"; `eval` (src/scene.rs:816-830) never reads
  it. At t = 60 s a 500 ms shake still displaces the entity. Every other finite effect clamps progress and
  settles. Inside `Sequence` the shake's "final contribution" is therefore a non-zero offset forever.
  The *behaviour* is **PROVEN** (`tests/capability.rs::finite_shake_and_jitter_never_settle`,
  `docs/upstream/repro_shake_duration.rs`; source scene.rs:816-830 for `Shake`, ~709-725 for `Jitter`). Whether it is a
  *defect* is **open**: the variant doc says "for `duration`", but DESIGN.md §40 calls `Shake` "legacy" with "absolute
  placement semantics" and discusses "persistent effects" — so the filed issue asks, rather than asserts. **Filed.**
* **P2 — `HumanMusicSynth::render` with a non-contiguous `RenderCtx::start` silently plays every skipped
  event in one sample and returns audio that is not the audio at that position** (21–22 simultaneous voices
  right after the jump vs a sequential peak of 13–16, depending on the trace; block hashes differ). `AudioSource::render` is documented as "fill `out` with audio for
  `[ctx.start, ctx.start + out.frames())`"; the synth is a forward-only state machine whose cursors only
  advance. `rewind()` is honestly documented as scheduling-only. **PROVEN** (`tests/audio.rs::probe_synth_cannot_seek…`,
  `docs/upstream/repro_synth_jumped_start.rs`). Classified *probable defect / contract hole*: either honour
  random access or refuse it. **Filed.**
* **P3 — `Context::run_once` / `poll_event` never returns (busy-spins ≈ 65–90% CPU) once the controlling
  terminal has hung up and the process survived SIGHUP.** Isolated to `crossterm::event::poll` (crossterm-only
  loop spins; `render_now` and plain stdout writes fail promptly with EIO and exit). Default SIGHUP kills the
  process first, so this bites `nohup`, any SIGHUP handler (including the one this app installs to restore the
  terminal on SIGTERM/SIGHUP), and PTY harnesses that abandon a child. **Observed, with
  recorded evidence** (one host, crossterm 0.29; deterministic here: `docs/upstream/hangup-observed.txt` — LibGibson loop
  `Rs 67 %`, crossterm-only loop `Rs 67 %`, both default-SIGHUP controls die of signal 1;
  `docs/upstream/hangup-probe-observed.txt` — render and plain-write loops end with EIO; sources
  `repro_hangup_spin.rs`, `repro_hangup_crossterm_only.rs`, `hangup_probe.rs`, drivers `*.py`;
  `tests/pty.rs::a_vanished_terminal_*`, `tests/pty_audio.rs::a_vanished_terminal_*`).
  Workaround: a watchdog thread polling `POLLHUP` on stdin. **Filed** (mechanism is crossterm 0.29; LibGibson's
  loop is what consumers copy).

## DELIBERATE SAFETY BOUNDARIES (recorded, not complained about)

* `UiRuntime` clamps time backwards ("create a fresh runtime for an independent replay") — `tests/capability.rs`.
* Modals capture all keyboard input including unbound keys; `Esc` is the contract (`tests/ui.rs`).
* LibGibson restores the terminal on explicit restore, `Drop` and panic; **not** on a signal's default action.
  The app installs SIGTERM/SIGHUP handlers that turn them into a normal exit (exit 143/129, restored —
  `tests/pty.rs`). **SIGKILL cannot be survived** (README says so; `tests/pty.rs::sigkill_*` records it).
* One terminal owner per process; headless contexts are the supported way to test.
* `gibson::temporal` (residual temporal dithering) requires an *externally measured* `PresentationProfile` and
  says plainly that cadence "cannot be observed through a PTY"; it was **not used**. The project's substitute for
  translucency is world-anchored stipple (see EXPRESSIVE WALLS), which needs no cadence assumption.
* Experimental API: `ui`, `scene`, `story`, `audio` carry no stability promise; this report pins v0.4.0.
* (App) The VM runs no external code, has no host I/O and is bounded by `MAX_STEPS` = 640; resource contention,
  deadlock and "catastrophe" are simulated state, not host behaviour.

## EXPRESSIVE WALLS (things the medium or the substrate will not say)

* **Musical continuity across a fork is statistical, not guaranteed.** Measured over 27 fork points
  (steps 40–300): a child composed from its *whole* trace lands on the parent's chord at the fork 17/27 times
  (mean pitch-class overlap 0.79); a *future-only* child 10/27 (0.59). Neither is "the same music". The default
  is therefore whole-trace composition with the **past taken from the parent's PCM bit-for-bit**; the seam is a
  0.35 s equal-power crossfade whose largest sample step is 4,621/32,767 (FullTrace, the default; 4,656 for
  FutureOnly) against 2,629 for a pure-parent window.
  (`docs/evidence/audio.md`.)
* **The past cannot be re-composed.** Whatever the user heard stays heard; the *new* future is new music.
  Re-deriving history in place would change audio from beat 0. "Audiovisual continuity" therefore means: same
  past, a crossfaded present, a different future — and the ribbon's glow is the *actual rendered energy* of each
  step, so what you see and what you heard cannot drift.
* **No translucency.** Ghosts are dotted (every third step), foretold futures dashed and dimmer, lived history
  solid; in Mono the same three states survive as stipple density. Colour carries epoch only where there is colour.
* **Braille cells carry one colour.** Dots in a cell share the nearest dot's colour; crossing wires bleed colour.
* **Glow is background colour**, available at TrueColor/ANSI256 only; ANSI16 and Mono rely on shape.
* **Real audio-device playback is out of reach in CI.** The player spawns `pw-play`/`paplay`/`aplay` on a WAV;
  `tests/pty_audio.rs` asserts the lifecycle (argv, RIFF/WAVE header and size, stop, replace, dead player, killed app,
  vanished terminal) against a *fake* `pw-play`. No sound reached a device and nobody listened.

---

## Upstream actions

Searched first: 30 issues (`gh issue list -R femboy2112/libgibson --state all`), plus keyword searches
(seek, Shake, remove entity, snapshot, rewind, duration, UiRuntime time, backwards). Nothing matched; closest are
#74 (offline render materializes the whole PCM buffer — Cathedral, today), #73 (`Story::start` silent finish) and
#15 (queued input after resize).

| action | what | link |
|---|---|---|
| issue | `Effect::Shake`/`Effect::Jitter` ignore `duration` (P1) | see `docs/upstream/ISSUES.md` for numbers once filed |
| issue | `Scene` has no entity removal (P-scene) | " |
| issue | `HumanMusicSynth::render` with a non-contiguous `RenderCtx::start` (P2) | " |
| issue | `Context::run_once` never returns after the terminal hangs up (P3) | " |
| comment | #74: seek + cancellation + measured numbers (P-music-seek, P-cancel) | " |
| comment | #15: negative result — 48 resize→key trials, 0 held back; one multi-resize session flaked before a settle delay was added | " |
