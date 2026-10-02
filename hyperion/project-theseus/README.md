# Project Theseus // HumanMusic Quotient Machine

An interactive, high-tech terminal application built on LibGibson v0.4.0 that explores the "Ship of Theseus" musical paradox through procedural invariant decomposition and synthesis.

## Concept
Project Theseus acts as a cybernetic audio-visual machine that physically and perceptually disassembles the identity of a piece of music into an abstract quotient (`CoverMap`), allowing the user to strip away eight independent preservation dimensions. A fresh performance is then synthesized from **only** the surviving quotient in a target `MusicWorld` (Vapor95, Black Ice, or Swiss Signal).

## Visual Architecture
- **Reference DNA Tier**: Subcell `BrailleCanvas` piano roll plotting the original melodic trajectory.
- **Identity Quotient Tier (The Star)**:
  - **Recombinant Energy Reactor**: Procedural `HalfBlockCanvas` plasma field responding dynamically to disassembly entropy.
  - **Theseus Identity Index**: Real-time preservation gauge measuring what percentage of the original song survives.
  - **Cybernetic Conduits**: 8 glowing, pulsing channels connecting Source -> Quotient -> Cover that physically rupture and fracture into severed warning nodes when an axis is stripped.
- **Fresh Cover Tier**: Real-time synthesized score visualization and performance telemetry.
- **Lawful Refusal HUD**: High-voltage collision telemetry rendered natively when target world invariants reject impossible pins.

## Interactive Controls
| Key | Action |
|-----|--------|
| `1` | Toggle **Motif** (Melodic Contour) |
| `2` | Toggle **Riff** (Secondary Hooks) |
| `3` | Toggle **Groove** (Rhythmic Pocket) |
| `4` | Toggle **Harmonic Contour** (Harmonic Path) |
| `5` | Toggle **Harmonic Loop** (Cadence Turnaround) |
| `6` | Toggle **Form** (AABA Phrasing) |
| `7` | Toggle **Orchestration** (Voice Topology) |
| `8` | Toggle **Bass Figure** (Sub-bass Foundation) |
| `F` | Cycle **Fidelity Regime** (`Loose` → `Interpretive` → `Faithful` → `Strict`) |
| `W` | Cycle **Music World** (`VAPOR95` → `BLACK_ICE` → `SWISS_SIGNAL`) |
| `S` | Mutate **Deterministic Seed** |
| `D` | **WTF Moment**: Instant drop to single-axis threshold / Snap back |
| `R` | Reset All Pins to Full Identity |
| `Q` / `Esc` | Clean Quit and Terminal Restoration |

## Running the App
```bash
cargo run --bin project-theseus
```

## Running the Tests
```bash
cargo test
```
Runs the 7 adversarial structural proofs and the real virtual PTY stress harness.
