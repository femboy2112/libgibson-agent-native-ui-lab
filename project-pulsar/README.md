# Project Pulsar

A deterministic, synthetic **deep-space signal observatory** for the terminal, built on
[LibGibson](https://github.com/femboy2112/libgibson) (pinned to revision `068807d`, the v0.5
`gibson::plot` "Observable Geometry" layer).

Three persistent periodic signals are buried in noisy receiver data. You investigate the *same*
three signals through five scientifically meaningful representations and watch an interpretation
emerge as the integration time grows: noise, ambiguity, interference, false candidates, lock
acquisition, rising confidence, and a final localisation on the sky.

Everything is synthetic. Units are invented, no real object is described, there is no network and
there are no image assets. **The same seed always produces the same bytes.**

## The idea in one sentence

> Time is integration depth. Every signal is a persistent *identity* (a glyph, a letter, a hue and
> a profile silhouette) that is **transported** across five bases, and uncertainty is drawn as
> diffusion (hollow glyphs, mirror ghosts, wide glows) that condenses into crisp geometry as
> evidence accumulates.

| key | representation | what it shows | built with |
|---|---|---|---|
| `1` | **TRACE** | station-1 amplitude vs time. The pulses are invisible in the noise; the marker rows are each tracked identity's *predicted* pulse epochs. Masked samples (receiver dropouts, glitches, `NaN`/`inf`) are gaps, never bridged. | `gibson::plot` |
| `2` | **SPECTRUM** | normalised power vs frequency on a log axis; harmonic combs; identity tags on the lines. | `gibson::plot` (Log10 axis, extrema reducer) |
| `3` | **FOLD** | the data wrapped at the candidate's period: folded profile + a phase-time map whose ridge is straight when the period is right. `,` / `.` detune it and the ridge shears. | `gibson::plot` + custom shade raster |
| `4` | **RELATE** | the delay plane (tau21, tau31) with the physically allowed region, a convergence trail and 2-sigma ellipses; significance vs integration time; an evidence checklist for the focused candidate. | `gibson::plot` |
| `5` | **SKY** | inferred source geometry on an all-sky Hammer projection (or a local tangent view). Each identity is a posterior glow with 1/2-sigma contours *and a mirror ghost twin* until station S4 collapses the ambiguity. | custom Braille/shade graphics |

A persistent **ledger** (one line per identity: state, frequency, significance, folded-profile
sparkline, delays, sky position), an **event ticker** and a **scrubber** with lock/reject marks stay
on screen in every representation, so an identity can be followed by eye as the basis changes. Changing
representation plays a short *identity transport*: a seam wipes across the hero while each
identity's glyph flies from its old site to its new one.

## Quick start

```sh
cd project-pulsar
cargo run --release                       # interactive; starts playing at 8 mission-s per second
```

Needs a terminal of at least ~40x14. Resize freely. `q` quits and prints the key script that replays
the session exactly.

### Headless commands (all deterministic: same command, identical bytes)

```sh
# bounded tour (12 keyframes + an identity-transport freeze); prints a digest; terminates in ~1 s
cargo run --release -- --demo                 # 100x34
cargo run --release -- --demo 120x40
cargo run --release -- --demo 80x24:mono

# one frame at a size (static: t = 0, trace)
cargo run --release -- --capture 120x40
cargo run --release -- --capture 80x24
cargo run --release -- --capture 42x15
cargo run --release -- --capture 120x40:mono       # Mono fallback
cargo run --release -- --capture 120x40:color      # same frame with ANSI colour (text captures drop colour)

# temporal captures: frame(t), plus a representation
cargo run --release -- --capture 120x40 --at 64  --view spectrum
cargo run --release -- --capture 120x40 --at 100 --view fold --select A
cargo run --release -- --capture 120x40 --at 132 --view sky            # mirror ghosts everywhere
cargo run --release -- --capture 120x40 --at 168 --view sky            # after S4: ghosts collapsed
cargo run --release -- --capture 80x24  --at 200 --view relate
cargo run --release -- --capture 42x15:mono --at 256 --view sky

# focus, inspection and comparison
cargo run --release -- --capture 120x40 --at 256 --view sky --compare 90 --select A
cargo run --release -- --capture 120x40 --at 256 --view spectrum --cursor 0.19,0.5 --zoom 1 --select A
cargo run --release -- --capture 120x40 --at 100 --view fold --select A --detune 4
cargo run --release -- --capture 120x40 --at 200 --view sky --from spectrum --progress 0.55   # mid-transport

# replay a session exactly (the app prints this line when you quit)
cargo run --release -- --capture 120x40 --script "5 @150 down c z @200"

cargo run --release -- --report               # the pipeline's timeline vs the injected truth
cargo run --release --example seed_sweep 32   # robustness over 32 other worlds
cargo run --release -- --help
```

