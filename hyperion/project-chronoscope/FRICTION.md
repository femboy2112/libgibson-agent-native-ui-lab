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
| A1 | `Model` asked for HumanMusic performances on every fork/switch/compare **even when audio was off**, rendering ~4 s of DSP each, and never enforced the audio byte budget: first 1,500-frame trial = 72 performances, **744 MiB resident, RSS 890 MB** | first `--sustained` trial (RSS series 18 → 554 → 890 MB) | **fixed**: `AudioMode::{Off,Silent,Play}`, `want_audio()`, `enforce_budget` in `tick` (now 41.6 MB RSS after 12,000 frames) |
| A2 | The catastrophe beat used `Effect::Shake`; it **never stops** (→ P1 below). The meltdown banner would have shaken for the rest of the beat | `tests/story.rs` settle assertion failed | **fixed**: `settling_shake` composed from `Effect::translate` ending at (0,0) |
| A3 | Arrow keys did nothing in the real PTY (but worked headless): with any keyed control on screen the UI layer spends arrows on focus traversal | PTY session; headless rig had no side panel at 80×24 | **fixed**: the viewport is a keyed `on_event` sink (see E3) |
| A4 | Quitting while a HumanMusic performance was rendering **blocked until the render finished** (`Drop` joined the worker; `OfflineRenderer::render` has no cancel — G4) | `tests/pty.rs::quit_does_not_wait_for_an_in_flight_music_render` design | **fixed**: worker is detached on drop |
| A5 | Epoch classifier flickered **Contradiction** for 5 steps every ~40: CREDITS/LEDGER are legitimately two instructions apart | epoch spans dump of the first fixture | **fixed**: mismatch must persist 14 steps |
| A6 | The guided demo rewound to step 51 and the "change one input" fork was refused (the input lives at step **50**) | storyboard stopped after 4 frames | **fixed** |
| A7 | PTY harness raced: resizing the emulator *after* the PTY made frames for the new size parse at the old one (phantom "renderer corruption") | `tests/pty.rs` | **fixed**: emulator first, then PTY. (Palimpsest met the same trap class, #48 E-02.) |
| A8 | After the terminal vanished the app **spun at ~70–90% CPU forever** (22+ minutes observed on three leaked test children) | `ps` after a failed PTY test | **fixed with a watchdog thread**; root cause is not mine (P3 below) |
| A9 | `StoryDirector` checkpoints grew without bound | code review during the sustained run | **fixed**: capped (`MAX_STORY_CKPTS`), oldest branches dropped |
| A10 | **RSS grows ≈ linearly with branch count** (≈ 11 KB per retained branch skeleton; +28 MB from frame 2k to 60k as branches went 100 → 2,604) and p90 frame time rose 6.8 → 18.7 ms at 60k (rehydration churn + O(branches) scans). Fossilization bounds resident *records*, not branches | `docs/evidence/sustained-60000.md` | **open**: only `max_branches = 4096` bounds it; skeleton eviction and a parent index are the obvious fixes, not done |

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
  Responsive layouts remove the history list below 100 columns; after widening again focus returns to the
  *first* control, not the one the user had. UI_LAYER.md says "an absent key is not retained forever" — a
  choice, but there is no opt-in "remember last focus for this scope". Proven (`tests/ui.rs`, probe in the
  session log). Not filed (documented design).
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
  differ). There is no "continue this score", no stable prefix, no checkpointable plan. Proven
  (`tests/audio.rs::probe_compose_is_not_prefix_causal`). Not a defect (the docs describe a global planning
  pipeline); it is what forced the architecture.
* **P-music-seek — `HumanMusicSynth` has no seek or checkpoint.** Reaching 20 s costs 0.62 s of
  render-and-discard at ~30× realtime; 80 s costs 2.0 s (`docs/evidence/audio.md`). Proven.
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
  **PROVEN** (`tests/capability.rs::finite_shake_and_jitter_never_settle`, `docs/upstream/repro_shake_duration.rs`).
  **Filed.**
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
  terminal on SIGTERM/SIGHUP), and PTY harnesses that abandon a child. **PROVEN**
  (`docs/upstream/repro_hangup_spin.rs` + `hangup_driver.py`; `tests/pty.rs::a_vanished_terminal_*`).
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
  0.35 s equal-power crossfade whose largest sample step is 4,656/32,767 against 2,629 for a pure-parent window.
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
  this run asserted argv and file integrity, not sound.

---

## Upstream actions

Searched first: 29 issues (`gh issue list -R femboy2112/libgibson --state all`), plus keyword searches
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
