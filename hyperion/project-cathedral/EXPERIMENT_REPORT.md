# Project Cathedral — Experiment Report

External, non-binding pressure test of the released **LibGibson `v0.4.0`** public Rust API.
This report records what was built, what was measured, and — with equal care — what is **not**
being claimed.

Standalone package: `hyperion/project-cathedral/`.
Dependency: `libgibson = { git = "https://github.com/femboy2112/libgibson", tag = "v0.4.0" }`.
Resolved commit: `c2f6483d92fe2b351e6cd50936a97d8cdf73cb79` (also hard-coded as
`LIBSIBSON_COMMIT` so a binary can report it without reading `Cargo.lock` at runtime).
`Cargo.lock` is committed. No path override, no `[patch]`, no LibGibson source change, no new
main dependency beyond `serde`/`serde_json` for the record file.

Host: Ubuntu (Linux), Rust 1.98.1 pinned by the repository's `rust-toolchain.toml`. Timings are
host-specific and the headless runs are unpaced.

---

## 1. The simulation model (fictional, deterministic)

`src/sim/fixture.rs` deterministically generates the named `cathedral` fixture from the fixed
seed `0xCA7ED_BA5E_2026_04`:

- **9 districts**: NAVE, TRANSEPT, APSE, CRYPT, BELLTOWER, CLOISTER, CHAPTER, ROSE, VAULT.
- **269 services**, each with an id, district, dependency tier `0..=5`, replicas, per-replica
  capacity, base latency, queue capacity, retry factor and entry rate.
- **482 dependency edges**, acyclic by construction (every dependency has a strictly deeper
  tier). Each service calls 1–3 deeper services, 72% of the time within its own district.
- **Provisioning**: entry demand is propagated through the DAG in topological order and each
  service's capacity is raised so the **expected resting load is ≈60% utilization**. Queue
  capacity is a few ticks of total capacity. This makes the resting system stable, so the
  scripted fault — not a permanent baseline overload — is what produces the incident.

This is a **fictional stress domain**. The graph shape, capacities and failure couplings were
chosen so a small perturbation can propagate; they are not calibrated against, and make no
claim about, any production distributed system.

`src/sim/engine.rs` advances one logical tick:

1. **Pass 1 (callers before callees)**: demand = entry rate + share of each caller's admitted
   load + carried retry debt, times a small deterministic per-`(frame, service)` jitter.
   Admission is capped by effective capacity; a queue accumulates the shortfall. Queue overflow
   times out, becomes error rate and re-enters as retry debt at the service's retry factor.
2. **Pass 2 (callees before callers)**: dependency stress is inherited from the worst dependency
   state, so failure flows *up* the call graph.
3. **Health & latency**: integrity is a *recoverable relaxation* — stress below a knee (0.7)
   leaves a service fully sound; above it integrity eases toward a stress-implied target, and
   a live fault/bad deployment caps it. Removing the fault lets the same geometry heal.
4. **Circuit breakers** with states `Closed → Open (24-tick cooldown) → HalfOpen (12-tick trial)
   → Closed/Open`. A breaker that is *open rejects immediately*: no queue forms and a rejected
   request is not retried, so an open breaker cannot feed a retry runaway. This is what lets the
   system actually recover.
5. **Aggregate metrics** and realized `LinkFlow` per edge (flow and inherited error) for the
   renderer.

`Engine::semantic_digest()` quantizes every service's health, queue, latency, error rate, shed,
breaker state, version, bad-deploy, isolation and fault to `1e-3` and folds them into one
`u64`. There is no wall clock, no entropy and no OS state in the model; jitter comes from a
hand-rolled SplitMix64/hash (`src/rng.rs`, `src/hash.rs`).

---

## 2. Incident semantics

`src/incident.rs` classifies raw tick metrics into nine phases:
`NORMAL`, `RISING UNCERTAINTY`, `OVERLOAD`, `LOCAL FAULT`, `CASCADE`, `DIAGNOSIS`,
`INTERVENTION`, `PARTIAL RECOVERY`, `RESTORED`.

