# Project Galileo

A real-time Jovian mission operations and scientific visualization environment built entirely in Rust, consuming the released **LibGibson v0.2.0** API as an external host.

## Mission

Project Galileo is a **procedural mission-operations simulation**, not flight software or measured Europa data. Its Jovian moons follow prescribed circular orbits; its spacecraft follows an illustrative Keplerian ellipse. A local-frame impulsive maneuver adds a first-order displacement to the displayed path, without a gravitational repropagation. A unified simulation clock also drives synthetic telemetry, a selected Europa survey site, and a generated ice-shell radargram. The application tests whether a released terminal UI library can make these linked states readable and explorable.

## Why This Application Is Intentionally Unreasonable ("Europa-Shot")

Traditional terminal dashboards display text metrics, tables, and perhaps ASCII bar charts. Project Galileo treats the terminal cell framebuffer as a high-density, multi-scale scientific instrument:
- **Linked Scale Views**: The operator moves from a Jovian orbit map into an illustrative Europa encounter, a selected surface survey site, and a radargram at that site. Zooming across adjacent views reveals the next scientific scene from the selected moon or survey feature; labels and diagnostic panels switch intact at the midpoint. Compact screens switch the entire scene at the midpoint. Direct view tabs switch immediately. These are authored scale changes, not one continuously integrated 3D camera.
- **Pure Terminal Graphics**: Zero Sixel, zero Kitty graphics, zero image protocols, zero embedded webviews. Every pixel, crater, fracture, magnetic field line, and radar wavefront is rasterized into LibGibson's `RgbRaster`, half-block subcell buffers, and `BrailleCanvas` glyphs, mapped through LibGibson's semantic `gibson::ui` layout tree.
- **State Coupling**: The orbital model, eclipse indicator, synthetic power/link budget, bounded alerts, surface target, and radar depth gate share mission state. Many scientific readings are invented model outputs and must not be interpreted as observations or validated instrument predictions.

## What Visual & Interaction Capabilities It Stress-Tests

1. **Subcell Rasterization & Braille Compositing**: Simultaneous compositing of 24-bit RGB rasters (`RgbRaster`) with geometric line overlays (`BrailleCanvas`); measured throughput is reported below for this specific host and capture mode.
2. **Semantic UI Compositing (`gibson::ui`)**: Dynamic assembly of `screen()`, `row()`, `column()`, `card()`, `badge()`, `progress()`, `button()`, `text_input()`, and raw `surface()` visualizers inside responsive flex containers.
3. **Spatial Scale Handoff**: Bounded, deterministic site-anchored reveals across the three adjacent scientific view boundaries. The selected surface feature and its nominal ice thickness feed the radargram.
4. **Degradation Across Terminal Capabilities**: Coherent degradation from 24-bit TrueColor to ANSI-256, ANSI-16, and 1-bit Monochrome (`RgbRaster::to_mono_surface()`).
5. **Interactive Controls vs Modal State**: Text input routing, focus handling, keyboard accelerator chords (`Alt+1..5`, `z`/`x`, `h`/`j`/`k`/`l`, space, warp), and command line parsing.

## Exact LibGibson Dependency

Pinned strictly to the official release tag `v0.2.0` on GitHub. Zero local path overrides, zero forks, zero patches:

```toml
[dependencies]
libgibson = { git = "https://github.com/femboy2112/libgibson", tag = "v0.2.0" }
```

## Architecture Overview

```
project-galileo/
├── src/
│   ├── main.rs                   # CLI arguments, headless runner, event loop, profiler
│   ├── lib.rs                    # Library root exposing sim, render, and ui modules
│   ├── demo.rs                   # Deterministic headless demo script and timeline
│   ├── profile.rs                # Frame bytes, timing, and animation cost profiler
│   ├── sim/
│   │   ├── mod.rs                # Simulation root
│   │   ├── jovian.rs             # Jovian system orbital kinematics, dipole B-field, shadow cone
│   │   ├── trajectory.rs         # Trajectory lab, impulsive delta-V maneuvers, B-plane flyby coordinates
│   │   └── telemetry.rs          # Spacecraft avionics, RTG, battery SoC, RF DSN link, payload states
│   ├── render/
│   │   ├── mod.rs                # Visualization exports
│   │   ├── system_view.rs        # 3D Jovian orbital system, atmospheric belts, moon tracks, B-field
│   │   ├── trajectory_view.rs    # Dual-orbit flyby encounter lab, B-plane deflection HUD, scrub bar
│   │   ├── surface_view.rs       # 3D Europa globe shading, cycloids, chaos terrain, SAR ground swath
│   │   ├── tomography_view.rs    # Ice shell radar radargram (B-scan) and A-scan logarithmic power trace
│   │   └── scale.rs              # Continuous scale coordinator and camera zoom interpolator
│   └── ui/
│       ├── mod.rs                # UI module exports
│       ├── command.rs            # Command syntax parser (view, zoom, burn, radar, gate, skin, etc.)
│       └── dashboard.rs          # Semantic mission control dashboard and responsive layout tree
└── tests/
    └── responsive_and_modes.rs   # 13 integration tests for all views, dimensions, depths, and zoom
```

