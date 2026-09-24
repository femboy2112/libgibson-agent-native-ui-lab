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

Details and failed consumer probes: [manga first contact](MANGA_FIRST_CONTACT.md).
Predictions such as G1 are not findings.
