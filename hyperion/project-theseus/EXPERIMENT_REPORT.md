# Experiment Report: Project Theseus (LibGibson v0.4.0)

## Executive Summary
Project Theseus explores the "Ship of Theseus" musical paradox using LibGibson v0.4.0's `HumanMusic` Cover Mode and `CoverMap` quotient pipeline. The application acts as a cybernetic visual machine that physically and acoustically disassembles the identity of a piece of music across eight preservation dimensions and four fidelity regimes, synthesizing new performances exclusively from the surviving invariant quotient.

---

## 1. Visual Machine Architecture
The terminal interface is structured into three continuous visual tiers connected by dynamic conduits:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. REFERENCE PERFORMANCE (SOURCE DNA)                                       │
│    - Ode to Joy (Beethoven) monophonic lead trajectory                      │
│    - Rendered via Subcell BrailleCanvas piano roll                          │
│    - Invariant observation: 4/4 metric, D Major, 16 bars                   │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼ (Conduits: Pulsing / Severed)
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. THE IDENTITY QUOTIENT // COVERMAP (THE MIDDLE STAR)                      │
│    - Recombinant Energy Reactor: Procedural HalfBlockCanvas plasma field   │
│      modulated by disassembly entropy (turbulence increases as pins drop)   │
│    - Ship of Theseus Gauge: Dynamic % identity preservation meter           │
│    - 8 Recombinant Channels:                                                │
│      [1] MOTIF      [2] RIFF       [3] GROOVE     [4] H-CONTOUR             │
│      [5] H-LOOP     [6] FORM       [7] ORCHESTRA  [8] BASS                  │
│    - Preserved axes display traveling pulse waves (◈);                       │
│      Stripped axes rupture into severed spark fractures (⚡ ░░ [PURGED] ░░ ⚡) │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼ (Invariant Injection Only)
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. FRESH COVER SYNTHESIS                                                    │
│    - Reconstructed score generated strictly from the CoverMap quotient      │
│    - Realized in target world: VAPOR95, BLACK_ICE, or SWISS_SIGNAL          │
│    - Reconstructed Braille piano roll + performance receipt metrics         │
│    - Lawful Refusal HUD: Intercepts and visualizes target world collisions  │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. The WTF Moment: Ship of Theseus Applied to Song
By pressing `[D]`, the engine triggers the WTF threshold:
1. Seven out of eight axes are simultaneously severed, leaving only one isolated dimension (e.g. Motif contour with zero rhythm, harmony, or form).
2. The fresh performance perceptually disintegrates into an alien, unrecognizable composition ("New Music").
3. Toggling `[1]` drops the final anchor, completely evaporating classical identity (0% Theseus Index).
4. Restoring `[1]` causes recognizability to instantly snap back, demonstrating the minimal quotient necessary for human musical identity recognition.

---

## 3. Lawful Refusal as a First-Class Visual Event
LibGibson's `cover()` API enforces invariant laws under `PerformanceProfile::BAND`. When a target world cannot sound requested pins (for instance, requesting `Strict` fidelity in `SWISS_SIGNAL` where harmonic vocabulary or tempo constraints reject the source intervals):
- The engine does **not** catch and hide errors in a dialog box.
- It renders a high-voltage warning collision barrier:
  `⚡ LAWFUL REFUSAL: TARGET WORLD REJECTED IDENTITY CONSTRAINTS ⚡`
- The exact typed check failures from `CoverAdmission::conformance::checks` are displayed inline with involved axes highlighted.

---

## 4. Adversarial Test Suite Validation
All seven external verification properties pass cleanly in `tests/adversarial.rs`:
1. **Data Isolation (`test_data_isolation`)**: Proves that the source `ReferenceSong` is detached after extraction; `cover()` only receives the `CoverMap` and `CoverTarget`.
2. **Non-Identity Mutation (`test_non_identity_mutation`)**: Modifying non-identity note durations in the source does not mutate invariant metric lengths or preserved structures.
3. **Determinism (`test_determinism`)**: Fixed fixture + fixed seed + fixed world produces identical output scores note-for-note.
4. **Refusal Stability (`test_refusal_behavior`)**: Incompatible world/preset combinations deterministically emit `CoverError::Rejected(admission)` with populated check traces.
5. **Fidelity Nesting (`test_fidelity_nesting`)**: `Strict` fidelity profile preserves at least as many axes as `Loose`.
6. **Lane Independence (`test_lane_sharing`)**: Multi-register voices on a single lane extract distinctly without collapsing into a single identity.
7. **Capability Degradation (`test_capability_degradation`)**: The quotient architecture and conduit brackets preserve readability across TrueColor, ANSI256, ANSI16, and Mono modes.

---

## 5. PTY Smoke / Stress Harness
`tests/pty_smoke.rs` uses `portable-pty` to simulate a real terminal session:
- Spawns the binary under virtual terminal dimensions (80x24).
- Injects rapid VT100 keystrokes (`ffw1234r`).
- Exercises dynamic PTY resize to 120x40.
- Reads and validates terminal buffer captures.
- Executes clean quit (`q`) and verifies clean process exit and terminal restoration.