## Major Application Views

1. **1: Jovian System (`1:SYS`)**
   - 3D perspective projection of Jupiter, atmospheric bands, Great Red Spot, Io, Europa, Ganymede, and Callisto on prescribed approximate orbital periods.
   - Jovian dipole magnetic field ($\vec{B}$, 4.28 G at equator, $9.6^\circ$ tilt) rendered on Braille canvas.
   - Jupiter's cylindrical umbral shadow cone with real-time eclipse telemetry warning banner.
   - Camera azimuth and elevation panning, auto-orbit, and orbital target tracking reticles.
2. **2: Trajectory Lab (`2:TRAJ`)**
   - Dual-trajectory comparison: nominal sampled orbit vs first-order local-frame burn displacement; it is **not** a propagated spacecraft navigation solution.
   - Illustrative Europa Sphere of Influence (SOI, $\approx 9{,}700\text{ km}$) and sampled closest approach marker. Nominal default path currently samples a 3,471 km altitude pass; planned deviations are first-order linear shifts and can grow much larger after a burn, so the planner is a visual thought experiment rather than a flight-dynamics prediction.
   - B-plane targeting coordinate frame ($B \cdot T, B \cdot R, |B|$), miss distance, and deflection vector.
   - Interactive mission timeline scrub bar (`[` and `]`) and prograde/radial burn execution (`b`).
3. **3: Europa Regional Survey (`3:SURF`)**
   - 3D spherical raster shading of Europa with sub-solar illumination and Jovian back-shine.
   - Procedural cycloid ridges (*lineae*), disrupted ice rafts (*Conamara Chaos*), and upwelling domes (*Murias Chaos*).
   - Thermal IR false-color layer (`g`) revealing active cryogenic heat flux.
   - 8 named geological features with coordinates and invented illustrative thickness, stress, temperature, and plume parameters.
   - Ground track swath from synthetic aperture radar (SAR) and REASON sounder nadir footprint.
4. **4: Ice Shell Tomography (`4:TOMO`)**
   - **Hero Scientific View**: Procedurally generated B-scan and illustrative A-scan, inspired by ice-penetrating radar displays. No spacecraft data or calibrated REASON response is used.
   - Band controls choose a 0–35 km HF depth range, a 0–7 km VHF depth range, or a split label with the deep range. Their synthetic reflectivity law is largely shared; the split mode does **not** fuse independent measured frequencies.
   - Invented layered medium includes fractures, brine pockets, warm diapirs and a modeled basal interface. Deep ocean/plume coloration is an illustrative visualization, not a radar return from below an opaque ocean.
   - Interactive gate displays model permittivity, local two-way delay, illustrative loss/temperature and offset from the selected site's nominal basal depth. Gate position persists across simulation ticks and stays within the selected band.
5. **5: Mission Control (`5:OPS`)**
   - Full-scale scientific telemetry matrix: RTG electrical power, battery state-of-charge, bus voltage/current, Canberra/Goldstone/Madrid 70m DSN carrier link SNR, one-way light time, radiation dose rate, payload instrument telemetry, and active alert logs.

## Controls

### Global Keyboard Shortcuts
- `1` – `5` or `Alt+1` – `Alt+5`: Switch active view (`1:SYS`, `2:TRAJ`, `3:SURF`, `4:TOMO`, `5:OPS`)
- `z` / `x`: **Continuous Zoom**:
  - Within any view, zooms in/out dynamically.
  - In `SystemView`: select Europa before zooming past the threshold into the Europa-only `TrajectoryView`; selecting another moon retains its reticle and cannot route into an unrelated Europa encounter.
  - In `TrajectoryView`: zooming in past threshold penetrates down to `SurfaceView`.
  - In `SurfaceView`: zooming in past threshold penetrates down to `TomographyView`.
  - In `TomographyView`: zooming in switches to high-resolution 60 MHz VHF radar; zooming out ascends back up the orbital chain.
