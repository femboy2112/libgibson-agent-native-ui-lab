# Bounded bass-window discriminator

The archived bass extractor consumes Demucs at **44,100 Hz**, not the 22,050 Hz semantic view. Both pYIN and YIN use a 2,048-sample frame with C1 minimum F0. All 17 extraction logs warn that fewer than two C1 periods fit. This is an ordinary instrument/configuration failure candidate, not a separator verdict.

Preregistered for this bounded probe: first corpus source with nonzero bass notes, first source with zero bass notes, identical crop 30–40 seconds; fixed pYIN fmin C1/fmax G4, hop 256, confidence threshold 0.3 and segmentation; change only frame length 2,048→4,096. Known harmonic-tone controls contain three two-second notes, separately C1/E1/G1 and C2/E2/G2. No source model was replaced or source evidence pin promoted.

| Input | Frame | Emitted notes | Fraction probability > .3 | Known pitch frame recall |
| --- | ---: | ---: | ---: | ---: |
| synthetic_C1_E1_G1 | 2048 | 0 | 0.000 | 0.824 |
| synthetic_C1_E1_G1 | 4096 | 1 | 0.322 | 0.988 |
| synthetic_C2_E2_G2 | 2048 | 1 | 0.328 | 0.995 |
| synthetic_C2_E2_G2 | 4096 | 3 | 0.972 | 0.989 |
| track-001 | 2048 | 4 | 0.265 | Unknown |
| track-001 | 4096 | 19 | 0.558 | Unknown |
| track-007 | 2048 | 0 | 0.000 | Unknown |
| track-007 | 4096 | 4 | 0.228 | Unknown |

Observed: even perfectly synthetic bass is largely dropped by the fixed confidence gate. Changing the window recovers all three higher-register control notes but only one low-register control note. More source detections are **not** proof of more accurate source pitches. The 4,096-frame route is therefore a discriminating improvement, not a certified replacement.

The raw archived F0 arrays also expose the distinction: track-007 has a Viterbi voiced fraction of 0.597 yet zero admitted notes, while track-016 has voiced fraction 0.933 yet only one note. Both have low median instantaneous voiced probability. Source silence is not established by the emitted CSV.

The next probe should use a preregistered grid of physical window durations across known low-bass HumanMusic stems, measuring note F1 and octave error, with the unchanged confidence rule first. Then cross frame duration with the confidence policy in a complete factorial experiment. This separates time/frequency resolution from segmentation rejection; it does not route-shop on real-song success.

Local exact receipts: `/home/leah/ANAL_MUSIC/cover_out/BASS_WINDOW_PROBE.json`, `BASS_WINDOW_PROBE.log`, and `BASS_ROUTE_AUDIT.json`. Crop PCM hashes bind comparisons without storing source audio in Git. The script depends on the immutable archived segmentation implementation to hold that factor fixed.

```sh
OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 /home/leah/ANAL_MUSIC/.venv_audio/bin/python \
  experiments/semantic-audio-inversion/round2/scripts/bass_window_probe.py \
  --corpus /home/leah/ANAL_MUSIC/analysis_out --out /home/leah/ANAL_MUSIC/cover_out/bass_probe_replay
```
