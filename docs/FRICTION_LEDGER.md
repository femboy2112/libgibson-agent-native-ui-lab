# First-contact friction ledger

Established before implementation. No LibGibson change may be made in response
until the three planned consumers have been reconciled.

Each entry records ID, experiment, desired behavior, public route attempted,
observed result, workaround, complexity where useful and classification.

Allowed classifications: ERGONOMIC INCONVENIENCE, HARNESS-SPECIFIC,
DELIBERATE SAFETY BOUNDARY, GENERIC PRIMITIVE GAP, EXPRESSIVE WALL.

| ID | Experiment | Desired behavior / attempted public route | Result / workaround | Classification |
|---|---|---|---|---|
| F-A1 | Manga | Stable focus across relayout / FocusId + FocusRing | Works; app maps keyboard to typed actions (~30 lines) | ERGONOMIC INCONVENIENCE |
| F-A2 | Manga | Nonrectangular gutter repair / Node paint + Surface overlay | Works; app owns seam geometry (~35 lines) | HARNESS-SPECIFIC |
| F-A3 | Manga | Responsive comic page / Node + computed Rect policy | Works; manual reflow policy (~25 lines) | ERGONOMIC INCONVENIENCE |
| F-A4 | Manga | Exact interaction replay / semantic action journal | Initial journal omitted focus-only keys; shared bounded input journal reconstructs a fresh consumer. Negative control confirms actions alone give a different frame | HARNESS-SPECIFIC |
| F-A5 | Manga | Visible resource rejection without losing permission response / app action budget | Initial consumer silently dropped excess actions. Corrected to visible rejection and one reserved permission slot; denial after selection spam remains effective | HARNESS-SPECIFIC |
| F-B1 | Reactions | One useful action survives modes, metaphor replacement and reflow / FocusId + FocusRing + KeyEvent | Works; four registered targets, availability checks and explicit dispatch remain app-owned. Mapping/availability/dispatch is about 100 lines, including domain validation | ERGONOMIC INCONVENIENCE |
| F-B2 | Reactions | Replace presentation without owning decisions / Node + BrailleCanvas + Surface | Works; one typed cue drives different visual metaphors with the same semantic action | HARNESS-SPECIFIC |
| F-B3 | Reactions | Replay focus and rejected inputs, not only decisions / focus ring + input journal | Full ordered key replay reproduces focus, notice, actions, complete snapshot and frame. Actions-only history is insufficient | HARNESS-SPECIFIC |
| F-B4 | Reactions | Bound policy configuration and overlay authority / validate before Node construction | Unknown targets/configuration and unavailable actions reject; policy cannot emit actions. Permission slot reserved | DELIBERATE SAFETY BOUNDARY |
| F-C1 | Instruments | Stable identity across generated replacement and mount/unmount / FocusId + FocusRing | Works with a bounded string-to-ID registry, ring rebuild, prior-focus stack and binding lookup. Dynamic lifecycle needs more glue than A/B | ERGONOMIC INCONVENIENCE |
| F-C2 | Instruments | Admit untrusted specs without granting terminal/tool authority / validated public Node construction | Lab-owned schema, byte/tree/raster/action budgets and atomic transactions reject invalid specs. Static IR deliberately rejects effect programs and nonzero animation rates | DELIBERATE SAFETY BOUNDARY |
| F-C3 | Instruments | Replay chosen representation, patches, focus and actions / deterministic public primitives | Reconstruct port from immutable source event times plus ordered serialized intents; compare the entire port and frames | HARNESS-SPECIFIC |
| F-C4 | Instruments | Multiple diagram families share one compiler / Node + BrailleCanvas | Bounded line segments plus textual fallback suffice for the four planned representations | HARNESS-SPECIFIC |
| F-C5 | Instruments | Any validated mount must be visible / app composition | Initial renderer recognized only planned IDs. Corrected so the focused arbitrary ID owns the primary card and other mounts receive secondary cards | HARNESS-SPECIFIC |
| F-C6 | Instruments | Smaller caller retention budget immediately applies / lab retained output rail | Initial single-pop policy could retain too much after a budget reduction. Corrected to truncate to the new shared-rail budget; regression covers the reduction | HARNESS-SPECIFIC |

Code-size estimates describe local implementations, not measured minimum effort
or missing-core line counts. In particular the roughly 1,000-line instrument
module includes its schema, validator, four representations, compiler, authority
policy, lifecycle and replay; that total is not routing boilerplate.

Detailed attempts, validation and failed probes:
[manga](MANGA_FIRST_CONTACT.md), [reactions](REACTION_FIRST_CONTACT.md),
[instruments](INSTRUMENT_FIRST_CONTACT.md).

Reconciled before freeze in
[Cross-experiment friction](CROSS_EXPERIMENT_FRICTION.md): **no core promotion
is justified by this first-contact set**. G1 now has observed ergonomic pressure
and a working safe public route. Neither a generic primitive gap nor an
expressive wall has been demonstrated. Planned tests share one semantic fixture
and implementation context; do not count them as independent source families.
