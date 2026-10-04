# Inverse Causal Synthesis

Status: architectural research note / leading Round-3 hypothesis

This note is motivated by the measured Round-2 failures.

Stem isolation substantially improves note recovery, but it does not solve harmonic or groove identity. The archived peak-as-note route mistakes partials and residual spectral structure for independent notes. A bounded harmonic-family competition probe materially improves pitch occupancy on known controls, while still failing a quiet chord in noise. The bass-window probe separately shows that a physically mismatched observation scale can erase low-frequency information before any semantic reasoning occurs.

Together these results support a stronger hypothesis:

> Reverse HumanMusic should be treated as inverse causal synthesis, not as direct feature-to-label transcription.

The operational rule is:

\[
\boxed{\text{explain downward before quotienting upward}}
\]

When inferring a semantic layer, model at least one causally upstream primitive layer that generates the measured acoustic evidence. Only after the signal has a plausible causal explanation should nuisance realization variables be quotiented or marginalized away.

---

## 1. The direct inverse is too coarse

The naive inverse is

\[
Y \longrightarrow M
\]

where Y is audio and M is musical structure: notes, rhythm, harmony, motif and form.

Round 1 and Round 2 show why this is structurally incomplete.

Even after source separation, one stem is not a symbolic lane. It is still the output of a physical or virtual sound-producing process. A single nominal note can produce a fundamental, many harmonics or partials, missing or weak fundamentals, inharmonic partials, attack noise, body resonances, sympathetic resonances, nonstationary decay, bends, vibrato, nonlinear distortion, intermodulation, room response and separator residue.

A peak detector can therefore confuse effects of one latent source with additional latent sources.

The harmonic-family competition probe already demonstrates this specific pathology: when harmonically related spectral peaks compete as explanations of one latent note instead of being labelled independently, pitch occupancy improves dramatically on several known controls.

---

## 2. Primitive-above principle

For every semantic object we want to recover, model at least one causally upstream primitive that could have generated its observation.

A useful general hierarchy is

\[
\boxed{
M \rightarrow G \rightarrow E \rightarrow R \rightarrow O \rightarrow Y
}
\]

where

- M is the musical object: pitch, rhythm, chord, motif or phrase;
- G is performer or controller gesture;
- E is physical or virtual excitation;
- R is the resonant or synthesis state of the instrument;
- O is the output chain: radiation, pickup, amplifier, microphone, room, effects and mastering;
- Y is the observed waveform.

The word above means causally upstream of the layer being inferred.

For inversion, the desired path is

\[
Y
\longrightarrow
(\hat O,\hat R,\hat E,\hat G,\hat M)
\longrightarrow
(\hat G_{\mathrm{meaningful}},\hat M).
\]

The second arrow is a quotient or marginalization step. Nuisance realization variables are discarded only after they have helped explain the acoustic evidence.

---

## 3. Guitar example

For a plucked guitar string, the same nominal pitch can sound radically different depending on upstream primitive variables such as

- pick or finger hardness;
- attack velocity;
- attack angle;
- pick penetration depth;
- pluck position along the string;
- travel direction;
- force versus time;
- palm and fret muting;
- fretting pressure;
- bend trajectory;
- vibrato;
- pickup configuration;
- amplifier and effect state.

A schematic event model is therefore closer to

\[
Y(t)
=
\mathcal P_{\theta_O}
\left[
\mathcal R_{\theta_R}
\left(
\mathcal X_{\theta_E}(G(t),M)
\right)
\right]
+\epsilon(t)
\]

than to MIDI note to sinusoid.

Two events with the same M can occupy very different points in acoustic space because G, E, R and O differ.

Conversely, several different latent configurations can produce nearly indistinguishable audio. That is an identifiability problem, not merely an optimization problem.

---

## 4. The principle generalizes across instruments

The causal variables differ by instrument, but the structure repeats.

| Instrument family | Useful upstream primitive variables |
| --- | --- |
| Plucked string | pick or finger hardness, velocity, angle, position, penetration, muting |
| Bowed string | bow velocity, pressure, contact position, angle, direction |
| Piano | hammer velocity, felt or compliance state, damper and pedal state |
| Drum or percussion | stick or mallet hardness, impact velocity, location, angle, damping |
| Voice | subglottal pressure, glottal closure, fold tension, vocal-tract configuration |
| Brass | breath pressure, lip tension and aperture, valve state, embouchure |
| Flute or reed | jet or reed excitation, pressure, aperture, angle, fingering |
| Synth | oscillator state, envelopes, modulation trajectories, controller gesture |
| Electric or amplified source | source controls plus nonlinear amplifier and effect parameters |

