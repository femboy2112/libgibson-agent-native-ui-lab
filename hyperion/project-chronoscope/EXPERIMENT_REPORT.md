# EXPERIMENT REPORT — Project Chronoscope

**What was tested.** The *released* LibGibson **v0.4.0** (`tag v0.4.0`, commit
`c2f6483d92fe2b351e6cd50936a97d8cdf73cb79`), as an outside consumer: public API only, no path override, no
`[patch]`, no `main`, nothing from LibGibson modified. `Cargo.lock` committed. Host: Linux 7.0 x86_64, 8 cores,
Rust 1.98.1, `cargo test --release` (HumanMusic is ≈50× slower in debug).

**The question.** Time travel is hostile to APIs designed for forward progress. Build a debugger for alternate
histories — run, pause, step, scrub backward, jump to landmarks, fork at an earlier decision, compare futures,
hear them — and find out what is easy going forward and painful going backward.

Evidence is labelled **Proven** (deterministic reproduction in this repo), **Observed** (seen, not isolated) or
**Boundary** (documented limit). Raw measurements live in `docs/evidence/`; friction in `FRICTION.md`.

---

## 1. The system

### 1.1 The VM (`src/vm.rs`, `src/fixture.rs`)

A bounded, cooperative, fully deterministic machine — built for this experiment, executing nothing from outside:

* six tasks (REACTOR, COOLER, SENSOR, OPERATOR, SUPERVISOR, AUDITOR), a 36-op ISA, round-robin scheduler
  with per-task quanta, **two mutexes taken in opposite orders** (a real deadlock, found by wait-for-cycle
  detection), **bounded channels with back-pressure**, a *racy* credit path that sometimes violates the
  `CREDITS == LEDGER` invariant, **supervised recovery** (nudge → restart → steal lock → SCRAM), crashes that keep
  their locks until restarted, and a thermal invariant whose violation is the terminal **meltdown**.
* **Decisions.** `Fate`/`Rnd` instructions draw from a seeded xorshift stream. A *fate point* is a forkable
  decision; overriding one changes its outcome **but not the random stream** (verified: the next decision is
  unaffected), so a fork is one controlled intervention, not a re-roll of the universe.
* **Causality.** Every step records four parents (program order; data/lock/message/RNG source; wake cause;
  auxiliary). External inputs are causal roots named by the step that injected them.