`--glyphs braille|halfblock|block|ascii` exercises the glyph-capability axis; `--seed N` picks another
world. The default world is seed `0x5015A2112112`.

## Controls

| key | action |
|---|---|
| `1`..`5`, `Tab`, `Shift-Tab` | choose the representation (starts an identity transport) |
| `Left` / `Right` | scrub one analysis epoch (4 s); `Shift` = 16 s |
| `Home` / `End` (`g`) | start / end of the mission |
| `n` / `p` (`PgDn`/`PgUp`) | jump to the next / previous pipeline event |
| `Space`, `+` `-` | play / pause, playback speed (2..64 mission-s per second) |
| `Up` / `Down` | pick the focused candidate (A..E) |
| `z` | zoom: all-sky -> local sky; spectrum around the candidate; trace window 24 s -> 8 s -> 3 s |
| `,` `.` | detune the fold's trial period (analysis focus; the pipeline's published solution is untouched) |
| `h` `j` `k` `l`, `x` | move / hide the inspect cursor: reads frequency, time, delays, phase, sky coordinates |
| `m`, `c` | mark the present as the *earlier state*, / compare it with now (ghosted overlay + a then-vs-now note); `c` alone picks an early uncertain moment for you |
| `r` | replay from t = 0 |
| `?` | help overlay (floats; the world never reflows) |
| `q`, `Esc` | quit |

There is no mouse: LibGibson's input layer has no mouse events, so inspection is a keyboard cursor.

## What is in the data

Five "slots" are tracked. Three are real, two are false:

| id | what it is | how it is told apart |
|---|---|---|
| `A` `●` | narrow single pulse, f = 1.29731 Hz | locks at ~96 s, sky position ~1 deg from truth |
| `B` `◆` | double-peaked profile, f = 2.17184 Hz | locks at ~124 s; its 2nd harmonic lands in slot E's band |
| `C` `▲` | broad hump + narrow spike, f = 0.61307 Hz | its 2nd harmonic (1.226 Hz) sits next to A: early on it is *blended*, A is even withdrawn as a harmonic ghost at 64 s |
| `D` `■` | continuous-wave interference at 3.00213 Hz, identical at every station | pure sinusoid (no harmonics) **and** common-mode: zero inter-station delay. Rejected at 44 s |
| `E` `▼` | a pulse train that is only on for 66..118 s | looks real while it is on, then its significance fades; rejected at 204 s, never locks |

Glyph shape is hollow while a candidate is unconfirmed (`○A?` candidate, `◇B~` tracking, `●A` locked,
`□Dx` rejected) and solid once locked, so *lock acquisition* is visible in every view and survives Mono.

The receiver has four stations (positions in light-seconds, see `src/sim.rs`). Stations 1-3 lie in a
plane, so a three-station delay solution cannot tell a source from its mirror image through that plane
(every sky glow has a ghost twin, weight 50/50). Station 4 is out of plane and comes online at
t = 140 s; once it has integrated a few seconds the ghosts collapse (t = 148 s). Station 1 also has
receiver dropouts, saturated samples and one `+inf`, which the pipeline must mask without bridging.

