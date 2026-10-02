# Semantic Audio Inversion — Experiment Protocol

Preregistered research plan, 2026-10-02.

This protocol is subordinate to the architecture and epistemic boundaries in
[README.md](README.md). It exists to prevent the implementation from quietly changing the
question after seeing failures.

## Object under test

The object under test is not a waveform-to-score oracle.

It is a family of analyzers

\[
A_f(x) = (Q_f, E, U)
\]

for explicitly declared fidelity relations \(f\), where \(Q_f\) is a semantic musical
quotient, \(E\) is evidence/provenance, and \(U\) is unresolved ambiguity.

The primary first-round target is a quotient compatible in meaning with HumanMusic's
\`CoverMap\` axes and fidelity relations.

## Source boundary

No changes to \`femboy2112/libgibson.git\` are required or authorized by this experiment.
No failure may be "fixed" by weakening HumanMusic truth conditions.

The root of this lab is a frozen older public-API campaign. Do not update its dependency,
helper/IR freeze, or freeze receipts. New work belongs under:

\`experiments/semantic-audio-inversion/\`

as an independent package/tool.

The experiment may inspect the released LibGibson v0.4.0 source and consume public APIs.
If an API gap is found, record it as a finding. Do not patch upstream during the experiment.

## Round 0 — baseline and environment inventory

Before implementation:

1. record repository branch and commit;
2. record OS/arch, Rust, Python, ffmpeg and available CPU/GPU resources;
3. verify the existing lab's frozen validation still passes unchanged;
4. inspect LibGibson v0.4.0 public HumanMusic/audio surfaces used by the design;
5. identify every external analyzer, exact version, license and model-weight license;
6. measure whether each chosen analyzer can execute on the available machine;
7. provide cached-artifact replay for analyzers that are too expensive for every test run.

A tool that cannot run is an access gap, not evidence against the research hypothesis.

## Round 1 — evidence schema before clever inference

Build the neutral evidence IR first.

Requirements:

- schema version;
- source SHA-256;
- sample rate/channel metadata;
- analyzer ID/version/configuration;
- raw time coordinates;
- optional beat-relative coordinates;
- confidence/support fields that preserve analyzer-native uncertainty when available;
- explicit evidence class;
- explicit unknown/refusal;
- no \`CoverMap\` fields hidden inside generic blobs.

Minimum evidence families:

- timing/beat;
- note/transcription;
- tonal/chroma/chord;
- onset/percussion;
- recurrence;
- section/form;
- source/provenance.

Round-1 acceptance: fixtures serialize, deserialize and canonicalize deterministically;
invalid/non-finite/out-of-range observations fail closed.

## Round 2 — calibrated audio fixtures

Build tiny source-generated WAV fixtures with exact truths:

- silence;
- isolated sine pitches;
- two-note intervals;
- major/minor triads and inversions;
- click tracks at multiple BPMs;
- one-bar and two-bar kick/snare patterns;
- monophonic motif with transposed repeat;
- tempo-scaled repeat;
- deliberately off-grid onset;
- detuned pitch;
- reverb tail;
- amplitude/EQ variants.

Do not tune against HumanMusic songs yet.

Required controls:

- positive;
- negative;
- null;
- one-unit mutation.

Every adapter receives an isolated characterization report. An adapter's failure must not be
silently repaired by a downstream heuristic and then counted as adapter success.

## Round 3 — timing transport

Construct raw-seconds and beat-domain views.

Preregistered checks:

- click-track BPM and beat positions;
- time-stretch mutation;
- pickup/anacrusis case;
- at least one tempo-change or drift fixture;
- quantization must preserve the raw coordinate beside the rationalized coordinate.

No global fixed BPM assumption is allowed in the core evidence schema.

## Round 4 — note and role evidence

Run at least two materially distinct routes where practical.

Possible routes:

- full-mix multi-instrument transcription;
- lightweight instrument-agnostic transcription;
- optional separated-stem transcription.

Do not call two parameterizations of one model independent corroboration.

Measure on synthetic fixtures first, then generated HumanMusic.

Preserve:

- onset;
- offset;
- pitch;
- instrument/role label or candidate set;
- confidence/support;
- originating route.

## Round 5 — harmony triangulation

Implement two different harmonic routes:

1. acoustic/chroma/HPCP route;
2. transcribed-note/heard-window route.

Do not force agreement.

For each harmonic interval retain:

- candidate chord(s);
- root;
- quality/family;
- inversion/bass evidence where available;
- temporal span;
- source routes;
- disagreement status.

The quotient projector may select a weaker relation (e.g. QualityFamily instead of Exact)
when evidence supports only that ceiling.

## Round 6 — motif/lick inference

Use explicit transformations from HumanMusic's motif semantics as the starting relation:

- transposition;
- interval contour;
- normalized rhythm;
- direction signature;
- augmentation/diminution;
- fragment;
- inversion;
- retrograde;
- sequence.

Do not fit an embedding threshold to the final holdout.

Preregister a small development corpus, freeze thresholds/relations, then evaluate fresh
generated songs.

Mutation controls:

- one wrong pitch;
- one deleted onset;
- global transposition;
- uniform time scale;
- reordered phrase;
- unrelated phrase with similar rhythm.

## Round 7 — groove and form

Groove:

- infer canonical kick/snare evidence in beat coordinates;
- distinguish metric skeleton from microtiming;
- preserve uncertainty from transcription/onset routes.

Form:

- infer candidate section boundaries using recurrence plus musical change;
- keep topology distinct from exact phrase/bar boundaries;
- repeated material alone must not manufacture semantic section labels.

## Round 8 — quotient projection

Only now implement projection to a HumanMusic-compatible semantic quotient.

Required output per axis:

- requested relation;
- effective relation;
- evidence family/families;
- reason for lowering;
- ambiguity;
- unknown/free distinction.

The projector must be able to say:

- "unknown";
- "derived by method X";
- "two live rivals";
- "insufficient timing resolution";
- "relation ceiling is Metric, not Faithful".

It must never fill missing evidence merely because a stronger preset was requested.

## Round 9 — blind HumanMusic test

Generate a development set and a separately seeded hidden holdout set.

For every item retain ground truth outside the analyzer:

- \`SongMap\`;
- \`PerformancePlan\`;
- \`Score\`;
- direct quotient(s);
- rendered PCM.

The analyzer receives PCM only.

Primary metrics are per-axis; do not collapse them into one truth score.

Report at minimum:

- timing residual;
- line event residual;
- motif-relation recovery;
- harmony relation recovery;
- groove relation recovery;
- form/topology recovery;
- unknown/free/derived classification errors.

A plausible-looking global report cannot compensate for one catastrophically wrong
load-bearing axis.

## Round 10 — hostile transformations

Freeze the analyzer before this round.

For each hidden test track create transformations with preregistered expectations:

| Mutation | Must change | Should remain invariant |
| --- | --- | --- |
| global pitch shift | tonic/absolute pitch | relative motif, relative harmony topology |
| time stretch | BPM/seconds | beat-relative identity |
| EQ | spectral/production evidence | notes/form where still audible |
| compression | dynamics/production | pitch/rhythm identity |
| reverb | tails/acoustic evidence | core metric identity |
| stem mute | corresponding evidence | unrelated surviving axes |
| one-note edit | affected line relation | unrelated form/groove |
| chord edit | affected harmonic relation | unrelated motif |
| onset edit | local rhythm/groove | unrelated pitch contour |

Unexpected changes are residuals to explain, not thresholds to tune away post hoc.

## Round 11 — analysis-by-synthesis probe

When a quotient is recovered, generate a constrained HumanMusic realization only as a
diagnostic.

Do not compare waveform MSE as the main witness.

Compare source and re-realization through independently computed invariant features:

- beat;
- onset;
- chroma;
- note-event structure;
- harmony;
- motif;
- form.

A good re-realization supports consistency of the inferred quotient. It does not establish
uniqueness or authorial truth.

## Round 12 — paired external recordings

Select external material with legitimate structured reference evidence:

- public-domain score plus recording;
- user-supplied MIDI/stems;
- appropriately licensed paired material.

The structured reference and audio analyzer are separate evidence families.

Do not call agreement with a notation source proof that every performed ornament, voicing
or production detail was recovered.

## Holdout discipline

At least one holdout family must remain untouched until:

- schemas are frozen;
- comparison relations are frozen;
- motif thresholds/relations are frozen;
- harmonic arbitration policy is frozen;
- mutation expectations are written.

After contact, no fitting on those examples. A second holdout is required after any
verdict-changing repair.

## Performance / resource discipline

The experiment must support two execution modes:

1. **replay mode**: consume cached, checksummed evidence artifacts; fast enough for ordinary CI;
2. **inference mode**: run external analyzers and emit those artifacts.

Do not make repeated neural inference part of \`cargo test\`.

Heavyweight transcription or separation may be explicitly optional. The core semantic
pipeline and its tests must remain reproducible from retained evidence fixtures.

## Machine-readable receipts

Every inference run should produce a receipt containing:

- source hash and duration;
- decoder and ffmpeg version;
- adapter names and versions;
- model/weights identifiers where applicable;
- command/config;
- wall-clock/resource summary;
- artifact hashes;
- git commit;
- schema version;
- warnings/refusals.

No copyrighted full recording is committed merely to make a test reproducible. Retain
hashes/metadata and use generated/distributable fixtures in-repo.

## Claim levels

Use these meanings in reports:

- **Disclosed**: proven from inspected code or decisive calibrated measurement inside the
  stated boundary.
- **Corroborated**: independent routes survive provenance, residual and holdout checks.
- **Observed**: seen in one exact run.
- **Conjectured**: supported, but truth debt remains.
- **UNVERIFIED**: suitable probe has not run.
- **Dark**: no present discriminator can reach it.
- **Refuted**: decisive probe/counterexample killed it.

Conclusions inherit the weakest load-bearing premise.

## Stop conditions

Stop and report rather than hiding the failure if:

- the beat basis is unstable enough to make downstream metric relations meaningless;
- transcription routes disagree without a discriminator;
- a claimed motif depends on one fitted song-specific exception;
- unknown evidence is being promoted to invariant;
- analysis-by-synthesis succeeds only because the same inferred artifact is reused by both
  sides of the comparison;
- the hidden ground-truth quotient cannot be recovered reliably even on clean HumanMusic
  audio.

A negative result is a useful map of the boundary.

## First-round completion gate

A first research round is complete only when all of the following exist:

1. independent experiment package under the lab;
2. neutral evidence schema and receipts;
3. calibrated timing adapter;
4. at least one transcription adapter plus one materially distinct check/alternate route;
5. basic harmonic triangulation;
6. motif-equivalence implementation;
7. quotient projection with honest evidence ceilings;
8. generated HumanMusic blind-development harness;
9. at least one untouched holdout manifest;
10. mutation suite with preregistered expected invariants;
11. written report separating observed results from hypotheses;
12. existing root lab freeze remains unchanged and verifies.

The deliverable is evidence that the system has begun to **listen**, not a demo that merely
prints chord names.
