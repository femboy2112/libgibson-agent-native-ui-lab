# Semantic Audio Inversion — Round 1 Report

Date: 2026-10-02. Branch: `research/audio-semantic-inversion`.
This report separates **observed** results from **hypotheses** and reports per-axis residuals,
not one accuracy number.

---

## 1. Repository HEAD and anchor

- Lab repo HEAD at report time: `9cce0eea8b19e76ff0532419d1b9154395ea6a48`
  (branch `research/audio-semantic-inversion`, clean except the new `experiments/` tree).
- LibGibson anchor consumed (public API, read-only, not modified):
  `femboy2112/libgibson@c2f6483d92fe2b351e6cd50936a97d8cdf73cb79` (the v0.4.0 HumanMusic &
  Audio release merge).
- **Observation, not a defect:** git tag `v0.4.0` resolves to
  `38ba12d240344de1909e4ca4b39c6387df6f1291`, which differs from the merge commit anchor
  `c2f6483…`. The experiment pins the *merge* anchor, as the research README names it.
- Frozen root campaign untouched: `python3 scripts/verify_freeze.py` → **PASS** (pinned
  substrate + 21 frozen files unchanged); root `cargo +1.98.1 test --locked` → **61 passed**.
- The experiment is its own Cargo workspace (`experiments/semantic-audio-inversion/Cargo.toml`,
  own `Cargo.lock`, own `.gitignore`). Only `sai-harness` links LibGibson.

## 2. Architecture

A staged evidence pipeline, not one opaque model:

```
PCM → canonical boundary → AcousticObservation → DerivedMusicalEvent → DerivedStructural → SemanticQuotient
```

- **Python adapters** do decode and measurement and emit versioned evidence; an external model is
  a replaceable adapter behind a refusal gate.
- **`sai-core` (Rust)** owns the typed evidence IR, transport, motif/harmony/groove/form inference,
  quotient projection and per-axis metrics. It has **no ML runtime dependency**, so ordinary
  `cargo test` is cheap.
- **`sai-harness` (Rust)** is the blind generator: it builds hidden `Score`s from HumanMusic
  traces, renders PCM, writes the truth *outside* the analyzer's view, and applies the mutation
  suite. It is the only crate linked to the LibGibson anchor.
- Two execution modes: **inference** (Python, run adapters, cache evidence) and **replay**
  (Rust, consume cached evidence — the committed `cached/` corpus; fast, no audio, no models).

Details: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## 3. Files and layout

```
experiments/semantic-audio-inversion/
  Cargo.toml, Cargo.lock, .gitignore
  crates/sai-core/        typed evidence IR, transport, motif, harmony, groove, form,
                          quotient, metrics, truth, error, receipt; bin/sai.rs
  crates/sai-harness/     dataset.rs, mutations.rs, bin/sai_gen.rs (LibGibson anchor dep)
  adapters/               canonicalize.py, baseline_dsp.py, sai_evidence.py,
                          run_analysis.py, external_adapters.py
  scripts/                make_fixtures.py, calibrate.py, run_experiment.py,
                          run_mutations.py, seal_holdout.py, cache_evidence.py
  fixtures/               8 tiny generated WAV controls (committed)
  cached/                 committed evidence + receipts for the fixtures (replay corpus)
  holdout/                manifest.json + SEAL.json (holdout not analyzed)
  docs/                   ARCHITECTURE.md, THRESHOLDS.md, CLAIM_LEDGER.md
  tests/                  test_pipeline.py (9 tests)
  REPORT.md               this report
```

No copyrighted full recording is committed; fixtures are synthetically generated.

## 4. Tools executed (exact)

