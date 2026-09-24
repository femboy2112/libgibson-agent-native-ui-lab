# Cross-experiment friction, before API freeze

2026-09-24. This is the Phase 8 reconciliation of the three planned consumers.
It proposes **no LibGibson core change**. It does not select or preview holdouts.
The substrate remains the public Git revision
`e5ede0a4ab8ff52caa567ee10d45824e42c1ebc6` throughout first contact.

## Question and evidence boundary

The live question was whether sequential art, a replaceable semantic reaction
layer, and validated runtime instruments require private renderer changes or
unsafe terminal output to preserve meaningful interaction. The alternative was
that public rendering/focus primitives suffice with consumer-owned policy.

Source review finds the latter route in all three implementations. Their
contracts are executable in `tests/manga.rs`, `tests/reactions.rs` and
`tests/instruments.rs`; first-contact notes record commands and local outcomes.
The campaign's final validation record is authoritative for the final committed
revision. This synthesis does not turn test definitions into additional executed
runs or claim visual acceptance from an assertion count.

All three use the **same** semantic reducer, finite trace, host runner, pinned
LibGibson and Rust environment. Consumers have different rendering and routing
implementations, but were developed by the same model family in collaborating
contexts. Static review, direct semantic tests and PTY reconstruction provide
different instruments, not independent semantic datasets. A shared fixture or
runner defect can affect every result. No universal expressivity or comparative
ease claim follows from this set.

## What worked, and through which public boundary

| Consumer | Distinct pressure | Public route that succeeded | What stayed external |
|---|---|---|---|
| A: Manga | Concurrent panels, reflow, nonrectangular failure/repair seam, permission | Node/layout/painter, Surface overlay, SurfaceFx, FocusId/FocusRing | Comic reading order, seam geometry, action policy, journal |
| B: Reactions | Useful action survives NONE/RESTRAINED/HEAVY and a different metaphor | Node, BrailleCanvas, clipped Surface composition, KeyEvent and focus IDs | Typed editorial ontology, cooldowns, mode policy, target registry |
| C: Instruments | Runtime schema admission, replacement, mount/unmount, focus restoration | Node compiler, BrailleCanvas, public focus IDs/ring | IR, capability port, validation, ID allocation, transactions, planner |

None of those routes requires a LibGibson patch, private module, custom ANSI
emitter, or generated native code. The manga repair seam is the unforeseen
within-experiment device; it is already spent evidence and is **not** a fresh
post-freeze holdout. The instrument's four representation families were planned
probes and likewise cannot be reused as holdouts.

## Repeated friction

### G1: stable interaction identity and event routing

This is real repeated ergonomic pressure: F-A1, F-B1 and F-C1. `FocusId` decouples
selection from geometry, and `FocusRing` orders/traverses current targets, but
applications still associate IDs with semantic targets and decide which typed
action a key activates.

A owns four fixed panel identities. B owns four fixed action identities plus
availability checks. C needs a bounded dynamic namespace, prior-owner history,
ring reconstruction and binding lookup because specifications are replaced and
temporarily mounted. Reflow and replacement tests retain the intended target;
explicit actions update semantic state through the common reducer. C additionally
tests a custom mount name so success does not depend on a renderer whitelist.

The common part is small ID-to-action association, not the complete consumer
policy. B's availability rules and C's authority/restore semantics are not
interchangeable. A single generic router has not been shown to remove meaningful
duplication without importing those policies. The approximately 30 lines of A's
keyboard dispatch and about 100 lines of B's mapping/availability/dispatch are
rough source measurements, not a controlled implementation-effort benchmark.

**Classification: ERGONOMIC INCONVENIENCE.** The prediction that G1 would receive
pressure is supported within this workload. A stronger claim that interaction
is blocked by the API is not supported. Mouse hit-testing, deep nested event
propagation and large mutable target sets were not tested here.

### Replay must include presentation inputs

F-A4, F-B3 and F-C3 repeat a trace-completeness requirement. Semantic actions do
not record focus-only keys. A's negative control demonstrates that replaying only
actions preserves decisions while producing a different visible selection.
The lab input journal now retains ordered application keys; C retains typed
input intents and reconstructs mount/patch/focus history from them plus source
event times. Tests compare complete state and frames, not just final permission.

**Classification: HARNESS-SPECIFIC.** This correction belongs to the experiment's
recording boundary. It does not imply that the renderer should journal arbitrary
application inputs or own the host's pause/exit controls. The shared runner is a
useful lab helper and will be part of the frozen substrate for holdouts.

