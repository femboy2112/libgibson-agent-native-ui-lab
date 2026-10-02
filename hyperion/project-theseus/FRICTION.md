# Friction Ledger

## [GENERIC ERGONOMIC GAP] Crate Name Mismatch
**Mechanism**: The dependency is declared as `libgibson = { git = "..." }` in `Cargo.toml`, but the rust crate name is actually `gibson`.
**Impact**: Causes initial compilation errors (`unresolved import libgibson`). It requires discovering that `name = "gibson"` is set under `[lib]` in the upstream `Cargo.toml`.

## [GENERIC ERGONOMIC GAP] Private Module Public Re-exports
**Mechanism**: `cover_fidelity.rs` is declared as a private module `mod fidelity` inside `cover.rs`, but its contents are publicly re-exported using `pub use fidelity::{...}`.
**Impact**: When trying to import `CoverFidelityPreset`, rust-analyzer or compiler errors might suggest it lives in `cover::fidelity`, but importing from there yields a "module `fidelity` is private" error. Consumers must realize they are re-exported directly in the `cover` module.

## [DELIBERATE SAFETY BOUNDARY] Semantic UI Styling
**Mechanism**: The UI framework intentionally omits `fg(Color)` methods on `Element` in favor of semantic `tone(Tone)` and `emphasis(Emphasis)` methods.
**Impact**: Forces consumers to map their visual intentions into the framework's semantic palette (`Accent`, `Danger`, `Success`). This prevents "visually fucking insane" direct RGB manipulation on standard text, keeping the application unified with the selected `MusicWorld` skin (like VAPOR95).

## [EXPRESSIVE WALL] BrailleCanvas / RgbRaster Integration
**Mechanism**: Drawing a rich `CoverMap` requires using `BrailleCanvas` or `RgbRaster`. While `to_rich_text` and `NodeKind::RichText` exist, mapping the precise `CoverNote` events to the visual grid coordinates requires manual Bresenham rasterization logic in the consumer app.
**Impact**: The consumer bears the burden of building the visualizer logic from the IR. (This is arguably correct for an API, but represents an expressive wall for rapid UI construction).
