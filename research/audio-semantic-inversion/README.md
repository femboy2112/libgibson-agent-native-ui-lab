# Semantic Audio Inversion
## Teaching the HomoSapians to listen to raw audio and learn the music

Status: **research program / UNVERIFIED experiment**

Repository role: this work lives in the external LibGibson lab. It must not modify
\`femboy2112/libgibson.git\` merely to make the experiment pass.

LibGibson reference inspected for this proposal:
\`femboy2112/libgibson@c2f6483d92fe2b351e6cd50936a97d8cdf73cb79\`
(the v0.4.0 HumanMusic & Audio release merge, 2026-10-02).

## 1. Research question

HumanMusic already supplies a forward semantic-realization pipeline:

\[
S \xrightarrow{C} M \xrightarrow{P_{w,\ell}} \Pi
\xrightarrow{E} \Sigma \xrightarrow{R_{w,p}} x(t)
\]

where, approximately:

- \(S\) is a \`SemanticTrace\`;
- \(M\) is a \`SongMap\`, the song-level identity;
- \(\Pi\) is a \`PerformancePlan\`;
- \(\Sigma\) is a realized \`Score\`;
- \(x(t)\) is rendered PCM after synthesis and production.

The experiment asks whether a useful part of this process can be traversed in the
opposite direction for an arbitrary host-supplied recording.

The naive target,

\[
x(t) \mapsto S,
\]

is not identifiable in general. Different scores, performances, timbres and production
chains can produce acoustically equivalent evidence, and even perfect musical structure
does not uniquely reveal the composer's extramusical intent.

The viable target is an approximate inverse **after quotienting away distinctions that
the requested musical identity relation does not care about**.

For a fidelity relation \(f\), let

\[
q_f : \Sigma \rightarrow Q_f
\]

be the semantic/identity quotient. The primary experimental object is an analyzer

\[
A_f : \mathrm{Audio} \rightarrow (Q_f, E, U)
\]

where:

- \(Q_f\) is the recovered musical quotient;
- \(E\) is explicit evidence/provenance for every recovered coordinate;
- \(U\) is unresolved ambiguity, including competing hypotheses.

The central falsifiable claim is not that \(A_f\) reconstructs the original score. It is:

\[
A_f(R(\Sigma)) \approx q_f(\Sigma)
\]

for declared classes of HumanMusic-generated and external audio, with the approximation
relation and error surface stated per axis.

## 2. Why LibGibson is unusually well positioned

The current HumanMusic architecture already contains most of the desired codomain.

### 2.1 Existing quotient object

\`src/audio/human_music/cover.rs\` defines \`CoverMap\` as a quotient value that contains
no source seed, semantic trace, complete plan, score, patch or lookup key. It retains
selected identity axes:

- Motif
- Riff
- BassFigure
- HarmonicContour / HarmonicLoop
- Groove
- Form
- Orchestration

This is already close to the desired semantic representation of a real recording after
irrelevant realization details have been discarded.

### 2.2 Existing fidelity relations

\`cover_fidelity.rs\` already rejects a single vague "similarity" score in favor of exact
relations per axis:

- line: Free / Theme / Metric / Faithful;
- harmony: Free / Ordered / QualityFamily / Exact;
- groove: Free / PocketSkeleton / KickSnare;
- form: Free / Topology / Exact;
- orchestration: Free / Exact.

The inverse experiment should preserve this discipline. "Semantically useless" is never a
global property of an audio feature; it is relative to the declared quotient.

### 2.3 Existing source boundary

\`ReferenceSong\` already ingests external symbolic observations and projects them into
\`CoverMap\`. It separates source observations from derived harmonic analysis and from
unknown axes.

The missing pathway is therefore narrow and explicit:

\[
\text{raw audio}
\rightarrow
\text{calibrated musical observations}
\rightarrow
\text{ReferenceSong-like evidence object}
\rightarrow
\text{CoverMap / semantic quotient}.
\]

### 2.4 Existing real-audio transport

\`src/audio/media.rs\` already decodes local host-provided media through ffmpeg to canonical
stereo f32 PCM (\`ClipAudio\`). File decoding, sample-rate normalization and safe local
source handling therefore need not be reinvented conceptually. This lab may implement its
own adapter rather than changing LibGibson.

### 2.5 Existing semantic machinery above notes

HumanMusic already contains useful analyzers/representations for:

- motif identity and transformations;
- harmonic context and heard chord identity;
- phrase/form topology;
- groove coordinates;
- pitch function;
- discourse and listener-facing meaning;
- explicit source/observer witnesses.

The reverse experiment should reuse their *semantics* where possible, but never manufacture
provenance that only the forward generator knows.

## 3. Non-goals and hard epistemic boundaries

The project does **not** initially claim to recover:

1. the original HumanMusic \`SemanticTrace\`;
2. an author's psychological intention;
3. the original generator seed;
4. forward-only provenance/action ledgers;
5. exact production parameters;
6. a unique original score where the mixture is observationally ambiguous.

Extramusical labels such as Danger, Success, UI emphasis or narrative event kinds may later
be reported only as conditional hypotheses under an explicitly declared generative model.
They are not facts recovered from sound.

Musical semantics are a stronger and more legitimate target: motif identity, recurrence,
tension, harmonic function, groove, expectation, release, section role and transformations
can be derived from musical evidence with explicit uncertainty.

## 4. Proposed architecture

The analyzer should be a staged evidence pipeline, not one opaque model:

\[
\text{PCM}
\rightarrow
\text{AcousticEvidence}
\rightarrow
\text{MusicalEvents}
\rightarrow
\text{StructuralHypotheses}
\rightarrow
\text{SemanticQuotient}.
\]

### Stage A — canonical audio boundary

Input is a local host-supplied recording. Decode to a canonical representation:

- sample rate recorded in receipt;
- mono and stereo views available;
- source hash;
- no source audio committed by default;
- exact analyzer versions and command lines retained.

### Stage B — independent low-level observation routes

At minimum, keep timing and transcription as separate constraint surfaces.

Candidate adapters, not authorities:

- **Beat This!** for beat/downbeat timing;
- **MuScriptor** for modern multi-instrument full-mix transcription;
- **Basic Pitch** as a lightweight, instrument-agnostic alternate transcription route,
  especially useful for isolated or simplified material;
- optional source separation as an ablation before per-stem transcription;
- HPCP/chroma/tonal analysis, e.g. Essentia, as an independent harmony route.

Heavy models must be adapters behind stable files/IR. The experiment must remain runnable
with cached observations, and a lightweight CPU-capable baseline must exist even if a
heavier model is used for stronger measurements.

### Stage C — normalized musical evidence

Do not immediately coerce model output into \`CoverMap\`. Preserve a richer evidence object.

Suggested conceptual records:

- \`BeatEvidence\`: beat/downbeat timestamps, tempo segments, meter candidates;
- \`NoteEvidence\`: onset, offset, pitch, instrument/role candidate, confidence, method;
- \`OnsetEvidence\`: transient/onset families, percussion candidates;
- \`TonalEvidence\`: chroma/HPCP frames, key candidates, chord candidates;
- \`RecurrenceEvidence\`: self-similarity/recurrence links;
- \`SectionEvidence\`: candidate boundaries and repeated-section equivalence;
- \`SourceEvidence\`: source hash, decoder, analyzer identity, versions and parameters.

Every inferred record carries provenance. No raw model score is silently promoted to
"Observed".

### Stage D — beat-domain transport

Seconds are not song identity. Construct a beat-domain coordinate system from timing
evidence while retaining the raw seconds view.

Do not destroy disagreement. If two beat trackers or tempo hypotheses disagree, represent
the live alternatives until a discriminator resolves them.

Required views:

- raw seconds;
- beat-relative positions;
- rationalized/quantized metric positions only when warranted;
- local tempo map for rubato or drift.

### Stage E — musical-object inference

Infer candidate objects with explicit rival sets:

- melody and bass lines;
- chord spans and competing chord interpretations;
- kick/snare pocket;
- motifs/riffs and transformations;
- phrase/section boundaries;
- orchestration/role occupancy.

Harmony must be triangulated. For example:

\[
H_1 =
\text{PCM}\rightarrow\text{HPCP/chroma}\rightarrow\text{chord candidates}
\]

versus

\[
H_2 =
\text{PCM}\rightarrow\text{note transcription}\rightarrow
\text{heard pitch-class windows}\rightarrow\text{chord candidates}.
\]

Agreement is corroboration. Disagreement is evidence about ambiguity, transcription error,
nonharmonic tones, rootless voicing, inversion, effects or model failure.

### Stage F — identity quotient

Only after the evidence graph exists should it be projected into a \`CoverMap\`-like object.

The projection must preserve the existing epistemic distinction:

- established from direct source evidence;
- derived analysis, with method;
- unknown;
- deliberately free under the requested fidelity relation.

An audio transcription model's "C4 at 2.13 s" is itself derived analysis of PCM, not an
unmediated source fact. The lab should therefore introduce a richer evidence vocabulary than
the symbolic \`ReferenceSong\` boundary if necessary.

### Stage G — optional higher semantics

After motif, harmony, form and groove are stable enough, infer higher musical hypotheses:

- Establish
- Learn / Reinforce
- Develop
- Prepare
- Miss
- Open
- Reset
- Recognize
- Payoff
- Answer

These are listener-facing musical interpretations, not reconstructed authorial intention.
They must carry the lower-level evidence on which they depend.

## 5. Motif and lick inference

HumanMusic already has a strong candidate invariant in \`motif.rs\`.

A motif identity strips away absolute transposition and absolute tempo while retaining
interval contour, normalized rhythmic profile and direction. It also has typed transforms:
transposition, inversion, retrograde, rhythmic scaling, fragmentation, sequencing and
concatenation.

The reverse analyzer should exploit this rather than use a language model to declare riffs.

A candidate route:

\[
\text{transcribed note stream}
\rightarrow
\text{phrase/window candidates}
\rightarrow
\frac{\text{windows}}
     {\text{transposition, tempo and declared transforms}}
\rightarrow
\text{motif families}
\rightarrow
\text{recurrence/development graph}.
\]

The system should retain exact notes beside normalized identities so the quotient can be
audited.

## 6. Analysis by synthesis

A recovered quotient can be tested by closing the loop:

\[
x
\xrightarrow{A_f}
q
\xrightarrow{\text{HumanMusic constrained lift}}
\hat{x}.
\]

Waveform MSE is the wrong witness because timbre and production may differ while musical
identity is preserved.

Compare invariant evidence instead, e.g.

\[
D(x,\hat{x}) =
w_b D_{\rm beat}
+ w_o D_{\rm onset}
+ w_c D_{\rm chroma}
+ w_n D_{\rm notes}
+ w_h D_{\rm harmony}
+ w_m D_{\rm motif}
+ w_f D_{\rm form}.
\]

This is a consistency probe, not proof that the inferred quotient is uniquely true.

## 7. Calibration hierarchy

### Phase 0 — fixtures and adapter calibration

Use tiny synthetic positive/negative/null controls to verify every parser and coordinate
transform before touching a real song.

Examples:

- one sine note;
- two simultaneous tones;
- known triads/inversions;
- click track with known BPM/downbeats;
- kick/snare skeleton;
- deliberately detuned and off-grid mutations.

### Phase 1 — blinded HumanMusic reconstruction

Generate fresh HumanMusic songs and retain the complete hidden ground truth:

\[
(\text{SemanticTrace},\text{SongMap},\text{PerformancePlan},\text{Score},\text{PCM}).
\]

Expose only PCM to the analyzer.

Primary target: recover the quotient computed directly from the hidden score, not the full
hidden score.

Measure per axis:

- onset/pitch event precision and recall where applicable;
- metric transport error;
- motif-equivalence recovery;
- harmony span/root/quality relation;
- groove relation;
- form topology;
- orchestration relation;
- exact unknown/free/derived classifications.

### Phase 2 — hostile invariance mutations

Apply controlled audio transformations whose expected semantic effect is known.

Examples:

- global pitch shift: relative motif/harmony survives, tonic changes;
- time stretch: beat-relative identity survives, BPM changes;
- EQ/compression/reverb: production evidence changes, core identity should mostly survive;
- stem mute: removed evidence must become absent/unknown, not hallucinated;
- single-note mutation: corresponding line identity changes locally;
- chord mutation: harmonic quotient changes without rewriting unrelated melody;
- timing perturbation: distinguish expressive microtiming from metric identity;
- added reverb tail: duration/production changes must not invent new note events.

Define expected pass/fail/ambiguous outcomes before each mutation family.

### Phase 3 — symbolic/audio paired external music

Use recordings for which a legitimate structured reference is available: public-domain
notation, licensed stems/MIDI, or user-supplied ground truth.

The structured source is a separate evidence family, not an oracle for every production
detail.

### Phase 4 — ordinary real recordings

Only after the blind and paired-source phases pass should the analyzer be evaluated on
ordinary mixed recordings with partial ground truth.

Claims here must be narrower: consistency, useful analysis and calibrated ambiguity, not
"the true score".

## 8. Required controls

Every serious result needs:

- positive control;
- negative control;
- null control;
- mutation control;
- fresh holdout not used to tune thresholds;
- at least one alternate inference route for load-bearing coordinates;
- preserved raw outputs and exact versions;
- deterministic normalization where possible;
- explicit refusal/unknown states.

A model agreeing with itself through two wrappers is not independent corroboration.

## 9. Claim ledger for this program

Current status at document creation:

- **Disclosed:** LibGibson v0.4.0 contains the forward HumanMusic layering, \`CoverMap\`,
  fidelity relations, symbolic \`ReferenceSong\` extraction and local real-audio decoding
  described above.
- **Corroborated externally:** practical beat tracking and automatic music transcription
  systems exist and can supply candidate evidence routes.
- **Observed:** no reverse-audio reconstruction run has yet been performed in this lab.
- **Conjectured:** a \`CoverMap\`-like quotient can be recovered usefully from real mixed audio.
- **UNVERIFIED:** all quantitative reconstruction quality claims.
- **Dark:** unique authorial intent and any forward-only provenance not encoded in the sound.
  The missing lamp would be independent contextual/source evidence, not a better waveform
  analyzer.

## 10. Initial implementation boundary

This lab should implement the experiment as an independent consumer, not by editing
LibGibson internals.

Recommended location:

\`experiments/semantic-audio-inversion/\`

It should be a standalone package/tool with its own lockfile and documented dependency on
the reviewed LibGibson v0.4.0 surface or a pinned v0.4.0 commit. Do not mutate the root
lab's frozen pre-v0.2 dependency or freeze receipts.

Suggested top-level architecture:

- \`audio/\`: decode/hash/canonicalization;
- \`adapters/\`: beat, transcription, tonal and optional separation adapters;
- \`evidence/\`: versioned neutral evidence schema;
- \`transport/\`: seconds ↔ beat/rational coordinate hypotheses;
- \`infer/\`: notes, harmony, groove, motifs, form, roles;
- \`quotient/\`: projection into HumanMusic-compatible identity relations;
- \`synthesis_probe/\`: optional constrained-lift comparison;
- \`receipts/\`: machine-readable run manifests;
- \`fixtures/\`: tiny generated or distributable fixtures only;
- \`tests/\`: unit, mutation, integration and holdout harnesses.

Do not make a heavyweight ML runtime a Rust library dependency. Treat inference systems as
versioned external adapters producing stable artifacts that the experiment can consume,
cache and replay.

## 11. First success criterion

The first meaningful victory is intentionally narrower than "understand arbitrary music":

> Given a fresh HumanMusic WAV whose hidden score was not exposed to the analyzer, recover
> a provenance-bearing quotient that agrees with the hidden score's direct quotient on
> beat-relative motif identity, a useful harmonic relation and a groove relation, while
> correctly marking unsupported axes unknown and surviving preregistered pitch/time/production
> mutations.

That result would establish a real inverse path through the quotient.

Only then should we pay the truth debt required to claim that the HomoSapians can listen to
an arbitrary human recording and learn its musical structure.

## 12. Candidate external instruments

These are replaceable adapters, not trusted authorities:

- MuScriptor: https://github.com/muscriptor/muscriptor
- Beat This!: https://github.com/CPJKU/beat_this
- Basic Pitch: https://github.com/spotify/basic-pitch
- Essentia tonal/chord analysis: https://essentia.upf.edu/
- Demucs family as an optional separation ablation:
  https://github.com/facebookresearch/demucs

Licensing, model-weight terms, compute requirements and reproducibility must be checked
before any adapter is adopted. In particular, code licensing and model-weight licensing
may differ.

## 13. Verdict-changing probes

The following outcomes would materially change the project:

1. **Kill:** even on clean hidden HumanMusic renders, independent timing/transcription routes
   cannot recover stable quotient coordinates above trivial tempo/key.
2. **Narrow:** symbolic identity is recoverable only after stem isolation; then the valid
   claim is a separated-source analyzer, not full-mix listening.
3. **Proceed:** blinded HumanMusic quotient recovery survives holdouts and adversarial
   transformations with calibrated uncertainty.
4. **Escalate:** paired external recordings show the same quotient machinery transfers without
   song-specific heuristics.
5. **Higher-semantics gate:** discourse/meaning inference is attempted only after lower-level
   residuals stop explaining its apparent successes.

The experiment certifies dominance of a working representation within tested boundaries,
never finality.
