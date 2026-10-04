# Round 2: built loop, failed full identity, useful measurement-playback discriminator

| Item | Measured result / receipt |
| --- | --- |
| Remote research base verified | `d2c2f63acd1512249eb5f8db3bc68be1f5ba14c2` |
| Core instrument freeze | `0eb6748edc7cb82dc248de66fa872c0f7c10bb85`; exact file hashes in `receipts/instrument_freeze.json` |
| LibGibson consumed | `c2f6483d92fe2b351e6cd50936a97d8cdf73cb79`, pinned independent Cargo workspace; upstream untouched |
| Branch / PR | `feat/sai-round2-cover-synthesis`; [PR 8](https://github.com/femboy2112/libgibson-agent-native-ui-lab/pull/8), targets research branch, unmerged |
| DeepSeek custody | 17/17 source hashes; 1,806 track files; exact manifest/archive hashes in [corpus audit](docs/CORPUS_AUDIT.md) |
| Architecture | stem observations → time-aligned events/role hypotheses → evidence ceilings → public CoverMap → checked candidate → aligned PCM/stems → independent audio observation → residual vector → gated ranking |
| Schema | additive `sai.stem_evidence/v2`; seconds and metric time, route/stem provenance, ambiguity, rivals, support, occupancy and production; v1 unchanged |
| Harmonic probe repair | v2 overlap-weighted root/family/quality plus transitions; equivalent splitting/merging invariant; raw spans/alignment retained; v1 unchanged |
| Synthetic development | six fresh HumanMusic items, three worlds; all generator pipelines pass; isolated-note gains measured below |
| Real compilation | 16 partial diagnostic lifts; track-013 refused unsupported inferred meter. No full-source structural winner |
| Initial native candidates | 96 configs: 50 internal admissions, 16 rendered rejections, 30 refusals; 66 WAV mixes |
| Native-pocket treatment | two sources, 12 configs: 4 internal admissions, 4 rendered rejections, 4 refusals; 8 WAV mixes |
| Total checked candidate search | 108 attempts; 54 internal admissions; 74 full mixes with aligned role stems |
| Dynamic measurement controls | two sources × three worlds = six additional unfitted mixes, independent observed lanes and exact source seconds; admission NOT APPLICABLE |
| World experiment | BLACK_ICE, VAPOR95, SWISS_SIGNAL tried. Production vectors retained; zero structurally eligible world winners |
| Production | fitter implemented and synthetic-audio tested; real checked candidates refused production because structure failed |
| Execution-order exception | six early source-fitted dynamic exploratory mixes were made before structural admission. They are preserved separately as exploratory, excluded from acceptance, and followed by six unfitted controls. This was an ordering error, not validated production improvement |
| Maintainer audition | native track-003 candidate-002 unrecognizable; unfitted dynamic BLACK_ICE playback partly recognizable under severe garble, unacceptable quality |
| Holdouts | old 30 untouched; fresh six-item Round-2 family generated after core freeze, sealed without truth/audio inspection; commitment in `receipts/holdout_seal.json` |
| Tests | 69 Python, 17 standalone Rust, plus unchanged Round-1 40 core + 4 harness; fmt/Clippy passed |
| Data / hack audit | all changes confined to additive round2 directory; no real audio or full real transcription tracked; no source-ID branches in compiler, inference, scoring or rendering |
| Claim boundary | executable analysis-by-synthesis loop demonstrated; convincing admitted real-song cover NOT demonstrated |
| Post-freeze mathematical probe | harmonic-family competition on nine pitched + two null synthetic controls: frame-note F1 0.486479→0.811872, false pitch frames 3175→452; quiet chord in noise fails completely |

The main success criterion has **not passed**. Native admission proves conformance to the pins the API accepted, not preservation of every source identity. Every checked candidate fails the complete observational profile. `best/` is a diagnostic queue, never a success label. The maintainer's failed audition overrides a favorable attack-only metric.

## Stem-isolation experiment

| Matched frozen v1 instrument | Full mix mean F1 | Isolated role mean F1 |
| --- | ---: | ---: |
| All notes | 0.393904 | 0.639160 |
| Lead | 0.024849 | 0.676551 |
| Bass | 0.032760 | 0.687764 |

These six development items establish an isolation benefit, not generalization. Aggregate all-note F1 decreases on the two VAPOR95 examples: the mean is not uniform improvement. The additional YIN route yields lead F1 0.775556 and absolute bass F1 0.287879. BLACK_ICE/SWISS bass often has octave ambiguity from subharmonic synthesis; absolute F1 remains published beside the one-global-octave quotient.

Whole-line motif reaches Metric on 4/6 dev items and bass on 5/6. Harmony remains unknown because rival interpretations survive; roots are measurable but isolation does not consistently improve them. Groove family relations remain unknown; family ambiguity is not converted into invented simultaneous hits. Therefore separation is useful, and transcription/interpretation remains a wall too. [Exact six-item receipt](receipts/results.json).

## Real-source diagnostic selections

No row below is an eligible winner. F1 is timing-conditioned and permits only one global octave where labeled. Missing bass evidence is unknown, not successful silence. Baseline whole-line motif is Free on all 66 renders; bass is Metric on three renders (the extremely short four-note track-009 source) and otherwise Free/unknown. Harmony/groove relations remain unknown and form topology Free on all baseline renders. Local source-declared motif occurrence measurements are retained in the receipt, not cherry-picked to replace whole-source failure.

| Source | Admitted / attempted | Diagnostic candidate / world | Lead octave F1 | Bass octave F1 | Harmony roots | Drum onset F1 |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| track-001 | 0/6 | candidate-001 / BLACK_ICE | 0.909 | 0.788 | 0.102 | 0.216 |
| track-002 | 0/6 | candidate-001 / BLACK_ICE | 0.913 | 0.820 | 0.128 | 0.460 |
| track-003 | 4/6 | candidate-001 / BLACK_ICE | 0.870 | 0.000 | 0.140 | 0.417 |
| track-004 | 4/6 | candidate-002 / BLACK_ICE | 0.896 | 0.000 | 0.141 | 0.324 |
| track-005 | 4/6 | candidate-002 / BLACK_ICE | 0.786 | 0.376 | 0.098 | 0.588 |
| track-006 | 4/6 | candidate-001 / BLACK_ICE | 0.889 | 0.000 | 0.093 | 0.522 |
| track-007 | 6/6 | candidate-006 / SWISS_SIGNAL | 0.937 | 0.000 | 0.099 | 0.504 |
| track-008 | 4/6 | candidate-001 / BLACK_ICE | 0.903 | 0.835 | 0.123 | 0.670 |
| track-009 | 4/6 | candidate-004 / VAPOR95 | 0.779 | 1.000 | 0.150 | 0.592 |
| track-010 | 4/6 | candidate-001 / BLACK_ICE | 0.925 | 0.000 | 0.106 | 0.406 |
| track-011 | 4/6 | candidate-002 / BLACK_ICE | 0.909 | 0.533 | 0.078 | 0.561 |
| track-012 | 0/6 | candidate-001 / BLACK_ICE | 0.802 | 0.912 | 0.124 | 0.268 |
| track-013 | 0/0 | refused / — | — | — | — | — |
| track-014 | 0/6 | candidate-001 / BLACK_ICE | 0.761 | 0.000 | 0.061 | 0.165 |
| track-015 | 4/6 | candidate-002 / BLACK_ICE | 0.960 | 0.750 | 0.042 | 0.698 |
| track-016 | 4/6 | candidate-001 / BLACK_ICE | 0.834 | 0.000 | 0.118 | 0.346 |
| track-017 | 4/6 | candidate-001 / BLACK_ICE | 0.791 | 0.824 | 0.156 | 0.268 |

Every diagnostic WAV is `/home/leah/ANAL_MUSIC/cover_out/track-NNN/best/raw_humanmusic.wav` except track-013, which has no render. Exact candidate/config/source/blueprint/render hashes, seeds, conformance, score fingerprint, performance receipts, independent observations and every residual are local; compact per-track values and paths are in [results.json](receipts/results.json). Full inventory is `/home/leah/ANAL_MUSIC/cover_out/CORPUS_INVENTORY.json`; aggregate listening queue is `/home/leah/ANAL_MUSIC/cover_out/LISTENING_QUEUE.md`. The local source graph and note plots are deliberately not committed.

## Listening falsifier and dynamic band

The maintainer heard native track-003 candidate-002 as unrecognizable, catastrophically quantized generic harmony with missing groove and identity-bearing notes. Its render hash is bound in [the listening receipt](receipts/listening_feedback.json). That candidate's recognition claim is **Refuted**.

Investigation did not support grid rounding as the dominant explanation: maximum compiler onset rounding is about 0.00195 beat (roughly 1.2 ms here). Metric melody projection discards measured release bounds; native note durations use the next attack and an articulation factor. The symbolic score occupies about 59% of measured source rests; independent PCM observation measures 86.3%, including the audible release behavior. The source has 269 lead notes but only one bass note; the checked lift pins Metric lead and generates much of the backing. It cannot honestly pin the missing BassFigure or unresolved source harmony. Attack F1 alone hid these omissions. The evaluator now measures interval-union rest intrusion and matched release error, with unknown release evidence kept unknown.

A separate dynamic ensemble uses public Score, Instrument and Synth objects to play observation lanes without invented backing, in original source seconds with measured gate lengths. It preserves source lane identity, avoids duplicate four-stem/ six-stem support, and allocates independent voice groups to avoid fixed-pool voice stealing. It is **measurement playback**, not a checked-cover success. Observed lanes do not prove the number of physical instruments.

The maintainer then heard the song underneath severe garble in the unfitted BLACK_ICE control. This is a limited positive observation: recognizable information exists in the extracted event collection. It is not evidence of acceptable sound or a successful fresh constrained performance. Six source-fitted exploratory versions were made prematurely; they remain separately labeled and are not used to support this finding.

| track-003 BLACK_ICE surface | Lead attack F1 | Lead rest intrusion | Drum onset F1 | Listener |
| --- | ---: | ---: | ---: | --- |
| Native candidate-002 | about 0.865 (one global octave) | 0.863 | 0.439 | Unrecognizable |
| Unfitted source-seconds ensemble | 0.926 | 0.223 | 0.962 | Song audible underneath severe garble |

Drum-onset F1 is not drum-family or PocketSkeleton acceptance. Dynamic track-003 SWISS lead F1 is 0.975 and rest intrusion 0.100, but it has no listener acceptance. No whole-source relation thresholds were loosened. Source tempo transport, note gates and backing all change between native and playback treatments, so their individual causal contributions are not identified. Same-gain role ablations isolate one factor without editing notes.

Useful local listening paths:

- Failed native control: `/home/leah/ANAL_MUSIC/cover_out/track-003/candidates/candidate-002/raw_mix.wav`.
- Partly recognizable, garbled measurement playback: `/home/leah/ANAL_MUSIC/cover_out/track-003/dynamic_ensemble_control/BLACK_ICE/raw_mix.wav`.
- Support alone: `/home/leah/ANAL_MUSIC/cover_out/track-003/dynamic_ensemble_control/BLACK_ICE/ablations/support_only.wav`.
- Lead/bass/drums alone: `/home/leah/ANAL_MUSIC/cover_out/track-003/dynamic_ensemble_control/BLACK_ICE/ablations/rhythm_lead.wav`.
- Lead alone: `/home/leah/ANAL_MUSIC/cover_out/track-003/dynamic_ensemble_control/BLACK_ICE/ablations/lead_only.wav`.
- Independent world contrast: `/home/leah/ANAL_MUSIC/cover_out/track-003/dynamic_ensemble_control/SWISS_SIGNAL/raw_mix.wav`.
- Additional source: `/home/leah/ANAL_MUSIC/cover_out/track-001/dynamic_ensemble_control/SWISS_SIGNAL/raw_mix.wav`.

Compare corresponding original seconds for measurement playback; native covers use the declared beat transport. Listen at the first extracted lead entry and recurring phrases, then contrast support-only with lead/rhythm-only. Per-track queues include measured first-entry timestamps. No `produced_cover.wav` is offered as an accepted final product while structure fails.

## Source defects and native API limits

The corpus audit found packaging receipt disagreement, invalid attack measurements, mislabeled spectral-tilt units, non-identifying room proxies, inclusive harmonic window indices, and sparse/missing bass notes. Supplemental validation streamed 374 FLOAT WAVs and 119 NPZ files (680 arrays): all PCM samples finite, but five −Inf loudness values on track-014 remain quarantined/unusable as finite targets. The two Demucs routes and librosa-derived alternatives are dependent bearings.

The bass-window discriminator independently shows an underresolved 2048-sample/C1/44.1 kHz pYIN configuration and a destructive confidence/segmentation gate. Doubling the window recovers more synthetic and real detections, but does not certify their real pitches. Source evidence was not silently replaced. [Exact probe](docs/BASS_WINDOW_PROBE.md).

Public API gaps are documented in [PUBLIC_API.md](docs/PUBLIC_API.md): native harmony expects complete metric partitions, native form expects supported bar structure, weak Ordered/Topology evidence cannot always coexist with metric voices, secondary identity lanes and uncertain seating do not map cleanly, and the render clock lacks a general source tempo map. Metric note pins do not preserve releases. A tiny measured chord span triggers a general minimum-pad-duration/boundary-tail defect; a synthetic counterexample remains red across profiles. No upstream library code was changed to rescue it. These are separate from source correctness and observer failures.

## Architectural conclusion and next discriminator

The useful object is a time-aligned graph of **observations**, with role projections and musical quotients. The lift is partial: an evidence ceiling, an API constraint and a fresh audio observation are three different gates. Unknown evidence cannot become a free successful coordinate. Musical identity also includes temporal occupancy and rests; an attack set with high F1 can lose phrase behavior and recognition.

The user's mathematical inversion question triggered an actual post-freeze harmonic-family competition probe. The source extractor labels up to six CQT peaks independently and normalizes each stem by its own maximum. A single note's overtones and very quiet separation residue can therefore become an apparent ensemble. The new sparse nonnegative spectral fit makes harmonic families compete to explain peaks. On nine synthetic pitched controls, frame-note F1 rises from 0.486479 to 0.811872 and false pitch frames fall from 3175 to 452. Missing fundamentals and a real octave pair improve; silence/noise false events disappear. However, a quiet chord under noise is rejected completely, and the 371.5 ms analysis window is unsuitable as evidence of precise syncopation. These are occupancy metrics, not the earlier event F1. [Full mathematical probe and exact receipts](docs/HARMONIC_COMPETITION_PROBE.md).

The single best next verdict-changing experiment is to run that frozen harmonic proposal model on a fresh set of short polyphonic HumanMusic stems with known notes across all worlds, measuring **onsets and offsets as well as pitch occupancy**, especially octave-overlap and low-SNR cases. This changes the forward generator away from the additive harmonic assumptions shared by the first probe. Neither sealed holdout should be opened for this development test. No source evidence or cover pin has yet been replaced by the exploratory route.

**Answer to the closed-loop question:** the band-building loop has been executed, not merely described. It produces native admitted fresh audio and measures its failures. But it has not demonstrated the claimed recognizable, structurally faithful native cover. A separate direct playback returns enough identity for partial human recognition under garble. That is a concrete foothold, not the requested success.