* **State identity, three levels.** `digest_computational` (everything that determines future behaviour),
  `digest_provenance` (why the state holds its values), `digest_full` (both). Plus a *declared* semantic
  projection (`src/sem.rs`) that ignores bookkeeping (TICKS, OUT, LEDGER's raw value). The UI says `≡ byte-identical`
  only when `digest_full` matches, `= same computation` when only provenance differs, `≈ equivalent under the
  projection` when classes match, `≠` otherwise.
* **Fixture.** seed 16, three rapid BOOST commands at steps 50/62/74 → stable → uncertainty → **deadlock at
  214** (supervisor nudge 225, restart 258) → escalation (surge rolled 272, lock stolen 303) → **contradiction**
  (audit failed 332) → **meltdown at step 344**. Each of the ten single-change forks in `docs/evidence/causal.md`
  (replace/drop the command at 50, drop at 62 or 74, four inserts, two decision overrides) avoids the meltdown and
  runs to the 640-step cap. *How the fixture was found:* a grid search over VM physics × operator scripts × seeds
  located the neighbourhood (`examples/tune.rs`, restored verbatim from the session that ran it; it varies five of
  the nine `Params` and does **not** by itself reproduce the shipped fixture — `grace`, `surge_heat`, `pump_odds`
  and `operator_rest` were then adjusted by hand-probing that was not kept as code).

### 1.2 Replay model (`src/history.rs`)

> A branch is fully determined by `(program, seed, script)`. Everything else is a cache.

A *position* `p` means "`p` steps have executed". Per-step `Rec`s (event, state vars, epoch, three digests,
task/lock/queue codes), checkpoints every 32 steps, and epoch spans are all derivable. Consequences, all tested:

| property | evidence |
|---|---|
| two fresh runs produce identical events and digests | `core::replay_equality_two_fresh_runs` |
| replay from scratch equals the cached records | `core::replay_from_scratch_matches_cached_records` |
| `machine_at(b, p)` (checkpoint + replay) equals the recorded digest at **every** position | `core::checkpoint_reconstruction_is_exact_at_every_position` |
| 400 random-order jumps/rewinds all land on the recorded state *and* epoch tracker | `core::jump_rewind_correctness_in_arbitrary_order` |
| a fossilized branch (≈1 KiB) rebuilds bit-for-bit | `core::fossilize_and_rehydrate_reproduces_everything` |

### 1.3 Branch model

A fork at position `p` shares positions `0..=p` with its parent and owns only what follows. The old future stays
(`fork_shares_prefix_and_leaves_old_future_intact`). The edit is exactly one of: override a decision, insert,
drop or replace an operator command. Forking with the decision's *natural* outcome yields a byte-identical
future for every step (`forking_with_the_natural_decision_is_byte_identical_forever`) — the determinism
witness; flipping a fate point produces **one** intervention root and zero divergence before it. Retention is
bounded by LRU fossilization (§5).

**Comparison** aligns branches by step, classifies every step `≡ / = / ≈ / ≠` and every dimension
`Same / Equiv / Apart`, and attributes divergence causally. A step is *affected* if it diverged, was injected by a
command the two scripts disagree about, or has an affected parent (even if its own event record looks identical —
e.g. a register that now holds another value). A **root** is the **Intervention** (the step where the scripts
differ: a changed/inserted/dropped command, an overridden decision — which can be ten steps before any event looks
different) or a **Reorder**: a diverged event with *no* affected ancestor — a scheduler-order consequence the
data-causality graph cannot explain. The distinction is stated in the UI and here because it is a *limit*:
scheduler order is not a traced cause.

Across ten single-change forks (`docs/evidence/causal.md`): **every fork has exactly one intervention root**;
**reorder roots: 0 in seven forks, 1 in two, 2 in one**; all ten forks avoid the meltdown. E.g. replacing the first
BOOST with THROTTLE (step 50) diverges 267 of 344 aligned events, the first one at step 60, **all** inside the
causal cone of the one changed input (0 reorder roots).

### 1.4 Audio strategy — an honest architecture (`src/audio.rs`)

What the public API allows, **measured** (`docs/evidence/audio.md`, `tests/audio.rs`):

| fact | result |
|---|---|
| same `(world, seed, trace)` → PCM | **bit-identical** (hash equal across fresh stores, threaded vs inline, rebuild after eviction) |
| `compose` is prefix-causal? | **No.** Two traces identical for 48 beats differ from beat 0 (first 8 beats compared); the raw whole-trace performances of a parent and its fork first differ at **frame 0** (`audio.md`, fork at 8.5 s) |
| jump `RenderCtx::start` on a fresh synth = seek? | **No.** Different audio; 21–22 voices fire at once |
| `rewind()` = reset? | **No.** Scheduling only; DSP tails survive |
| exact reconstruction | fresh synth + render-and-discard prefix: **exact**; 153 ms to 10 s, 344 ms to 20 s, 715 ms to 40 s, 1.14 s to the end of the 63 s root performance (one run; a previous run of the same generator gave 171 / 381 / 791 / 1,255 ms — expect ±15 %) |
| rendering speed | 35–55× realtime in the latest run (BlackIce 63 s of audio in 1.2 s, 103 s in 2.0–2.1 s; Vapor95 is the slowest at 35×); run-to-run variation ≈ ±15 % |
| robustness | 600 random semantic traces composed over the three worlds (committed test): 0 panics, 0 `validate()` failures. Non-finite output is checked only on the fixture traces (`audio::every_world_composes_every_epoch…`); an earlier, uncommitted 3,000-trace probe is not claimed |

So nothing is re-composed in place and nothing fakes a seek:

* **The past is immutable scrollback.** A child's PCM before the fork sample *is the parent's*: 409,091 samples,
  `parent == child`, bit for bit.
* **The future is a new whole performance.** Branch audio = one HumanMusic performance per branch future,
  rendered on a worker thread (inline in deterministic mode), seeded by branch lineage (fork position + the
  ordered inputs), so the same history always sounds the same and different histories sound different.
* **Seek is an offset into rendered PCM** (≈ 9–10 µs for 4,096 frames); an evicted performance is rebuilt by
  deterministic re-render and *checked against its previous hash* (`rebuild_hashes_checked`,
  `hash_mismatch_on_rebuild = 0`).
* **The seam** is an equal-power crossfade over 0.35 s *after* the fork sample (so the past stays untouched).
  Measured, per strategy (`audio.md`): **FutureOnly** max sample step across the seam window 4,656/32,767, RMS
  0.023 → 0.063; **FullTrace (the default)** 4,621/32,767, RMS 0.023 → 0.023; both against 2,629 for a pure-parent
  window of equal length. (The RMS window is coarse and the FullTrace equality is a measurement, not a finding.)
  Audible? Unlistened: see §8.
* **Harmony across the fork** is the real expressive wall. Whole-trace composition of the child lands on the
  parent's chord at the fork in **17/27** fork points (mean pitch-class overlap 0.79); future-only composition
  in 10/27 (0.59); prefix harmony beat-by-beat is *not* preserved (0/12 beats equal in the example). The default
  is therefore whole-trace (the shared prefix audio is still taken from the parent, so nothing audible changes
  before the fork).
* **A/B** switches which branch's PCM is played from the same sample; before the fork the two are identical.
* **Playback rate is tempo-locked.** 1× advances the cursor at the *selected world's* step rate (`BPM/60 × 4`: BlackIce
  88 BPM → 5.87 steps/s ≈ the 5.9 shown in the UI; Vapor95 5.6; SwissSignal 7.9), so the cursor and the external
  player stay in step (`ui::playback_rate_is_locked_to_the_selected_worlds_tempo`). Faster speeds are multiples; the
  player is only used at 1×.
