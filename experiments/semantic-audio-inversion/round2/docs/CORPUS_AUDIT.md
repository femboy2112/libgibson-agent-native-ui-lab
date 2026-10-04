# Round 2 corpus audit

The audit reads the immutable local extraction tree; all event content and private audio stay outside Git. These are measurement outputs, not source truth. No extraction was rerun.

| Receipt | SHA256 |
| --- | --- |
| source MANIFEST.json | `888c15d59aba377aa3e9177d62276e5e2a21738e7423914912b2a3e4fd6f5164` |
| bundles/BUNDLE_MANIFEST.json | `83958454e51f68f07e324f73e1a73ea4dcac9e5f2ae42c3843370cc1b82b5744` |
| reverse-humanmusic-analysis.tar.zst | `6f73f51e6a6474ee97fde523b94b072c01b22b1924361a66e802250f372c605a` |
| reverse-humanmusic-private-stems.tar.zst | `a28ce87e484783347da23944482e86d2809d62b1f2b462f3c4ad5ee089ba222c` |

Both compressed archive hashes match their bundle manifest. All 17 source MP3 hashes match. The audit hashed 1,806 track files, checked JSON syntax and finite values, event ordering, pitch/confidence bounds, spans, referenced paths, source IDs, stem receipt hashes, and live bundle member hashes. All 17 imports succeeded. Validation does not establish transcription accuracy.

The analysis archive has three live-tree differences: `FINAL_REPORT.md`, `FINAL_REPORT.json`, and `environment/finalize.log`. The private stems archive has no live member differences; all 476 actual archived members also match their declared hash and size. Streaming the actual analysis archive independently verifies 1,489 members and exposes one internal packaging mismatch: `environment/finalize.log` is 3,645 bytes in the archive but 3,737 bytes in the member manifest. Its archive hash is `a38103a53c7689e07093c63be4f4fe84d6722d6e3a97367fc04e2473e835179b`; declared member hash is `60daecc539b089d6340a0091223427f54a8d990250bf58cebd7e2c7c5c56c4e4`. The compressed archive hash does not repair this failed member receipt. These report snapshots are preserved as distinct versions; current blueprint hashes identify the consumed musical evidence.

| Source | Source SHA256 | Seconds | Vocal notes | Bass notes | Harmonic spans | Motif families | Sections |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| track-001 | `8df84e48a9f2a633aa99524e75679dbddcac4f5601cc38eeafdb111e5e599188` | 228.467 | 114 | 137 | 400 | 23 | 9 |
| track-002 | `00fc284a81d644fd402294999a7c7b4ee89559b758cf503a139d60e1590c96e3` | 208.823 | 389 | 94 | 243 | 33 | 8 |
| track-003 | `9b05b6100e4d2bc3ed6a0ff7656dc96fd5aae5b84fadb650c7d5d43df1f06dad` | 179.304 | 269 | 1 | 206 | 30 | 7 |
| track-004 | `19a9762a096ae96d5c2bd83ba027c161ce208dc7e39b854aff43507b2aa5cc5a` | 197.146 | 424 | 3 | 266 | 30 | 10 |
| track-005 | `864e5f6669eab1e6c7825f1d579f63fda78f51706a09e0d39e0b83dbaddb55fd` | 255.451 | 105 | 107 | 296 | 40 | 11 |
| track-006 | `b31a12978a599ce8e845004ac71dbc2e37a48633a2a6e12c5fb5b81167859ca7` | 214.962 | 323 | 1 | 154 | 30 | 9 |
| track-007 | `a72cf4b6ad065b607c2844cf13d78e2e5d280467325d82cc8fe3099bf5ef3c41` | 197.982 | 126 | 0 | 361 | 30 | 10 |
| track-008 | `65b8de72fca1cec81c73c1d93193814045ff462b4c6948a3b2868bbe7651dab1` | 262.139 | 219 | 46 | 369 | 40 | 13 |
| track-009 | `7bbb9107df2fc01ac76fb15c8563fa447389d491f86aeee0010e70e707c52777` | 237.923 | 141 | 4 | 291 | 30 | 9 |
| track-010 | `8e118d0e2bb59be58f5686622eedc868e8407a3ba7acff9f4f76c7ab89c0e5ab` | 226.691 | 143 | 0 | 250 | 30 | 13 |
| track-011 | `f76a3475785e4beb06d01db28095c20e1bec087d816354fa03836b65eafb095a` | 234.841 | 165 | 47 | 480 | 33 | 10 |
| track-012 | `9776f4a6f2e153c5e3c8c94aab006178317cc4fb892133175b5cf0971c074157` | 242.024 | 311 | 37 | 298 | 34 | 7 |
| track-013 | `68610df3b950be927a7beffdfe3e4f4e721a8a14f98bc03c3ded191dce1e6d39` | 160.183 | 111 | 0 | 177 | 21 | 12 |
| track-014 | `51556851e4ebb3afdb73df4a3c1a4c16c8b614b6420076184f6a90a3a6e3c292` | 203.128 | 227 | 0 | 284 | 30 | 11 |
| track-015 | `fc7521223cf48317f2910f89fab403efd0ee684caa78e7e044fa4bf5235d8bb4` | 256.893 | 52 | 6 | 399 | 10 | 17 |
| track-016 | `627ddae0436906f51292425868381408cba461c92ab83895410b2565edf10696` | 311.359 | 256 | 1 | 482 | 30 | 10 |
| track-017 | `d2382e080b2a1baf190e9ee6e48a17725ca7e124cd4ac1d8dc866af1a29a836b` | 376.587 | 214 | 70 | 506 | 33 | 8 |

