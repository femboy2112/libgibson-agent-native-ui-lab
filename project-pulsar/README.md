# PROJECT PULSAR

A deterministic **synthetic** deep-space signal observatory for the terminal, built on the
pinned `libgibson` revision (`gibson::plot` for the scientific graphs, `gibson::ui` for the
interactive runtime, the canvas/surface/glyph APIs for everything that is not a graph).

Nothing here is real astrophysics and nothing touches the network or an image file: every
sample comes from a seeded generator, every number is a pure function of the seed.

## The story

Three persistent sources hide far below the noise in a three-station receiver array:

| identity | what it is | where the catalogue prior says to look |
|---|---|---|
| `◆ ALPHA` (cyan) | fast pulse train (a comb of harmonics) | 1.0–2.4 Hz fundamental |
| `● BETA` (amber) | drifting narrowband carrier | 5.0–8.4 Hz, any drift |
| `▲ GAMMA` (magenta) | slow broad-pulse train | 0.25–0.85 Hz fundamental |

Around them: a bright **terrestrial** line (louder at one station than another — a real
far-field source is equally bright everywhere), a **transient ghost** that grows like a source
for two minutes and then fades, red receiver wander, and — on the default seed — a
**misleading candidate**: the terrestrial line and the ghost happen to sit on harmonics 5 and 2 of
a wrong fundamental, so the pulse search adopts a false ALPHA until the terrestrial line is
flagged and masked, and the candidate is *revised*.

You watch the same three identities go from "buried" to "interpreted" through six
representations. An identity is its **glyph + hue + rail slot + name** (redundant, so it
survives Mono), and the very same numbers (frequency, model, posterior) feed every view.

| key | representation | built with |
|---|---|---|
| `1` STREAM | receiver voltage vs time, plus the demodulated I/Q "lock monitor" per identity | `gibson::plot` (line, `ExtremaPerColumn`) |
| `2` SPECTRUM | whitened power vs frequency (log power), comb teeth / drift smear / flagged lines, dynamic spectrum | `gibson::plot` + custom raster |
| `3` FOLD | profile folded at the fitted period with ±1σ, the 2P rival fold, phase–time waterfall | `gibson::plot` + custom raster |
| `4` RELATION | phasor walk (Re vs Im), S/N growth law (log–log, slope ½), coherence table | `gibson::plot` |
| `5` SKY | orthographic all-sky: 160-sample posterior clouds, 1σ/2σ ellipses, position trails | custom Braille canvases (no polar plot in the library) |
| `6` DOSSIER | the interpretation in words, with the evidence that earns each claim | text |

Visual law: **uncertainty is diffusion, confidence is sharpness.** Before a measurement exists a
cloud is the prior (diffuse over the sky); as coherent integration accumulates the *same*
samples are drawn through a shrinking covariance, so the cloud visibly contracts onto the
source and the ellipses close around it.

## Run it

```sh
cargo build --release
cargo run --release                       # interactive (needs a terminal); a bare launch is a film that plays at x16
cargo run --release -- --paused --at 120 --view sky   # interactive, but still
cargo run --release -- --demo             # bounded, non-interactive: 12 frames to stdout
cargo run --release -- --demo --live      # the same cue sheet on the terminal itself (q quits)
cargo run --release -- --capture-dir captures   # regenerate the deterministic capture set
cargo run --release -- --dump --at 300 --view sky --size 80x24 --script "select beta; focus; truth"
cargo run --release -- --selftest
```

Common options: `--seed N` (default **62**), `--at SECONDS`, `--view stream|spectrum|fold|relation|sky|dossier`,
`--script "cmd; cmd"`, `--size WxH`, `--mono`, `--glyphs braille|halfblock|block|ascii`.
`cargo run -- --help` lists everything. (The `dev` profile is optimised — see `Cargo.toml` —
because the analysis does real FFTs; `cargo test` and a debug build are therefore usable.)

### Controls

```
1-6  view            ← →  ±8 s     , .  ±1 s     PgUp PgDn  ±60 s     Home End
g    go to t         n / N  next / previous analysis event
Space play/pause     [ ]  speed ×1 ×4 ×16 ×64 ×256
Tab / a b c  select an identity       f  overview ⇄ selected signal      + / -  zoom
p    pin the current time             m  compare pinned (earlier) vs now
i    inspect cursor, h j k l to move it — the readout is data coordinates, e.g. f = 5.4139 Hz
t    truth overlay (what was actually injected, drawn against the inference)
?    help            q / Ctrl-C  quit
```