* **Honest limits of the player path.** If the player exits within 1.5 s of starting (a box with `pw-play` and no
  sound server) the app says so and stops retrying (`m` re-arms); the player is killed with the app even on SIGKILL
  (`PR_SET_PDEATHSIG`) and its WAV directory is removed when the terminal vanishes. All asserted with a fake
  `pw-play` on `PATH` (`tests/pty_audio.rs`); none of it touches a sound device.
* **The ribbon is made of the audio.** Each step's RMS (from the rendered PCM) modulates ring/floor brightness,
  so the visual energy of a branch is the rendered energy — and works with no sound device at all.

Playback is an external player process (`pw-play`/`paplay`/`aplay`) on a WAV written through the public
`gibson::audio::wav` writer; there is no device path in CI (§8). The default strategy is `FullTrace`; the
determinism, immutable-past and seam tests are pinned for both strategies (`tests/audio.rs::full_trace_*`).

### 1.5 Atmosphere: Story + Scene on *history* time (`src/director.rs`)

A beat per epoch, an arrow between every pair, reactions for input/override/fork, bundles for flashes. The story
clock is the history clock (one step = 100 ms). The COMPARE label has its own scene entity (nothing in the story
targets it): while it shared the banner entity, stepping onto a step that carries an input mounted a banner reveal
that started at zero opacity and blinked the label out for that step — found by the PTY session, fixed, and
pinned by `ui::the_compare_label_cannot_be_blinked_out_by_an_input_flash`. `StoryDirector` has no seek, snapshot or inverse, so the director at
`(branch, position)` is **a cloned checkpoint (every 16 updates) + replay of recorded updates**:

* equals replay-from-scratch at every sampled position on root and fork (`story::director_at_every_position_*`);
* equals forward-only when positions are visited in a shuffled order (`rewind_then_forward_*`);
* once the nearest checkpoint exists a jump replays ≤ 16 updates (`replay_cost_is_bounded_*`); a cold or evicted position replays from the fork (mean 23 updates per rebuild in the 12k run: 109,348 updates / 4,754 rebuilds); checkpoints are LRU-bounded at 600; a snapshot costs ≈ 11 µs fresh, 18 µs after 640 updates;
* the fork event lives on the *child's* timeline only (`a_forked_branch_replays_its_own_birth`).

`Effect::eval` is a pure function of time, so animations *are* repositionable (`Effect::Reverse` plays one
backwards exactly: `capability::effects_reposition_and_reverse_exactly`). The exception is the one that hurt:
`Shake`/`Jitter` never settle (FRICTION P1); the app composes its own settling jolt.

### 1.6 The view (`src/view3d.rs`) — what it looks like

* **Time is z.** One step = one world unit. The camera rides the active branch and **faces where time is going**:
  into the foretold future while playing, **turning around** (a sprung yaw) to look back along the lived past after
  a rewind. `o` swaps the outside chase camera for an inside-the-tunnel camera.
* **Each branch is a hexagonal tunnel; its six rails are the six tasks.** Rail colour = task state (healthy rails
  stay dim so anomalies pop: blocked = dashed amber/violet, crashed = a broken rail with a red spark). Rings every
  3 steps near the camera, sparser far away. Checkpoints are structural plates (white double rings with spokes) every
  32 steps.
* **The floor is the epoch ribbon** — a filled half-block channel (public `Rasterizer`, fog, lighting) glowing with
  the **rendered audio energy** of each step — under a **Braille wireframe** (own 2×4 depth-tested raster built from
  the public `Camera`). Variable threads (HEAT, LOAD, COOL, CREDITS, LEDGER, ESC) spiral inside, bright only near
  the cursor; CREDITS and LEDGER are drawn as two threads that visibly part when the invariant breaks.
