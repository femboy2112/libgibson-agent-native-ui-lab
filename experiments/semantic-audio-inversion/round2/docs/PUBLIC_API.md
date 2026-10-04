# HumanMusic public ingress consumed by Round 2

The standalone renderer consumes `libgibson` git revision
`c2f6483d92fe2b351e6cd50936a97d8cdf73cb79` through its Rust public API.
Its independent workspace and lockfile leave the Round-1 and root dependency
receipts intact. No upstream files are changed. This document describes source
contracts, not listening acceptance.

## Actual checked boundary

The public function is `cover`, not `cover_checked`. It executes
`cover_candidate`, then `CoverConformance::check`, then
`PerformanceReceipt::measure_under`. Both checks must pass. A rejection returns
the receipts but discards the composition. Our runner calls those same three
operations separately so it can retain a rejected take and its audio. It never
calls a red take admitted. `cover_skeleton` is an unchecked partial-chart route;
successful return is not checked-cover admission.

`PerformanceProfile::BAND` is the current hardened public profile: rehearsed
actions, arbitrated percussion, vocabulary-constrained harmony, earned support
functions, and the POCKET source/continuation laws. The older `perform` entry
point is historical and does not provide this admission claim. A supplementary
diagnostic probe compares BAND, POCKET, PHRASED, TEMPORAL, and WRITTEN with the
identical map, target frame, world, and seed; its rejections remain rejections.

`CoverMap` has public fields. Direct construction plus `validate()` is the
appropriate ingress for a multi-role observation. `ReferenceSong` is instead a
strict named-voice, monophonic symbolic adapter: it does not infer stems, drums,
orchestration, or sections. Its optional derived harmony uses its own declared
analyzer, which would add a different analysis route rather than import the
measured DeepSeek harmony. Round 2 therefore does not misuse that adapter.

## Invariants recovered from the source

| Object | Actual invariant / limit |
|---|---|
| `CoverSpec` | Canonical axis set; map length remains the shared domain. |
| `CoverKnowledge` | Invariant, explicitly Free, and Unknown are different. Unsupported but observed source coordinates remain in the local model and are disclosed as omitted constraints, not relabeled unknown. |
| `CoverLine` | Lead for Motif, Bass for BassFigure. Riff can occupy only Lead or Bass. Strictly ordered monophonic attacks; relative chromatic pitches; one global octave remains free. Conflicting identities on one lane are refused. |
| Line Metric | Exact projected attacks and relative pitch. Gate duration is free. It is a whole selected line, not evidence that a principal hook was independently discovered. |
| Line Faithful | Metric plus observed rest limits; gates may shorten but not fill known rests. |
| Theme | Motif only; first eight beats become thematic material. It does not retain original statement placement. |
| Harmony | Metric spans must partition the complete `[0,length]` domain without gaps. Exact or QualityFamily preserves roots and span endpoints. All chords must fit target vocabulary; family transport is not permission to change a root or major/minor family. |
| Form | Metric maps require exact partitioned 4/4 bar spans and section families. Topology alone belongs to the separate unmetered ordered-chart route. |
| Groove | Kick/snare only. PocketSkeleton retains quarter-beat-grid strokes; KickSnare retains all supplied strokes. Hats/dynamics/timbre are free. Source fills must not be silently promoted to anchors. |
| Seating | Exact five-seat per-bar vectors: Lead, Keys, Pad, Bass, Drums. Soft role occupancy cannot be asserted as exact seats. |
| `MetricPosition` | Reduced rational beats; `from_exact_beats` is not a quantizer and rejects unrepresentable float denominators. |
| Target | World, seed, grammar, performance options, and profile. Source composition, source seed, or hidden target score cannot cross the map boundary. |
| Identity projection | Reads declared identity material; a lane is an instrument, not automatically its identity. Pinned-source events receive native `cover-identity` provenance. |
| Fingerprints | Canonical fingerprint is a native deterministic identity receipt, not a cryptographic hash or an acoustic-quality measure. SHA256 supplements it for files and constraints. |

The renderer uses a declared binary rational measurement projection (1/256 beat)
and nearest equal-tempered semitone. Maximum onset and pitch errors are emitted;
the fractional measurements remain in the source object. It does not change a
note from musical familiarity with a recording. Equal projected onset rivals
are refused rather than selecting a convenient pitch. Fewer than three lane
events are retained observations but insufficient for an identity-bearing pin.

## Explicit gaps and the partial diagnostic route

The observed corpus frequently supplies Ordered harmony and Topology form.
These cannot be combined with metric lead/bass lines through the existing
public metric ingress. Silently manufacturing durations, resolving chord
quality rivals, inventing exact seats, or strengthening a quarter-grid pocket
into a complete drum part would overstate the evidence.

