# Project Synesthesia — LibGibson v0.2.0 external stress test

## Goal

Build an actual keyboard-playable terminal music workstation whose notes become the geometry of one animated signal-processing machine: editable piano-roll lanes feed oscillator, envelope, filter, pan, mixer, waveform, FFT spectrum and spectrogram, and a concert field. It must retain a deterministic silent path for CI while live audio runs on an independent callback. The goal was to push terminal-native subcell graphics and frame diff under simultaneous sound, animation, editing, and resizing, not merely to draw eight dashboard widgets.

This is the standalone `synesthesia` Cargo package under `europa/project-synesthesia/`. It consumes `libgibson = { git = "https://github.com/femboy2112/libgibson", tag = "v0.2.0" }`; `Cargo.lock` resolves `7a10e60bb55f58cec3364dad125bf1ae9127b0d2`. It has no path override, vendored copy, dependency patch, or LibGibson source edit. The lab root's older frozen-revision experiments are separate historical evidence and are not recast as v0.2.0 results.

## LibGibson surface exercised

- Public `Context::fullscreen` and `Context::headless`, terminal session lifecycle, `Node` text/raster/flex composition, public layout and painter, `Surface`, `Rect`, `Cell`, `Style`, and border drawing.
- `BrailleCanvas` lines, circles, rectangles and pixel masks; glyph realization via Braille, halfblock, block and ASCII; `ColorDepth` TrueColor, ANSI256, ANSI16 and Mono.
- LibGibson `Event`/`KeyEvent` and `poll_event`, rendered frame statistics, and public `compute_diff` for exact changed-cell measurement.

The application does **not** exercise `gibson::ui`, `BuildCx`, `PresentationCx`, `FocusRing`, skins, `Scene`, `SurfaceFx`, `RgbRaster`, or `raster3d`. Its keyboard/modal focus is application-owned; its raster is one custom `Surface` composed into five `Node`s.

## What worked surprisingly well

The public canvas and raster route expressed a continuous note→patch→output field without private renderer access or raw terminal escape authoring. A fixed seed/time produces byte-identical headless frames, and the golden test covers fallback encodings. On this Linux host, the 30-second live PTY probe at 120×40/30 FPS received all 88 expected keyboard events despite rapid repeat, modal edits, mode switches and a resize; a separate 90 ms output-drain pause also received all 88. These bounded probes do not prove the known queued-input issue is fixed.

The live CPAL callback remained independent of terminal output. The original app bug made its stream active but silent: new voices were killed because the first ADSR attack sample is exactly zero. A failing regression was added, then the voice lifecycle was corrected to retire only after release. In a 30-second live run, 728/728 callback buffers contained nonzero PCM, session peak was 0.3977, with zero callback over-budget reports and backend errors. PipeWire showed the Synesthesia stream connected to both active analog playback channels on this host. That verifies the PCM/output route, not what a listener heard.

## Friction encountered

**Application-specific friction and defects.** The age-zero voice-retirement bug above was in Synesthesia, not LibGibson. The PTY probe's bounded rolling capture initially evicted the startup `audio active` text in longer runs and falsely reported it absent; it now preserves that receipt separately. Keeping the live callback and absolute-time visual simulation separate prevents terminal rendering from blocking audio, but they share controls rather than sample-identical PCM. Saw and square waves are intentionally non-bandlimited. A single global patch serves all four tracks.