### The pipeline (all in this crate; nothing of this is in LibGibson)

`src/dsp.rs`: robust statistics, glitch mask + causal 6 s high-pass, radix-2 FFT, Tukey-windowed
periodogram normalised to a local noise floor, harmonic summing, coherent Fourier sums, golden-section
frequency refinement, epoch folding, exact chi-square tail -> "n sigma".
`src/track.rs`: per-epoch detection (4-term harmonic sum with a harmonic/octave sieve), five band-slot
trackers, coherent significance Z, pulse-likeness, persistence, two-half stationarity, lock/reject
state machine, harmonic-weighted inter-station delays, planar sky solution with covariance and
mirror-weight from station 4. It is **causal**: the solution published at epoch k reads only samples
before k (a test perturbs the future and checks the past is bit-identical).

## Layout, Mono, determinism

* **120x40** (wide): header, hero, ledger header + 5 lanes, event ticker, 3-row scrubber, hints.
  **80x24** (medium): drops the ledger header and ticker. **42x15** (tiny): a one-line identity strip
  and a 1-row scrubber. The hero (the active representation) is always the largest object; secondary
  meaning is dropped, not squeezed. No terminal size panics (tested over a 20x18 grid of sizes).
* **Mono** (`:mono`, or `ColorDepth::Mono`): meaning is carried by glyph shape, letters, line-versus-dot
  and a 4-level shade ramp, never by colour alone; Mono frames paint no backgrounds.
* **Determinism**: all randomness is a seeded SplitMix64; there is no wall-clock read in any paint path
  (time is explicit model state, `frame = f(model)`). Captures are byte-identical across runs on a
  machine; floating-point `libm` differences across platforms could change low digits, so
  `--demo` prints an FNV digest you can compare.
* **Replay**: every session is a key script (`@T` seeks, `wait:S` advances real time). The app prints
  the script on quit; `--script` replays it.

## Tests

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings
cargo test            # 16 unit + 37 invariant + 7 plot-defect reproducers
```

`tests/invariants.rs` asserts scientific and semantic properties, not "it renders": determinism of
world and frames; **causality**; the fold recovers every injected period and phase and the pulse
collapses when detuned; trace markers land on the true pulses; spectral lines sit at the tracked
frequency; sky localisation matches the injected geometry and the mirror resolves to the right
hemisphere; the planar-array 50/50 ambiguity and its collapse; confidence grows with integration;
false candidates are rejected and never lock; noise-only data raises no candidates; masked samples
are counted on the plot receipt and break the line; axis captions name the modelled quantities;
cursor readouts invert the plot transform; **an identity is the same glyph at its anchor in every
representation and size, hollow before lock and solid after**; anchor mapping is total; transport,
comparison, detune, focus and key handling; layout classes, Mono, the help overlay's locality; the
bounded demo; hostile command lines. `tests/plot_defects.rs` pins the `gibson::plot` surprises
documented in `FRICTION.md`.

## Source map

```
src/sim.rs          the synthetic world: sources, 4-station array, raw data (seeded)
src/dsp.rs          FFT, periodogram, folding, Fourier sums, statistics        (app-owned)
src/track.rs        detection, tracking, lock/reject, delays, sky fix, mirror  (app-owned)
src/observatory.rs  raw + cleaned streams + causal timeline + spectrum cache
src/identity.rs     glyph / letter / hue / state suffix: one identity everywhere
src/views/*.rs      trace, spectrum, fold, relate, sky; each reports identity Anchors
src/compose.rs      hero + ledger + ticker + scrubber + hints; semantic zoom; transport
src/state.rs        explicit-time model, keys, replay scripts
src/app.rs          gibson::ui App shell (one Node::canvas), headless capture through the real runtime
src/cli.rs          --capture / --demo / --report
```