* **Causality is geometry.** Message, lock-hand-off, restart and steal edges are 3-D chords between rails; the
  causal ancestry (≤ 3 deep) of the inspected event is drawn in white; external inputs are needles into the
  OPERATOR rail; fate points are yellow ticks; an overridden decision is a magenta diamond; the deadlock is a
  violet braid across the cross-section.
* **Forks are real splits.** The child tunnel peels from its parent over 14 steps into its own lane — a different
  position *and roll* — so branches occupy different planes; a bloom of rings marks the birth.
* **Dead futures persist as ghost geometry**: dotted (every third step), dim, desaturated; the end of a branch that
  melted down is a burnt red star that stays visible.
* **Foretold ≠ lived ≠ ghost** are three drawing styles (dim-dashed / solid / dotted), which survive in Mono.
* **Compare = a braid of dimensions.** Identical dimensions *fuse* into one thread; equivalent ones run side by side
  with weld ticks; divergent ones **pull apart** (separation ∝ divergence) over red floor lanes; the cause plane of
  an intervention is a glowing cross-section. `x` collapses the lanes back into the trunk and unfolds them again
  (animated, monotone: `ui::compare_requires_two_histories_and_collapse_rejoin_animates`).
* Dust streams past in world space, so motion through time reads as motion through space.

---

## 2. Capability results (the brief's questions)

| question | answer | evidence |
|---|---|---|
| Can scene state rewind? | **Not natively.** `StoryDirector` has no seek/restore; `Scene` keeps no history of its own (its `Presentation` is a pure function of the director state and time, so there is nothing in it to rewind — but it also cannot drop an entity, P-scene). Rewind = clone a checkpoint + replay, **exact and cheap**. | `tests/story.rs`, `capability::story_director_snapshots…` |
| Can focus survive rebuilding historical trees? | **Yes.** Keyed focus survived 9 time-travel/branch/compare/collapse operations; modal capture/restore returned focus to the interrupted control. **No** when the keyed control itself disappears and returns (responsive layout: focus does not come back by itself; `UiRuntime::set_focus` restores it). | `tests/ui.rs::focus_survives_time_travel…`, `…::focus_is_lost_when_its_keyed_control_vanishes…` |
| Can animations be deterministically repositioned? | **Yes** for `Effect::eval` (pure in `t`; `Reverse` exact). **No** for `UiRuntime` motion (monotone time). | `tests/capability.rs`, `tests/story.rs` |
| Can external consumers inspect enough render state? | **Text, counters and deltas: yes.** `last_frame_lines`, `FrameReport`, `RenderStats`, `last_dirty_cells`; agreed with an independent VT emulator at all five sizes. **Styles: no** (VT emulator needed). | `render::libgibsons_visible_text_agrees…`, `capability::context_exposes_text…` |
| Is there an ergonomic gap for replacing a full semantic world state? | The app owns it, so little to ask — except `UiRuntime` has no reset/snapshot, and focus is not restored after removal. | FRICTION G-world-state, G-focus |
| HumanMusic lifecycle / audio replacement | Deterministic, fast enough to rebuild on demand; **no seek, no prefix stability, no cancel**. Replacement = per-branch performance + immutable past. | §1.4, `tests/audio.rs` |
| Resize during scrub/fork | Headless: 15 interleaved resizes/steps/forks never produce an over-wide or over-tall frame. PTY: four resizes mid-compare repaint coherently (emulator-first ordering). | `render::resize_during_scrub_and_fork…`, `pty::full_session…` |

---

## 3. Tests

`cargo test --release -- --test-threads=1` — **104 tests + 1 opt-in measurement** (the exact last run is at the end of this section).

