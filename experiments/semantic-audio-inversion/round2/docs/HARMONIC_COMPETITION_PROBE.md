# Exploratory harmonic-family competition, after the v2 freeze

This separate probe tests a specific inverse-model gap: the archived CQT route
labels strong spectral peaks as notes without requiring that several peaks belong
to one fundamental. It does not change v2, source evidence, cover pins, evaluator
thresholds, the old holdout, or the new sealed holdout.

The probe contrasts the predictions of peak-as-note and harmonic-family
explanations, retaining null controls and known failure cases.

## Mathematical object and its limit

For each frame, let `m >= 0` be the magnitude STFT and let `D` contain analytic
harmonic-family spectra. We approximately solve

`min_{a >= 0, ||a||_0 <= 6} ||m - Da||_2^2`

by greedy residual selection and nonnegative least-squares refitting of the
selected columns. This is spectral explanation for note proposals, **not** a
source-versus-cover PCM objective or musical success score. The Lagrangian
alternative `||m-Da||² + lambda ||a||_1` is a related possible relaxation; it is
not the implemented solver. The dictionary encodes the physical relation
`partial_frequency = harmonic_number * fundamental_frequency`. Competing pitch
families can explain many peaks with one latent note. No harmonic analysis can
read musical roles or compositional intent directly out of that equality.

A genuine ambiguity remains: `sin(wt) + .5 sin(2wt)` can be one instrument with
a fundamental and overtone, or two sinusoidal instruments one octave apart.
Their sum is identical. A sparsity prior prefers one explanation; it does not
prove the number of players. Onsets, independently changing envelopes, spatial
cues, stem evidence and phrase continuity are possible additional discriminators.
The real-octave fixture below tests one separable instance; it does not remove
this identifiability problem.