* **Scrub / seek.** The timeline on the bottom row shows progress, lock events (`◆ ● ▲`),
  flagged interference (`✕`, `~`), the pinned time (`◇`) and the playhead. `g 120 ↵` jumps to t = 120 s.
* **Compare.** `m` puts an earlier, uncertain state (the pin; default = when the selected
  identity first became a candidate) next to the later interpretation, in any view.
* **Resize.** Layout is semantic zoom, not shrinkage: ≥110 columns a full identity rail,
  ≥78 a compact rail, <60 a one-row identity strip. Tested live in a PTY at 120×40 → 42×15 → 80×24.

### Exact replay

All state lives in one value that changes only through commands (`src/session.rs`). Time is an
explicit sample index; wall-clock enters only as an already-quantised `advance N` command that is
itself recorded.

```sh
cargo run --release -- --record session.log     # interactive; the command log is written on quit
cargo run --release -- --replay session.log     # headless: prints the final frame
```

A PTY test drives the real binary with keystrokes, records, replays through a second process and
asserts the replayed frame equals the live screen.

## Deterministic captures

`captures/` (134 files, regenerate with `--capture-dir captures`; see `captures/INDEX.txt` for
the one-line reproduction command of each file):

* `frames/{120x40,80x24,42x15}/` and `frames/mono-*/` — the six representations (`.txt` visible
  text; `.ansi` for sky and spectrum so the SGR attributes can be inspected);
* `keyframes/sky-120x40/t008.txt … t480.txt` — **the temporal keyframe sequence**: the same
  scene at 8, 32, 56, 80, 104, 152, 200, 300 and 480 s, showing uncertainty collapsing into
  interpretation; the same sequence for spectrum, fold, relation, stream, and for sky at 80×24,
  42×15 and 80×24 Mono;
* `compare/` — earlier (64 s) vs later (300 s) states side by side;
* `glyphs/` — the capability ladder (braille / half-block / block / ASCII).

Captures are byte-deterministic (`tests/captures.rs`). The drift guard against the committed tree
(`cargo test -- --ignored committed_captures`) is opt-in because transcendental functions are
platform-libm dependent.

## Architecture

```
scenario.rs   seeded universe: ground truth + the three precomputed station streams
dsp.rs        FFT, windows, robust floors, false-alarm / σ conversions (application-owned analysis)
analysis/     causal, checkpointed pipeline (one checkpoint per 8 s of data)
   spectral   whitened 3-station spectrum, robust harmonic-sum search, line finder
   chirp      causal baseband + Fourier-domain dechirp (drift search)
   coherent   phasor sums, ML polish, delays, sky solve, folding
   mod        interference masking, lock state machine, line classification, Engine
session.rs    State, Cmd, keys, script language, Session (log + exact replay)
render/       pure (State, Engine, size, capability) -> cells; each view reports `Probe`s
app.rs        gibson::ui App loop, the one view() used live and headless, demo, help modal
capture.rs    capture set, keyframes, cue sheet
```

**What the analysis does** (all of it in this application, none of it in the library): a whitened
three-station periodogram; a robust harmonic-sum search (per-harmonic clip, flagged interference
masked) for the pulse trains; a dechirp search for the drifting carrier; coherent maximum-likelihood
polish of the phase model; a lock state machine with hysteresis (`SEARCH → CANDIDATE → LOCKED →
LOCALIZED → RESOLVED`); inter-station delays from phase differences with a full covariance;
a 2-D sky posterior `A⁻¹ τ`; cross-station amplitude-uniformity and growth-law tests that classify
unassociated lines as terrestrial or transient.

