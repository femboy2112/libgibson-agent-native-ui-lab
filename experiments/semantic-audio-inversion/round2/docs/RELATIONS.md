# Round 2 observational relations

`python/sai_v2/relations.py` adds `sai.observational-relations/v2`. Round 1 Rust
metrics, thresholds, evidence schema, and sealed holdout remain unchanged. The
functions consume measured events; they do not accept a Score, CoverConformance,
world, seed, source name, or generator fingerprint. Internal conformance and
observational agreement are separate gates.

The caller must supply an explicit common beat coordinate system and measured
role lanes. This module does not silently choose a beat phase, tempo octave,
transposition, song segment, or rival source route. Its only normalization is the
declared global transposition and affine onset normalization for motif identity.
No dynamic time warping, arbitrary phrase trimming, or per-song threshold tuning
is used. Canonical fields and supported importer aliases are:

| Object | Fields |
| --- | --- |
| Harmony | `start_beat`, `end_beat`, `root_pc` (alias `root`), `quality`, optional `rivals`, `confidence` |
| Note | `onset_beat`, `pitch_midi` (alias `midi`), optional `offset_beat` (or `duration_beats`), `role`, `confidence` |
| Drum | `onset_beat`, `family`, optional `is_fill`, `is_ornament`, `fill_status`, `confidence` |

Raw fields such as seconds, provenance, source stems, and microtiming survive in
copied event rows. All finite/range checks are fail closed. The importer remains
responsible for source hashes, monotonic source streams, domain duration, and
provenance validation. Evaluation sorts valid event lists; it rejects overlapping
harmonic spans within one route. Compare rival routes separately.

## Harmony: a measure on metric time

First merge adjacent equal `(root, quality, rivals)` windows. Preserve every
original window in `members`. Split the union of source and candidate support at
all span endpoints. For each resulting cell, record the source/candidate indices
and equality of root, root-plus-family, and root-plus-exact-quality. Weight each
match by cell duration. Uncovered support counts against coverage and agreement;
a silence gap shared by both lists contributes no musical support. Extra tails
are penalized symmetrically. Equal window count is never required.

Match transitions independently at the root, family, and exact levels. A
transition must have the same incoming and outgoing labels and occur within
0.125 beats. Record maximum one-to-one matches, missing and extra transitions,
precision and recall. A constant chord has zero transitions; two constant equal
chords pass the no-unmatched-transition condition without pretending a measured
transition F1 exists.

| v2 relation | Necessary conditions |
| --- | --- |
| `ordered` | coverage and root agreement >= 0.90; no unmatched root transitions |
| `quality-family` | coverage and root-plus-family agreement >= 0.95; no unmatched family transitions |
| `exact` | coverage and root-plus-exact-quality agreement >= 0.95; no unmatched exact transitions |
| `free` | Measurements exist but those conditions fail |
| `null` with `unknown` | One span route is missing or has no known roots |

These are **temporal observational labels**, not identical to HumanMusic's exact
symbolic relations. In particular, v2 `ordered` requires temporal root coverage;
it does not claim duration freedom. v2 `exact` means equality within the declared
measurement tolerance, not an exact mathematical chord identity certificate.
Thresholds are development policy, not perceptually calibrated probabilities.

Unknown qualities cannot equal other unknown qualities. Quality labels are not
silently rewritten to match: `maj` and `major` share a family but do not earn exact
quality equality. `min7b5` belongs to the diminished triad family in v2, correcting
the conceptual family without changing v1's historical minor-family assignment.

Rivals are retained and are never confidence-voted away. A rival root conflict
caps the identified relation at unknown. Same-root, different-family rivals cap
it at ordered; same-family extension rivals cap it at quality-family. The report
keeps the selected-label diagnostic relation separately. Rival template scores
are not posterior probabilities. A candidate cannot earn a stronger source pin
by agreeing only with a convenient selected label.

The split/merge positive controls prove the former list-length degeneracy is
removed. The half-bar root mutation and even a 0.01-beat spurious root transition
break a relation despite high average coverage. Extension-only mutations lower
Exact to QualityFamily. Missing support and missing transitions remain failures.

## Notes and motifs

`note_f1(reference, observed, role=None)` computes maximum-cardinality one-to-one
onset/pitch matching with 0.12-beat onset and 0.75-semitone pitch tolerances,
retaining pairs, precision, recall, and F1. These are the v1 note tolerances, but
v2 removes greedy-order undercounting. Empty source evidence has unknown status;
F1=0 is a diagnostic placeholder and cannot be a positive structural gate.

`compare_motif` selects only the explicit role when given; it never assumes that
the highest pitch is the melody. Unresolved onset collisions return unknown.
At least two events in each statement are required. Pitches relative to the
first note are compared within 0.5 semitones; onset profiles normalized by the
first-to-last onset span are compared within 0.04. Metric requires equal event
counts and every event matching its corresponding position. Global transposition
and uniform tempo scaling therefore preserve the relation; one wrong interval,
a deleted event, or accompaniment pollution cannot earn Metric.

