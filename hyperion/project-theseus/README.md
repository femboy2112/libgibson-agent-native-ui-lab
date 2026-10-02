# Project Theseus

An interactive terminal application built on LibGibson v0.4.0 that explores the "Ship of Theseus" paradox applied to music.

## Overview
Project Theseus acts as a visual and acoustic machine that disassembles a piece of music into an "identity quotient" (`CoverMap`), allowing the user to progressively strip away preservation dimensions (Motif, Harmony, Groove, etc.). It then attempts to generate a fresh performance using **only** the surviving quotient in a target `MusicWorld`.

The core experience revolves around the "WTF Moment": as axes are removed, the music fundamentally changes until it's "new music". Restoring an axis snaps the recognizability back.

## Usage
Run the application interactively:
```bash
cargo run --bin project-theseus
```

### Controls
- **1-8**: Toggle preservation axes (Motif, Riff, Groove, HarmonicContour, HarmonicLoop, Form, Orchestration, BassFigure).
- **f**: Cycle fidelity presets (Loose, Interpretive, Faithful, Strict).
- **w**: Cycle target world (Vapor95, Black Ice, Swiss Signal).
- **q / Esc**: Quit.

## Design
- **Strict Separation**: The source `ReferenceSong` is completely isolated from the `cover` generation step. 
- **Lawful Refusal**: When the target world rejects constraints (e.g., Strict fidelity in an incompatible world), the typed refusal (`CoverAdmission`) is visualized natively in the UI.

