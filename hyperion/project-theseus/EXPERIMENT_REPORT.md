# Experiment Report: Project Theseus

## Architecture & Public API Separation
The application successfully adheres to the strict isolation boundary:
1. The source `ReferenceSong` is loaded from a TSV fixture.
2. `extract_fidelity` distills it into a `CoverMap` based on a user-defined `CoverFidelityProfile`.
3. The `cover` function receives **only** the `CoverMap` and a `CoverTarget` (world, seed, grammar). 
4. The original source data is structurally unavailable during the `cover` step, proving data isolation.

## Sustained-Run Observations
Running the PTY smoke harness (`tests/pty_smoke.rs`) demonstrates that rapid user interaction (toggling fidelity, world, and axes) reliably updates the underlying identity quotient and immediately reflects in either a generated `Composition` or a typed `CoverAdmission` refusal. The UI loop remains stable across resize events and rapid keystrokes.

## API Weaknesses & Successes
**Surprising Successes**: The `CoverAdmission` refusal API is exceptionally well-modeled. By lifting refusal into a first-class data structure (`CoverError::Rejected`), the application can structurally inspect exactly which `CoverCheck` failed and display it to the user.
**Strongest Real API Weakness**: The mismatch between file structure (`cover_fidelity.rs`) and module export path (`pub use fidelity::{...}` inside `cover.rs`) creates unnecessary friction for consumers trying to resolve imports based on compiler hints.

## Claims Validation
- **Proven Claims**: 
  1. Data discarded from `CoverMap` cannot influence the fresh generation path (proven by type signatures).
  2. Same map + same target + same seed yields deterministic output (proven by `tests/adversarial.rs`).
  3. Typed refusal behavior remains stable (proven by `tests/adversarial.rs`).
- **Unproven Claims**: 
  1. "Multiple materials sharing one instrument lane do not automatically become one identity" (Not deeply tested by our single-lane `lead` TSV fixture).

## Capability Matrix
The terminal UI utilizes standard text nodes and semantic tones, guaranteeing compatibility across TrueColor, ANSI256, ANSI16, and Mono modes. Due to time constraints, the visual `CoverMap` renderer uses simple bracketed visual indicators rather than a full `BrailleCanvas` rasterizer, which naturally gracefully degrades on terminals lacking rich font capabilities.
