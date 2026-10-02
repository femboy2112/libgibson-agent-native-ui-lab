# Claim ledger — Semantic Audio Inversion (round 1)

Meanings per the experiment protocol: **Disclosed** (proven from inspected code / decisive
calibrated measurement inside the stated boundary), **Corroborated** (independent routes survive
provenance, residual and holdout checks), **Observed** (seen in one exact run), **Conjectured**
(supported, truth debt remains), **UNVERIFIED** (suitable probe has not run), **Dark** (no present
discriminator reaches it), **Refuted** (a decisive probe/counterexample killed it).

| # | claim | level | load-bearing evidence |
| --- | --- | --- | --- |
| L1 | LibGibson `c2f6483…` exposes the forward layering, `CoverMap`, per-axis fidelity relations and a real-audio decode path described in the research README. | **Disclosed** | source inspected during Round 0; public APIs consumed by `sai-harness`. |
| L2 | The canonical audio boundary (mono 22.05 kHz f32le) is deterministic and hash-auditable, keeping source hash distinct from canonical-PCM hash. | **Disclosed** | `adapters/canonicalize.py`; receipts; `evidence`/`receipt` builders; tests. |
| L3 | Timing can be recovered from clean mixed audio at the tempo level, and beat-relative transport is stable on most dev items. | **Observed** | dev: tempo 13/15 ok (mean 0.950); beats 10/15 ok (mean 0.836); synthetic click 120.05 BPM, median grid error 0.040. |
| L4 | Harmony triangulation works as designed (two independent routes; rivals retained; ceiling lowered) on acoustic material. | **Observed** | effective harmony `QualityFamily` on 15/15 dev; calibration triads recover `{0,4,7}` and `{2,6,9}`. |
| L5 | A provenance-bearing quotient with requested-vs-effective relations, lowering reasons, and `Unknown`/`Free` distinction can be produced end-to-end. | **Observed** | `sai recover` on all dev + mutation evidence; orchestration correctly stays `Unknown`/`Free`; `classification` 15/15 with zero false unknown/free/present. |
| L6 | The inverse analyzer cannot yet recover note-level musical content at a useful rate from full mixes with one monophonic route. | **Refuted** (as stated) | notes.all mean F1 0.229; monophonic peak estimator cannot represent the truth's 4-note chords. |
| L7 | Per-beat recovered chord windows can express the hidden harmonic `relation` axis. | **Refuted** | `relation_between` requires equal-length sequences; recovered windows (~36) vs truth spans (~8) force `Free` at floor. See §12. |
| L8 | Recovering at least part of a `CoverMap`-like quotient from *generated HumanMusic* audio is feasible within a small calibratable baseline. | **Conjectured** | form topology 7/15, tempo/beat stable, classification clean; but motif/harmony/groove relations sit at floor. |
| L9 | The same quotient machinery transfers to the untouched holdout. | **UNVERIFIED** | holdout sealed (`n=30`, commitment `db8f86…`); not decoded. |
| L10 | External heavy-ML adapters (Beat This!, Basic Pitch, Essentia, MuScriptor, Demucs) improve the reading on this host. | **UNVERIFIED** (access gap) | none installed; adapter probes refuse explicitly; no GPU and ≈7 GB RAM. |
| L11 | Analysis-by-synthesis can validate the recovered quotient as a consistency probe. | **UNVERIFIED** | no constrained lift is implemented this round by design (a diagnostic, not a witness). |
| L12 | Production mutations (EQ/compression/reverb) leave structural identity invariant in our analyzer. | **Refuted** (partially) | metric held, but recovered motif *content* drifted in 5/5 EQ, 5/5 compression, 6/10 reverb checks. |
| L13 | The analyzer distinguishes a real semantic change from a production change. | **Refuted** (this round) | score-level `chord-edit`, `shift-lead-note`, `mute-drums` cannot move their relation axis because it is at floor. See §7/§9. |
| L14 | Authorial intent, generator seed, or any forward-only provenance is recoverable from sound. | **Dark** | no present discriminator; the missing lamp is contextual/source evidence, not a better waveform model. |

Conclusions inherit the weakest load-bearing premise. The load-bearing premise for any positive
"listening" claim is L6/L8, both currently negative or conjectural — so the overall program
status remains **Conjectured at best**, matching the research README's `UNVERIFIED experiment`.