The universal object is therefore not one pitch model per instrument. It is a causal grammar:

\[
\text{control or gesture}
\rightarrow
\text{excitation}
\rightarrow
\text{resonance or synthesis}
\rightarrow
\text{radiation or production}.
\]

Instrument-specific physics live in the morphisms and parameter families.

---

## 5. What should and should not be quotiented away

Not every low-level variable is semantically useless.

Exact pick thickness may be nuisance for one cover relation. But palm muting, bow articulation, vibrato, bends, attack strength, pedal state, note release and microtiming can be musically identity-bearing.

The latent state should therefore be factored at least as

\[
(M,A,\theta)
\]

where

- M is structural music: pitch, rhythm, harmony and form;
- A is meaningful performance articulation and expression;
- theta is nuisance instrument and production state.

The quotient should be fidelity-relative:

\[
(M,A,\theta)
\longrightarrow
(M,A)/{\sim_f}
\]

rather than deleting all performance information.

A loose cover may retain only accented attack. A forensic reproduction may retain much more.

This mirrors HumanMusic's existing fidelity discipline: what counts as irrelevant depends on the declared identity relation.

---

## 6. Probabilistic formulation

Let

- I be an instrument or source-family hypothesis;
- theta_I be its shared physical or acoustic parameters;
- G_e be the gesture for event e;
- M_e be the musical event;
- Y_e be the measured waveform region.

An honest inverse seeks something like

\[
p(M_{1:N},G_{1:N},I,\theta_I \mid Y_{1:N}).
\]

For the musical quotient, nuisance parameters are marginalized:

\[
p(M_{1:N},G_{1:N}^{\mathrm{meaningful}} \mid Y_{1:N})
=
\sum_I \int
p(M_{1:N},G_{1:N},I,\theta_I \mid Y_{1:N})\,d\theta_I.
\]

The desired answer can therefore be a rival set rather than one forced score.

---

## 7. Identifiability: overtone versus octave source

The existing harmonic competition note records the canonical ambiguity

\[
\sin(\omega t)+\frac12\sin(2\omega t).
\]

It can represent

1. one source at omega with a second harmonic; or
2. two sinusoidal sources at omega and two omega.

The summed waveform is identical.

No spectral algorithm can distinguish those explanations from that observation alone.

Additional discriminators may include

- common versus independent onset;
- envelope covariance;
- different release times;
- stereo or spatial evidence;
- repeated-event consistency;
- known instrument partial structure;
- phrase continuity;
- source-separated lane evidence.

Instrument modeling can narrow the equivalence class. It cannot abolish genuine non-identifiability.

The correct output in a saturated case is an explicit rival set or Unknown.

---

## 8. Shared instrument parameters create leverage

A major advantage of analyzing a whole stem rather than isolated frames is that the music changes while many source parameters remain shared.

For events e from 1 to N,

\[
Y_e = \mathcal R(M_e,G_e;\theta_I).
\]

The note and gesture variables vary from event to event while much of theta_I is approximately common.

This creates a factorization opportunity unavailable to frame-local peak picking.

A plausible inference loop is

1. initialize high-confidence note and event hypotheses;
2. estimate an effective instrument or source model from those events;
3. re-infer notes and gestures by explaining the waveform under that model;
4. update the shared source model;
5. iterate;
6. validate on held-out events from the same stem;
7. test transport to fresh stems and instruments;
8. retain semantic variables and marginalize nuisance source parameters.

This is analogous in spirit to alternating or EM-style latent-variable estimation, but no specific solver is prescribed here.

The operational requirement is stricter: every refinement must improve a predeclared fresh reconstruction or discrimination probe, not merely reduce fitted-frame loss.

---

## 9. A hierarchy of inverse instruments

The observer should become a stack of causal inverse instruments rather than one flat transcriber.

A possible hierarchy is

\[
\text{waveform}
\rightarrow
\text{spectral or modal evidence}
\rightarrow
\text{latent excitation and resonance hypotheses}
\rightarrow
\text{note and articulation hypotheses}
\rightarrow
\text{role and gesture trajectories}
\rightarrow
\text{phrase, motif, harmony and groove}
\rightarrow
\text{song-level quotient}.
\]