| suite | tests | covers |
|---|---:|---|
| `core` | 23 | replay equality, reconstruction at every position, random-order jump/rewind, fork stability, natural-decision twin, single-intervention root, fossilize/rehydrate, LRU budget, comparison ladder, **a replaced/inserted command is the one intervention root even when its effect shows 10 steps later**, landmarks (input/fate landmarks sit *before* the step they name), step cap (`StepCap` is terminal and replayable), causal ancestry |
| `vm` | 10 | deadlock detection, lock hand-off + wake cause, back-pressure + FIFO, crash keeps locks until restart, override keeps the RNG stream, input as causal root, step cap, thermal runaway, digest ladder, semantic projection |
| `audio` | 15 | pure-function determinism, distinct futures ≠, **immutable past (bit-exact)**, seam metrics, rebuild-after-evict bit-identical, byte-budget LRU (lineage protected), threaded == inline, strategies, **FullTrace (the default) determinism / immutable past / seam**, every world, short futures, **synth cannot seek**, **compose not prefix-causal**, unplayable-without-parent rebuilds the lineage |
| `story` | 7 | director = replay at every position, rewind-then-forward = forward-only, bounded replay cost, semantics-not-wall-clock, fork birth on the child only, effects pure in `t`, `jump_to` not replayable |
| `render` | 9 | **5 sizes × 4 depths × 8 scenes** (160 combinations), colour depth honoured **on the wire** (SGR assertions), Mono keeps distinctions, **glyph fallback ladder** (Braille→half-block→block→ASCII), byte-identical re-runs, settled frame = 0 changed cells, text = VT emulator, scrub-and-return idempotence, resize storms |
| `ui` | 15 | focus walk + Enter pick, focus survives time travel, **focus lost when its control vanishes / `set_focus` restores it**, modal capture/restore, keyboard-only fork, a refused fork says why, Space/arrows follow focus, 600 rapid keys exact, shifted/page keys, landmark jumps, compare + collapse/rejoin, **tempo-locked 1× rate for every world**, **camera spring stable for any frame time**, **timeline strip rows are resident**, **COMPARE label cannot be blinked out** |
| `capability` | 8 | the probes behind FRICTION (Shake/Jitter, Scene growth, monotone UI time, director cost, inspectability, headless resize, random-trace composition) |
| `pty` | 8 | real binary in a real PTY (§4) |
| `pty_audio` | 5 | the audio lifecycle in the real binary with a **fake** `pw-play` on `PATH`: argv + RIFF/WAVE + size, pause stops it, a fork replaces the audio; an instantly-exiting player is not restarted every frame; the player dies with a SIGKILLed app; a vanished terminal removes the WAV directory and the player; stderr never paints over the screen |
| `sustained` | 2 | determinism + bounded retention, seed sensitivity |
| `demo` | 2 | the guided sequence is real commands; any key takes over |
| `pty_resize_collision` | (1, ignored) | opt-in measurement for LibGibson #15 |

`cargo fmt --check` and `cargo clippy --release --all-targets -- -D warnings` are clean (see the last run below).

**Last run** (2026-10-02, Linux 7.0 x86_64, Rust 1.98.1, this tree, before the commit that carries this file):

```
$ cargo fmt --check                                                   → clean
$ cargo clippy --release --all-targets -- -D warnings                 → clean
$ cargo test --test core --test vm        (debug)                     → 23 + 10 passed
$ cargo test --release -- --test-threads=1
  audio 15 · capability 8 · core 23 · demo 2 · pty 8 · pty_audio 5 · render 9 · story 7 · sustained 2 · ui 15 · vm 10
  = 104 passed, 0 failed; pty_resize_collision: 1 ignored (opt-in)
$ 20× cargo test --release --test pty -- --test-threads=1 full_session → 20 passed, 0 failed; no leaked child process afterwards
```

`--test-threads=1` because the PTY and audio suites are timing-sensitive and share the machine. The default parallel
`cargo test` run was **not** measured.

### Deterministic test mode

`Rig::headless(w, h, depth, glyphs, options)` shares `Model`, `build_screen`, `UiRuntime::frame` and `route_event`
with the interactive loop and drives a headless `Context` with a virtual terminal, a fixed 33 ms presentation clock,
no wall time and synchronous audio. It does **not** share terminal I/O, signal handling, the resize path or the audio
player — those are what the PTY suites test. Capture at any step/branch: `--capture --script="goto 214; inspect" --ansi=…`.
Fixed program + seed + action sequence + virtual time ⇒ identical bytes (proved at 60×20 and 120×40, TrueColor and
Mono). The one *intended* path-dependence is the camera's **facing** (it follows the last movement direction); the
idempotence test normalizes it and says so.

## 4. PTY

`portable-pty` + `vt100`, the release binary, `TERM=xterm-256color`, 100×32.

One session drives: `...` `,` `→×5` `PgDn` `n` `c` `0` `g` **`f` + Tab + Enter (a fork)** `b` `v` **three asserted
resizes** (60×20, 160×50, 42×15; each must repaint a coherent frame within bounds) then 100×32 again `v` `.`
**`i` (modal)** `q` (swallowed by the modal) `Esc` **440 keystrokes in one write (`.,`×200 then `.`×40: net +40 steps,
none dropped or reordered)** `x` `q`. Then verified: exit 0; **alternate screen left; cursor shown; both
`\x1b[?1049l` and `\x1b[?25h` on the wire; the session journal printed to the normal screen**.

