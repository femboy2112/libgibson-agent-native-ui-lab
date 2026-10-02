# Architecture — Semantic Audio Inversion

The inverse analyzer is a staged evidence pipeline, not one opaque model:

```
PCM
 └─ canonical boundary (ffmpeg → mono 22.05 kHz f32le, two hashes)
     └─ AcousticObservation        (chroma, onsets, raw spectra)
         └─ DerivedMusicalEvent    (beats, notes, key/chord candidates)
             └─ DerivedStructural  (recurrence, sections)
                 └─ SemanticQuotient Q_f = (motif, harmony, groove, form, orchestration)
```

Every layer is labelled. The coarsest honest level for the inverse map is **acoustic
observation** of PCM; anything a pitch/beat/tonal estimator emits is a *derived* musical
event. Nothing is ever promoted to unmediated source fact.

## Two halves, deliberately separated

| half | language | responsibility | may depend on ML? |
| --- | --- | --- | --- |
| `adapters/` | Python | decode, DSP measurement, external-model adapters, emit evidence | yes (optional) |
| `crates/sai-core` | Rust | typed evidence IR, transport, motif, harmony, groove, form, quotient, metrics, truth | **no** |
| `crates/sai-harness` | Rust | blind dataset + mutation generator; depends on LibGibson anchor | no |

`sai-core` consumes evidence JSON and never embeds a model runtime, so ordinary `cargo test`
stays cheap. `sai-harness` is the only crate that links the reviewed LibGibson revision
(`c2f6483…`), and it is the *generator*, never the analyzer.

## Data flow

```
sai_gen dataset ─┬─► wav/*.wav          (analyzer sees this)
                 ├─► truth/*.json       (held outside the analyzer)
                 └─► manifest.json      (sai.dataset/v1)

adapters/run_analysis.py
  canonicalize.py ──► CanonicalAudio (source sha256 + canonical-PCM sha256)
  baseline_dsp.py ──► sai.evidence/v1  +  sai.receipt/v1

sai recover  --evidence E --profile P --out Q      (sai.quotient/v1)
sai evaluate --quotient Q --truth T --out M        (sai.metrics/v1)
```

## Schemas (frozen for this round)

| schema | defined in | producer | consumer |
| --- | --- | --- | --- |
| `sai.evidence/v1` | `crates/sai-core/src/evidence.rs` | Python adapters | `sai recover` |
| `sai.receipt/v1` | `crates/sai-core/src/receipt.rs` | Python adapters | audit / replay |
| `sai.quotient/v1` | `crates/sai-core/src/quotient.rs` | `sai recover` | `sai evaluate` |
| `sai.truth/v1` | `crates/sai-core/src/truth.rs` | harness (hidden) | `sai evaluate` |
| `sai.metrics/v1` | `crates/sai-core/src/metrics.rs` | `sai evaluate` | report |
| `sai.dataset/v1` | `crates/sai-harness/src/dataset.rs` | `sai_gen dataset` | driver |
| `sai.mutations/v1` | `crates/sai-harness/src/mutations.rs` | `sai_gen mutations` | driver |

The Python builder (`adapters/sai_evidence.py`) mirrors the Rust field names explicitly, so the
wire format cannot drift silently.

## Module map (`sai-core`)

- `error.rs` — `finite`/range guards; invalid observations fail closed.
- `evidence.rs` — neutral IR, `EvidenceLevel`, `Provenance`, `Refusal`, `Unknown`.
- `receipt.rs` — `sha256_file`/`sha256_hex`; source vs canonical hashes kept distinct.
- `transport.rs` — `BeatGrid`: piecewise-linear seconds↔beat, raw seconds authoritative.
- `motif.rs` — explicit-transform motif identity (contour, direction, proportional rhythm,
  normalized onsets); declared tolerances; `Free < Theme < Metric < Faithful`.
- `harmony.rs` — two independent routes (chroma template vs note window); `reconcile` keeps
  rivals and lowers the ceiling; `Free < Ordered < QualityFamily < Exact`.
- `groove.rs` — kick/snare strokes with the quarter-note skeleton kept distinct; family
  candidates are never asserted as instrument identity.
- `form.rs` — novelty boundaries + recurrence families; `Free < Topology < Exact`.
- `quotient.rs` — `recover(evidence, profile) -> RecoveredQuotient`; per-axis requested vs
  effective relation, evidence families, lowering reason, unknown/free distinction.
- `metrics.rs` — per-axis residual, no single accuracy number.
- `truth.rs` — neutral hidden ground truth.

## Reproducible CLI

```
# inference mode (Python)
python3 scripts/make_fixtures.py
python3 scripts/calibrate.py --out /tmp/calibration
python3 scripts/run_experiment.py --dataset <dir> --out <dir> --split dev --profiles interpretive,faithful

# replay mode (Rust, cached evidence only)
cargo build -p sai-core --bin sai
target/debug/sai validate cached/evidence/<x>.evidence.json
target/debug/sai recover  --evidence cached/evidence/<x>.evidence.json --profile interpretive
```