Each transition should state

- what forward process it is inverting;
- what parameters are shared;
- what variables are nuisance;
- what ambiguities remain;
- what measurement could falsify the selected explanation.

The governing rule is:

> Never promote a feature directly into a semantic object when a plausible lower causal generator of that feature has not been represented and tested.

A perfect physical simulator is not required at every layer. The model only has to be rich enough to distinguish the live rival explanations that matter to the next semantic quotient.

---

## 10. Observation scale must respect source physics

The bass-window probe is the same principle at the measurement level.

At 44.1 kHz, a 2048-sample window is only about 46 ms. For C1 near 32.7 Hz, that contains only about 1.5 periods. The extractor was attempting low-F0 inference with an observation window physically mismatched to the latent oscillation.

Doubling the window recovers additional detections on known and real material, while still not certifying their correctness.

So the analysis instrument itself must respect source physics:

\[
\text{observation scale}
\leftrightarrow
\text{timescale and frequency scale of the generating process}.
\]

There is no universally correct FFT or F0 window.

Pitch accuracy, event timing and transient localization create a real uncertainty tradeoff.

---

## 11. Current evidence for the hypothesis

The hypothesis remains Conjectured, supported by bounded Observations.

### Stem isolation

On six generated development items, role isolation moved

- all-note F1 from about 0.394 to about 0.639;
- lead F1 from about 0.025 to about 0.677;
- bass F1 from about 0.033 to about 0.688.

This establishes that mixture entanglement was one obstruction.

### Harmonic-family competition

The post-freeze synthetic probe replaces independent peak labels with competing harmonic-family explanations.

Across nine pitched controls

- pitched-frame micro F1 moves from about 0.486 to about 0.812;
- false pitch frames fall from 3175 to 452;
- the missing-fundamental control moves from 0.000 to about 0.989;
- the real octave-pair control moves from 0.500 to about 0.936.

The same route fails the quiet-triad-in-noise fixture completely, refuting universal superiority.

The supported statement is only:

> A better causal source model can materially improve inversion in some known regimes.

It does not establish correct real-song transcription, unique instrument recovery, precise event timing, generalization or cover recognition.

---

## 12. HumanMusic provides an Instrument Quotient Test

LibGibson can render the same musical object through different acoustic worlds.

Let a hidden score S be rendered as

\[
Y_B = R_{\mathrm{BLACK\_ICE}}(S),
\]

\[
Y_S = R_{\mathrm{SWISS\_SIGNAL}}(S),
\]

\[
Y_V = R_{\mathrm{VAPOR95}}(S).
\]

A successful structural inverse should satisfy, at an appropriate quotient,

\[
A(Y_B)\sim_f A(Y_S)\sim_f A(Y_V)\sim_f q_f(S).
\]

This is the positive invariance arm.

The negative arm holds the acoustic realization family fixed while changing one musical variable:

\[
S' \neq S.
\]

The inverse must detect the intended semantic change.

| Music | Instrument or world | Expected quotient |
| --- | --- | --- |
| S | I1 | q_f(S) |
| S | I2 | q_f(S) |
| S-prime | I1 | q_f(S-prime) |
| S-prime | I2 | q_f(S-prime) |

A method that changes its musical answer when only the world changes has failed instrument-quotient invariance.

A method that ignores the S to S-prime mutation has over-quotiented and lost music.

---

## 13. Gesture invariance and gesture preservation

The same framework should distinguish structural invariance from meaningful expression.

Expected structural invariance can be tested with the same note or phrase under altered

- pick hardness;
- attack velocity;
- microphone EQ;
- room or reverb;
- synth oscillator timbre.

At a structural fidelity, note, motif and harmony should survive.

Expected expression changes can be tested with the same note or phrase under altered

- palm mute versus sustained articulation;
- bend trajectory;
- vibrato;
- bow articulation;
- pedal state;
- attack accent.

Structural pitch may survive while the expression axis changes.

Round 3 should therefore evaluate a vector:

\[
(\text{structure},\text{gesture or expression},\text{source or timbre})
\]

rather than one yes or no score.

---

## 14. Proposed next architecture

A future additive experiment should look approximately like

\[
\text{separated acoustic lane}
\rightarrow
\text{source-family hypothesis}
\rightarrow
\text{primitive gesture or excitation model}
\rightarrow
\text{shared effective instrument model}
\rightarrow
\text{event-level latent music and articulation}
\rightarrow
\text{temporal or role graph}
\rightarrow
\text{semantic quotient}
\rightarrow
\text{HumanMusic constrained lift}
\rightarrow
\text{fresh PCM}
\rightarrow
\text{independent re-observation}.
\]

