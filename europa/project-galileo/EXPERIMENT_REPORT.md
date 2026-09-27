# Project Galileo — LibGibson v0.2.0 external stress test

## Goal

The objective of Project Galileo was to test whether the released LibGibson v0.2.0 API could support a detailed pure-terminal **procedural mission simulator**: a Jovian operations dashboard with authored scientific visualizations. Neither the dynamics nor the synthetic radargram are validated spacecraft or Europa measurements.

The core challenge was linking authored scenes at approximate Jovian-system ($\sim 2{,}500{,}000\text{ km}$), encounter ($\sim 50{,}000\text{ km}$), surface ($\sim 500\text{ km}$), deep ice ($\sim 25\text{ km}$), and shallow ice ($\sim 5\text{ km}$) extents. A short reveal follows the selected Europa/site into the next scene; labels switch as complete blocks, and compact terminals change the whole scene at midpoint. This is not a continuous spatial coordinate system or one integrated camera. Native terminal cells carry the images without Sixel, Kitty, or GUI embedding.

## LibGibson Surface Exercised

The following parts of LibGibson v0.2.0 were materially exercised:

- **`gibson::ui` Semantic Hierarchy**:
  - `screen()`, `row()`, `column()` layout builders
  - `card()`, `badge()`, `progress()`, `button()`, `text_input()` semantic components
  - Responsive density modes (`Density::Compact`, `Density::Normal`)
  - Semantic tone and emphasis styling (`Tone::Accent`, `Tone::Warning`, `Tone::Success`, `Emphasis::Strong`, etc.)
- **`BuildCx` & `PresentationCx`**:
  - Environment inspection (`cx.environment.width`, `height`, `color_depth`)
  - Dynamic frame timestamp evaluation (`Duration` progression)
- **`UiRuntime`**:
  - Semantic layout tree framing (`runtime.frame(&tree, env, elapsed)`)
  - State reconciliation across configured update frames (30 FPS interactive default; headless accepts `--fps`)
  - Event dispatching and controlled focus (`runtime.handle_event(&event)`)
  - Animation tracking (`runtime.active_animation_count()`, `retained_key_count()`)
- **Cell Framebuffer & Subcell Rasterization**:
  - `RgbRaster`: High-density 24-bit RGB pixel rasterization (spherical 3D lighting, procedural textures, radar reflectivity grids)
  - `RgbRaster::to_surface()`: Converting raster pixels into half-block character cells
  - `RgbRaster::to_mono_surface()`: 1-bit monochrome half-block degradation
- **`BrailleCanvas`**:
  - High-resolution dot-matrix geometry ($2\times 4$ subcell grid per character)
  - 3D coordinate projection of planetary orbit tracks, Jovian magnetic dipole field lines, latitude/longitude graticules, radar ground tracks, and SAR beam footprints
- **`Surface`**:
  - Compositing Braille line glyphs onto color raster backgrounds
  - Direct cell manipulation (`get_mut`, `print_str`, `Style`, `Glyph`)
  - Embedding custom surfaces into the semantic UI via `gibson::ui::surface(Arc<Surface>)`
- **Skins & Themes**:
  - `skins::BLACK_ICE`, `skins::SWISS_SIGNAL`, `skins::VAPOR95` runtime switching
- **Context & Headless Pipeline**:
  - `Context::headless(RenderMode::Fullscreen, width, height)`
  - `context.set_color_depth(ColorDepth)`
  - Deterministic fixed-time captures (`--at-ms=`) and headless profiling

## What Worked Surprisingly Well

1. **Subcell Raster Performance**:
   With the published release build and 120×40 demo, measured frame construction/render averaged **0.69 ms** over 3,600 virtual frames on the AMD EPYC host described below. End-to-end interactive throughput, latency, and refresh rate were not measured.
2. **Surface Integration into Semantic UI**:
   The `gibson::ui::surface(Arc<Surface>)` element embeds application-drawn rasters alongside semantic controls. Galileo computes the available stage size from `BuildCx.environment`, then hands the sized buffer to the layout engine.
3. **Color Degradation Fidelity**:
   LibGibson's built-in capability pipeline translates TrueColor 24-bit RGB rasters into ANSI-256 and ANSI-16 with automatic palette quantization, while `RgbRaster::to_mono_surface()` allows clean, authored 1-bit monochrome fallback.
