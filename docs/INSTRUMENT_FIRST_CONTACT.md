# Generated instruments: first public-API contact

2026-09-24. **EXPERIMENTAL consumer-owned IR**, not a LibGibson proposal.
Substrate remains exactly `e5ede0a4ab8ff52caa567ee10d45824e42c1ebc6`.
No core patch, private access, generated native code, raw ANSI, model service,
filesystem tool, or network tool is needed by this experiment.

## Probe and implementation

The hypothesis was that validated runtime-created instruments can bind stable
semantic identities to changing views and explicit user actions through public
LibGibson APIs. The competing failure would be needing private renderer/event
state to keep selection usable after representation replacement or relayout.

`cargo run --release --bin instruments` runs the common fixture. Tab/arrows
move focus; Enter selects an agent; Y/N resolves the actual permission request.
The dependency explorer becomes a timeline when the tool fails, a source map
when replay begins, and a decision inspector during permission/synthesis.
A transient authority notice actually mounts and unmounts with the pending
request. The same source events, failure, recovery and permission default remain
authoritative; presentation does not change them. Explicit denial withholds the
artifact through the shared reducer.

The deterministic planner produces `InstrumentSpec` values containing six
composition forms: Text, Column, Row, Panel, Choice, and bounded line raster.
The compiler uses ordinary public Node/layout/painter plus BrailleCanvas. It
is not a graph widget library or texture language. JSON admission has a byte
bound before serde decoding, denied unknown fields (including action payloads),
and type/resource/action validation before a transaction changes mounted state.
Both generated JSON and native planner specs take the same validation route.

`SessionPort` owns:

- a read-only snapshot of the shared semantic reducer;
- at most four mounted validated specifications;
- atomic mount, patch, unmount, focus and bound activation;
- stable string-to-public-FocusId allocation and FocusRing rebuilding;
- a bounded previous-focus stack, lifecycle receipts, plain output rail, and
  explicit semantic action receipts.

The primary card follows the currently focused instrument. Other mounted IDs
receive secondary cards; names are not a renderer whitelist. Read-only cards
can be clipped in small secondary regions, like other fixed terminal layouts.
A single selected action updates the live inspector. No generated spec receives
Context, renderer, terminal transaction, file, network, or memory authority.
`commit_output` is retained structured text in the lab UI, not terminal I/O or
an unbounded native-scrollback insertion interface.

## Bounds and explicit exclusions

| Resource | Enforced bound |
|---|---|
| JSON input | 32,768 bytes before decoding; serde recursion guard retained |
| Nodes / nesting per instrument | 64 / 8 |
| Individual raster dimensions | declared budget, at most 160 × 160 |
| Aggregate declared raster area | at most 25,600 samples per instrument |
| Segments per line view | 128; coordinates within the declared raster |
| Text leaf | 2,048 UTF-8 bytes; ASCII/C1 controls rejected |
| Mounted instruments | 4 |
| Lifetime focus identities | 256, exhaustion rejects atomically |
| Source interaction inputs / semantic actions | 64 each |
| Lifecycle receipts | most recent 128 operations |
| Output rail / focus restoration history | at most 64; default output 16 |

A smaller caller output budget truncates the **shared** output rail to that
budget. This is explicit consumer policy, not a claim of per-instrument durable
logs. A history-limit rejection appears visibly; it does not silently authorize
a pending permission. The initial IR accepts no effect programs and only static
animation rate zero. Requests for nonzero effects or rates are rejected rather
than accepted without realization. The fixture still evolves over time as
new validated specifications replace old ones. Animated instrument programs
are outside this probe, not an expressive wall in LibGibson.

Replay-requested specs require an explicit seed, although these six static
forms currently use no randomness. Mono requires textual fallback declarations;
line views display their fallback and selected choices retain reverse/bold
markers. No color-only action state is used.

## Observations and friction