The important addition is

\[
\boxed{
\text{acoustic evidence}
\rightarrow
(I,\theta,G,M)
\rightarrow
(G_{\mathrm{meaningful}},M)
}
\]

instead of acoustic evidence directly to M.

---

## 15. Implementation doctrine

Any implementation following this note should obey:

1. Preserve Round-1 and Round-2 observers and receipts as controls.
2. Do not open either sealed holdout during model design.
3. Do not use source-ID heuristics.
4. Do not manually correct individual songs.
5. Fit source parameters on one subset and validate on fresh events.
6. Preserve rival explanations.
7. Measure onset and offset as well as pitch occupancy.
8. Preserve absolute and quotient views.
9. Do not use waveform reconstruction loss as a musical truth score.
10. A better acoustic fit is not a better semantic explanation unless a discriminating probe says so.

---

## 16. Round-3 candidate probe ladder

### Probe A: frozen harmonic proposal across HumanMusic worlds

Generate fresh short polyphonic stems with known notes from all HumanMusic worlds.

Run the existing frozen harmonic-family competition route.

Measure frame occupancy, event onset, event offset, octave confusion, missing fundamentals, false partial-as-note errors and low-SNR behavior.

This tests transport away from the synthetic additive generator used in the first competition probe.

### Probe B: learn an effective source envelope

For each generated role and world, estimate a shared harmonic or inharmonic envelope from a training subset of events.

Then re-infer a held-out subset.

Compare against the generic analytic dictionary.

Pass criterion: held-out semantic recovery improves without song-specific or event-specific fitting.

### Probe C: gesture mutation factorial

Render the same symbolic material with controlled variations in attack, gate or release, detune or vibrato, filter or brightness and amplitude.

The inverse should preserve the declared structural quotient while detecting expression changes where appropriate.

### Probe D: overtone versus genuine octave pair

Construct matched cases:

1. one note with a strong second harmonic;
2. two genuine notes an octave apart.

Match coarse spectral energy as closely as practical.

Use temporal and envelope evidence to distinguish them.

If no available observation separates them, retain the rival set explicitly.

### Probe E: low-SNR source model

Preserve the current quiet-triad failure.

Only then introduce an explicit noise likelihood or source-noise decomposition.

Test on fresh noise levels rather than tuning only to the spent fixture.

---

## 17. Claim ledger implied by this note

| Claim | Status |
| --- | --- |
| Source separation alone solves music inversion | Refuted for current probes |
| Instrument or source physics matters to inversion | Observed, bounded |
| Harmonic-family competition can reduce partial-as-note errors | Observed, bounded |
| A general primitive-above inverse architecture will solve real-song transcription | Conjectured |
| Exact physical instrument identity is necessary | Not established; an effective generator may suffice |
| Waveform uniquely determines performer, instrument and note decomposition | Refuted without additional assumptions |
| Gesture parameters are all nuisance | Refuted conceptually; some are musical expression |
| Jointly learned source parameters improve fresh-event semantic recovery | UNVERIFIED |
| Instrument or world quotient invariance can be demonstrated on fresh HumanMusic controls | UNVERIFIED; next discriminator |

---

## 18. Core research statement

Reverse HumanMusic is no longer best described as audio transcription.

It is an attempt to recover a musical object by approximately inverting the causal process that made the sound:

\[
\boxed{
\text{music}
\rightarrow
\text{gesture}
\rightarrow
\text{excitation}
\rightarrow
\text{instrument}
\rightarrow
\text{production}
\rightarrow
\text{audio}
}
\]

and then quotienting the recovered latent realization down to the information required by the declared musical identity relation.

The strongest current methodological rule is:

> Model at least one primitive causal layer upstream of the signal feature you want to interpret.
>
> If a peak may be a harmonic, model the harmonic source.
> If a transient depends on pick, bow, hammer, stick or breath gesture, model the excitation.
> If a low fundamental cannot be resolved at the chosen observation scale, change the observation instrument rather than interpreting the absence as musical silence.
>
> Only after the signal has a plausible causal explanation should Reverse HumanMusic discard instrument-specific nuisance variables and promote the remainder into music theory.

That is the current leading shape of the inverse.