**Generic LibGibson ergonomics.** A custom raster needs concrete dimensions before `Node::raster` participates in layout, so this consumer calculates its surface size and responsive gutter/lane policy itself. The same general pressure appears in existing [issue #43](https://github.com/femboy2112/libgibson/issues/43) (size-aware custom surfaces) and [issue #42](https://github.com/femboy2112/libgibson/issues/42) (responsive layout breakpoints), filed by other Europa consumers. Synesthesia provides another concrete usage site without a distinct layout mechanism warranting duplicate issues. A separate profiling gap did warrant [issue #47](https://github.com/femboy2112/libgibson/issues/47): `Context` reports logical affected cells and wire bytes, while its renderer already computes the exact semantic changed-cell count internally. To measure that count, Synesthesia clones its `Node` and repeats layout, paint, and diff against a second retained surface on every profiled frame. The consumer also owns energy-to-color mapping and a bounded spectrogram; those are domain-specific, not missing library primitives.

**Probable LibGibson defects.** None newly isolated. Existing [issue #15](https://github.com/femboy2112/libgibson/issues/15) concerns queued input coincident with resize under graphical PTY backpressure. Synesthesia's 90 ms probe passed, but it counts application receipts and cannot establish syscall-to-backend delivery for the adversarial timing in #15. It neither reproduces nor resolves that case.

**Proven LibGibson defects from this project.** None. No LibGibson source change was made.

## GitHub issues filed

- [#47 — `[v0.2.0 consumer: Synesthesia] Ergonomics report: exact rendered-cell deltas need a second layout/paint pass`](https://github.com/femboy2112/libgibson/issues/47): `Context` does not expose the exact semantic changed-cell scalar already computed by its renderer, so the app duplicates the public layout/paint/diff pipeline only for profiling. Workaround: a second `Surface` and previous-frame buffer under `--profile`. The issue also records working APIs, related issues #42/#43/#15, and application-owned stumbles; it does not assert a renderer or audio defect.

A tracker search found no existing issue for this `Context` observability gap. The only concrete silence defect was application-owned; responsive/custom-surface observations overlap #42/#43; the input probe supplies no new failure beyond #15. No application-specific feature wish was filed as a library bug.

## Performance / scale observations

Environment: Linux Mint 22.3, kernel 7.0.0-28-generic x86_64, Rust/Cargo 1.98.1, LibGibson tag v0.2.0. The interactive PTY probe used `--demo --profile --fps=30`, initially 120×40, resized through 56×24, with `TERM=xterm-256color` and `COLORTERM=truecolor`. The numbers below are one host/session, not a guaranteed FPS or general performance bound. Profiling adds a second public layout/paint/diff pass.

| Probe | Observed result |
|---|---|
| 30 s live audio + keyboard PTY | 938 frames; frame mean 1.90 ms, p95 4.00 ms, max 9.10 ms; zero application deadline misses and zero renderer scheduler skips |
| Live render components | deterministic DSP 0.79 ms/frame; custom Surface 0.25 ms/frame; LibGibson render 0.45 ms/frame; five generated Nodes |
| Live diff/wire | 1,131.9 exact changed cells/frame; 1,145.6 affected cells/frame; 3,906.7 emitted bytes/frame, max 6,941 |
| Live retention | spectra 96/96; RSS 11,652 KiB at warm frame 64 and at exit |
| Live audio callback | 728 callbacks, all nonzero; PCM peak 0.3977; mean 624.33 μs, max 1,692.43 μs; zero over-budget and backend errors |
| 9,000-frame silent concert simulation from the lab worktree | mean 1.19 ms, p95 2.00 ms, max 2.32 ms; zero late/skipped; 1,192.0 exact changed cells and 4,113.6 emitted bytes/frame; spectra 96/96; RSS warm/end 5,684 KiB |

The **pre-polish** 9,000-frame command was `target/release/synesthesia --demo --silent --performance --profile --frames=9000 --fps=30 --width=120 --height=40`; it advanced a synthetic timeline without sleeping. The 30-second live metrics came from the same application source tree before import, using `python3 scripts/pty_stress.py --audio --no-backpressure --live-seconds=30`. The lab worktree reran 5-second live and 90 ms backpressure probes: both exited zero with 88/88 expected keys; their 142/142 and 149/149 callbacks carried nonzero PCM, respectively. The fixed 120×32 TrueColor/Braille capture's SHA-256 was identical before and after import (`9819e2da3d6f598890f3eb15fb3fc0192597dfa1ee2d48d439eb15c9aa059acb`). The bounded RSS observations rule out unbounded growth in these sessions, not all allocations or longer-run retention.

## Capability/fallback observations

The golden test reproduces a fixed 120×32 TrueColor/Braille frame and checks distinct ANSI256/halfblock, ANSI16/block, and Mono/ASCII output. It asserts fallback frames retain the sequencer field. Compact concert tests check track labels, route and exit affordance at 80×24 and 56×24; a condensed 60×20 view checks track, route, output and quit labels, while a 44×14 fallback keeps the quit instruction. The visual grammar uses shape, lane separation, line strength, and text labels in addition to color. The tests establish deterministic rendering and presence of structural cues; aesthetic judgment and every terminal/font implementation remain unproven. Below 48 columns or 20 rows, the app shows a resize message in place of the field.

## Second pass: instrument and visual polish

This pass corrected a project-owned ADSR input defect: numeric modal entry added the submitted value to the old stage, even though the prompt said “type value.” It now assigns the typed value, while Up/Down still performs incremental adjustment. J/K now navigate tracks even when Up/Down edits the pitch of an occupied step. Tests exercise both behaviors. A PTY modal inspection exposed clipped Enter/Esc instructions at 80 columns; width-aware modal help now retains the action and cancel keys. The visual transfer diagrams under OSC, ENV and VCF change with the corresponding patch controls while the sampled audio frame is held constant; PAN uses measured left/right PCM balance. Concert mode aligns its larger frequency/stereo field with the sequencer and route gutter, and draws separate left/right PCM threads.

The original amplitude scale made ordinary valid output (e.g. raw OUT RMS 0.09 at the fixed offbeat capture) look empty. FFT and PCM geometry now use **fixed** soft-knee visual transfers (`m/(m+0.018)` and `s/(|s|+0.12)`, respectively). They preserve zero signal and relative order, but are intentionally *not* a calibrated amplitude scale; route numbers remain raw RMS. More space goes to the FFT/history at medium widths, and 60×20 intentionally drops the FFT panel while retaining the four tracks, patch route and stereo output. On HOLD, the spectrogram timeline stops accumulating duplicate spectral rows unless an edit changes the simulated signal. The synthetic timeline computes `floor(frame_index × 1000 / FPS)` milliseconds rather than repeatedly truncating 33⅓ ms to 33 ms. Audio snapshots are published on actual user edits and transport changes; repeated, identical per-frame writes were removed. The audio callback remains independent of rendering.

**Discriminating visual probes:** deterministic headless captures at 120×40 TrueColor/Braille with `--demo --silent --dump --width=120 --height=40 --seed=2112 --at-ms=3750` and `--pattern=empty|pulse|offbeat` were compared. Empty rendered route output and downstream FFT/stereo energy at zero; pulse and offbeat produced distinct route figures, frequency contours and stereo threads. Offbeat at 3750 versus 3900 ms changed envelope, FFT and waveform shapes. This is observed visual coupling for these inputs; it does not prove live audio and simulation are sample-identical. An independent visual review found readable lane separation at 120×40 TrueColor and 80×24 Mono/ASCII after adding a one-column gutter between narrow track labels and the grid. Layout/capability captures were also inspected at 160×50, 100×30, 60×20 and a 44×14 resize fallback. No new generic LibGibson bug was isolated, and existing issues #15/#43/#47 remain accurately scoped.

**Post-polish silent profile on this execution host:** Ubuntu 24.04.3 x86_64, Rust 1.98.1, released LibGibson tag v0.2.0, `--performance --profile --silent --demo --width=120 --height=40`. The profiled path repeats layout/paint/diff for exact changed-cell counting (#47), so it costs more than normal rendering. The run did not sleep or open an audio device; its FPS target denotes virtual timeline spacing and a per-frame budget, not measured real-time cadence.

| Synthetic run | Frame timing | Diff / output | Retention |
|---|---|---|---|
| 9,000 frames at 30 FPS (300 virtual seconds) | mean 1.03 ms, p95 2.00 ms, max 5.26 ms; 0 budget misses | 1,680.3 exact changed cells and 5,953.5 bytes/frame | history 96/96; RSS warm/end 4,376/4,376 KiB |
| 3,600 frames at 15 FPS (240 virtual seconds) | mean 1.03 ms, p95 2.00 ms, max 3.20 ms; 0 budget misses | 1,775.7 exact changed cells and 6,186.5 bytes/frame | history 96/96; RSS warm/end 4,380/4,380 KiB |

The 30 FPS run used `target/release/synesthesia --demo --silent --performance --profile --frames=9000 --fps=30 --width=120 --height=40`; the 15 FPS run changed only `--frames=3600 --fps=15`. Under comparable 9,000-frame concert settings, the **earlier implementation on the Linux Mint host** recorded 1.19 ms mean, 1,192 exact changed cells/frame and 4,113.6 bytes/frame; the **polished implementation on this Ubuntu host** recorded 1.03 ms, 1,680.3 and 5,953.5, respectively. The richer field enlarges changed-cell and wire footprint. The observed render-time difference crosses hosts and source versions, so it cannot establish a causal speed improvement. Long-session bounded history and RSS were observed, not proven for every environment or workload.

**Post-polish interactive probes:** `python3 scripts/pty_stress.py --binary target/release/synesthesia --live-seconds=5 --no-backpressure` and the same command with `--output-pause-ms=90` both exited zero with 88/88 expected application key receipts and zero input backend errors. The first run reported 188 frames, mean 1.15 ms/frame, 1,439.4 exact changed cells and 5,232.9 bytes/frame, with 96/96 history and warm/end RSS 4,548/4,548 KiB. The 90 ms output-pause run reported 191 frames, mean 1.09 ms/frame, 1,439.7 changed cells, 5,209.9 bytes/frame and warm/end RSS 4,696/4,696 KiB. Both reported zero application deadline misses. The pause run counted one resize event; the no-pause run counted none, though both invoked PTY window-size changes. These probes only attest to application-dispatched key receipts in this timing and do not resolve issue #15 or prove every resize event was delivered. They requested silent mode; no post-polish hardware-audio or acoustic playback claim follows from them.

**Build/test gate:** Rust 1.98.1 with `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (10 unit and 5 integration tests), `cargo clippy --locked --all-targets --all-features -- -D warnings`, and `cargo build --locked --release` all passed after the polish. The deterministic TrueColor/Braille golden hash was rebaselined only after inspecting the changed visual field; fallback assertions remained active. Local Linux build requires `pkg-config` and `libasound2-dev` for CPAL's ALSA backend. No release dependency was changed or patched.

## What this experiment demonstrates

For this application on the stated host, released LibGibson v0.2.0 public `Node`/`Surface`/`BrailleCanvas` APIs support a responsive high-churn audiovisual terminal composition with deterministic capture and measured diffs. The app can be played and edited while a separate audio callback produces nonzero PCM, and the same musical controls drive the visual simulation. This is evidence for this consumer, not universal terminal UI expressivity or exact equivalence between the two synth implementations.

## What remains unproven

- Physical listening quality, acoustic output at a person's ears, latency as perceived by a performer, and long-term audio glitch freedom. Callback budget counts are not hardware underrun telemetry.
- Sample-identical live and visual PCM, phase-accurate UI/audio transport sync after arbitrary edits, and high-order DSP fidelity. The two paths share pattern/patch/seed but have separate implementations and can use different sample rates.
- Non-Linux devices/terminals, all color and Unicode realizations, accessibility, mouse support, extreme terminal dimensions, or guaranteed 60 FPS.
- Resolution of LibGibson issue #15. The bounded Synesthesia input probe passed but does not supply the issue's required layer-specific adversarial reproducer.

No screenshot or giant ANSI capture is committed. The exact capture and validation commands are in [README.md](README.md); tests preserve a deterministic golden hash and responsive/fallback assertions.