```
# environment (Round 0)
rustc 1.98.1 (48a229cea 2026-09-01); cargo 1.98.1; Python 3.12.3; ffmpeg 6.1.1
numpy 2.5.0; scipy 1.18.0; no torch/librosa/soundfile/basic-pitch/essentia/madmom; no GPU

# frozen root campaign (unchanged)
python3 scripts/verify_freeze.py                         # PASS
cargo +1.98.1 test --locked                             # 61 passed

# experiment core
cargo +1.98.1 test -p sai-core                          # 40 passed
cargo +1.98.1 test -p sai-harness                       # 4 passed
cargo +1.98.1 run -q -p sai-core --bin sai -- schema   # 5 schemas

# calibration
python3 scripts/make_fixtures.py
python3 scripts/calibrate.py --out /tmp/opencode/sai/calibration

# blind dev round
cargo run -q -p sai-harness --bin sai_gen -- dataset --out /tmp/opencode/sai/dev \
    --split dev --per-kind 1 --total-beats 32
python3 scripts/run_experiment.py --dataset /tmp/opencode/sai/dev \
    --out /tmp/opencode/sai/dev-run --split dev --profiles interpretive,faithful

# hostile transformations
cargo run -q -p sai-harness --bin sai_gen -- mutations \
    --dev /tmp/opencode/sai/dev --out /tmp/opencode/sai/mutations --limit 5
python3 scripts/run_mutations.py --mutations /tmp/opencode/sai/mutations \
    --base-run /tmp/opencode/sai/dev-run --out /tmp/opencode/sai/mutations-run

# holdout seal
cargo run -q -p sai-harness --bin sai_gen -- dataset --out /tmp/opencode/sai/holdout \
    --split holdout --per-kind 2 --seed-base 50000 --total-beats 32
python3 scripts/seal_holdout.py --dataset /tmp/opencode/sai/holdout --out holdout

# replay corpus + tests
python3 scripts/cache_evidence.py --fixtures fixtures --out cached
python3 -m unittest discover -s tests -v                # 9 passed
```

## 5. Calibration results (known cases, before any song)

8/8 controls pass (`scripts/calibrate.py`). This is the Instrument Rule: the readings on unknown
audio are only as trustworthy as these known-case readings.

| control | class | result |
| --- | --- | --- |
| `positive_tone_a4.wav` | positive | top chroma pc = A(9); pitch ≈ 69.06; key tonic A |
| `positive_triad_c_maj.wav` | positive | top-3 chroma pcs = {0,4,7}; ≥2 triad notes recovered |
| `positive_click_120.wav` | positive | tempo 120.054 BPM; 15 beats; median 0.5 s-grid error **0.040** |
| `positive_kicksnare_120.wav` | positive | kick 8, snare 16, hat 11; 23 kick/snare strokes; tempo 120.050 |
| `null_silence.wav` | null | 0 onsets, no tempo, 0 notes (no invention) |
| `negative_white_noise.wav` | negative | no confident key (top conf 0.697) |
| `negative_sweep.wav` | negative | decodes; key not near-certain (0.682) |
| `mutation_triad_d_maj.wav` | mutation | top-3 pcs = {2,6,9} (relative structure survives, roots shift) |

## 6. Blind development reconstruction, per axis

15 dev items (3 worlds × 5 trace kinds), 32 beats, profile `interpretive`. `notes.all`
precision/recall are shown for the profiled item; means are over 15.

| axis | ok | mismatch | mean value | reading |
| --- | --- | --- | --- | --- |
| `timing.tempo` | 13 | 2 | 0.950 | clean; the two misses are `*-false-climax` items |
| `timing.beat` | 10 | 5 | 0.836 | all 5 misses are `swiss-signal` items |
| `notes.all` | 0 | 15 | 0.229 | e.g. p=0.30 r=0.38 f1=0.34 on `black-ice-demo` |
| `notes.lead` | 0 | 15 | 0.000 | roles deliberately `Unknown` this round |
| `motif.relation` | 0 | 15 | 0.000 | recovered line is polyphonic and unlabeled |
| `harmony.relation` | 0 | 15 | 0.000 | **length mismatch, see §12** |
| `harmony.root` | 0 | 15 | 0.090 | ~1/8 roots agree in order |
| `groove.relation` | 0 | 15 | 0.000 | pocket cannot fully match the grid |
| `form.topology` | 7 | 8 | 0.467 | correct on demo + calm-loop families |
| `classification` | 15 | 0 | — | 0 false unknown / false free / false present |

**Effective relations** (lowered from requested, honestly):

- `interpretive`: motif `Metric`, harmony `QualityFamily`, groove `PocketSkeleton`, form `Free`,
  orchestration `Free` — 15/15 each.
- `faithful`: identical except form is requested `Exact` and **lowered to `Topology`** (15/15),
  demonstrating the ceiling logic. Orchestration never rises above `Free`/`Unknown`.

The strongest, load-bearing positive is **timing + classification**; the load-bearing negative is
**note-level content**. No plausible global score is allowed to hide that (§9).