For these cases `full_profile_ingress_refusal.json` records the full-request
refusal. Separately, the predeclared partial diagnostic lift pins only supported
metric lines and any fully representable axes. Candidate configs explicitly say
`partial_diagnostic` and `full_structural_success: false`. A partial admission
does not erase the full-request refusal. Source-vs-render observation is still
required, even on axes where internal native conformance holds.

The native model also has no third independent secondary-riff lane. It cannot
preserve a lead hook and different guitar riff on the same Lead lane as distinct
constraints; inconsistent same-lane pins are rejected. Exact instrumentation
and source production are outside the cover quotient.

An additive `native-pocket` treatment supplies a valid observed quarter-grid
origin through one positive preroll: `shift = (-beat_origin_offset) mod 1`.
It applies the same shift to every melodic attack, raw/canonical drum attack,
harmonic/section boundary, and domain endpoint. Canonical pocket members then
occupy integer beats. Acoustic microtiming is retained separately and does not
rewrite membership. The members are provisional corpus inferences supported
by closeness to a beat and recurrence across bars; unknown fill labels are not
claims of confirmed core-groove truth. The local transport receipt declares this
boundary, and the observational scorer must remove the known global preroll.
This is a stronger-pin treatment in a separate directory, never a replacement
for a failed original candidate. Ordered harmony and Topology form still retain
their ingress refusal.

## Rendering and free coordinates

The runner preserves source tempo and reference tonic (or declares chromatic
origin zero if no tonic was observed), uses the three unmodified world palettes,
and selects two declared seeds. It sets swing to zero because source metric
coordinates already contain the measured timing; applying a new inferred swing
would double-transform that evidence. Target register/articulation and the
unpinned accompaniment remain native generation decisions.

`HumanMusicSynth::new(score,world,SampleRate)` plus `OfflineRenderer` and
`write_wav_i16` is device-free. `StemMask` exposes lead, bass, drums, keys, pad,
and sfx. We render all masks over `total_samples()` at the same sample rate and
with the identical score/world. An extra keys+pad support surface is supplied.
These are native masked production renders, not an assertion that summing the
solos is sample-identical to the full nonlinear production bus.

World production includes bus gains, patch/ADSR/filter/pan, saturation, room,
chorus and tempo echo. Changing production may change observed acoustic
lifetime even when note identity is unchanged. The optional lab post-production
stage therefore belongs after structural observation, and preserves raw audio.

## First concrete refusal mechanism

The first full-length real-source diagnostic generated internally conformant
Lead/Bass covers in BLACK_ICE and VAPOR95 but failed ordinary admission. The
native independent temporal observer found pad chord-tone gates held across a
very short generated harmonic span into a nonmember chord. The short harmonic
span arose between closely spaced imported attacks from separate measured
lanes. In the inspected witness the pad gate was 0.1 beat while the next harmony
began about 0.04 beat later. The source measurement timing and the native
minimum-duration behavior interact; this is not proof that the source recording
contains that harmonic transition.

The same fixed-map profile probe retains the failure under all five public
profiles (BAND has fewer false claims than older profiles). SWISS_SIGNAL refuses
the simultaneous-pitch constraint when its vocabulary has no lawful chord.
These are consumer observations, not upstream repairs. Per-note proof receipts
remain local with the copyrighted-input-derived artifacts.

A synthetic regression reproduces this without any musical data from a song:
32 beats, three G lead attacks at beats 0, 8 and 16, and G-minor harmony except
for C minor during `[4,4+10/256)`. At the reviewed anchor, BLACK_ICE BAND seed
220901 passes native cover conformance but returns a checked rejection with
two false temporal-function claims. The test deliberately verifies that this
known-red admission stays red; it does not call the underlying behavior fixed.
The independent all-profile diagnostic also reproduced the two claims in
VAPOR95. This isolates a native duration interaction from the uncertainty in
real-song transcription.

## Commands

```sh
cargo test --offline --manifest-path renderer/Cargo.toml
cargo build --release --offline --manifest-path renderer/Cargo.toml
renderer/target/release/sai-round2-renderer dev /LOCAL/round2-dev 22050
renderer/target/release/sai-round2-renderer cover /LOCAL/model.json /LOCAL/track 220901,220902 22050
renderer/target/release/sai-round2-renderer probe /LOCAL/model.json /LOCAL/profile-probe
renderer/target/release/sai-round2-renderer native-pocket /LOCAL/model.json /LOCAL/track/treatments/native-pocket 220901,220902 22050
```

`dev` creates six freshly generated development objects, two trace shapes in
each world, using a distinct documented Round-2 seed family. Ground-truth note,
chord, drum, and section exports are for scorer use only; the WAV analyzer must
not receive them. `holdout OUT SR` uses the distinct seed family
`9220100 + world_index*17 + trace_index`, declares `holdout: true`, and emits
`sai.round2_holdout/v1`. It is only for execution after the instrument freeze;
the generator has no analysis/evaluation route. Both dataset commands require a
fresh empty directory. The original Round-1 holdout is never accessed.