Classification uses the critical fraction, overloaded fraction, open-breaker count and active
faults/isolation, plus recency of operator actions. A candidate phase must persist for a
**6-tick dwell** before it commits, and `CASCADE` only releases below a lower critical threshold
(Schmitt trigger), so a one-tick breaker flicker cannot thrash the score.

### Scripted "WTF" incident (`src/scenario.rs`)

The scenario is driven by LibGibson's public `story` director with a fixed 50 ms tick. Each
beat sets a one-shot fact; the application maps it to an `ActionKind` and chooses the live
target, then performs it through the real simulation. The directives are appended to the *same*
ordered journal a human operator would write, which is why replay can run with the scenario off.

The chosen target is `#138`, the single most-depended-upon service in the fixture (the load-bearing
keystone). Observed phase timeline for `--wtf` (deterministic):

| frame | phase | trigger |
|---:|---|---|
| 65 | LOCAL FAULT | minor deployment fault applied to #138 |
| 73 | RISING UNCERTAINTY | queue/latency pressure spreads |
| 82 | CASCADE | critical fraction passes 0.10, first breaker trips |
| 107 | LOCAL FAULT | breaker rejects stop the retry run-away |
| 121 | CASCADE | second wave |
| 306 | DIAGNOSIS | operator acknowledge (frame 300) |
| 346 | INTERVENTION | operator rollback (frame 340) |
| 358 | RISING UNCERTAINTY | reroute (frame 360) rebalances |
| 365 | PARTIAL RECOVERY | failure breaks |
| 405 | RESTORED | critical/overloaded/breakers all zero; compact incident report committed |
| 411 | NORMAL | system settles |

At frame 405 the engine is back to `mean_health > 0.999`, zero critical services and zero open
breakers. The scenario continues to a terminal "resolution" beat at frame 601 (a second
acknowledge), but the system has already recovered.

### Native scrollback vs the live region

Operator actions and milestone transitions are inserted **above** the live region with
`Context::insert_text_before_live` while the live cathedral keeps animating. A *finalized*
block — the compact incident report — is committed the same way at resolution:

```
╔══ INCIDENT REPORT ─────────────────────────────────────────────╗
  resolved at t=405 after 10 phase transitions
  peak: mean integrity 0.997  critical 0.0%  breakers 0
  hottest districts at resolution: ...
  operator actions: 4   journal digest: b91ed62f9fe1e028
╚════════════════════════════════════════════════════════════════╝
```

History is bounded: the application keeps no unbounded event vector, and the renderer reported
11 insertions with a maximum of 2 in a single frame across all runs.

---

## 3. Visual grammar and the four scales

`src/visual.rs` renders one custom `Surface` per frame, composed with public `Node` text/raster
nodes. The mapping is total and hand-written:

| semantic | spatial |
|---|---|
| service | tower/chamber; height = stress, color = integrity |
| dependency edge | bridge between tower crowns |
| request flow | moving energy glyph on the bridge |
| queue pressure | compressed amber band climbing from the tower base |
| latency | vertical stretch of the tower |
| retry storm | recursive echo copies of the travelling glyph |
| cascading failure | bridges fade, crowns collapse to rubble |
| availability | structural integrity (color + solidity) |
| isolated service | severed wing displaced across a gap |
| recovery | the same geometry easing back into coherent form |

The four scale transitions (`Whole → Cluster → Chain → Service`) are one continuous camera over
**one** fixed scene (`src/visual.rs` `Layout`/`Camera`), so a transition reads as travelling
through the same structure rather than swapping dashboards.

**The most absurd visual moment.** The fault target is not a random leaf but the single most
depended-upon service — a "minor deployment" lands on the keystone of the entire cathedral. At
the `LOCAL FAULT → CASCADE` transition, the deepest CRYPT ledger tower's crown collapses, and
because 482 bridges are anchored to tower *crowns*, every bridge that depends on it visibly
drags downward while the retry-echo glyphs recursively multiply along those same spans. For
roughly twenty frames the whole nine-district skyline sags in unison under a fault the operator
injected deliberately as "minor". It is the intended visual joke: one bad deploy, an entire city
genuflects.

