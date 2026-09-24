# Public substrate orientation for fresh consumers

This neutral guide is a directory of public contracts, not a list of proposed
applications. Substrate: LibGibson `e5ede0a4ab8ff52caa567ee10d45824e42c1ebc6`.
Rust 1.98.1. The local checkout is `/home/leah/LibGibson`; its current source
is identical to that revision (later changes are documentation only).

- `src/lib.rs`: exports and crate documentation.
- `src/node.rs`, `layout.rs`, `painter.rs`: Node row/column/stack, rich text,
  borders, explicit dimensions, viewport, Surface-backed nodes and realization.
- `src/surface.rs`, `cell.rs`, `style.rs`: cell framebuffer with glyph/style,
  safe wide-glyph mutation, clipping and ordinary compositor semantics.
- `src/focus.rs`, `input.rs`: stable numeric FocusId, FocusRing membership and
  keyboard traversal; typed input events. Domain actions remain consumer-owned.
- `src/canvas.rs`: BrailleCanvas at 2×4 samples/cell, HalfBlockCanvas at 1×2 RGB.
- `src/raster.rs`, `raster_fx.rs`, `raster3d.rs`: bounded RGB raster, software
  fields/effects/feedback, triangle depth rasterization and cameras.
- `src/geom.rs`: vectors, projection helpers, CubicPath3.
- `src/scene.rs`, `story.rs`, `surface_fx.rs`: experimental deterministic
  composition, timeline/story control and cell-level effects.
- `src/context.rs`, `session.rs`: terminal ownership, rendering and input.
- `src/diff.rs`, `ansi.rs`: exact changed cells vs affected footprint vs emitted
  bytes. Identical Surfaces must produce zero for all three.

Consumer may combine high-level Nodes with lower-level Surfaces/canvases.
No private-module access, direct terminal ANSI, generated native code,
network/model requirement, image protocol or core patch. Resources must be
bounded. Demonstrate keyboard interaction, deterministic replay, responsive
56×24 / 80×24 / 120×32 / 160×40, TrueColor and deliberate Mono.

For independent implementation, the frozen lab `ui::App` adapter owns runtime
I/O; its pure `frame` and explicit `key` methods are sufficient to launch a
consumer. Consumer-specific data/model/types are unrestricted; frozen helper
or provisional schema extension is prohibited. A limitation is an admissible
research result. This guide intentionally does not prescribe widgets or ideas.