This direction has established precedents: [Benetos and Dixon's convolutive
polyphonic transcription model](https://webspace.eecs.qmul.ac.uk/s.e.dixon/pub/2011/Benetos-Dixon-SMC2011.pdf)
uses instrument templates and temporal continuity; [DDSP](https://arxiv.org/abs/2001.04643)
makes signal-processing components differentiable for model fitting. The small
analytic-dictionary probe here is neither implementation and makes no claim to
their performance. Its objective is not a differential-form reconstruction theorem.

## Declared instrument

The script writes `PREREGISTRATION.json` before analyzing any case. Two preserved runs: v1 used the library default -38 dB; v2 corrected only the
control call to the corpus caller threshold -40 dB after source review. No inverse
threshold search or changes. Input hashes and all inverse outputs are identical
between runs:

- 22050 Hz; 8192-sample Hann STFT; 512-sample hop; MIDI 36 through 95 inclusive.
- Four analytic envelopes: `1/h`, `1/h²`, odd-only `1/h`, and `1/h` with the
  fundamental multiplied by .08. Twelve partials, analytic centered Hann lobes
  truncated to four FFT bins around each partial; columns have unit L2 norm.
- At most six atoms. Each addition must explain at least .025 of original frame
  spectral energy. NNLS refits all selected atoms after each proposed addition.
- A frame is accepted only when total explained spectral energy is at least .60.
  Retain coefficients at least .12 times the strongest selected coefficient.
- Absolute window RMS below `1e-5` is silence. Events need .06 seconds.
- Archived control: import the actual supplied `mir_lib.py` and call
  `cqt_poly_notes(y, 22050, threshold_db=-40)` as in `33_harmonic.py`
  line 50: C2–C7, hop 512, six peaks/frame, .06-second events. The corpus uses
  native 44100-Hz stems, whereas this bounded synthetic probe uses 22050 Hz:
  this is a changed-sample-rate instrument contrast, not an exact production replay.
- Eleven deterministic 2.8-second fixtures, RNG seed 2026100401. The additive
  generator uses random phases, sixteen partials and a `1/h^1.35` regular
  envelope, so it is not exactly the inverse dictionary. Strong overtones,
  missing fundamental, octave pair, odd spectra, detuning, inharmonicity,
  staggered gates, quiet notes in noise, silence and noise are retained.

The generator and dictionary nevertheless share harmonic assumptions. This is
one controlled synthetic bearing, not independent real-recording validation.
The RNG seed is distinct from both development and sealed holdout seed families.
The script does not import or enumerate either holdout.

## Observed result

These are **frame-level exact-MIDI note occupancy** F1 values on a shared
512-sample time grid. They are not onset-matched event F1 and are not comparable
numerically with the Round 1 event metric. Full durations, including attack and
release errors, enter the counts. Event counts are shown to expose fragmentation.
Silence/noise F1 is defined as zero because there are no true positives; evaluate
those cases by false events.

| Fixture | Archived CQT F1 | Competition F1 | CQT events | Competition events |
|---|---:|---:|---:|---:|
| triad_regular | 0.6774 | 0.8146 | 6 | 6 |
| triad_strong_overtones | 0.0286 | 0.7504 | 15 | 16 |
| single_missing_fundamental | 0.0000 | 0.9891 | 6 | 1 |
| real_octave_pair | 0.5000 | 0.9357 | 6 | 6 |
| odd_partial_triad | 0.6659 | 0.9479 | 6 | 7 |
| detuned_triad | 0.6733 | 0.9536 | 6 | 6 |
| inharmonic_triad | 0.6775 | 0.8341 | 8 | 4 |
| staggered_phrase | 0.3449 | 0.8264 | 20 | 8 |
| quiet_triad_in_noise | 0.6618 | 0.0000 | 19 | 0 |
| silence | 0.0000 | 0.0000 | 6 | 0 |
| noise | 0.0000 | 0.0000 | 37 | 0 |

Across nine pitched fixtures, micro F1 is **0.486479 -> 0.811872**: true-positive
pitch frames 1673 -> 1696; false positives 3175 -> 452; false negatives 357 -> 334.
The predeclared gate (aggregate improvement, no null events, octave recall >= .8)
passes; octave recall is 1.0. This is a bounded observed result only.

The new route **fails the quiet chord in noise completely**. Its fixed .60
spectral-explanation threshold refuses every frame, whereas the CQT route still
recovers that chord. This refutes universal superiority. False positives remain
on clean regular/strong-overtone cases, and event fragmentation remains substantial.
The 8192-sample window spans 371.5 ms (185.8 ms on either side of a centered
frame), and the hop is 23.2 ms. Events can therefore smear over rhythmically
important boundaries even with a fine output hop. This is a pitch-occupancy
probe, not a final event transcriber or a recovery of syncopation.

The archived implementation emits six events on digital silence because
`amplitude_to_db(C, ref=np.max)` makes the zero-spectrum bins equally maximal
under its finite floor; its local-peak test accepts ties and then retains six.
The control is deliberately unchanged. This is a demonstrated measurement defect,
not a diagnosis of how many real-source events are wrong.

## Provenance and reproduction

Command (local dependencies already available):

```sh
OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 \
  ~/ANAL_MUSIC/.venv_audio/bin/python \
  experiments/semantic-audio-inversion/round2/scripts/harmonic_competition_probe.py \
  --mir-lib ~/ANAL_MUSIC/analysis_out/scripts/mir_lib.py \
  --out ~/ANAL_MUSIC/round2_calibration/harmonic_competition_v2
```

Use a new output directory to repeat; existing nonempty directories are refused.
Local output holds synthetic WAVs, frame-by-frame selected atoms/explained energy,
raw extracted events, truth, preregistration and results. No private song content
was read or copied by this probe. The original -38 dB run and exact script are
preserved under `~/ANAL_MUSIC/round2_calibration/harmonic_competition_v1/`;
its pitched micro F1 was 0.486549 -> 0.811872 (one fewer CQT false-positive frame). Exact receipts:

- Script SHA256: `ae6fbf712151b143e0dfb3f24981758be57693401581111f70027fca60ce8ca5`
- Archived MIR source SHA256: `c8610967367fa21de6d02487a0d4d984ef9d5f0000d8e573cc0a13cb62713942`
- Results SHA256: `e07eabdf29b160f27bd84555acf96c23ac7d644a3b9c5a2d7bca5bf183a1f9a3`
- Runtime: Python 3.12.3, NumPy 2.5.3, SciPy 1.18.1, librosa 1.0.0.
- Local result: `~/ANAL_MUSIC/round2_calibration/harmonic_competition_v2/RESULTS.json`.

## Claim boundary and next discriminator

| Claim | Status | Boundary |
|---|---|---|
| Harmonic competition removes many overtone-as-note errors | Observed | Nine known synthetic pitched fixtures; shared harmonic model assumptions |
| This implementation universally improves transcription | Refuted | Quiet triad in noise loses all notes |
| Absolute-energy handling is needed for this archived peak route | Observed | Digital silence emits six events |
| Real-song inversion or perceptual cover quality improves | UNVERIFIED | No real-source replacement, cover generation or listening in this probe |
| A waveform uniquely determines its ensemble and song interpretation | Refuted without extra assumptions | Identical summed sinusoidal construction above |

The next verdict-changing test is a fresh, public-API-generated short polyphonic
HumanMusic fixture set with known notes and varied world/timbre, explicitly outside
both sealed holdouts. Preregister metrics including onset/offset error and separate
notes under octave overlap. That rotates the generator away from this additive
harmonic model. Separately, a noise-aware likelihood could address the observed
quiet-chord failure, but it must be tested as a new instrument rather than tuned
against a spent control and advertised as validation.