Also: Ctrl-C restores; SIGTERM/SIGHUP restore through the app's handler (exit 143/129); **SIGKILL leaves the alternate
screen up (recorded as a boundary, not a failure)**; non-TTY stdout exits 2 with an actionable message and no escape
bytes; quitting does **not** wait for an in-flight music render; **a vanished terminal ends the process within 4 s**
(watchdog; FRICTION P3). Eight tests in `pty.rs` plus five in `pty_audio.rs`. The long session passed **20/20** consecutive runs in the last run (earlier: 15/15 before the review fixes). Settle delays after resizes are
documented in FRICTION A7 / `pty_resize_collision.rs` (before them the session flaked ~5/8; not isolated — see §8).

## 5. Sustained run

The autopilot (`src/bench.rs`) is a restless user driven by a seeded xorshift stream: step, goto, landmark jumps
(8 kinds), play/speed, **fork at the cursor with a random available edit**, switch branch, compare/next target,
collapse/rejoin, turn, inspect, open fork modal, close, jump to end, **resize across the matrix**. Every frame goes
through the full model → `build_screen` → `UiRuntime` → `Context` → differential renderer → ANSI path on a headless
120×40-class context.

**12,000 frames** (`docs/evidence/sustained-12000.md`; TrueColor; audio off; one host; one run per row — wall time
for this same run was 41.7 s, 31.9 s and 29.9 s in three successive regenerations, so treat timings as ±20 %):

| metric | value |
|---|---|
| wall time | 29.9 s (401 frames/s unthrottled) |
| frame total | p50 2.0 ms · p90 4.9 · p99 7.6 · max 15.5 |
| view build / generation / write (µs) | p50 1,238 / 447 / 1 |
| bytes per frame | p50 9,259 · p99 46,915 · max 74,893 · **mean 12,449** (149 MB total) |
| exact changed cells per frame | p50 573 · p99 4,746 · max 8,000 |
| full repaints | 101 (133 resizes) |
| wire segments per frame | p50 1,148 · max 13,876 |
| branches | **510** (509 forks, 0 refused) |
| retained history | peak **29,998** resident records (≈5.2 MiB at the end), 1,651 fossilizations, 1,221 deterministic rehydrations |
| VM steps | 165,268 executed; 417,105 replay steps |
| story | 4,754 director rebuilds (109,348 updates, mean 23); 600 checkpoints retained (LRU cap) |
| RSS | 4.6 → 16.9 (1k) → 31.6 (2k) → 40.6 MB (12k); HWM 41.8 MB |
| determinism | digest `a27b3774d0745af2` (the `sustained` test re-runs a 1,500-frame version twice and compares digests, bytes, branch counts) |

**60,000 frames** (`docs/evidence/sustained-60000.md`; same autopilot, one run):

| metric | value |
|---|---|
| wall time | 334 s (180 frames/s unthrottled) |
| frame total | p50 2.3 ms · p90 17.4 · p99 25.8 · max 39.3 (p90 is 3.5× the 12k run: see below) |
| bytes per frame | p50 9,217 · p99 49,904 · **mean 12,838** (770 MB total) — within 3 % of the 12k run |
| branches | **2,606** (2,605 forks, 0 refused), 651 resizes, 516 full repaints |
| retained history | peak **35,678** resident records (the 30,000 budget + the protected lineage/compare/ghost sets), end 29,784; **18,072 fossilizations, 15,567 deterministic rehydrations** |
| VM / replay | 814,345 steps executed; **5,113,189 replay steps** |
| story | 25,913 director rebuilds (735,268 updates) |
| RSS | 4.3 → 32.0 MB (2k) → 41.2 (12k) → 47.9 (30k) → 59.1 MB (60k); HWM 60.8 MB |
| digest | `9459e8012cbd9601` |

**What the 60k run says that the 12k run could not.** Resident *records* are bounded (the LRU budget holds), frame
cost per byte emitted is flat, and the byte stream per frame did not drift. Two things are **not** flat:
(1) **RSS grows roughly linearly with branch count** — ≈ +27 MB from frame 2,000 to 60,000 while branches went
≈ 90 → 2,606, i.e. ≈ 11 KB per retained branch skeleton (the fossil + frontier machine + script + spans + allocator
overhead; an estimate from two RSS readings, not a heap profile). Retention fossilizes but never *deletes* a branch;
the only hard bound is `max_branches = 4096` (≈ +17 MB more, extrapolated, not measured). (2) **p90 frame time grew
from 4.9 to 17.4 ms** (view build p90 15.4 ms): rehydration churn (15.6k replays) plus several O(branches) scans per
frame in `Model::visible_branches`. Both are application-level and fixable (evict skeletons; index branches by
parent); neither was fixed here, because the point of the run is to report what a bounded-but-not-finished design does.

