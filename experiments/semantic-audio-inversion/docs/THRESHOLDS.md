# Frozen thresholds, relations and ceilings (first round)

Frozen **before** the holdout was opened. Recorded so later rounds cannot quietly change the
question after seeing failures. Values live in code; this file is the human-readable ledger.

## Fidelity relations (declared, never fitted)

| axis | order (weakest → strongest) |
| --- | --- |
| motif / line | `Free < Theme < Metric < Faithful` |
| harmony | `Free < Ordered < QualityFamily < Exact` |
| groove | `Free < PocketSkeleton < KickSnare` |
| form | `Free < Topology < Exact` |
| orchestration | `Free < Exact` |

## Motif tolerances (`crates/sai-core/src/motif.rs`)

| constant | value | meaning |
| --- | --- | --- |
| `PITCH_TOL_SEMITONES` | `0.5` | interval equality bound |
| `ONSET_TOL_FRACTION` | `0.04` | normalized-onset equality bound |
| `RHYTHM_TOL` | `0.12` | proportional-rhythm equality bound |
| `FAMILY_SIMILARITY` | `0.80` | family clustering bound |

## Ceilings declared for audio (frozen policy)

- **motif**: ceiling is `Metric`, **never** `Faithful`, for audio.
- **groove**: `KickSnare` iff ≥2 strokes, both kick and snare present, and every stroke within
  `0.03` of the 1/16 grid; otherwise `PocketSkeleton`.
- **form**: ceiling is `Topology` (no bar/phrase spans from audio).
- **orchestration**: always `Unknown` this round (role/orchestration is not recovered from audio).

## Evaluation tolerances (`crates/sai-core/src/metrics.rs`)

| constant | value |
| --- | --- |
| `NOTE_ONSET_TOL_BEATS` | `0.12` |
| `NOTE_PITCH_TOL_SEMITONES` | `0.75` |

Form recurrence membership tolerance `RECURRENCE_TOL = 0.70`; groove quarter-note grid tolerance
`0.03`.

## Canonical audio boundary

- target: **mono, 22050 Hz, f32le**;
- decoder: `ffmpeg` (version recorded), mono downmix, resample;
- two hashes retained and kept distinct: **source file sha256** and **canonical PCM sha256**.

## Requested profiles

`loose | interpretive | faithful | strict | free`. In `faithful`, form is requested `Exact` and the
projector lowers it to `Topology`; in `interpretive`, form is left `Free`. No missing measurement
is filled merely because a stronger profile was requested.

## Analyzer configuration

`DspConfig` defaults (recorded verbatim in every artifact's `analyzers[].config`): onset
1024/256 with adaptive delta `0.35`; tempo prior 120 BPM (±0.60 octave); chroma 4096/1024 over
55–2093 Hz; pitch 55–1200 Hz; note window 2048, min confidence `0.30`, max duration `1.5 s`;
section kernel 8 beats; recurrence floor `0.85`.

## Holdout

Generated under a distinct seed base and committed as a manifest + hash commitment only. The
development analyzer has not decoded those WAVs. Analysis is deferred until after freeze (already
the case) and is **not** part of this round.
