# Semantic Audio Inversion (experiment package)

Independent consumer experiment for the research program in
[`research/audio-semantic-inversion/`](../../research/audio-semantic-inversion/README.md):
recover a provenance-bearing semantic musical quotient from raw audio and test whether it agrees
with hidden HumanMusic ground truth.

This is **not** a member of the root lab package. It has its own Cargo workspace, its own lockfile
and its own `.gitignore`, and it does not modify the frozen root campaign or the LibGibson
upstream repository.

## Layout

| path | contents |
| --- | --- |
| `crates/sai-core` | typed evidence IR, transport, motif/harmony/groove/form, quotient, metrics, truth, CLI `sai` |
| `crates/sai-harness` | blind dataset + mutation generator (links LibGibson `c2f6483…`), CLI `sai_gen` |
| `adapters/` | canonical audio boundary and inspectable DSP baseline; evidence builders; external-model refusal gate |
| `scripts/` | fixtures, calibration, dev driver, mutation checker, holdout seal, cached-evidence builder |
| `fixtures/` | 8 tiny generated WAV controls |
| `cached/` | committed evidence + receipts used by the replay tests |
| `holdout/` | sealed holdout manifest (not analyzed) |
| `docs/` | architecture, frozen thresholds, claim ledger |
| `REPORT.md` | round-1 final report (14 sections) |

## Quickstart

```bash
# Rust core (cheap; no ML runtime, no audio)
cargo +1.98.1 test -p sai-core          # 40 tests
cargo +1.98.1 test -p sai-harness       # 4 tests
cargo +1.98.1 build -p sai-core --bin sai

# Calibration on known cases
python3 scripts/make_fixtures.py
python3 scripts/calibrate.py --out /tmp/calibration

# Blind development round (inference mode)
cargo +1.98.1 run -q -p sai-harness --bin sai_gen -- \
    dataset --out /tmp/sai/dev --split dev --per-kind 1 --total-beats 32
python3 scripts/run_experiment.py --dataset /tmp/sai/dev --out /tmp/sai/dev-run \
    --split dev --profiles interpretive,faithful

# Tests (replay over cached evidence + DSP invariants + calibration)
python3 -m unittest discover -s tests -v
```

## Epistemic rules honoured

- Nothing a model or DSP emits is promoted above *derived* evidence; the coarsest honest level is
  *acoustic observation*.
- Every axis reports requested vs effective relation, the lowering reason, and an explicit
  unknown/free distinction; a stronger requested preset never fills missing evidence.
- External models are optional adapters behind a refusal gate; a model that cannot run is an
  access/compute gap, never evidence against the hypothesis.
- Raw seconds are authoritative; beat coordinates are a hypothesis and rival tempo/beat/phase
  hypotheses are preserved in `timing.ambiguity`.
- The holdout was sealed before contact and remains unanalyzed.

See [`REPORT.md`](REPORT.md) and [`docs/CLAIM_LEDGER.md`](docs/CLAIM_LEDGER.md) for the round-1
verdict.