**Causality.** Checkpoint *k* is a pure function of the first `k·256` samples (+ the previous
checkpoint's state machine). `tests/science.rs` proves it by replacing the unreceived data with
zeros and demanding bit-identical products. Seek therefore equals run, and a cold engine equals a
warm one (tested).

**Where `gibson::plot` is used and where it is not.** Every x–y relationship (voltage, spectra,
profiles, walks, growth curves) is a `PlotSpec` compiled through `gibson::plot::compile`; the
receipt (`PlotReport`) and the transform (`PlotTransform2D::unproject`) are used — the first to
prove no sample is silently lost, the second as the inspect cursor's hit test. The sky and the two
heat maps are custom (`BrailleCanvas` layers, glyph ramps) because the library has no heat-map or
orthographic projection (its own doc defers them).

## Tests (`cargo test`)

| file | what it asserts |
|---|---|
| `tests/structure.rs` | graph series equal the analysis observables value-for-value (spectrum, growth, walk, fold + error band, raw voltage, I/Q); receipts obey the conservation laws; same (seed, t, view) ⇒ identical geometry and text; terminal size changes realization not data (law E); colour depth / glyph mode change cells not geometry (law F); each identity is findable in every view in colour and Mono; one frequency is shared by rail, fold header, spectrum overlay and sky mark; seek = run; cold = warm analysis; random command sequences replay to the identical state and frame; inspect readout round-trips through the library transform for every cell of every plot; degenerate inputs (no data yet, sub-chunk time, 1×1 … 200×60, all-NaN and flat series, invalid axis configuration) are stated, not faked, and never overflow a row |
| `tests/science.rs` | analysis is causal; stated uncertainties are calibrated across 8 universes (frequency z-score rms < 1.7, ≥85 % of position errors inside the stated 95 % region); **pure noise never locks** (6 universes × 60 checkpoints) ; geometry sign/orientation conventions recover the sky at 8× amplitude; the terrestrial line is rejected by the amplitude test *before* ALPHA locks, the ghost is flagged transient only after it fades; the misleading candidate is formed then revised; precision improves monotonically with time and lock never regresses; the dossier refuses to print a direction for the source it cannot localize; coherent growth exponent ≈ ½ |
| `tests/captures.rs` | capture set is byte-deterministic and contains all sizes/Mono/keyframes; keyframes differ pairwise and the posterior cloud contracts; the demo is bounded and deterministic; the Mono stream contains no colour parameters while identity glyphs persist; glyph-mode ladder |
| `tests/pty.rs` | the real binary in a real pseudo-terminal (vt100): keys, goto prompt, help modal, `n/N`, Tab, inspect, compare, live resize, clean quit, live demo terminates, record → replay equals the live screen |
| `tests/library_defects.rs` | reproducers for the library defects in `FRICTION.md` (pinned to the observed behaviour) |

## Calibration and honesty notes

`cargo run --release --example calibration -- 24` (24 universes, final estimates vs injected truth,
in units of the analysis' *own* 1σ): frequency z-score rms **1.03 / 0.76 / 0.95**
(ALPHA / BETA / GAMMA); position Mahalanobis² mean **1.34 / 0.87 / 1.76** (ideal 2: the stated
sky uncertainty is slightly conservative), inside the 95 % region 23/24, 24/24, 24/24.
`--example null_scan -- 24`: on 24 noise-only universes (4320 identity-checkpoints) there are
**0 locks**, 101 candidate-checkpoints (2.3 %), 10/24 universes with a transient false candidate.

Things to know before trusting a picture:

* The catalogue priors (search bands) are constants the generator respects: this is *refining
  known targets*, not blind discovery. Delay phases are unambiguous only below ≈ 0.45/τ_max; the
  generator keeps the sources inside that, and the analysis does not detect a violation.
* **Seed 62 was chosen by scanning seeds** (`examples/seed_scan.rs`) for the story it tells; the
  misleading candidate is emergent from the noise and the interference geometry, not scripted, but
  other seeds tell other stories (tested only for determinism, calibration and the null).
* GAMMA (0.53 Hz) is locked but its position stays unconstrained: a 56 ms baseline spans ≈ 0.19 rad
  of its cycle. That is physics, and the dossier says so.
* Lock/localised/resolved thresholds, confidence = −log10(FAP)/10 and the posterior being a
  truncated Gaussian are *definitions made here*, not measurements.
* Not verified: a real Linux virtual console (`TERM=linux` Braille realization — use `--glyphs`),
  any terminal emulator other than the `vt100` crate, wide-glyph treatment of `✕ ❚ ◆` outside it,
  human usability.

## Developer tools (`examples/`)

`analysis_dump` (timeline vs truth), `calibration`, `null_scan`, `seed_scan`, `frame_timing`,
`peek` (print one frame), `ansi_dump` (raw ANSI of one frame, for rendering to an image).

See `FRICTION.md` for notes on building against the library.