---

## 4. HumanMusic mapping and checked BAND performances

The score is a **musical response to the incident's semantic trajectory**. It does **not**
understand distributed systems. The complete public semantic input surface used is nine phase
symbols plus four scalar axes and eight morphism kinds; nothing else about the simulation
reaches the composer.

| incident phase | Tone | Emphasis | Density | Elevation | EventKind |
|---|---|---|---|---|---|
| Normal | Neutral | Muted | Spacious | Flat | ActChanged |
| Rising | Info | Normal | Normal | Raised | FocusAcquired |
| Overload | Warning | Strong | Compact | Raised | ModalEntered |
| LocalFault | Warning | Strong | Compact | Raised | Impact |
| Cascade | Danger | Strong | Compact | Overlay | Impact |
| Diagnosis | Info | Normal | Normal | Overlay | FocusAcquired |
| Intervention | Accent | Strong | Compact | Raised | ActChanged |
| PartialRecovery | Success | Normal | Normal | Raised | Confirmation |
| Restored | Success | Muted | Spacious | Flat | SectionResolved |

Implementation (`src/music.rs`):

- The incident record becomes a `SemanticTrace`; `at_beat = frame · 0.5 + 0.5`, so a 20 Hz
  simulation advances the score at 10 beats/second.
- Each phase change (or a bounded interval) rebuilds the score through
  `SongMap::build(&trace, seed, None)` and the **checked** route
  `perform_checked(&song, &world, opts, PerformanceProfile::BAND)`. If the checked route
  rejects, the take is still audible via `perform` and the rejection is recorded — the
  experiment reported **zero rejections** in every run.
- `PerformanceReceipt::measure_under(&c, &world, BAND)` records pass/fail.
- The live horizon is bounded (`MUSIC_HORIZON_FRAMES = 1400`): past that the score stops
  lengthening, so a 10,000-frame run does not grow the live-score cost without limit. The
  scripted incident lives well inside this window.
- Offline export uses `HumanMusicSynth` + `OfflineRenderer` + `write_wav_i16`. The reported hash
  is the SHA-256 of the **interleaved i16 PCM**, not of the WAV file; the two necessarily differ
  because the latter includes the 44-byte RIFF header and any padding. Both are reported.
- Audibility is **best-effort and outside LibGibson**: on `W` or `--play` the host spawns the
  first player it finds (`ffplay`/`paplay`/`aplay`/`mpv`/`afplay`). The verified artifact remains
  the WAV + PCM hash; playback is a convenience and its success depends on the host sound
  device, so no claim is made that sound reached a speaker.

This is a hand-written response function, not comprehension. No claim is made that the music
"knows" an incident is happening.

---

## 5. Determinism and replay

`App::step` is a pure function of `(state, journal, scenario beats)`; the record file stores the
ordered journal, frame count, final semantic digest, periodic engine checkpoints, the music
setting, and the WAV PCM hash. Replay rebuilds an `App` with the scripted scenario **off**
(its directives were journaled like any other action), restores the journal, steps the recorded
number of frames and compares every checkpoint and the final digest.

Verified:

| run | frames | music | result |
|---|---:|---|---|
| `--no-music` | 300 | off | REPLAY OK |
| `--no-music` | 650 | off | REPLAY OK |
| `--music` | 500 | on | REPLAY OK |
| `--music` | 240 | on | REPLAY OK (integration test) |
| `--music` | 10,000 | on | **REPLAY OK: 10,000 frames, 5 actions, 50 checkpoints** |

10,000-frame replay output:

```
REPLAY OK: 10000 frames, 5 actions, 50 checkpoints, final sim b22330570bb7875f
  digest sim:b22330570bb7875f|phase:0|journal:5|recs:11|music:f7a59c0861db2314651cc11c5f75aa2a972cffef9438645e0d457a0847b399b8
```