- `h` / `j` / `k` / `l` or `Arrow Keys`: View-contextual navigation:
  - `SystemView`: Panning camera azimuth and elevation
  - `SurfaceView`: Spinning globe longitude and latitude
  - `TrajectoryView`: Scrubbing encounter timeline
  - `TomographyView`: Moving radar depth gate cursor
- `Tab` / `Shift+Tab`: Move focus among buttons and the command input. While the command input is focused, letters and arrows edit the command. `Alt+1`–`Alt+5` switch views even while editing.
- `Space`: Pause / Resume simulation clock
- `w` / `t`: Cycle simulation warp rate (`1x`, `10x`, `60x`, `300x`, `1000x`)
- `[` / `]`: Scrub trajectory timeline backward / forward (5-hour increments)
- `+` / `-`: Increase / decrease radar receiver gain (dB)
- `r`: Cycle radar sounder frequency band (`9 MHz HF` $\to$ `60 MHz VHF` $\to$ `Split-Band`)
- `b`: Execute planned impulsive maneuver burn ($\Delta V = 25\text{ m/s}$)
- `n` / `p`: Select next / previous orbital or surface target
- `g`: Toggle Europa thermal IR false-color overlay
- `m`: Toggle Jovian magnetic dipole field line overlay
- `o`: Toggle Jovian camera auto-orbit
- `s` or `Alt+S`: Cycle UI theme skin (`BLACK_ICE`, `SWISS_SIGNAL`, `VAPOR95`)
- `a`: Acknowledge active mission alerts
- `Esc` / `q`: Clear command input or exit application

### Console Command Syntax
Type commands into the `CMD> ` input bar at the bottom:
- `view <1-5|system|traj|surf|tomo|ops>`: Switch view
- `zoom <in|out|sys|orbit|surf|ice|<value>>`: Adjust zoom scale
- `target <io|europa|ganymede|callisto|jupiter>`: Track target body
- `burn <delta_v_m_per_s>`: Execute prograde maneuver burn
- `radar <hf|vhf|split>`: Configure REASON sounder band
- `gate <depth_km>`: Set interactive radar depth gate
- `gain <db>`: Adjust radar receiver gain
- `warp <rate>`: Set simulation time acceleration
- `pause` / `resume`: Toggle mission simulation clock
- `scrub <hours>`: Scrub trajectory encounter timeline
- `thermal`: Toggle thermal infrared surface mode
- `mag`: Toggle magnetic field rendering
- `skin <black_ice|swiss|vapor95>`: Switch UI theme skin
- `help`: Display command list

## Build Instructions

Prerequisite: a stable Rust toolchain (verified with Rust 1.98.1).

```bash
# Debug build
cargo build

# Optimized release build (LTO enabled)
cargo build --release
```

## Run Instructions

```bash
# Interactive real-time operations console
./target/release/project-galileo

# Specific initial view
./target/release/project-galileo --view=tomography
./target/release/project-galileo --view=surface
./target/release/project-galileo --view=trajectory

# Terminal dimension overrides
./target/release/project-galileo --width=160 --height=50
./target/release/project-galileo --width=80 --height=24

# Color depth overrides
./target/release/project-galileo --truecolor
./target/release/project-galileo --ansi256
./target/release/project-galileo --ansi16
./target/release/project-galileo --mono
```

## Deterministic, Demo & Headless Commands

```bash
# Deterministic headless frame capture at exactly t = 1000 ms
./target/release/project-galileo --at-ms=1000 --view=system

# Deterministic demo execution (120 headless frames, no real-time sleep)
./target/release/project-galileo --demo --headless --frames=120

# Frame profiling (timing, byte volume, animations, retained keys)
./target/release/project-galileo --at-ms=2500 --view=tomography --profile
./target/release/project-galileo --demo --headless --frames=3600 --fps=60 --width=120 --height=40 --profile
```

## Test Commands

```bash
# Run all unit and integration tests
cargo test

# Run tests in release mode
cargo test --release

# Clippy linter with all warnings denied
cargo clippy --all-targets --all-features -- -D warnings

# Formatting check
cargo fmt --check
```