**With real audio** (`--audio-every=40`, `docs/evidence/sustained-12000-audio.md`): the autopilot asked for the audio
of the current branch every 40 frames, and because a child's past *is* its ancestors' PCM the whole lineage has to be
resident to play it. Result: **162 performances composed and rendered** (10,300 s of music, 330 s of render, 31×
realtime) — an 8-performance working set (≈ 90 MiB, under the 96 MiB budget) is far smaller than the set of branches
visited, so most requests rebuilt something. Because the headless rig renders synchronously the run took **374 s
instead of 30 s (32 frames/s)**; frame time p50/p99 stayed 2.0 / 7.7 ms, max 85 ms (a frame that waited on a
rebuild). **RSS peaked at 488 MB (HWM 495), 278 MB at the end** — the byte budget bounds resident *performance PCM*,
not the transient planar-`f32` `StereoBlock` that `OfflineRenderer::render` materializes for the whole performance
(≈ 39 MB for 100 s; LibGibson #74's observation, from the other side) nor allocator behaviour. An earlier run of the
same command (before this session's review fixes) reported 12 performances and 192 MB RSS. The code changed in
between (lineage-protecting budget, LRU `touch` of the current branches, player handling), so the two runs are not
comparable and I did **not** isolate which change moved the number. Memory under audio is therefore **bounded in
PCM, not in RSS**; I did not chase the remaining 400 MB.

**Rendering stats available through the public API** and used above: `Context::last_frame_report()`
(`bytes_emitted`, `exact_changed_cells`, `affected_cells`, `total_cells`, `full_repaint`, `generation_duration`,
`write_duration`), `Context::stats()` (`RenderStats`), `last_dirty_cells()`, `last_frame_lines()`,
`missed_periods_*`.

## 6. Render matrix (`docs/evidence/matrix.md`)

5 sizes (42×15, 60×20, 80×24, 120×40, 160×50) × 4 colour depths × 6 scenes (+ the two modal scenes in tests).
Mean full-paint wire size by depth over all 30 size×scene combinations: **TrueColor 16.3 KB, ANSI256 11.4 KB,
ANSI16 6.6 KB, Mono 4.5 KB**. Layout responds: below 100 columns the side panel drops; the timeline strip is 1–5
rows by height; level of detail (thread count, ring spacing, ghost count, labels) follows the viewport area.

Graphics realization fallback: braille → half-block → block → ASCII, all four verified to keep the viewport
legible in Mono with no stray braille left (`render::glyph_realization_ladder_*`); colour fallback verified on the
wire for all four depths.

## 7. Upstream

Searched all 30 issues (`gh issue list --state all`) plus keyword searches first (Shake, Jitter duration, Scene remove
entity, RenderCtx start, seek HumanMusicSynth, SIGHUP, run_once poll spin, crossterm poll): none of the four was
already filed. Filed, each with a minimal public-API repro and recorded output under `docs/upstream/`, titled
`[v0.4.0 consumer: Project Chronoscope] <mechanism>`:

| # | mechanism | status of the claim |
|---|---|---|
| [#76](https://github.com/femboy2112/libgibson/issues/76) | `Effect::Shake`/`Jitter` never settle (`eval` ignores `duration`) | behaviour proven; defect-ness asked, not asserted |
| [#77](https://github.com/femboy2112/libgibson/issues/77) | `Scene` has no entity removal; hidden entities are still cloned/evaluated | proven (measured growth) |
| [#78](https://github.com/femboy2112/libgibson/issues/78) | non-contiguous `RenderCtx::start` on `HumanMusicSynth` fires every skipped event at once | proven |
| [#79](https://github.com/femboy2112/libgibson/issues/79) | `run_once` never returns after the terminal hangs up if SIGHUP was survived | observed with recorded evidence; mechanism is crossterm 0.29 |

Comments: [#74](https://github.com/femboy2112/libgibson/issues/74#issuecomment-5948489460) (cancellation, memory shape, seek cost, from the interactive
side) and [#15](https://github.com/femboy2112/libgibson/issues/15#issuecomment-5948489722) (a positive data point: see §8). Full table in
`FRICTION.md` and `docs/upstream/ISSUES.md`.

## 8. Claims

### Supported (by a test or measurement that exists in this repo)

1. A branch is a pure function of `(program, seed, script)`; reconstruction, replay, fossilize/rehydrate and
   fork-twin determinism are **exact** (bit-for-bit digests at every position).
2. Forking leaves the old future intact, shares the prefix bit-for-bit, and a single changed input gives **one**
   causal intervention root and zero divergence before it.
3. Rendering is a pure function of `(program, seed, actions, virtual time)` (identical bytes), and a settled frame
   costs 0 changed cells.
4. 160 size×depth×scene combinations render without overflow; colour depth and glyph family are honoured on the
   wire; `Context::last_frame_lines` equals an independent VT emulator at all five sizes.
5. The real binary survives step/scrub/fork/switch/resize/modal/rapid-input/quit in a PTY and restores the terminal
   (alternate screen, cursor); SIGTERM/SIGHUP/Ctrl-C restore; a vanished terminal ends the process.
6. 12,000 and 60,000 frames of mixed navigation over 510 and 2,606 branches keep *resident history records* bounded
   (30k budget; protected sets lift the peak to 35.7k) by fossilization, deterministically (identical digests across runs of the same length); **total RSS is
   not bounded** — it grows with branch count (A10).
7. HumanMusic output is bit-reproducible per `(world, seed, trace)`; the **past of a fork is bit-identical to its
   parent's**; evicted performances rebuild bit-identically; the synth cannot seek and `compose` is not
   prefix-causal (both demonstrated).
8. `StoryDirector` rewinds exactly by checkpoint + replay at ≤ 16 updates per jump; effects are pure in time.
9. Keyed focus survives time travel and modal capture/restore; it is lost when its keyed control vanishes and can be
   restored through `UiRuntime::set_focus`.
10. The audio *lifecycle* of the real binary (start, stop, replace, dead player, killed app, vanished terminal) behaves
    as documented against a **fake** `pw-play`.

### Not proven / not claimed

* **Anyone has listened.** Every audio claim is about bytes, hashes, levels, RMS and chords. Whether the seam or
  the harmonic jump is *musically acceptable* is untested. Real device playback is untested (no device assertion).
* **Terminals.** Only a PTY + `vt100` + `pyte` (and font-rendered previews). Not tmux/screen/SSH/xterm/kitty/
  WezTerm/Windows/macOS. Glyph shapes in the PNGs are DejaVu's, not any terminal's.
* **Legibility to humans.** The visual design was iterated by looking at rendered frames; no user study. The
  "unreasonable for a TUI" judgement is the author's.
* **A memory plateau.** There isn't one: RSS grows ≈ linearly with branch count (≈ 11 KB per retained branch,
  A10); only `max_branches` bounds it. With audio, RSS reached 488 MB although the PCM budget is 96 MiB (A12). One
  run per length, one machine.
* **That the fixture is "the best" or reproducible from a search.** `examples/tune.rs` is the final-stage search; the
  last four parameters were hand-adjusted afterwards.
* **Perceptual claims about seams, harmony or the tempo lock.** The 1× rate is locked to the world's tempo
  arithmetically and tested; whether it *sounds* in step was not listened to.
* **The cause of #15 (queued input after resize).** One multi-resize PTY session flaked ~5/8 *before* settle delays
  were added. The opt-in measurement (`tests/pty_resize_collision.rs`, three runs × 12 trials per gap) shows keys
  written 0 ms after a resize applied within 1.5 s in only 5/12, 4/12 and 3/12 trials, and 12/12 at gaps ≥ 5 ms.
  That reproduces the *symptom* in a second consumer; whether the layer is this app's loop, crossterm or the
  harness's reader is **not attributed**, and I did not check that a held-back key is applied after a further key.
  (An earlier claim in this project that 48 isolated trials never reproduced it is **withdrawn**: the committed
  measurement contradicts it.)
* **That `Shake`/`Jitter` not settling is a defect** rather than an intended "until the beat ends". The docs say
  "for `duration`"; the source ignores it. Filed as a question with a repro.
* **That `≈` (semantic equivalence) predicts equal futures.** It does not; it is a stated abstraction.
* **Causal completeness.** Data/lock/message/RNG/wake/program-order causality is traced; **scheduler-order
  causality is not**, so some diverged events are labelled "Reorder" rather than explained.
* **Anything beyond v0.4.0**, the experimental `ui`/`scene`/`story`/`audio` surfaces having no stability promise.