The semantic digest `b22330570bb7875f` and the recorded `music` form digest are reproduced
exactly. Record-file SHA-256: `6f5e89cb…b1b4b34` (`sustain10k.json`).

A negative control is included in the integration suite: the same fixture and seed with the
scripted incident **withheld** reaches a different digest at the same frame, so the journal —
not wall time — is what moves the system.

---

## 6. Sustained run: 10,000 deterministic frames

Command (final binary, live music, WAV export, record):

```sh
./target/release/cathedral --wtf --frames=10000 --music \
  --music-out=/tmp/opencode/out/audio10k --record=/tmp/opencode/out/sustain10k.json \
  --seed=0xCA7EDBA5202604
```

| metric | value |
|---|---|
| frames | 10,000 |
| wall time (frame loop + WAV export) | 73.0 s |
| mean frame time | 5.30 ms |
| p95 frame time | 1.30 ms |
| max frame time | 1.14 s (a music rebuild; profiler overhead included) |
| mean LibGibson render | 0.60 ms/frame |
| mean emitted bytes | 8,368.7 |
| total emitted bytes | 83,687,096 (~79.8 MiB) |
| mean affected / exact changed cells | 418.86 / 418.79 per frame |
| history insertions | 11 (max 2 in one frame) |
| RSS (frame loop) start → end → peak | 7,088 → 23,188 KiB |
| music checked rebuilds | 51 |
| music total / last rebuild cost | 41.77 s / 1.12 s |
| checked-route rejections | 0 |
| incident events / final phase | 11 / NORMAL |
| export peak RSS (`/usr/bin/time -v`) | 612,604 KiB |

Frame times vary a little run to run; an earlier identical run reported mean 4.59 ms, p95 1.29 ms and
a 1.03 s worst frame while reproducing the same `b22330570bb7875f` semantic digest and the same PCM
hash. The digest and audio are deterministic; the wall-clock outliers are host noise.

Mean frame time is dominated by outlier music rebuilds (p95 is 1.30 ms). `--frames` is a
headless, unpaced loop; these numbers are not a steady-state frame rate. The profiler adds
`profile::now_us()` calls around each phase and `rss_kib()` per frame; it is included in the
figures.

The 10,000-frame run emitted 11 scrollback commits total, confirming the application never
grows its own event vector and the renderer's history is bounded.

---

## 7. Capability matrix: 5 sizes × 4 depths

`--capability` reaches a representative mid-incident state deterministically (frame 220, in
`CASCADE`), then renders every size × depth and inspects the readable surface for incident
state, the hot cluster, an action affordance and an exit path.

| size | depth | bytes | exact | state | cluster | afford | exit | color codes |
|---|---|---:|---:|---|---|---|---|---:|
| 42×15 | TrueColor | 4,329 | 628 | yes | yes | yes | yes | 192 |
| 42×15 | ANSI256 | 2,958 | 628 | yes | yes | yes | yes | 179 |
| 42×15 | ANSI16 | 2,029 | 628 | yes | yes | yes | yes | 169 |
| 42×15 | Mono | 1,245 | 628 | yes | yes | yes | yes | 36 |
| 60×20 | TrueColor | 8,581 | 1,198 | yes | yes | yes | yes | 372 |
| 60×20 | ANSI256 | 5,555 | 1,198 | yes | yes | yes | yes | 327 |
| 60×20 | ANSI16 | 3,735 | 1,198 | yes | yes | yes | yes | 299 |
| 60×20 | Mono | 2,281 | 1,198 | yes | yes | yes | yes | 44 |
| 80×24 | TrueColor | 7,058 | 1,897 | yes | yes | yes | yes | 261 |
| 80×24 | ANSI256 | 5,050 | 1,897 | yes | yes | yes | yes | 234 |
| 80×24 | ANSI16 | 3,751 | 1,897 | yes | yes | yes | yes | 215 |
| 80×24 | Mono | 2,707 | 1,897 | yes | yes | yes | yes | 38 |
| 120×40 | TrueColor | 20,895 | 4,770 | yes | yes | yes | yes | 755 |
| 120×40 | ANSI256 | 14,812 | 4,770 | yes | yes | yes | yes | 704 |
| 120×40 | ANSI16 | 10,506 | 4,770 | yes | yes | yes | yes | 660 |
| 120×40 | Mono | 6,863 | 4,770 | yes | yes | yes | yes | 52 |
| 160×50 | TrueColor | 33,604 | 7,881 | yes | yes | yes | yes | 1,221 |
| 160×50 | ANSI256 | 23,588 | 7,881 | yes | yes | yes | yes | 1,118 |
| 160×50 | ANSI16 | 16,966 | 7,881 | yes | yes | yes | yes | 1,063 |
| 160×50 | Mono | 11,002 | 7,881 | yes | yes | yes | yes | 70 |

