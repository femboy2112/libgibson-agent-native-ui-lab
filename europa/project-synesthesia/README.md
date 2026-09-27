# Project Synesthesia

**SYNESTHESIA** is a playable terminal step sequencer, synthesizer, and live signal visualization. Four editable tracks feed a patch route, a waveform, an FFT spectrum and bounded spectrogram, a stereo field, and mixer energy on one subcell canvas. The released **LibGibson v0.2.0** Git tag is its only terminal UI engine; CPAL 0.16 provides sound when an output device exists. This directory is a standalone Cargo package and an external consumer, independent of the lab's historical root crate.

The Europa-shot is deliberately unreasonable: it asks a terminal renderer to behave like an audiovisual performance instrument while notes are edited, sound is generated on a separate callback, the terminal resizes, and input arrives during animation. Its pressure points are the public `Node`/`Surface`/`BrailleCanvas` composition path, high-churn frame diff and wire output, event delivery, headless capture, and color/glyph degradation. It is an instrument with a small synth, rather than a spectrum-only demo.

## Start here

From this directory, on Linux with a working default audio output:

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
| Arrows | Navigate steps/tracks; adjust the focused patch or mixer control |
| Enter | Toggle the selected note |
| `+` / `-`, `,` / `.` | Change velocity; transpose a note |
| Tab / BackTab | Move between grid, patch, and mixer focus |
| `F`, `E`, `W` | Edit filter, edit ADSR, cycle oscillator waveform |
| `M`, `S`, `L` | Mute, solo, change pattern length |
| `[` / `]` | Lower/raise BPM |
| Space, `P`, `Q` | Play/pause, concert/arrange mode, quit |

The arrange view shows the editable four-lane piano roll flowing through `SEQ → OSC → ENV → VCF → PAN → MIX → OUT` into the spectrum, stereo waveform, and mixer field. Concert mode compresses the note lanes and expands the harmonic/wave field. Both views use the same pattern and patch state. Amplitude and FFT bins affect geometry and density; track/routing energy affects color intensity; beat phase positions routing pulses; pan affects the stereo traces. Mono still carries the path through position, glyph density, line hierarchy, and labels.

## Architecture and bounds

- `src/engine.rs` computes a deterministic absolute-time stereo frame, Hann-windowed 1024-point FFT, waveform buckets, note/routing energy, and a seed-derived oscillator phase. This model drives headless output and the visual field.
- `src/audio.rs` owns the realtime CPAL callback, transport, fixed voice pool, ADSR, filter, stereo panning, and PCM sample conversion. The UI publishes a bounded atomic control snapshot. The callback neither locks nor allocates and never waits for terminal rendering. The audio and visual paths share pattern, patch, and seed but are distinct synth implementations; their PCM is **not** asserted sample-identical.
- `src/visual.rs` builds one custom `Surface` from `BrailleCanvas`, then composes it with public `Node` text/raster nodes. Spectrum history is a 96-row ring. `src/app.rs` owns keyboard editing and modal controls. `src/input_pump.rs` places LibGibson events into a 512-event bounded queue independent of frame rendering. `src/profile.rs` keeps fixed-size counters/histograms.
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

`tests/render_golden.rs` asserts a fixed byte capture and TrueColor/ANSI256/ANSI16/Mono plus Braille/halfblock/block/ASCII fallbacks. It also checks all four labels in 80×24 and 56×24 concert frames. Unit tests cover deterministic PCM/FFT, filter response, mute/solo, keyboard/modal editing, bounded history, and the live synth's first-sample/PCM regression.

## Results and limits

On Linux Mint 22.3, kernel 7.0, Rust 1.98.1, a 120×40 interactive 30 FPS PTY run lasted 30 seconds: 938 frames, zero application deadline misses, 728/728 callback buffers with nonzero PCM, PCM peak 0.3977, zero callback over-budget reports and backend errors, 96/96 retained spectra, and RSS 11,652 KiB both after warmup and at exit. Its mean frame time was 1.90 ms, p95 4.00 ms, with 3,906.7 emitted bytes and 1,131.9 exact changed cells per frame. A separate 9,000-frame silent concert simulation from this lab worktree averaged 1.19 ms/frame with 4,113.6 bytes/frame and stable 5,684 KiB warm/end RSS. These are host-specific observations, not a throughput promise. See [EXPERIMENT_REPORT.md](EXPERIMENT_REPORT.md) for exact scope and provenance.

**Project-local defects corrected during development:** an age-zero ADSR sample initially killed every live voice, leaving an active but silent CPAL stream; a regression now checks nonzero PCM. The 30-second stress probe initially forgot that its bounded rolling capture could evict the startup audio-status string; that receipt is retained separately. **Known project-local limitations:** visual and live audio synths share controls but do not produce sample-identical PCM; saw/square oscillators are not bandlimited; the demo's four tracks share one global patch; terminals below roughly 48 columns show a resize message instead of the full field. Physical listening quality and non-Linux audio/terminal behavior were not certified.

**LibGibson defects discovered here:** none demonstrated or newly filed. Synesthesia's manual custom-Surface sizing is additional consumer evidence for existing [issue #43](https://github.com/femboy2112/libgibson/issues/43), and its event stress does not close known [issue #15](https://github.com/femboy2112/libgibson/issues/15). No LibGibson source, patch, vendor copy, or path dependency is used. `Cargo.lock` resolves `tag=v0.2.0` to `7a10e60bb55f58cec3364dad125bf1ae9127b0d2`.
