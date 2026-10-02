# Experiment Report — Project Theseus (LibGibson v0.4.0)

| Field | Value |
|-------|-------|
| Repository | `femboy2112/libgibson-agent-native-ui-lab` |
| Branch | `hyperion/project-theseus-v0.4.0` |
| Package | `hyperion/project-theseus/` |
| Pull request | [#5](https://github.com/femboy2112/libgibson-agent-native-ui-lab/pull/5) — `experiment(v0.4.0): Project Theseus — the Ship of Theseus machine for music` (open, **not merged**) |
| LibGibson resolved revision | `v0.4.0` = `c2f6483d92fe2b351e6cd50936a97d8cdf73cb79` (pinned by `tag`, recorded in `Cargo.lock`) |
| Dependency form | `libgibson = { git = "https://github.com/femboy2112/libgibson", tag = "v0.4.0", features = ["audio-cpal"] }` |

---

## 1. Architecture

```
src/lib.rs      exposes `model` + `visual` so integration tests drive the real
                pipeline and renderer in-process (no subprocess for structural tests)
src/model.rs    Model: reference + quotient extraction, cover generation, typed
                refusal capture, ceiling knowledge, evidence log, audio, particles
src/visual.rs   RGB software raster engine: identity reactor, piano rolls, spectrum,
                axis console, build_frame, capability/glyph realization
src/main.rs     clap CLI, semantic header/footer shell, headless capture
                (text/ansi/json/ppm/lines), ordered key replay/record, matrix runner,
                and the interactive App loop
tests/adversarial.rs   7 external structural properties
tests/pty_smoke.rs     2 real-PTY tests + 1 ignored upstream regression guard
fixtures/ode_to_joy.tsv  public-domain SATB reference (249 lines, 4 voices)
```

### The load-bearing boundary

`Model::regenerate` extracts a `CoverMap` from the `ReferenceSong`, then calls
`cover(&map, &target)`. The source `Composition` is **not** in the `cover` call and
is not retained in any field the generator reads. The visual reactor is fed only the
quotient state (axis on/off, knowledge, relations, refusal axes) plus the real cover
score; there is no hidden copy of the reference behind it. `adversarial_01` proves
freed material cannot leak: altering a duration that the quotient does not preserve
leaves the map fingerprint, the cover score hash, and the audio hash unchanged.

### Axis knowledge: active map vs. ceiling

Two extractions per regeneration:
* the **active** map — the fidelity profile with the user's axis toggles applied
  (this is what is handed to `cover`);
* the **ceiling** map — `CoverFidelityProfile::preset(preset)` with every axis pinned
  (this is what the regime *could* know).

The console uses the ceiling to distinguish `PURGED` (information was available and
the operator removed it) from `UNKNOWN` (the reference never established it), and
`THESEUS INDEX = preserved-reachable / reachable`. Without this split, a toggled-off
axis reads `UNKNOWN` and the "removed info visibly disappears" demonstration is lost.

---

## 2. Fixture

`fixtures/ode_to_joy.tsv` — the public-domain Ode to Joy theme (Beethoven, 1824),
authored as a four-voice SATB TSV (`sop`, `alto`, `tenor`, `bass`), 100 BPM, 64
beats. The app selects the `sop` line as the melody; the presence of a voice named
`bass` is what makes the `BassFigure` axis reachable. No copyrighted material is
used; no symbolic fixtures are downloaded.

---

## 3. The most impressive quotient visualization — the WTF sequence

`Faithful`, `BLACK_ICE`, seed 42, `--harmony`, geometry 120×40. Same target, same
seed; only the axis pins change:

| Axes kept | Theseus Index | Console | Notes | `covermap_fingerprint` | `score_hash` | `wav_hash` |
|-----------|--------------:|---------|------:|------------------------|--------------|------------|
| `11111111` full | **100 %** | 5 `PRESERVED`, 3 `UNKNOWN` | 330 | `28a3ee601cfc78dd` | `e8b39ac74a30e8f8` | `34e070aaf2b114a8` |
| `00000000` none | **0 %** | 0 `PRESERVED`, 5 `PURGED`, 3 `UNKNOWN` | 234 | `834c596c513f166c` | `01e6ff3845337da1` | `3e86f418f55869b8` |
| `10000000` motif | **20 %** | 1 `PRESERVED`, 4 `PURGED`, 3 `UNKNOWN` | 305 | `2b96ffe171dc95dc` | `8254330070a7fcaf` | `651f6b93d06e56db` |

Strip the identity and the cover becomes a different piece (234 notes, different
score and WAV hashes); restore one axis and recognizability snaps back (motif-only,
305 notes). The three `UNKNOWN` axes (Groove, Form, Orchestration) are never
available from a reference song — see finding §10.4.

Visual corroboration: `--headless text --geometry 120x40` prints the full eight-axis
console with `PRESERVED` / `PURGED` / `UNKNOWN` / `REFUSED` per axis, and
`--headless ppm` writes the raw RGB reactor (`theseus_identity_120x40.ppm`) for the
"most spectacular" full-colour view.

---

## 4. Cover evidence (machine conformance)

Default covered capture (`Interpretive`, `VAPOR95`, seed 42, axes `11111111`):

```json
{"world":"VAPOR95","seed":42,"fidelity":"interpretive","axes":"11111111",
 "covermap_fingerprint":"b86b4ba6a1ac3d75",
 "relations":"motif=Metric riff=Metric bass=Free harmony=Free groove=Free form=Free orch=Free",
 "outcome":"covered","notes":305,"perf_fingerprint":"73eb0fdeeefc96ed",
 "score_hash":"21a33989dd57b12d","conformance":true,"receipt":true,
 "wav_hash":"3f7234610b8a2418"}
```

`CoverConformance::check` and `PerformanceReceipt::measure_under(BAND)` both pass for
every covered cell. Determinism: two independent `--headless json` runs are
**byte-identical** (`diff -q` clean). Machine conformance is not claimed to establish
human "same song" recognition.

---

## 5. Lawful-refusal evidence

Refusals are `CoverError::Invalid`, displayed as a first-class event with the
involved axes marked `REFUSED`:

| Reference | Regime / world | Typed reason | Axis marks |
|-----------|----------------|--------------|------------|
| Ode (no harmony) | `Strict`, `SWISS_SIGNAL` | `no lawful harmony contains the pinned simultaneous attacks` | motif, riff |
| Ode + `--harmony` | `Strict`, `BLACK_ICE` | `a pinned bass event has no lawful pitch function in the target harmony` | harmony, bass |
| Ode + `--harmony` | `Strict`, `VAPOR95` | same as above | harmony, bass |
| Ode + `--harmony` | `Strict`, `SWISS_SIGNAL` | `pinned harmony outside target vocabulary` | harmony |
| generated lane map | `CoverMap::extract_lane` | `pinned harmony outside target vocabulary` | harmony |

Refusal is stable across runs and world-specific (`adversarial_04`), and the visual
asserts `LAWFUL REFUSAL` plus a `REFUSED` axis mark (`adversarial_07`).

---

## 6. Real-PTY harness results

`tests/pty_smoke.rs` spawns the real `--no-audio` binary over a `portable-pty`
pseudo-terminal with the package dir as CWD and a draining reader thread.

| Test | Covers | Result |
|------|--------|--------|
| `pty_navigation_resize_help_and_clean_quit` | navigation, all 8 axes, fidelity/world/seed edits, resize 80×24→140×44, help open/close, clean quit, alt-screen restore | **ok** |
| `pty_rapid_interaction_stress` | ~90-key backlog of mixed edits fired faster than the frame rate, then clean quit | **ok** |
| `pty_resize_under_input_backpressure_regression` | resize **with** a queued backlog — the upstream defect | **ignored** (reproduces bug; see §10.5) |

Restoration proxy: the run asserts the alternate-screen enter (`ESC[?1049h`) and
leave (`ESC[?1049l`) sequences plus application chrome are present.

**Sustained-run observations.**
* The interactive loop applies **one queued key per rendered frame**; a 90-key burst
  takes ≈7–8 s to drain at `opt-level = 1` (debug), with continuous frame output
  (~1.3 MB over 45 s). This is a real responsiveness ceiling for scripted bursts, not
  a crash.
* Per-frame cost is dominated by per-pixel transcendental math, so the package sets
  `[profile.dev] opt-level = 1`; this cut the adversarial suite from 1.8 s to 0.33 s
  and makes the PTY harness usable while keeping incremental compiles fast.
* The process stayed live across resize storms and restored termios, cursor, and the
  primary screen on exit.

---

## 7. Geometry × capability × glyph matrix

Five geometries (42×15, 60×20, 80×24, 120×40, 160×50) × four colour capabilities
(TrueColor, Ansi256, Ansi16, Mono) × four glyph families (halfblock, block, braille,
ascii) = **80 cells**.

* `project-theseus --matrix` (covered): **80/80 OK, 0 failures** (each cell renders
  `height-2`+ lines, non-blank, quotient marker present).
* `project-theseus --matrix --fidelity Strict --world SWISS_SIGNAL`: **80/80 OK,
  0 failures** (same gate under the refusal path; the `refusal`/`axes` columns are
  frame-text visibility probes, not semantic assertions).
* `adversarial_07` independently asserts exact width/height, visible line count,
  non-blank output, the quotient marker, a realized reactor surface, and that a
  removed axis shows `PURGED` while a refusal shows `LAWFUL REFUSAL` + `REFUSED`.

---

## 8. Adversarial tests (all pass, 0.33 s)

| # | Property | Type |
|---|----------|------|
| 01 | freed material does not leak into the quotient or cover | falsification |
| 02 | pinned material *does* change the quotient (positive control) | positive control |
| 03 | same map + target + seed ⇒ identical plan and audio hash | determinism |
| 04 | typed refusal is stable and world-specific | falsification |
| 05 | fidelity invariant count is monotone | invariant |
| 06 | identity (`extract`) vs lane (`extract_lane`) projections differ | separation |
| 07 | render matrix never panics; removed axes disappear; refusal is visible | capability |

---

## 9. Findings

### 10.1 Machine conformance
* `cover()` + `CoverConformance::check` + `PerformanceReceipt::measure_under(BAND)`
  pass for every covered cell tested; refusals are typed `CoverError::Invalid`.
* Fully deterministic: fixed fixture/seed/target/fidelity ⇒ byte-identical JSON and
  identical audio hash. Verified two runs byte-identical.
* No machine metric is claimed to determine "same song"; `conformance == true` means
  the invariants the world can check hold, nothing more.

### 10.2 Human recognition
* The WTF sequence is *plausible* evidence of recognizability snapping back
  (motif-only reads as the tune; zero-axis reads as generic new music), but this was
  judged by listening/reading the generated rolls, not by any human study. It is
  **not** proven that a listener identifies the tune, and no recognition metric is
  claimed. See §11.

### 10.3 App bugs found and fixed
* The help overlay was originally a library `modal`, which consumes *every* key while
  open — including `q` — so a lone `Esc` arriving fused to the next byte in a fast
  burst stranded the operator inside help. Replaced with a non-trapping `panel`;
  `Esc` now closes help when open and quits otherwise.
* The console showed `PRESERVED` for all eight axes even when the reference never
  established them, and never showed `PURGED` for a toggled axis. Fixed with the
  active/ceiling split (§1).
* The PTY drain thread originally died on `ErrorKind::Interrupted` (a `SIGWINCH`
  from `resize` interrupts a blocked read); the PTY buffer then filled and the app
  blocked on write. Fixed by retrying `Interrupted`.

### 10.4 API friction (see `FRICTION.md`)
* `ReferenceSong::extract_fidelity` hardcodes `form: None`, `groove: None`,
  `orchestration: None` (`reference_song.rs:232,242,244` and `:453,463,467`), so
  **three of the eight quotient axes are unreachable from any reference song**
  regardless of fidelity regime. The app surfaces this honestly as `UNKNOWN`.
* `CoverMap::extract` / `extract_lane` take a `CoverSpec` (binary subset), not a
  `CoverFidelityProfile`; there is no public path to re-project a generated source
  under a changing named fidelity profile. The interactive model therefore uses the
  reference path only; the generated lane path is exercised by `adversarial_06`.
* `CoverFidelityProfile` shares one `HarmonyRelation` between HarmonicContour and
  HarmonicLoop; independent control requires the `harmony_axis` label
  (`Contour`/`Loop`/`Both`), which is not obvious from the axis names.
* Refusals of kind `CoverError::Invalid` carry only a `&'static str` reason, not
  structured per-axis information (`CoverError::Rejected` carries `CoverAdmission`),
  so the app keyword-matches to attribute `REFUSED` axes.
* Semantic UI styling deliberately withholds direct RGB on text elements; the RGB
  reactor must be expressed as a full-body `Node::raster(Surface)` rather than styled
  text.

### 10.5 Upstream defects
* **`femboy2112/libgibson#15` — queued input after resize under graphical PTY
  backpressure (open).** This harness reproduces it deterministically: the same
  ~90-key burst quits cleanly **4/4 without a resize** and hangs **4/4 with a
  resize** during the backlog. A second lone `q` also fails, while Ctrl-C is
  received — i.e. the queued key is stranded, not lost to backpressure. This matches
  the owner's diagnosis (a key sharing one crossterm/Mio readiness batch with
  `SIGWINCH` is abandoned; upstream `crossterm-rs/crossterm#1126`, fix PR #1128
  unmerged). The regression guard is kept `#[ignore]`d as the acceptance test.