## 7. Hostile mutation results

70 mutations over 5 source items, profile `interpretive`. Pass/fail is judged by the
**preregistered evaluator axis** moving (or holding) as expected; `content drift` is reported
separately — it is when the recovered musical content moved even though the axis metric did not.

| mutation kind | pass | fail | checks | content drifted | metric witnessed |
| --- | --- | --- | --- | --- | --- |
| chord-edit | 0 | 5 | 10 | 0 | 0 |
| compress | 5 | 0 | 10 | 5 | 0 |
| delete-lead-note | 3 | 2 | 10 | 3 | 3 |
| eq | 5 | 0 | 10 | 5 | 0 |
| mute-drums | 0 | 5 | 10 | 10 | 0 |
| mute-role | 10 | 0 | 20 | 10 | 10 |
| noise | 4 | 1 | 5 | 1 | 1 |
| reverb | 4 | 1 | 10 | 6 | 1 |
| shift-lead-note | 0 | 5 | 10 | 5 | 0 |
| tempo-scale | 10 | 0 | 20 | 20 | 10 |
| transpose | 9 | 1 | 40 | 30 | 19 |

Overall **50/70**. Interpretation:

- `transpose`, `tempo-scale`, `mute-role`, `eq`, `compress` behave: relative identity survives or
  the requested change is witnessed.
- `chord-edit`, `shift-lead-note`, `mute-drums` fail because their target relation axis is stuck
  at `Free` — the metric has no dynamic range to witness the change, even though content drifted.
- `eq`/`compress`/`reverb` show a real robustness residual: production processing shifts our
  recovered motif *content* (5/5, 5/5, 6/10) even when the metric holds.

## 8. Holdout status

- Generated under a distinct seed base: 30 items, `split=holdout`, seed base `50000`.
- Committed only as `holdout/manifest.json` + `holdout/SEAL.json`:
  `manifest_sha256 = 80422b1915b3073ee075580e06cd15c599767834651d8180b966d42d7c19eb11`,
  `commitment_sha256 = db8f863465b659b49e79d1f618170122cbef4e53e2977f9261709e9b7185c037`.
- **Not analyzed.** Schemas, relations, thresholds and mutation expectations were frozen first
  (see [`docs/THRESHOLDS.md`](docs/THRESHOLDS.md)). After first contact there will be no fitting
  on those examples, and any verdict-changing repair requires a second holdout.

## 9. Failures and residuals (named, not hidden)

1. **Note content is the load-bearing failure.** F1 ≈ 0.23. A monophonic peak estimator cannot
   represent the truth's 4-note pad/keys chords; recall is capped well below the metric's 0.5 bar.
2. **`harmony.relation` cannot exceed `Free` on this data** because `relation_between` requires
   equal-length sequences while recovered per-beat windows (~36) never equal truth chord spans
   (~8). `harmony.root` is the only partially informative harmonic witness (mean 0.09).
3. **`motif.relation` cannot exceed `Free`** because the metric consumes the raw unlabeled
   polyphonic note list; roles are `Unknown` by design, so the recovered "line" is not isolated.
4. **`groove.relation` sits at floor**: family classification over-detects snare/hat and misses
   some kicks, and the strict `KickSnare` requirement is not met.
5. **Production mutations leak into structure** (motif content drift under EQ/compression/reverb).
6. **`swiss-signal` beats** are consistently unstable (5/5 beat misses) — one world's timbre/rhythm
   defeats the low-band-phase transport.
7. **Two `false-climax` items miss tempo** (mean value 0.665): a tempo change confuses the single
   autocorrelation period.

None of these were tuned away; they are the boundary of this round.

## 10. Resources

- Host: Linux Mint 22.3, no GPU, ≈7 GB RAM.
- Full dev round (15 items × analysis × 2 profiles × recover+evaluate): **≈10 s**.
- Mutation round (70 items): **≈52 s**.
- Calibration (8 fixtures): **< 3 s**. Replay tests: **< 3 s**.
- Evidence per dev item: tens of KB JSON. Dev WAVs 64 MB; holdout WAVs 127 MB (not committed).
- No heavyweight neural inference in `cargo test`; external models are absent and refuse cleanly.

## 11. Claim ledger