`compare_motif_occurrences(reference, observed, occurrences, role=None)` accepts
the selected source family's **complete** list of metric occurrence windows. It
evaluates every window, retaining all results, Metric and Theme-or-better
fractions, minimum/median relative note F1, and the strict whole-line result.
Source family selection happens upstream from source-only recurrence support;
the helper never searches for whichever candidate fragment happens to fit best.
Missing candidate occurrences count as failures, while a source occurrence with
insufficient monophonic evidence makes the aggregate relation unknown. Aggregate
Metric requires every occurrence to earn Metric. Overlapping windows are marked
and must not be described as independent observations. Window membership is
onset in `[start_beat, end_beat)`; no boundary is shifted to rescue a note.

Theme requires equal lengths, onset profiles, and direction signatures, while
allowing changed interval sizes. `relative_note_match` reports approximate
precision/recall/F1 even when the strict relation fails. Its first-note/span
anchor can be sensitive to a missing first or last note; the normalization and
estimated global transformation are explicit. It is not a robust motif search
or proof of a recurring hook. Measured occurrence windows must be chosen upstream.

Optional normalized note durations use a 0.12 comparison tolerance. Durations are
diagnostic only: the audio ceiling remains Metric, never Faithful. No absent
offset is filled with a default duration.

## Groove

`compare_groove` matches measured kick/snare families within 0.125 beats and
reports all-stroke precision/recall/F1. KickSnare requires every stroke to match,
equal counts, and both families observed on both routes. Unknown families are
not wildcard instruments.

The pocket projects strokes within 0.10 beats of the quarter grid to integer
beats, retaining original onsets. It excludes only explicitly measured
fill/ornament events; `fill_status=unknown` grants no exemption. PocketSkeleton
requires identical nonempty one-to-one family/grid sets. Offbeat changes or
declared fills may break KickSnare while preserving PocketSkeleton. Missing
backbeats, duplicate strokes, and unknown events on the quarter grid prevent a
positive pocket claim. An empty pocket never passes by vacuity.

## Form

`compare_form` compares spans and recurrence-family topology separately.
`family` or `label` must identify measured recurrence membership, not semantic
human names. Optional `reference_links`/`observed_links` override those labels;
they have `at_beat`, `to_beat`, and `similarity`, use the frozen v1 0.70 floor,
and form connected components of section recurrence. IDs are canonicalized in
first-appearance order and consecutive equal families collapse for topology.
An explicitly measured empty link set retains the distinct-family diagnostic
but cannot establish nontrivial recurrence identity. No families or links is
Unknown. Boundary precision/recall use 0.5-beat one-to-one matching; start/end
extent errors and raw spans remain separate. Audio ceiling is Topology even
when spans also agree. Splitting a section into identical-family pieces preserves
topology but creates an extra boundary; changing recurrence breaks topology.

## Frozen Round 1 source findings

All `crates/sai-core/src/` was read, including CLI, tests and schemas. Relevant
historical invariants and limitations are:

- v1 validates hashes, timing/note domains, chroma and section spans, but does not
  validate every nested confidence/provenance field or schema on quotient load.
- Seconds remain authoritative; `BeatGrid` uses piecewise interpolation and
  extrapolation. Its meter selector takes the maximum proposed meter, not the
  most supported one, and a one-beat grid maps every time to beat zero.
- v1 recovery's line reducer takes the highest pitch within 0.08 beats when no
  lead is supplied; highest-confidence role selection does not preserve ties.
- v1 phrase windows split on >1-beat onset gaps or eight notes; fields named
  `start_seconds`/`end_seconds` in those windows actually contain beats.
- v1 evaluation reads raw lead-or-unlabelled notes, not the reduced line, and
  replaces an invalid first phrase by a fabricated single MIDI-60 statement.
  v2 refuses polyphonic statements instead.
- v1 harmonic route reconciliation groups by start time and lowers every dispute
  to QualityFamily even when roots disagree. v2 rivalry caps are stricter.
- v1 harmonic comparison requires equal sequence length and discards spans;
  v2 additive overlap comparison addresses precisely that probe gap.
- v1 drum matching permits unknown-family wildcard compatibility. v2 does not.
- v1 form recurrence unions sections above 0.70 and canonicalizes family IDs;
  its topology is not a Verse/Chorus semantic label. Audio ceiling is Topology.
- v1 orchestration is always Unknown when requested. Requested Free remains a
  deliberate unpinned axis; missing transport forces every axis Unknown.

These are inspected implementation facts, not edits to the historical system.
No old holdout artifact was opened during this work.

## Reproduce controls

From repository root:

```sh
python3 -m unittest discover -s experiments/semantic-audio-inversion/round2/tests -p test_relations.py -v
```

Passing event-level mutations calibrates the comparator logic. It does not show
that an audio analyzer can recover those events, that a real-song lift succeeds,
or that a listener recognizes a song. Those claims require fresh WAV analysis
and maintainer audition, recorded elsewhere by the candidate runner.