---

## 10. Upstream issues / comments

* `femboy2112/libgibson#15` — searched first; the defect is already covered, so a
  **comment** with the consumer data point above was posted rather than a new issue.
* All other observed friction was checked against the issue list (74, 73, 68, 15,
  …). The reference-path axis-coverage gap (`form`/`groove`/`orchestration` hardcoded
  `None`) is generic, reproducible, and uncovered, so it was filed as a single new
  issue titled
  `[v0.4.0 consumer: Project Theseus] ReferenceSong::extract_fidelity hardcodes form/groove/orchestration to None`,
  with a minimal public-API reproducer and explicit non-claims (it may be intended
  v0.x scope; the report is that it is observable, undocumented, and caps the
  reachable axis set).
* No LibGibson source was modified; no patches or path overrides were added.

---

## 11. Proven vs. unproven claims

**Proven (machine-checkable):**
* `cover()` is invoked with only the quotient and target; freed material does not
  change the map or output (adversarial_01, adversarial_02).
* Determinism: fixed inputs ⇒ identical plan, score, and audio hash (adversarial_03).
* Typed refusals are stable and world-specific (adversarial_04); displayed as
  first-class events with named axes (adversarial_07).
* The render matrix is panic-free and non-blank across 80 capability/geometry/glyph
  cells (adversarial_07, `--matrix`).
