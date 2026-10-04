# Numeric artifact audit supplement

This supplements the inventory with direct numeric-content validation. The extraction and frozen importer were not changed or rerun.

| Surface | Result |
| --- | --- |
| Files / bytes | 493 / 19,423,860,888 |
| NPZ archives / arrays | 119 / 680 |
| Declared time axes | 102; all finite, nonnegative, strictly increasing and within source duration plus the declared 0.1-second frame tolerance |
| WAV files | 374; all FLOAT; headers, sample rates, channels, durations, decoded frame counts and every PCM sample checked |
| Nonfinite PCM | 0 samples |
| Unsupported arrays / unreadable files | 0 / 0 |
| Files passing all checks | 492 / 493 |
| Prior inventory consistency | All 493 newly computed SHA256 values match the earlier inventory |

**The audit fails one artifact:** `track-014/features/full_mix/fullmix_loudness.npz` contains five negative infinities in `short_term_lufs`, at 198.0, 198.5, 199.0, 199.5 and 200.0 seconds. They are neither NaN nor positive infinity. Negative-infinite loudness can represent silence, but these values are not finite production targets and must not enter ordinary numeric fitting. They remain in the untouched raw evidence; this supplement does not silently repair or certify them. All other audited numeric arrays are finite.

The validator uses `numpy.load(..., allow_pickle=False)` and fails closed on object arrays without unpickling. String metadata is classified separately. FLOAT/DOUBLE WAVs are read in bounded blocks and every sample checked. Integer PCM, if encountered in another corpus, is checked at the header and final frame; it cannot encode IEEE NaN/Inf. No integer WAV occurred in this run. Every inspected file is freshly hashed and checked for size/mtime changes during inspection.

Synthetic positive/negative controls passed: valid arrays and integer WAV, nonfinite arrays and floating WAV, reversed and out-of-duration time axes, rejected object arrays, and unreadable WAV. The corpus run exited **2**, as specified for observed invalid or unsupported data. This is a completed audit with a retained failure, not an execution failure.

Exact local receipt: `/home/leah/ANAL_MUSIC/cover_out/NUMERIC_ARTIFACT_AUDIT.json`.
SHA256: `107571cb3681da0c16f9543648d79ee3d08bd4827128e164899573ee270eeedc`.
Log: `/home/leah/ANAL_MUSIC/cover_out/NUMERIC_ARTIFACT_AUDIT.log`.

```sh
/home/leah/ANAL_MUSIC/.venv_audio/bin/python \
  experiments/semantic-audio-inversion/round2/scripts/validate_numeric_artifacts.py --self-test
/home/leah/ANAL_MUSIC/.venv_audio/bin/python \
  experiments/semantic-audio-inversion/round2/scripts/validate_numeric_artifacts.py \
  --corpus /home/leah/ANAL_MUSIC/analysis_out \
  --out /home/leah/ANAL_MUSIC/cover_out/NUMERIC_ARTIFACT_AUDIT.json
```

This verifies stored numeric integrity and declared time coordinates, not musical correctness, calibrated confidence or transcription accuracy.