4. **Deterministic Frame Capture**:
   LibGibson's separation of `UiEnvironment`, explicit timestamps, and `Context::headless` allowed bit-exact reproducible captures within the tested host/environment and regression tests across all five views and required terminal dimensions.

## Friction Encountered

### Application-Specific Friction
- **Scene Continuity**: Adjacent scenes use an anchored aperture at wider sizes and an atomic change on compact screens. Treating inspector text as an indivisible part of the scene prevents partial labels during reveals. Direct view tabs switch immediately.
- **Terminal Cell Aspect Ratio**: Character cells are typically 1:2 (width:height). Spherical math requires vertical subcell pixel doubling ($pixel\_h = height \times 2$) to render circular discs.
- **Bounded Headless Capture**: `Context::rendered_bytes()` exposes every byte since the previous drain. Galileo initially re-parsed a growing buffer each frame, causing inflated byte counts and quadratic profiling cost. Calling `take_output()` after every frame corrected this application bug; the library API behaves as documented. Fullscreen contexts stream to stdout, so the interactive profiler reports timing without claiming byte statistics.

### Generic LibGibson Ergonomic Friction
- **No Fractional / Percentage Sizing on Semantic Elements**: While substrate `Node` supports `percent_width` and `percent_height` via Taffy, `Element<A>` only accepts absolute `u16` cells or flex `grow(f32)`. Creating proportional columns required manually querying `cx.environment.width` and calculating cell counts. (Filed as Issue #36).
- **Manual Braille Blitting onto Surface**: `BrailleCanvas` cannot be blitted directly onto a `Surface` with background style preservation via a high-level helper; users must iterate over glyphs manually. (Included in Galileo's umbrella Issue #43.)

### Proven LibGibson Defects
- **`Alt+<char>` Swallowed by `TextInputState`**:
  `TextInputState::handle_event` treats any character without `KeyModifiers::CONTROL` as printable text, causing `Alt+1` or `Alt+S` to type literal characters into input fields and set `consumed = true`, blocking outer application hotkeys. (Filed as Issue #35).
- **`MotionRole::Enter` Dissolves Keyed Elements on Frame 0**:
  Assigning `.key("name")` to an element unconditionally triggers an entrance transition (`MotionRole::Enter`). Under skins like `BLACK_ICE`, this resolves to `SurfaceFx::Dissolve` at fraction `0.0`, rendering all keyed elements completely invisible on the initial frame or in headless testing unless motion is explicitly disabled. (Filed as Issue #37).

## GitHub Issues Filed

The following issues were filed on `femboy2112/libgibson`:

| Issue # | Title | Link | Mechanism & Impact | Workaround in Galileo |
|---|---|---|---|---|
| **#35** | `[v0.2.0 consumer: Galileo] TextInputState and UiRuntime::handle_event unconditionally consume Alt+<char> key chords as text input` | [Issue #35](https://github.com/femboy2112/libgibson/issues/35) | `is_edit_event` checks for absence of `CONTROL` only, swallowing `Alt` hotkeys. | Intercept `KeyEvent` with `KeyModifiers::ALT` before delegating to `UiRuntime`. |
| **#36** | `[v0.2.0 consumer: Galileo] Element<A> lacks percent_width / percent_height layout builders available on Node` | [Issue #36](https://github.com/femboy2112/libgibson/issues/36) | `Element` layout struct only stores `Option<u16>` widths/heights, omitting Taffy percentage units. | Calculate cell dimensions from `BuildCx.environment.width / height`. |
| **#37** | `[v0.2.0 consumer: Galileo] UiRuntime unconditionally triggers MotionRole::Enter for all Key::Named elements, causing Dissolve to hide them on first frame` | [Issue #37](https://github.com/femboy2112/libgibson/issues/37) | Named keys automatically register `MotionRole::Enter`, which at $t=0$ dissolves the element to blank cells. | Set `env.motion = MotionPreference::None` for fixed captures; omit `.key()` from static buttons. |
| **#43** | `[v0.2.0 consumer: Galileo] Ergonomics & Architecture Report: Real-time multi-scale scientific visualization` | [Issue #43](https://github.com/femboy2112/libgibson/issues/43) | Comprehensive report detailing size-blind surface embedding, canvas compositing boilerplate, greedy input routing, and entrance motion traps. | Documents workarounds used in Galileo and proposes concrete API enhancements for v0.3.0. |

## Performance & Scale Observations

Measured from published code commit `9802eb4` (Git tree `a5c664f`) on Linux x86_64 (AMD EPYC 9V74, Rust 1.98.1 release build) with `--demo --headless --frames=3600 --fps=60 --width=120 --height=40 --profile`. Subsequent documentation-only edits do not change this binary:

| Metric | Measured Value | Note |
|---|---|---|
| **Headless frame construction/render** | 0.69 ms mean, 0.38 ms min, 3.25 ms max | Excludes profiling/parser work and terminal display |
| **Total wall time** | 2.86 s for 3,600 frames | Fast headless virtual-time playback, no real-time frame pacing |
| **Incremental ANSI output** | 5,686 bytes/frame mean, 19.52 MiB total | TrueColor 120×40 frames; output drained every frame |
| **Observed max RSS** | 8,448 KiB at 3,600 frames; 8,576 KiB at 18,000 | Unprofiled headless `resource.getrusage(RUSAGE_CHILDREN).ru_maxrss`; two workload samples, not a proved bound |
| **Integration suite** | 23 tests passing | Five views × required sizes; four color depths; deterministic captures, transitions, coupling, DSN units, styling, incremental profile bytes |

The demo samples simulated time at 60 frames/s; no interactive 60 FPS claim follows from this measurement. The previous frame-time, byte-volume, simulation-step, RSS, and interactive-VSync numbers were not independently reproducible on this build and are withdrawn. Alert/history collections have explicit size caps, and headless output is drained per frame; a process-wide RSS bound has not been established.

## Capability & Fallback Observations

- **TrueColor**: Hero visual experience with atmospheric gradient bands on Jupiter, subtle surface coloration on Europa, and radar dielectric return colormaps.
- **ANSI-256 & ANSI-16**: Correctly quantized by LibGibson's color down-sampler. All structural lines and text remain legible.
- **Monochrome (`--mono`)**: Authored grayscale surfaces and text readouts remain available; a terminal font with Braille coverage is needed for fine lines (the screenshot host lacks those glyphs).
- **Responsive Dimensions**:
  - `160x50`: Spacious layout with full 32-column telemetry rails, flight dynamics cards, and expanded logs.
  - `120x40`: Standard layout preserving visualization stage, key metrics, and radar diagnostics.
  - `100x30`: Compact layout collapsing secondary telemetry cards into status bars.
  - `80x24`: Minimum standard terminal preserving primary visualization, condensed scale tag, and command deck.
  - `60x20`: Minimal stage and abbreviated controls; scene transitions are atomic.

## What This Experiment Demonstrates

1. **Terminal-Native Procedural Visualization Is Viable**: Half-blocks and Braille lines communicate the relative geometry of rendered bodies, prescribed trajectories, and illustrative radar slices without graphic protocols.
2. **LibGibson v0.2.0 Can Host a Multi-View Application**: Semantic controls compose with low-level cell buffers, theme and capability fallbacks, focus routing, and fixed-time captures. Its API frictions and the application's local workarounds are recorded above.

## What Remains Unproven

1. **Arbitrary 3D Mesh Pipelines**: Project Galileo uses analytical ray/sphere projection and procedural equations rather than loading arbitrary 3D OBJ/glTF polygon meshes into `Scene`.
2. **Native Scrollback Integration**: While the command console maintains an internal circular history buffer, integration with host terminal scrollback buffers (`live_region`) was not evaluated.
3. **Complex Multi-Windowing**: The application uses a single fullscreen modal deck rather than tiled floating windows or draggable viewports.
4. **Physical Fidelity**: Prescribed moon orbits, a Keplerian spacecraft ellipse, first-order linear maneuver offsets, approximate encounter sampling, and procedurally generated radar reflectivity do not constitute n-body propagation, calibrated B-plane planning, instrument simulation, or evidence for subsurface water.