Full ledger with levels in [`docs/CLAIM_LEDGER.md`](docs/CLAIM_LEDGER.md). Summary:

- **Disclosed:** forward LibGibson surfaces; deterministic canonical boundary; frozen schemas /
  relations / thresholds.
- **Observed:** timing and classification recover cleanly; harmony triangulation and the
  quotient-with-ceilings machinery run end-to-end; all calibration controls pass.
- **Corroborated:** *nothing at the program level yet* — holdout not contacted.
- **Conjectured:** a useful `CoverMap`-like quotient is recoverable from generated audio.
- **Refuted (round-1 scope):** monophonic transcription reaches F1 ≥ 0.5; per-beat windows can
  express the harmonic `relation`; production mutations leave structural content invariant.
- **UNVERIFIED / access gap:** external ML adapters; analysis-by-synthesis.
- **Dark:** authorial intent / generator seed / forward-only provenance.

## 12. Things that were wrong in the initial model

These corrections changed the implementation after first contact with data (dev only; holdout
untouched):

1. **Beat phase, not tempo, was the real timing failure.** The first DP beat tracker phase-locked
   onto the melody's offbeats, shifting every note by ~0.48 beat and making `notes.all` ≈ 0.01.
   Fix: a strict periodic transport with a low-band-weighted phase, anchored to the first onset.
   The DP tracker is retained only as a *rival hypothesis* in `timing.ambiguity`.
2. **A single HPS pitch is octave- and partial-prone.** Replaced with harmonic-suppressed spectral
   peak picking emitting up to 3 notes/onset plus an HPS/ACF cross-check. Notes F1 rose 0.01 → 0.34
   on the profiled item — better, still far from useful.
3. **`relation_between`'s equality requirement** was not anticipated: the evaluator compares the
   recovered `chords` list directly against truth chord spans, so per-beat windows can never
   exceed `Free`. This is a property of the frozen metric; recorded, not patched.
4. **Roles cannot be inferred** and are therefore deliberately left `Unknown`, which (3)+(4)
   together cap `harmony.relation` and `motif.relation` at `Free` regardless of audio quality.
5. **Mutation expectations name evaluator axes.** The first checker compared recovered *content*;
   the preregistered axes are metric axes, so the metric is the authority and content drift is a
   separate robustness diagnostic.

## 13. Verdict-changing probes (preregistered)

Per the research README §13:

1. **Kill** — even on clean hidden renders, independent routes cannot recover quotient
   coordinates above trivial tempo/key. *Current evidence leans toward "narrow", not kill:*
   timing/classification recover, content does not.
2. **Narrow** — symbolic identity is recoverable only after stem isolation. **This is the leading
   hypothesis.** The cheapest decisive next probe: run the same pipeline on *isolated stems*
   (or a monophonic stem) and test whether `motif.relation` and `notes.all` leave the floor.
3. **Proceed** — blinded recovery survives holdouts and adversarial transformations with
   calibrated uncertainty. Not yet testable (holdout sealed).
4. **Escalate** — paired external recordings transfer without song-specific heuristics.
5. **Higher-semantics gate** — discourse/meaning only after lower-level residuals stop explaining
   apparent successes.

## 14. Is there evidence that the HomoSapians can "listen"?

**Partially, and only at the coarsest layers — not at the semantic quotient.**

- They demonstrably **hear time**: tempo is recovered on 13/15 blinded items and exactly on the
  120 BPM click control; beats lock to 0.04 of a 0.5 s grid on the control.
- They demonstrably **know what they do not know**: orchestration stays `Free`/`Unknown`;
  `classification` is clean with zero false unknown/free/present; silence produces no invention;
  unavailable models refuse explicitly.
- They can **carry provenance** for every reading, with source and canonical hashes kept distinct.
- But the load-bearing semantic axes — motif identity, harmonic relation, groove relation, and
  per-note content — remain at or near `Free`, and one world's beats are unstable. The system
  **prints calibrated ambiguity, not a recovered semantic quotient.**

So: evidence exists that the analyzer has begun to **listen at the level of pulse and of honest
self-doubt**. There is **no** evidence yet that it can recover the musical identity quotient of a
full mix. The verdict is **narrow**: the next honest move is the stem-isolation probe (13.2),
not more prose. This conclusion inherits the weakest load-bearing premise (L6/L8), and the
holdout remains unopened pending that decision.