## Capability Fallbacks

- **TrueColor (24-bit RGB)**: Primary hero experience. Full atmospheric gradient belts, ice surface texture, Jovian back-shine, thermal IR false color, and radar dielectric return colormaps.
- **ANSI-256**: Automatically quantized by LibGibson's color pipeline.
- **ANSI-16**: Quantized to standard 16 terminal colors.
- **Monochrome (`--mono`)**: Uses `RgbRaster::to_mono_surface()` with thresholded half-blocks and dim/bold cell styles. All scientific readouts, Braille graticule, and telemetry remain legible.

## Performance Measurements

Observed in this environment (Linux x86_64, AMD EPYC 9V74, Rust 1.98.1 optimized release), with the 3,600-frame headless command above, TrueColor $120\times40$:

- **Mean frame construction/render time:** 0.66 ms, min 0.38 ms, max 2.68 ms. This timer ends before profiler parsing and does not measure display refresh or operator input latency.
- **Mean incremental terminal output:** 5,686 bytes/frame; 19.52 MiB over the run. The context buffer is drained after every frame, including when `--profile` is off. `--fps=60` advances the demo clock at 60 virtual frames/s; headless execution does not sleep or establish interactive 60 FPS delivery.
- **Observed process wall time:** 2.73 s for 3,600 headless frames including profiling; results depend on host load, terminal capability, view sequence, and dimensions. Interactive `--profile` reports render timing only because the fullscreen context sends bytes directly to stdout.
- **Observed peak resident memory:** 8,448 KiB over 3,600 unprofiled frames and 8,576 KiB over 18,000 on this host, measured with `resource.getrusage(RUSAGE_CHILDREN).ru_maxrss`. These two samples support stable memory in that workload but do not prove a process-wide bound.

## LibGibson Issues Discovered & Filed

1. **Issue [#35](https://github.com/femboy2112/libgibson/issues/35)**: `TextInputState` and `UiRuntime::handle_event` unconditionally consume `Alt+<char>` chords as text input.
   - *Impact*: Swallows `Alt` hotkeys when an input field is focused.
   - *Workaround*: Intercept `KeyEvent` with `KeyModifiers::ALT` before delegating to `UiRuntime`.
2. **Issue [#36](https://github.com/femboy2112/libgibson/issues/36)**: `Element<A>` lacks `percent_width` / `percent_height` layout builders available on `Node`.
   - *Impact*: Semantic elements cannot express fractional splits directly.
   - *Workaround*: Use `BuildCx.environment.width / height` to compute explicit cell constraints.
3. **Issue [#37](https://github.com/femboy2112/libgibson/issues/37)**: `UiRuntime` unconditionally triggers `MotionRole::Enter` for all `Key::Named` elements, causing `SurfaceFx::Dissolve` to clear them to spaces on the initial frame.
   - *Impact*: All keyed elements are invisible at $t=0$ or in headless captures without pre-roll.
   - *Workaround*: Set `env.motion = MotionPreference::None` for fixed captures, and omit `.key()` from static buttons.
4. **Issue [#43](https://github.com/femboy2112/libgibson/issues/43)**: `Ergonomics & Architecture Report: Real-time multi-scale scientific visualization`.
   - *Impact*: Comprehensive report detailing size-blind surface embedding, canvas compositing boilerplate, greedy input routing, and entrance motion traps.
   - *Workaround*: Documents workarounds used in Galileo and proposes concrete API enhancements for v0.3.0.

## Known Limitations

1. **Illustrative dynamics**: Moons use prescribed circular positions, the spacecraft a Keplerian ellipse, and maneuver response a linear displacement from a local-frame velocity impulse. No n-body integration, SPICE ephemerides, calibrated B-plane targeting, or validated flyby trajectory is implemented.
2. **Terminal Cell Aspect Ratio**: Assumes standard 1:2 cell aspect ratio (subcell pixels are half-blocks). On non-standard fonts, spheres may appear slightly elliptical.
3. **Synthetic instruments**: Surface geology, thermal overlay, ground swath, radar frequencies, radargram, telemetry, and alerts are procedural fixtures. The ice-ocean art does not establish radar penetration or ocean detection. The VHF view clips the cursor to its 7 km display range; bands share a synthetic reflectivity model.
