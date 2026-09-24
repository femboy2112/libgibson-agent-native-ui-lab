# Experiment B — semantic reaction desk

**OBSERVED: implemented through the pinned public LibGibson API, without a
substrate patch.** This is a lab-owned editorial policy over the same finite
session used by manga and generated instruments. It does not establish a
general semantic understanding model.

Substrate: `e5ede0a4ab8ff52caa567ee10d45824e42c1ebc6` (Cargo Git revision).
Implementation: `src/reactions.rs`, `src/bin/reactions.rs`; contracts:
`tests/reactions.rs`. No external images, font assets, network/model calls,
private renderer access, or raw terminal output in the presentation.

## Run and interact

```sh
cargo run --bin reactions -- --mode=heavy
cargo run --bin reactions -- --mode=restrained --metaphor=paperwork
cargo run --bin reactions -- --mode=none
cargo run --bin reactions -- --mode=heavy --at=5000 --dump --width=120 --height=32
```

Tab/arrows select a registered, available action. Enter activates it. `L` and
`E` record local-repair versus architecture-expedition **follow-up scope**;
they do not rewrite the immutable recovery result. `Y`/`N` explicitly resolve
the actual permission request. A denial withholds the artifact. The current
scope decision remains visible after later fixture receipts replace the main
receipt. NONE keeps exactly the same useful controls, with no reaction overlay.
Space pauses the host runner; Esc/Ctrl-C exits. The shared runner supports
bounded `--record=PATH` / `--replay=PATH` input journals, including focus keys.

## Policy, authority, and replacement

Eleven typed roles: Dispatch, PlanAccepted, Progress, ToolFailure,
RepeatedFailure, ScopeExplosion, Recovery, PermissionRequest, BoringSuccess,
Artifact, Synthesis. At least eight are observed over the actual fixed fixture;
RepeatedFailure has an additional explicit two-failure negative-control fixture.
Each accepted cue retains its source event index and timestamp. Reactions are
original terminal-native jokes, not quoted or bundled memes.

The pure reducer reads at most 128 ordered source steps, uses same-role cooldown,
explicit exclusions, priority, and finite lifetime, and returns at most three
active cues. Restrained shows at most one salient cue; Heavy exposes minor cues
and one secondary caption. NONE produces no cues. Intensity is bounded to 1–3.
Config JSON is capped at 2,048 bytes, rejects unknown fields, invalid enum values,
duplicates, and out-of-range budgets. Invalid configuration does not mount a
fallback with broader authority. No policy branch emits an action.

ScopeExplosion has two replaceable presentations under the same role and action
interface: an expanding Braille constellation versus a stack of bureaucratic
requests to request a larger request. The second changes geometry and rhetoric,
not the event, target registrations, action, or resulting session. At small
sizes the respective caption and receipt remain readable while ornamental
geometry is omitted. NONE is identical under both metaphor settings.

The input dispatcher admits only four registered target strings. Unknown targets,
unavailable actions, backward timestamps, and full action history are rejected.
Keyboard rejection is visible. One of 64 action slots is reserved for permission,
so repeated scope choices cannot consume the ability to decline. Unrelated keys
and Control/Alt-modified application keys cannot activate reaction actions.

## First-contact friction

| ID | Desired behavior and public route attempted | Observed result / local work | Classification |
|---|---|---|---|
| F-B1 | Preserve a useful action across NONE, two visual metaphors, and reflow. `FocusId`, `FocusRing`, typed `KeyEvent`, app-owned semantic reducer. | Works. Four static IDs/targets, availability check, bounded focus traversal, explicit dispatch, and input journal are consumer glue. No unsafe fallback needed. This is not evidence that stable identity is impossible. | ERGONOMIC INCONVENIENCE |
| F-B2 | Policy replaces presentation without owning decisions. Ordinary `Node::panel`, `Node::text_wrapped`, `BrailleCanvas`, and clipped `Surface` composition. | Works. One source-indexed cue feeds both renderings; the selected mode changes presentation only. Comic/meme ontology stays outside core. | HARNESS-SPECIFIC |
| F-B3 | Reproduce focus as well as semantic actions. Public focus ring plus shared lab input journal. | Semantic actions alone omit Tab/arrow history. Full input replay reconstructs focus and frames. This is a consumer trace-completeness requirement, not a renderer defect. | HARNESS-SPECIFIC |
| F-B4 | Bound untrusted policy configuration and prevent a decorative overlay from claiming authority. Serde validation plus a fixed target registry before any Node generation. | Rejection paths work; no arbitrary key subscription, ANSI, or generated native code is admitted. Explicit action budgets preserve the permission slot. | DELIBERATE SAFETY BOUNDARY |

The formatted consumer is approximately 620 lines, including policy, validation,
input handling, and rendering; binary wrapper 19 lines. Those are not all routing
cost. The mapping/availability/dispatch portion is approximately 100 lines,
partly domain validation already needed independently of a UI engine. No
GENERIC PRIMITIVE GAP or EXPRESSIVE WALL was observed here. Reconciliation with
the other consumers must precede any core-promotion decision.

## Validation and claim limits

Executed on Rust 1.98.1:

```sh
cargo +1.98.1 test --test reactions
cargo +1.98.1 clippy --bin reactions --test reactions -- -D warnings
python3 scripts/pty_smoke.py target/debug/reactions
```

Seven integration tests pass. The matrix covers 56×24, 80×24, 120×32, 160×40;
TrueColor, ANSI16, Mono; all three modes and both metaphors. It checks complete
Snapshot equality (including permissions, artifacts, scope and action receipts),
source event order, deterministic frozen frames, and zero exact delta / affected
footprint / wire bytes. A counterfactual no-action session differs, so the
equivalence check can detect semantic changes. These six configurations share
one fixture/provenance family; they are not six independent semantic witnesses.

Recorded keys replay focus, emitted actions, input rejection notice, complete
Snapshot, and exact rendered Surface at ten irregular sampling times. Mode and
metaphor are explicit replay configuration. Resize changes geometry only; focus
remains an ID. The two scope metaphors produce different actual frame content,
and NONE produces equal frames under both. A selected animation interval has
nonzero damage confined to less than half the canonical frame; this is a probe
of local change, not a universal traffic bound.

The first damage probe used 6000→6100 ms and observed zero changes because the
rounded subcell positions coincided. It was corrected to 6000→6800 ms to test an
actual geometric displacement. This was probe resolution, not a renderer fix.

Three bounded live PTY runs exercised pause, Tab, permission denial, resize to
56×24, and Esc: 56×24 Mono (2,711 bytes), 120×32 TrueColor (8,294), 160×40 ANSI16
(11,480). Each exited 0 in approximately 1.4 seconds and restored termios and
the alternate screen. These short lifecycle smokes do not establish long-session
or platform portability claims. The host runner subsequently gained journaling;
the complete lab gate should rerun these smokes on its final revision.

Inspected current cell reconstructions of both 120×32 scope metaphors and 56×24
NONE/Mono permission state. The first scratch image font lacked Braille; a
Braille-capable fallback corrected the inspection instrument. PNGs/JSON captures
stay under `/tmp`, not in the repository. These were reconstructed cell images,
not a claim of human terminal-emulator visual acceptance across fonts.

**Conclusion: CORROBORATED within this fixture/configuration domain.** Typed
semantic editorial presentation, interchangeable metaphors, useful explicit
interaction, capability fallback, resize, and exact input replay fit the public
substrate. No claim of universal expressivity or mature routing infrastructure
follows from this single consumer.