All 20 cells preserve incident state + hot cluster + affordance + exit. At the smallest size the
view switches to a causal-chain close-up while keeping a compact header (`CATHEDRAL CASCADE …`)
and footer affordances. Color codes fall monotonically TrueColor → 256 → 16 → Mono; the Mono
cell still contains a small number of escapes because the shared header/footer glyphs remain
styled.

The `--text` readable-frame mode is exercised by an integration test as a byte-stable contract
(ANSI-free) at 100×30.

---

## 8. Real-PTY evidence

`scripts/pty_stress.py` drives the real interactive binary in a pseudo-terminal: an operator
command burst, resize during the cascade (120×40 → 56×24 → 200×50 → 120×40), a 120 ms output
backpressure pause while the renderer stays live, sustained interaction, and quit. It reports
observable action side-effects (the app emits no per-key receipt), liveness, resize survival
and terminal restoration.

| probe | argv | bytes captured | actions observed | alt-enter/leave | exit |
|---|---|---:|---:|---|---:|
| chaos, no music | `--wtf --no-music --fps=30` | 2,289,667 | 7/7 operator actions | yes/yes | 0 |
| chaos, music | `--wtf --fps=30` | 2,284,281 | 7/7 operator actions | yes/yes | 0 |
| observe-only | `--wtf --no-music --fps=300` | 9,277,195 | — | yes/yes | 0 |

The chaos probe's operator fault is visible in native scrollback with its own blast radius,
which is the fix for the "I set a fault and nothing happened" trap: the default focus is a leaf,
so the committed line reads `◇ INJECT-FAULT t=21 target=#0 operator inject · 0 downstream
dependents`, and the footer persistently shows `focus #0·0↓`. Pressing `D` (or the arrow keys)
moves focus to the keystone `#138` (62 downstream dependents), where the same `F` cascades.

The observe-only probe sends only non-mutating view keys, so the scripted incident resolves,
and it confirms the **compact incident report committed to native scrollback while the live
region animated** (`compact_incident_report_committed: true`). Cursor hide/show and
alternate-screen enter/leave were observed in every probe.