* The PTY harness navigates, edits, resizes (without backlog), helps, and quits
  cleanly with terminal restoration (pty_smoke).
* Resize-under-backpressure strands queued input (4/4 vs 4/4 discriminator) — a
  consumer reproduction of an open upstream defect.

**Unproven / not claimed:**
* That any machine metric determines "same song".
* That a human recognizes the motif-only cover, or fails to recognize the
  zero-axis cover — this is an informal listening/reading judgement, not a study.
* That the generated `extract_lane` path can be driven under named fidelity
  profiles (no such public API was found).
* That the small-geometry matrix `refusal`/`axes` columns are semantic; they are
  frame-text visibility probes only.
* That a single consumer probe resolves `#15`; it is a reproducing data point, not a
  fix.

## 12. Surprising successes and the strongest real API weakness

**Surprising successes.**
* The `harmony_axis` (`Contour`/`Loop`/`Both`) label makes HarmonicContour and
  HarmonicLoop genuinely independent — a discoverable affordance that the axis names
  hide.
* The same quotient + target reproduces byte-for-byte while still producing a
  musically different cover when axes change; the "new music" state is a valid,
  conformant cover (234 notes), not a degenerate fallback.
* `to_mono_surface` + `transcode_surface_glyphs` degrade the rich RGB reactor to
  ANSI16/mono and braille/ascii without losing the quotient's shape.

**Strongest real API weakness.** The reference path cannot observe three of the
eight axes at all (`form`, `groove`, `orchestration` are hardcoded `None` in
`extract_fidelity`), so an axis-based identity instrument built on a reference song
can only ever *pin or purge* five axes meaningfully. The generated `extract_lane`
path reaches more axes but offers no fidelity-profile projection, so there is no
single public path that both establishes and preserves all eight under named regimes.
