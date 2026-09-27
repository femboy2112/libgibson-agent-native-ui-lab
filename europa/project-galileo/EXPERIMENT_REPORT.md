# Project Galileo — LibGibson v0.2.0 external stress test

## Goal

The objective of Project Galileo was to test whether the released LibGibson v0.2.0 API could support an absurdly ambitious, pure-terminal scientific instrument: a real-time Jovian mission operations and scientific visualization environment.

The core challenge was the "impossible" multi-scale visual continuum: enabling an operator to seamlessly transition from viewing the entire Jovian system ($\sim 2{,}500{,}000\text{ km}$ across Jupiter and the Galilean moons), into Europa's hyperbolic encounter orbit ($\sim 50{,}000\text{ km}$), down into regional surface cycloid fractures ($\sim 500\text{ km}$), into an ice shell cross-section ($\sim 25\text{ km}$), and finally down to high-resolution subsurface ocean radar tomography ($\sim 5\text{ km}$) — all rendered through native terminal cells without Sixel, Kitty, or external GUI embedding.

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
  - State reconciliation across 60 FPS continuous updates
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
  - Deterministic fixed-time captures (`--at-ms=`) and PTY headless benchmarks

## What Worked Surprisingly Well

1. **Subcell Raster Performance**:
   LibGibson's `RgbRaster` and `BrailleCanvas` render with exceptional speed. Generating a 3D shaded Jovian sphere, Keplerian orbits, magnetic dipole curves, and a full semantic dashboard takes approximately **1.11 ms per frame** in headless release mode, comfortably exceeding 900 FPS.
2. **Surface Integration into Semantic UI**:
   The `gibson::ui::surface(Arc<Surface>)` element makes integrating custom rasterizers into a Flexbox-based semantic UI remarkably clean. The layout engine computes dimensions, and the rasterizer fills the exact allotted space.
3. **Color Degradation Fidelity**:
   LibGibson's built-in capability pipeline translates TrueColor 24-bit RGB rasters into ANSI-256 and ANSI-16 with automatic palette quantization, while `RgbRaster::to_mono_surface()` allows clean, authored 1-bit monochrome fallback.
4. **Deterministic Frame Capture**:
   LibGibson's separation of `UiEnvironment`, explicit timestamps, and `Context::headless` allowed 100% bit-exact reproducible frame captures (`--at-ms=`) and verifiable regression testing.

## Friction Encountered

### Application-Specific Friction
- **Scale Continuity Tuning**: Harmonizing discrete view boundaries with continuous camera distance required careful threshold hysteresis to prevent camera flickering during zoom transitions.
- **Terminal Cell Aspect Ratio**: Character cells are typically 1:2 (width:height). Spherical math requires vertical subcell pixel doubling ($pixel\_h = height \times 2$) to render circular discs.

### Generic LibGibson Ergonomic Friction
- **No Fractional / Percentage Sizing on Semantic Elements**: While substrate `Node` supports `percent_width` and `percent_height` via Taffy, `Element<A>` only accepts absolute `u16` cells or flex `grow(f32)`. Creating proportional columns required manually querying `cx.environment.width` and calculating cell counts. (Filed as Issue #36).
- **Manual Braille Blitting onto Surface**: `BrailleCanvas` cannot be blitted directly onto a `Surface` with background style preservation via a high-level helper; users must iterate over glyphs manually. (Reported in related lab issue #38).

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

Measured on Linux x86_64 (`AMD Ryzen / Ubuntu 24.04`), release build:

| Metric | Measured Value | Note |
|---|---|---|
| **Headless Frame Time** | $1.11\text{ ms}$ | $>900\text{ FPS}$ sustained throughput |
| **Interactive Frame Time** | $< 1.5\text{ ms}$ | VSync capped at 60 FPS (90% idle time) |
| **Rendered ANSI Byte Stream** | $4.2\text{ KB} - 5.1\text{ KB}$ | Full TrueColor $120\times 40$ screen |
| **Simulation Step Cost** | $< 15\text{ }\mu\text{s}$ | Coupled orbital + telemetry physics |
| **Resident Set Size (RSS)** | $18.4\text{ MB}$ | Strictly bounded, no history leakage |
| **Integration Test Suite** | $0.08\text{ s}$ | 13 integration tests in release mode |

## Capability & Fallback Observations

- **TrueColor**: Hero visual experience with atmospheric gradient bands on Jupiter, subtle surface coloration on Europa, and radar dielectric return colormaps.
- **ANSI-256 & ANSI-16**: Correctly quantized by LibGibson's color down-sampler. All structural lines and text remain legible.
- **Monochrome (`--mono`)**: Fully legible scientific presentation using `RgbRaster::to_mono_surface()`, bold/dim typography, and Braille linework.
- **Responsive Dimensions**:
  - `160x50`: Spacious layout with full 32-column telemetry rails, flight dynamics cards, and expanded logs.
  - `120x40`: Standard layout preserving visualization stage, key metrics, and radar diagnostics.
  - `100x30`: Compact layout collapsing secondary telemetry cards into status bars.
  - `80x24`: Minimum standard terminal preserving primary visualization, condensed scale tag, and command deck.
  - `60x20`: Ultra-compact emergency mode collapsing header to 1 row and footer to 1 row.

## What This Experiment Demonstrates

1. **Terminal-Native Scientific Visualization is Viable**: The combination of subcell half-blocks and Braille vector lines provides sufficient spatial density to represent 3D planetary systems, orbital mechanics, and geophysical radar cross-sections without external graphic protocols.
2. **LibGibson v0.2.0 is Capable of Real Host Applications**: The semantic UI layer, when paired with low-level cell buffers, enables professional-grade, multi-view applications with clean code structure and rapid render performance.

## What Remains Unproven

1. **Arbitrary 3D Mesh Pipelines**: Project Galileo uses analytical ray/sphere projection and procedural equations rather than loading arbitrary 3D OBJ/glTF polygon meshes into `Scene`.
2. **Native Scrollback Integration**: While the command console maintains an internal circular history buffer, integration with host terminal scrollback buffers (`live_region`) was not evaluated.
3. **Complex Multi-Windowing**: The application uses a single fullscreen modal deck rather than tiled floating windows or draggable viewports.
