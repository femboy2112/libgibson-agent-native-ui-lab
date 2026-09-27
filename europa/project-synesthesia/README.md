# Project Synesthesia

**SYNESTHESIA** is a playable terminal step sequencer, synthesizer, and live signal visualization. Four editable tracks feed a patch route, a waveform, an FFT spectrum and bounded spectrogram, a stereo field, and mixer energy on one subcell canvas. The released **LibGibson v0.2.0** Git tag is its only terminal UI engine; CPAL 0.16 provides sound when an output device exists. This directory is a standalone Cargo package and an external consumer, independent of the lab's historical root crate.

The Europa-shot is deliberately unreasonable: it asks a terminal renderer to behave like an audiovisual performance instrument while notes are edited, sound is generated on a separate callback, the terminal resizes, and input arrives during animation. Its pressure points are the public `Node`/`Surface`/`BrailleCanvas` composition path, high-churn frame diff and wire output, event delivery, headless capture, and color/glyph degradation. It is an instrument with a small synth, rather than a spectrum-only demo.

## Start here

From this directory, on Linux with a working default audio output (`pkg-config` and ALSA development headers are required to build CPAL's Linux backend):

```sh
cargo run --locked --release -- --demo
```

`--demo` starts the four-track pattern immediately. The HUD reports the audio device and **PCM max**, the largest generated sample handed to its callback so far. If CPAL cannot open a device, the instrument continues as a silent visual simulation. A nonzero PCM max verifies generated samples; it does not by itself prove speaker audibility or correct system volume/routing. Press `P` for concert mode, Space to pause/resume, and `Q` to exit.

For machines without audio devices or a TTY:

```sh
cargo run --locked --release -- --demo --silent --seed=2112 --at-ms=3750 \
  --pattern=offbeat --width=120 --height=40 --truecolor --glyphs=braille --dump > frame.ansi
cargo run --locked --release -- --demo --silent --performance --profile \
  --frames=9000 --fps=30 --width=120 --height=40
```

The first command reproduces one byte-stable ANSI frame. View it with `cat frame.ansi` in a capable terminal. `--at-ms` forces a headless, silent capture; `--frames` runs a deterministic synthetic timeline without wall-clock pacing. The seed, pattern, time, dimensions, color mode, and glyph mode are explicit capture inputs. Supported patterns are `pulse`, `glass`, `offbeat`, and `empty`. Color flags are `--truecolor`, `--ansi256`, `--ansi16`, and `--mono`; glyph choices are `--glyphs=auto|braille|halfblock|block|ascii`. The live visual cadence defaults to 30 FPS and accepts `--fps=1..120`; this is a target, not a guaranteed refresh rate.

## Instrument controls

| Key | Action |
|---|---|
| Left/Right | Move across steps in the grid; adjust resonance or pan in other focus modes |
| Up/Down | Edit the selected note's pitch, or choose a track when the step is empty; adjust cutoff or mixer gain in other focus modes |
| `J` / `K` | Always choose the next/previous track, even when a note occupies the current step |
| Enter | Toggle the selected note |
| `+` / `-`, `,` / `.` | Change velocity; transpose a note |
| Tab / BackTab | Move between grid, patch, and mixer focus |
| `F`, `E`, `W` | Edit filter, edit ADSR, cycle oscillator waveform |
| `M`, `S`, `L` | Mute, solo, change pattern length |
| `[` / `]` | Lower/raise BPM |
| Space, `P`, `Q` | Play/pause, concert/arrange mode, quit |

The arrange view shows the editable four-lane piano roll flowing through `SEQ → OSC → ENV → VCF → PAN → MIX → OUT` into the spectrum, stereo waveform, and mixer field. Small diagrams under OSC, ENV, VCF and PAN show waveform shape, ADSR, filter response and measured stereo balance where space allows. Concert mode keeps the same sequencer and signal-route coordinates, expanding the downstream harmonic history and separate left/right PCM threads. Both views use the same pattern and patch state. Routing figures remain raw RMS; geometry uses fixed soft-knee display curves (`magnitude / (magnitude + 0.018)` for FFT, `sample / (|sample| + 0.12)` for PCM) so quiet signal is legible without per-frame normalization. Zero signal stays zero. Beat phase positions routing pulses; pan moves the stereo field. Mono carries the path through position, glyph density, line hierarchy, and labels.

At 60×20 the layout retains the four tracks, route, stereo output and mix while omitting the separate FFT panel. At 80×24 the small patch diagrams yield space to the frequency panel. At fewer than 48 columns or fewer than 20 total rows, the visual field becomes a resize message with transport and quit controls still visible.

## Architecture and bounds

- `src/engine.rs` computes a deterministic absolute-time stereo frame, Hann-windowed 1024-point FFT, waveform buckets, note/routing energy, and a seed-derived oscillator phase. This model drives headless output and the visual field.
- `src/audio.rs` owns the realtime CPAL callback, transport, fixed voice pool, ADSR, filter, stereo panning, and PCM sample conversion. The UI publishes a bounded atomic control snapshot. The callback neither locks nor allocates and never waits for terminal rendering. The audio and visual paths share pattern, patch, and seed but are distinct synth implementations; their PCM is **not** asserted sample-identical.
- `src/visual.rs` builds one custom `Surface` from `BrailleCanvas`, then composes it with public `Node` text/raster nodes. Spectrum history is a 96-row ring and freezes on transport HOLD unless an edit changes the simulated signal. `src/app.rs` owns keyboard editing and modal controls. `src/input_pump.rs` places LibGibson events into a 512-event bounded queue independent of frame rendering. `src/profile.rs` keeps fixed-size counters/histograms. The silent timeline advances by `floor(frame_index × 1000 / fps)` milliseconds to avoid long-run frame-period rounding drift.
- The app uses public LibGibson `Context` (fullscreen/headless), `Node`, `Surface`, `BrailleCanvas`, glyph realization, capability quantization, input events, layout/paint, and `compute_diff`. It does not use `gibson::ui`, skins, 3D rasterization, or private APIs.

## Validate

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked --release
python3 scripts/pty_stress.py --audio --no-backpressure --live-seconds=5
python3 scripts/pty_stress.py --audio --output-pause-ms=90 --live-seconds=5
```

The PTY probe is Linux-specific and needs the release binary for its default path. Omit `--audio` to avoid opening an output device. It exercises editing, modal text, key repeat, mode switching, resize, and a brief output-drain pause, then reports application key receipts, callback timing/PCM, frame timings, wire bytes, exact cell deltas, and retained history. `--profile` adds a second public layout/paint/diff pass to measure exact changed cells, so profile cost exceeds normal rendering cost.

`tests/render_golden.rs` asserts a fixed byte capture and TrueColor/ANSI256/ANSI16/Mono plus Braille/halfblock/block/ASCII fallbacks. It checks all four labels in 80×24 and 56×24 concert frames and the condensed 60×20 arrange view. Unit tests cover deterministic PCM/FFT, filter response, mute/solo, keyboard/modal editing (including absolute ADSR values and occupied-step track navigation), patch-coupled route geometry, bounded history, and the live synth's first-sample/PCM regression.

## Results and limits

The original imported version was measured on Linux Mint 22.3, kernel 7.0, Rust 1.98.1: a 120×40/30 FPS live PTY run lasted 30 seconds with 938 frames, 728/728 callback buffers containing PCM, zero application deadline misses, and stable warm/end RSS of 11,652 KiB. Its mean frame was 1.90 ms with 3,906.7 emitted bytes and 1,131.9 exact changed cells. Those numbers describe the **pre-polish source**, not this branch's changed geometry or visual transfer. See [EXPERIMENT_REPORT.md](EXPERIMENT_REPORT.md) for post-polish probes and provenance.

On the polished source, this Ubuntu 24.04.3 host ran 9,000 virtual frames at 30 FPS in silent concert mode: mean 1.03 ms, p95 2.00 ms, zero budget misses, 1,680.3 exact changed cells and 5,953.5 bytes/frame, with 96/96 history and unchanged 4,376 KiB warm/end RSS. A 3,600-frame 15 FPS run gave a 1.03 ms mean and 4,380/4,380 KiB warm/end RSS. Two 5-second silent PTY probes (one with a 90 ms output-drain pause during resize) delivered all 88 expected application keys; neither opened an audio device. The richer field increases changed cells and wire output. These measurements are host- and input-specific, and the virtual runs are unpaced.

**Project-local defects corrected:** an age-zero ADSR sample initially killed every live voice, leaving an active but silent CPAL stream; a regression now checks nonzero PCM. Typed ADSR edits previously added the requested value to the current value; they now assign it. Occupied steps previously blocked arrow-based track switching, so J/K now provide unconditional track navigation. **Known limitations:** visual and live audio synths share controls but do not produce sample-identical PCM; saw/square oscillators are not bandlimited; the demo's four tracks share one global patch. Physical listening quality and non-Linux audio/terminal behavior were not certified.

**LibGibson defects discovered here:** none demonstrated. The project's [ergonomics report #47](https://github.com/femboy2112/libgibson/issues/47) requests access to the exact changed-cell count already computed during a `Context` render; current profiling repeats layout/paint/diff to obtain it. Manual custom-Surface sizing is additional consumer evidence for existing [issue #43](https://github.com/femboy2112/libgibson/issues/43), and input stress does not close known [issue #15](https://github.com/femboy2112/libgibson/issues/15). No LibGibson source, patch, vendor copy, or path dependency is used. `Cargo.lock` resolves `tag=v0.2.0` to `7a10e60bb55f58cec3364dad125bf1ae9127b0d2`.