Every source includes htdemucs_ft bass/drums/other/vocals and htdemucs_6s bass/drums/guitar/other/piano/vocals; cross-model logmel comparison; tempo candidates, beat/downbeat CSV; pYIN plus YIN note-route support for vocal/bass; CQT polyphonic note candidates for guitar/piano/other; drum-family heuristics and microtiming; temporal harmonic spans with root/quality rivals; recurring motif families; section labels and recurrence links; section-level role occupancy; and production/stereo features. Full per-track values, exact artifact paths/hashes, commands, provenance receipts and unresolved fields are in local `/home/leah/ANAL_MUSIC/cover_out/CORPUS_INVENTORY.json`.

Observed defects and limitations:

- All tracks have invalid negative attack measurements: 391 occurrences across duplicated artifacts (23 per track). Archived `80_production.py` subtracts an absolute onset sample index from a segment-relative sample index. Importer sets the unusable blueprint value to null, records quarantine, and retains the raw artifact/hash.
- The field named `spectral_tilt_db_per_octave` is computed from log2(power) versus log2(frequency); the label is dimensionally wrong without a factor of `10*log10(2)`. It is not admitted as a dB production target.
- The decay/reverb proxies use the first individual sample below a threshold after a peak. They do not identify room decay and can be dominated by waveform zero crossings. Echo autocorrelation is also compatible with rhythmic periodicity.
- Harmonic `end_beat` is the inclusive index of the last contributing window, sometimes equal to `start_beat`. V2 transports the raw start/end seconds through observed beat times; it never treats these indices as half-open spans.
- pYIN/YIN routes share stem and implementation family; separator variants share Demucs lineage. Agreement is corroborating measurement support, not independent truth probability. Model confidence and event-level software version are absent.
- Bass note evidence is absent on four sources and extremely sparse on several others. A separated bass file is not a recovered BassFigure. Fewer than four notes cannot receive the metric bass ceiling.
- Tempo octave, beat phase, role assignment, harmonic quality/root rivals, principal motif identity, and section labels remain derived hypotheses. Unknown orchestration seats, fills, lyrics and intent are not filled.

The additive `sai.stem_evidence/v2` model retains raw seconds, observed metric coordinates, note-route support, role rivals, source hashes, artifact hashes, separator identity and disagreement, harmonic rivals, form recurrence and production observations. Candidate roles use register and observed polyphony in addition to stem labels. A monophonic high guitar may be Lead/Riff; a polyphonic vocal lane is not forced to Lead. Notes remain fractional MIDI until the compiler declares quantization.

Beat coordinates have the recording origin at zero. `beat_origin_offset` gives the observed beat-index origin in that coordinate system. Canonical drum membership requires a recurring family/beat-class and at most 0.15 beat displacement; raw microtiming remains separate. Fills remain unknown. Any renderer that adds a positive preroll must shift every lane together and receipt the transport.

Reproduction from the repository root:

```sh
PYTHONPATH=experiments/semantic-audio-inversion/round2/python python -m sai_v2.corpus \
  --corpus /home/leah/ANAL_MUSIC/analysis_out \
  --out /home/leah/ANAL_MUSIC/cover_out
PYTHONPATH=experiments/semantic-audio-inversion/round2/python python -m unittest discover \
  -s experiments/semantic-audio-inversion/round2/tests -p test_corpus.py -v
```

Eighteen synthetic tests cover schema round-trip, invalid hashes/times/confidence, non-finite values, provenance requirements, ambiguity preservation, unknown versus free, deterministic identity independent of production, behavioral role recovery, and tempo-scaled beat transport. The old holdout was not accessed. Inventory and provenance integrity are Observed; musical source correctness remains unresolved.

Metric statuses are representation ceilings over a measured lane: four or more note observations with beat coordinates can represent Metric constraints. They do not certify principal-hook selection, octave correctness, transcription accuracy or recurrence. The importer requires at least four bass notes and retains sparse/absent BassFigure evidence as unknown. The source-level unknown list explicitly preserves principal melody ambiguity.
