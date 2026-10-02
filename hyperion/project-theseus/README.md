# Project Theseus — the Ship of Theseus machine for music

An interactive RGB terminal instrument built on **LibGibson v0.4.0** that makes the
HumanMusic *Cover Mode* quotient visible and audible. It decomposes a reference
performance into an eight-axis **`CoverMap`** — the identity quotient — lets you
surgically remove identity one axis at a time, and synthesizes a *fresh* cover from
**only the surviving quotient** in a target `MusicWorld`.

The load-bearing law of the whole experiment:

> `cover()` receives **the `CoverMap` and the target** — never the reference
> performance. There is no hidden copy of the source behind the generator.

Every pixel on screen is computed from the quotient state and the real cover score.

---

## Build & run

```bash
cargo run --release                # interactive, full audio
cargo run                          # debug is fine; dev profile is opt-level = 1
```

Requires the pinned dependency (already in `Cargo.toml` and `Cargo.lock`):

```toml
libgibson = { git = "https://github.com/femboy2112/libgibson", tag = "v0.4.0", features = ["audio-cpal"] }
```

No local path overrides, no patches, no `main` dependency, no copied source.

## Interactive controls

| Key | Action |
|-----|--------|
| `1`–`8` | Toggle a quotient axis (Motif, Riff, Groove, HarmonicContour, HarmonicLoop, Form, Orchestration, BassFigure) |
| `f` | Cycle fidelity regime: `Loose → Interpretive → Faithful → Strict` |
| `w` | Cycle target world: `VAPOR95 → BLACK_ICE → SWISS_SIGNAL` |
| `s` | Reseed the deterministic generator |
| `d` | **WTF snap** — strip to a single axis / snap back |
| `r` | Reset all axes to full identity |
| `g` | A/B the reference against the fresh cover |
| `?` / `h` | Toggle the operator manual |
| `q` | Quit (also: `Esc` when the manual is closed; Ctrl-C always) |

`Esc` closes the manual when it is open, otherwise quits.

## The WTF moment

With `Faithful` + `--harmony` in `BLACK_ICE`, the same target and seed:

| Axes kept | Theseus Index | Notes | `score_hash` |
|-----------|--------------:|------:|--------------|
| full identity `11111111` | **100 %** | 330 | `e8b39ac74a30e8f8` |
| nothing `00000000` | **0 %** ("new music") | 234 | `01e6ff3845337da1` |
| Motif only `10000000` | **20 %** (recognizability snaps back) | 305 | `8254330070a7fcaf` |

`THESEUS INDEX` is the fraction of *reachable* identity preserved, so a reference's
full identity reads 100 % and stripping every axis reads 0 %. The cover is
re-synthesized on every edit; the note count and score hash change with it.

## Lawful refusal is a first-class visual event

A target world may lawfully reject the bridge. The app catches neither-and-hides
nothing: the refusal is typed, displayed, and the involved axes light up `REFUSED`.

| Reference | Fidelity / world | Typed outcome |
|-----------|------------------|---------------|
| Ode to Joy | `Strict`, `SWISS_SIGNAL`, no harmony | `Invalid("no lawful harmony contains the pinned simultaneous attacks")` |
| Ode to Joy + `--harmony` | `Strict`, `VAPOR95` or `BLACK_ICE` | `Invalid("a pinned bass event has no lawful pitch function in the target harmony")` |
| Ode to Joy + `--harmony` | `Strict`, `SWISS_SIGNAL` | `Invalid("pinned harmony outside target vocabulary")` |

## CLI, deterministic capture, and the matrix

```bash
# One deterministic frame (motion disabled), for CI / golden captures:
project-theseus --headless text|ansi|json|ppm|lines --geometry 120x40 --capability truecolor

# The full geometry × capability × glyph no-panic matrix (80 cells):
project-theseus --matrix

# Ordered key replay (deterministic input mode):
project-theseus --script "1f2w"            # or --replay keys.txt --record applied.log
project-theseus --no-audio                 # required for headless / CI
```

`--headless json` emits the quotient state plus an evidence record: `CoverMap`
fingerprint, relations, envelope hash, typed outcome, note count, performance
fingerprint, score hash, `CoverConformance`, `PerformanceReceipt`, and WAV hash.

## What is reachable (honest axis accounting)

The eight axes are the full control surface, but a *reference song* cannot establish
all of them through `ReferenceSong::extract_fidelity`:

| Axis | Reachable from a reference? |
|------|------------------------------|
| Motif, Riff | yes (from the selected melody) |
| HarmonicContour, HarmonicLoop | only with `--harmony` (derived harmony) |
| BassFigure | yes, **if** a voice named `bass` exists (the Ode fixture has one) |
| Groove, Form, Orchestration | **no** — `extract_fidelity` hardcodes them to `None` |

So the app prints `UNKNOWN` for axes the reference never established rather than
pretending a removed axis was ever there. `PURGED` means "the fidelity regime could
know this axis and you removed it." See `FRICTION.md` and the report. (The generated
`CoverMap::extract_lane` path *can* populate more axes; it is exercised by
`adversarial_06` but is not wired into the interactive model because `extract_lane`
takes no fidelity profile.)

## Visual design

* Full-body RGB software raster per frame: the identity reactor is a metaball
  plasma core with per-axis rings and beams, post-processed by LibGibson's `RasterFx`
  chain and realized to cells via `to_surface` / `to_mono_surface`.
* A preserved axis is a continuous pulsing ring with a live beam; a removed axis is
  a fractured, gapped ring whose beam is gone. There is no separate hidden state —
  the geometry *is* the quotient.
* `build_frame` composites the reference piano roll, the quotient reactor, the fresh
  cover roll, a spectrum, and the axis console into one raster, with a thin semantic
  header/footer shell and a non-trapping help panel.

## Tests

```bash
cargo test                                   # 7 adversarial + 2 real-PTY tests
cargo test --test pty_smoke -- --ignored     # reproduces upstream #15 (expected red)
```

* `tests/adversarial.rs` — 7 structural properties (freed material non-interference,
  pinned positive control, determinism, typed-refusal stability, fidelity
  monotonicity, identity-vs-lane projection, render matrix + PURGED/REFUSED visuals).
* `tests/pty_smoke.rs` — drives the real binary over a pseudo-terminal: navigation,
  all eight axes, fidelity/world/seed edits, resize, help, rapid-interaction stress,
  clean quit, and alternate-screen restoration; plus an `#[ignore]`d regression guard
  for the queued-input-after-resize defect (crossterm readiness-batch, upstream
  `femboy2112/libgibson#15` / `crossterm-rs/crossterm#1126`).

## Determinism

Fixed fixture + seed + target + fidelity ⇒ byte-identical `--headless json` across
runs (verified). The evidence record is reproducible; no machine metric is claimed
to determine "same song".