### Responsive policy and graphical escape routes

A's manual reflow and C's primary/secondary composition require domain choices;
public layout and clipping realize them. A's causal seam, B's alternative
metaphors, and C's line-based diagrams all use safe public lower layers. No
experiment fell back to raw terminal control. The work is composition code, not
evidence for a `MangaPanel`, reaction widget or diagram-specific core type.

**Classification: ERGONOMIC INCONVENIENCE for layout policy; HARNESS-SPECIFIC for
the visual vocabulary.** A general professional-quality usability judgment is
still unmeasured; assertion-based readability checks are narrower evidence.

### Admission, resource bounds and authority

B and C reject malformed configuration, unavailable actions and over-budget
history before granting authority. C validates JSON before mounting and performs
atomic state changes. Generated content never receives Context or terminal,
file/network or native-code authority. These are consumer security boundaries,
not requirements for a scene renderer to infer trust.

**Classification: DELIBERATE SAFETY BOUNDARY.** The provisional instrument IR is
static: no effect program or nonzero animation rate is admitted. Its diagrams
can change when the host generates a new spec. This limits the IR's domain and
must not be misreported as either programmable animation support or a LibGibson
expressive wall. Likewise the finite host input/action budgets limit a session;
they do not establish a general long-session resource policy for the engine.

## Counterexamples retained

| Probe / failure | Fault isolated to | Correction and surviving limit |
|---|---|---|
| A's 12-row fallback admitted an unsigned layout underflow | Consumer geometry policy | Minimum supported layout changed to 18 rows; smaller viewports show bounded fallback |
| A label sampled during dissolve was absent | Test's sampling time | Probe moved after reveal; not a rendering correctness fix |
| Actions-only replay lost focus | Consumer journal | Ordered key replay; negative control retained |
| A silently dropped actions at capacity | Consumer resource admission | Visible rejection and reserved permission slot; denial-after-spam regression |
| C focused a valid arbitrary mount without displaying it | Consumer composition whitelist | Focused arbitrary instrument is primary; custom-mount/unmount regression |
| C reduced output budget but removed only one old receipt | Consumer retained-history policy | Truncate until the new shared-rail limit holds; smaller-budget regression |
| B's chosen 100 ms animation sample did not move rounded subcells | Probe temporal resolution | A wider interval establishes actual changed geometry; no universal motion-rate claim |
| A raw byte search missed a displayed receipt split across ANSI diff runs | Inspection instrument | Reconstruct terminal state with vt100 before checking rendered text |

These corrections happened before freeze. They refute stronger claims about the
initial consumer implementations, not the public substrate. They also demonstrate
why healthy final snapshots alone are insufficient to certify the whole path.

## Promotion decision

The program's promotion rule makes repeated generic pressure **eligible for
consideration**, not mandatory core expansion. G1 satisfies the repetition
criterion, but this set has not established a missing generic contract:

- Existing stable focus IDs and traversal remain usable under every tested
  relayout/replacement.
- Typed domain dispatch is explicit and bounded. No unsafe workaround appeared.
- C's larger lifecycle/authority policy has only one dynamic-IR consumer here.
- No independently reproduced core correctness bug emerged from the probes.

**Decision: no core promotion PR.** Keep the IR, port, router policy and shared
input helper in the external lab. Do not refactor away first-contact friction
merely to make the evidence look cleaner. A later application cohort could
justify a small routing helper; that remains a candidate, not an API commitment.

## Claim ledger and next gate

| Claim | Status and scope |
|---|---|
| Three planned consumers can use the pinned public substrate without core patches | OBSERVED in these implementations; final campaign gates bind the committed revision |
| Rendering and keyboard interaction compose across these three representations | CORROBORATED within one shared finite fixture/provenance family |
| G1 is an ergonomic pressure point | OBSERVED through three different local mappings; no expressive wall established |
| Generated declarative content can stay behind a consumer-owned authority boundary | OBSERVED for this six-form, static, bounded IR |
| Ordinary professional dynamic UI is universally easy | UNVERIFIED; no human onboarding/comparative-effort study |
| The API supports unforeseen post-freeze interfaces | UNVERIFIED until the fresh holdout campaign executes |
| No future core routing primitive is needed | UNVERIFIED; not claimed |

Freeze the exact public revision and lab helper/IR commit after the planned
consumer validation completes. Only then choose fresh concepts in a context
that receives the frozen API, screen for contamination, and implement without
core/helper/schema rescue. A failing holdout must remain a failure for that
frozen campaign.
