# Reverse HumanMusic Round 2

An additive external consumer of LibGibson
`c2f6483d92fe2b351e6cd50936a97d8cdf73cb79`. Round-1 files, its old holdout,
and the root lab freeze remain untouched. No upstream library changes.

```
private extraction corpus -> audited stem lanes + event relations
 -> evidence ceilings -> public CoverMap -> checked candidate + aligned stems
 -> PCM-only independent observer -> vector residuals -> gated ranking
 -> optional production -> fresh observation
```

The implementation has three distinct gates: valid measurement custody, native
cover admission, and observed musical identity. The `best/` directory can contain
a **diagnostic listening selection with no eligible winner**; read `WHY_THIS_WON.md`.
Rejected candidates retain their failure receipts. Production never rescues a
musically failed candidate. Perceptual recognition requires maintainer listening.

## Run

Use a Python environment with the versions in `requirements.txt`. The existing
local extraction environment was used; no expensive separation is rerun.

From this directory:

```sh
export PYTHONPATH="$PWD/python"
python -m unittest discover -s tests -v
cargo test --locked --manifest-path renderer/Cargo.toml
cargo build --release --locked --manifest-path renderer/Cargo.toml
python -m sai_v2.corpus --corpus /path/to/analysis_out --out /private/cover_out
renderer/target/release/sai-round2-renderer dev /private/cover_out/calibration/dev 22050
python -m sai_v2.calibrate /private/cover_out/calibration/dev
python -m sai_v2.run /private/cover_out/track-001 \
  --renderer renderer/target/release/sai-round2-renderer
```

`run` without `--renderer` replays existing audio observations. Cache identity
includes WAV, observer and frozen baseline hashes plus numerical-library versions.
All numerical controls are global. No track ID selects musical data or thresholds.

The optional `native-pocket INPUT OUT [SEEDS] [SR]` renderer treatment applies one
positive preroll to every lane and pins measured recurrent canonical drum members.
Use a distinct output directory; compare it with `python -m sai_v2.run OUT`.
The evaluation receipts name the global preroll and the timing-conditioned basis.
One global octave is an allowed line freedom; absolute-pitch F1 remains alongside
octave-quotient F1. There is no per-note pitch correction or local time warping.

The `holdout OUT [SR]` renderer command generates a distinct seed family and marks
it `sai.round2_holdout/v1`; the calibration reader refuses it. Generate only after
freezing the observer, compiler and comparison code. Do not analyze it while tuning.

## Implementation and limits

- `python/sai_v2/model.py`, `corpus.py`: versioned validated evidence graph/views,
  artifact custody, role hypotheses, unresolved rivals and provenance.
- `renderer/`: standalone pinned Rust workspace; real public API, admission,
  conformance and waveform receipts; separate synthetic development and holdout.
- `observe.py`: receives only WAV and the known rendered role surface. Frozen
  Round-1 DSP is retained for the matched isolation contrast; YIN/energy is an
  additional monophonic route. Shared YIN lineage with source analysis is disclosed.
- `relations.py`, `evaluate.py`: temporal harmony overlap/transition alignment,
  line and occurrence relations, drum ambiguity, recurrence topology, residuals.
- `rank.py`: complete structural gates precede production distances; missing gates
  cannot pass vacuously. No weighted truth score.
- `production.py`: bounded role gain, spectral bands, width/pan fit; sample alignment,
  raw render and post-fit re-analysis preserved. Invalid source room/attack proxies
  are not fitted.

Read [CORPUS_AUDIT.md](docs/CORPUS_AUDIT.md),
[PUBLIC_API.md](docs/PUBLIC_API.md), [RELATIONS.md](docs/RELATIONS.md),
[PROTOCOL.md](docs/PROTOCOL.md) and [BASS_WINDOW_PROBE.md](docs/BASS_WINDOW_PROBE.md).

This is an analysis-by-synthesis consistency experiment, not unique recovery of the
original score. Role separation improves contact with a line; it does not establish
the correctness of its transcription, the source of a chord, or listener recognition.
The public native ingress cannot combine all weak real-source relations with metric
lines. Such axes remain visible in the source object and full-profile refusal.

No real audio, full real transcription, source filenames, lyrics, or generated real
cover WAVs belong in Git. Local receipts and listening artifacts live outside the repo.