**Issue #15 boundary.** The chaos probe deliberately reproduced the *shape* of the reported
stress (resize + graphical PTY backpressure + queued input) and all seven operator actions were
observably dispatched after the pause. That is negative evidence for this host and this 120 ms
window only. The application has no per-key input receipt, so one passing probe does **not**
show that the queued-input stall in
[libgibson#15](https://github.com/femboy2112/libgibson/issues/15) is fixed. It is reported as a
comment on the issue, not as a resolution.

---

## 9. Audio receipts and hashes

Checked BAND performances, exported to interleaved i16 PCM.

| run | rebuilds | checked | WAV size | PCM SHA-256 | file SHA-256 |
|---|---:|---|---:|---|---|
| 650-frame incident | 12 | true | 35,825,500 B | `3fd56c71368a7206c001f88b4cdae54f4d68cccec40b0ea453624b5ec8d343d4` | `83663a39f639324504b017d5ff67b21780f2c8e4147d2d56f08779a221b75a12` |
| 10,000-frame final take | 51 | true | 100,494,588 B | `5496b5d0825b63b55ae7727a46378cc402e65ee28870c3d810e552142044a5b5` | `86b2b04650f5048d8cdec8db7cf1089c2b5b86074bcb605e6965c84a303ec4e3` |

`checked=true` means `PerformanceReceipt::measure_under(..., PerformanceProfile::BAND)` passed
on the checked route; `music_rejections: 0` across every run. The 10,000-frame export holds the
whole PCM buffer in memory and peaked at ~614 MiB RSS — a real scaling observation recorded in
[FRICTION.md](FRICTION.md), not a defect claim.

WAVs are intentionally **not** committed (the package's `.gitignore` excludes `*.wav` and
`/out/`); the hashes above are the artifact.

---

## 10. What is supported, and what is not

**Supported.**

- A 10,000-frame deterministic run completes, and fixture + seed + journal replays to an
  equivalent semantic state with all 50 checkpoints matching.
- The simulation is stable at rest, cascades on a minor keystone fault, and recovers fully after
  rollback + reroute (unit tests + observed timeline).
- All 20 size × depth capability cells preserve incident state, hot cluster, action affordance
  and exit.
- The checked BAND route accepts every take (0 rejections) and exports reproducible WAVs with
  recorded PCM hashes.
- Native scrollback commits (actions, milestones, compact report) occur while the live region
  animates, observed in a real PTY.
- Bounded history (11 insertions, max 2/frame) over 10,000 frames.

**Unsupported / explicitly bounded.**

- No claim that this models any real distributed system; the fixture is fictional.
- No claim that the music understands distributed systems; it is a documented hand-written map
  from nine phases and four axes.
- No claim about steady-state FPS; `--frames` is an unpaced loop and outliers are music rebuilds.
- No claim that libgibson#15 is fixed, or that one PTY probe proves input correctness.
- No visual-quality claim; the capability check is semantic (labels present), not an aesthetic
  judgment.
- Cross-platform behavior (non-Linux terminals), actual speaker output (the WAV and PCM hash are
  verified; whether a host player emits sound is not), and production-scale topology were not
  certified.

---

## 11. Application defects found and corrected during the experiment

Recorded because they are consumer-side evidence, not upstream claims:

- A wrong `Story::new` start-beat id made the scenario director finish immediately and run zero
  beats. Corrected; a unit test now documents that `Story::start()` finishes silently on an
  unknown start (see [FRICTION.md](FRICTION.md)).
- A duplicate `inject-fault` was emitted because a `story` fact persists across beats; the
  one-shot fact is now cleared after dispatch.
- Replay sampled checkpoints before the step while keying the old frame, an off-by-one that made
  every checkpoint look divergent; sampling now mirrors the recorder.
- The resting fixture was originally unstable (baseline overload), making "a minor fault"
  indistinguishable from normal pressure; capacity is now provisioned to ~60% resting
  utilization.
- Open breakers originally fed the error-rate signal and retried rejected requests, preventing
  recovery; open breakers now reject without retry amplification.

These are all local to Project Cathedral. No LibGibson source was modified.

- The narrow-terminal footer welded the right-aligned music-axis string onto the left footer
  run, producing corrupt tokens such as `joTNeutral EMuted DSpacious LFlat` at every width up to
  ~95 (including the classic 80×24 default). `draw_footer` now reserves the right run's width
  before truncating the left run and backs the cut off to a separator. A black-box regression
  test renders 80×24 and asserts the two runs never touch.
- The operator fault affordance defaulted to the first service, a leaf with **zero** downstream
  dependents. A fault there degrades exactly one service and then sits at a fixed low state
  (measured: `crit=0.0037, degraded=1, mean_health≈0.9977` for hundreds of ticks), which reads as
  "the dynamic system is inert". The fault is still correct; the *affordance* was the problem.
  The footer now shows the focused service's blast radius (`focus #0·0↓`), the committed operator
  line names it (`0 downstream dependents`), and `D` tie-breaks toward the keystone so a fault can
  be placed where it propagates. The scripted incident is unchanged and still targets `#138`.
