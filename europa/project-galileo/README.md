# Project Galileo

A real-time Jovian mission operations and scientific visualization environment built entirely in Rust, consuming the released **LibGibson v0.2.0** API as an external host.

## Mission

Project Galileo models a deep-space exploration vehicle operating in the Jovian planetary system and executing scientific reconnaissance of Europa. It integrates a deterministic n-body gravitational and orbital mechanics engine, continuous multi-scale 3D camera navigation, synthetic aperture radar (SAR) swath projection, sub-surface ice shell radar sounding (synthetic radargram/tomography), spacecraft avionics and subsystem telemetry (RTG decay, battery SoC, RF link budget, thermal dissipation, propellant tracking), and an interactive command/control deck into a single, cohesive terminal application.

## Why This Application Is Intentionally Unreasonable ("Europa-Shot")

Traditional terminal dashboards display text metrics, tables, and perhaps ASCII bar charts. Project Galileo treats the terminal cell framebuffer as a high-density, multi-scale scientific instrument:
- **Continuous Multi-Scale World**: A user navigates seamlessly from the macro Jovian system ($\sim 2{,}500{,}000\text{ km}$ across all 4 Galilean moons) through Europa's hyperbolic encounter flyby ($\sim 50{,}000\text{ km}$), down to regional surface cycloid fractures and chaos terrain ($\sim 500\text{ km}$), into an ice shell cross-section ($\sim 25\text{ km}$), and finally down to high-resolution sub-ice ocean radar tomography ($\sim 5\text{ km}$) — without feeling like unrelated pages.
- **Pure Terminal Graphics**: Zero Sixel, zero Kitty graphics, zero image protocols, zero embedded webviews. Every pixel, crater, fracture, magnetic field line, and radar wavefront is rasterized into LibGibson's `RgbRaster`, half-block subcell buffers, and `BrailleCanvas` glyphs, mapped through LibGibson's semantic `gibson::ui` layout tree.
- **Physical Coherence**: The orbital mechanics, planetary positions, Jupiter shadow cone and umbral eclipse detection, tilted magnetic dipole field lines, radio propagation delay to Earth, battery charging/discharging, and propellant consumption are coupled to a unified mission clock.

## What Visual & Interaction Capabilities It Stress-Tests

1. **Subcell Rasterization & Braille Compositing**: Simultaneous compositing of dense 24-bit RGB rasters (`RgbRaster`) with high-resolution geometric line overlays (`BrailleCanvas`) in real time (>900 FPS headless, 60 FPS interactive).
2. **Semantic UI Compositing (`gibson::ui`)**: Dynamic assembly of `screen()`, `row()`, `column()`, `card()`, `badge()`, `progress()`, `button()`, `text_input()`, and raw `surface()` visualizers inside responsive flex containers.
3. **Continuous Multi-Scale Navigation**: Real-time zoom coordination interpolating between macro-orbital coordinate systems and local kilometer-scale depth gate coordinates.
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
   - 3D perspective projection of Jupiter, atmospheric bands, Great Red Spot, Io, Europa, Ganymede, and Callisto in resonant Keplerian orbits.
   - Jovian dipole magnetic field ($\vec{B}$, 4.28 G at equator, $9.6^\circ$ tilt) rendered on Braille canvas.
   - Jupiter's cylindrical umbral shadow cone with real-time eclipse telemetry warning banner.
   - Camera azimuth and elevation panning, auto-orbit, and orbital target tracking reticles.
2. **2: Trajectory Lab (`2:TRAJ`)**
   - Dual-trajectory comparison: nominal pre-burn orbital trajectory vs planned post-burn deflection.
   - Europa Sphere of Influence (SOI, $\approx 9{,}700\text{ km}$) and closest approach marker.
   - B-plane targeting coordinate frame ($B \cdot T, B \cdot R, |B|$), miss distance, and deflection vector.
   - Interactive mission timeline scrub bar (`[` and `]`) and prograde/radial burn execution (`b`).