| ID | Desired behavior | Public route attempted / observation | Workaround and classification |
|---|---|---|---|
| F-C1 | Semantic IDs survive generated replacement, mount/unmount and reflow | FocusId + FocusRing retain focus correctly, but do not map arbitrary schema node IDs or dispatch typed actions | Consumer registry, bounded prior-owner stack, ring reconstruction and binding lookup. **ERGONOMIC INCONVENIENCE**, not yet an earned core gap |
| F-C2 | Generated specs cannot acquire terminal/tool authority or exhaust presentation resources | Public Node/Surface are construction APIs, not a generated-content validator | Byte/schema/tree/action/resource admission and atomic transactions remain lab-owned. **DELIBERATE SAFETY BOUNDARY** |
| F-C3 | Exact replay includes chosen representation, patches, focus and actions | Public primitives are deterministic; neither FocusRing nor semantic action replay records input intent automatically | Reconstruct the port from source times plus ordered, serialized input intents; compare full port equality and frames. **HARNESS-SPECIFIC** |
| F-C4 | Dependency, timeline, provenance and decision diagrams share a compiler | Public BrailleCanvas and Node compose directly | A bounded normalized segment list plus textual fallback. **HARNESS-SPECIFIC**, no core graph widget needed |
| F-C5 | Dynamic mount produces a usable visible instrument | Initial consumer frame accidentally recognized only its planned workspace/inspector names | Fixed before freeze: arbitrary focused IDs become the primary card and every remaining mounted ID gets a secondary card. **HARNESS-SPECIFIC consumer bug**, not a LibGibson failure |

F-C1 is the expected G1 pressure actually observed. It is real application code,
but the current public lower-level route succeeds. This experiment alone does
not justify moving its entire registry or authority policy into the engine.
The complete module is about 1,000 formatted lines including IR types,
validation, transactions, compiler, the four probes, presentation and replay;
focus lifecycle/dispatch account for only part of that cost. Avoid treating the
whole consumer size as missing-library boilerplate.

## Verification

**OBSERVED:** `cargo test --test instruments` passes 11 grouped tests. They cover:

- byte-before-decode, malformed/type/unknown-field JSON, control text, IDs,
  node/depth/raster/effect/rate/history bounds and missing Mono/seed contracts;
- invalid subscriptions and targets, unavailable actions, duplicate/missing
  bindings, and atomic rejection with whole-port equality;
- custom mounted IDs actually render; focus survives patches/reflow and returns
  to its previous owner after a temporary instrument is unmounted;
- actual action receipt plus inspector update; explicit denial survives to the
  final common semantic state;
- deterministic selection of all four planned representations and authority
  instrument mount/unmount;
- serialized input replay equality of specifications, patches, current focus,
  prior focus, actions, snapshot and lifecycle receipts across irregular times;
- 56×24, 80×24, 120×32 and 160×40 with TrueColor, ANSI16 and Mono;
- identical realized frames with **zero exact delta, zero affected cells, zero
  ANSI bytes**; zero/tiny/hostile terminal dimensions remain bounded;
- bounded lifetime identity allocation, output under a newly smaller caller
  budget, action exhaustion, malformed/out-of-order replay, and visible rejection.

`cargo clippy --all-targets --all-features -- -D warnings` also passed at handoff.
The 56×24 Mono permission frame was inspected as reconstructed terminal text:
both permission choices and the inspector remain visible. Repository-wide
real-PTY evidence is recorded by the campaign runner, not inferred from these
Surface tests. No universal aesthetic or arbitrary generated-UI claim follows.

## Verdict before reconciliation

**OBSERVED:** this planned dynamic-instrument probe is buildable on the pinned
public API without LibGibson source changes. G1 is an ergonomic friction point
with a working safe lower-level route. The provisional IR stays external.
Resource and authority rejection are successful safety behavior. More evidence
is required before calling the same mechanism an earned generic primitive gap.