3. **3: Europa Regional Survey (`3:SURF`)**
   - 3D spherical raster shading of Europa with sub-solar illumination and Jovian back-shine.
   - Procedural cycloid ridges (*lineae*), disrupted ice rafts (*Conamara Chaos*), and upwelling domes (*Murias Chaos*).
   - Thermal IR false-color layer (`g`) revealing active cryogenic heat flux.
   - 8 named geological features with coordinates, ice thickness, tidal stress, and plume probabilities.
   - Ground track swath from synthetic aperture radar (SAR) and REASON sounder nadir footprint.
4. **4: Ice Shell Tomography (`4:TOMO`)**
   - **Hero Scientific View**: Real-time synthetic radargram (B-scan) and logarithmic power profile (A-scan) replicating the REASON dual-frequency ice-penetrating radar sounder.
   - Dual-band operation: 9 MHz HF (deep sounding down to 35 km) and 60 MHz VHF (high-resolution shallow sounding down to 7 km), plus combined Split-Band.
   - Layered physical modeling: brittle conductive lid (0–4 km), hyper-saline brine pockets, fault conduits, ductile convective ice with thermal diapirs (4–22 km), and basal ocean interface (>22 km).
   - Interactive depth gate cursor with live dielectric permittivity ($\epsilon_r$), two-way travel time ($\tau$), radar attenuation ($\alpha$), temperature ($T$), and basal ocean interface confidence metric.
5. **5: Mission Control (`5:OPS`)**
   - Full-scale scientific telemetry matrix: RTG electrical power, battery state-of-charge, bus voltage/current, Canberra/Goldstone/Madrid 70m DSN carrier link SNR, one-way light time, radiation dose rate, payload instrument telemetry, and active alert logs.

## Controls

### Global Keyboard Shortcuts
- `1` – `5` or `Alt+1` – `Alt+5`: Switch active view (`1:SYS`, `2:TRAJ`, `3:SURF`, `4:TOMO`, `5:OPS`)
- `z` / `x`: **Continuous Zoom**:
  - Within any view, zooms in/out dynamically.
  - In `SystemView`: zooming in past threshold automatically penetrates down to `TrajectoryView`.
  - In `TrajectoryView`: zooming in past threshold penetrates down to `SurfaceView`.
  - In `SurfaceView`: zooming in past threshold penetrates down to `TomographyView`.
  - In `TomographyView`: zooming in switches to high-resolution 60 MHz VHF radar; zooming out ascends back up the orbital chain.
- `h` / `j` / `k` / `l` or `Arrow Keys`: View-contextual navigation:
  - `SystemView`: Panning camera azimuth and elevation
  - `SurfaceView`: Spinning globe longitude and latitude
  - `TrajectoryView`: Scrubbing encounter timeline
  - `TomographyView`: Moving radar depth gate cursor
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

Prerequisites: Rust 1.80+ (stable or nightly).

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

# Deterministic demo execution (120 frames headless benchmark)
./target/release/project-galileo --demo --headless --max-frames=120

# Frame profiling (timing, byte volume, animations, retained keys)
./target/release/project-galileo --at-ms=2500 --view=tomography --profile
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

Benchmarked on Linux x86_64, release build:
- **Headless Frame Time**: $\sim 1.11\text{ ms}$ per frame ($>900\text{ FPS}$)
- **Rendered Output**: $\approx 4.5\text{ KB}$ ANSI TrueColor stream per $120\times 40$ frame
- **Simulation Update**: $< 15\text{ }\mu\text{s}$ per step
- **Memory Consumption**: Fixed bounded footprint ($\sim 18\text{ MB}$ RSS, zero unbounded history growth)

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

## Known Limitations

1. **Project-Local Simplification**: The gravitational simulation uses deterministic multi-body Keplerian approximations rather than high-precision SPICE ephemerides.
2. **Terminal Cell Aspect Ratio**: Assumes standard 1:2 cell aspect ratio (subcell pixels are half-blocks). On non-standard fonts, spheres may appear slightly elliptical.
