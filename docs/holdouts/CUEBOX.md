# CUEBOX post-freeze holdout

**Verdict: PUBLIC API SUFFICIENT within the finite rehearsal contract.**

CUEBOX is a fictional stage rehearsal desk: eight named performer tokens, four
patterned lighting regions, a twelve-cue sequence, and an explicit delayed-entrance
branch. The top-down stage, cue strip, inspector, and modal all consume one sampled
`Rehearsal` value. This is a bounded illustrative rehearsal, not theater control
software, a collision solver, or a photometric lighting model.

## Independence and authority

Implementation began after the recorded 2026-09-24 API freeze. The implementation
context read `docs/FROZEN_API_GUIDE.md`, `docs/API_FREEZE.md`, frozen `src/ui.rs`,
and public LibGibson source. It did not read planned consumers, their tests,
provisional IR, research dossier, or prior-context material. Other holdouts were
being implemented concurrently in the same filesystem; a compiler diagnostic
from a concurrent module was visible, but its source was not used.

The dependency is the frozen LibGibson revision
`e5ede0a4ab8ff52caa567ee10d45824e42c1ebc6`; the helper freeze is
`1d1d27286b50c752527021697c14fd8a32ccd0ea`. Owned files are
`src/cuebox.rs`, `src/bin/cuebox.rs`, `tests/cuebox.rs`, and this record. Root adds
the allowed `src/lib.rs` module export. No core, helper, dependency, or IR changes
were made to obtain a pass. Context separation is not process isolation, and the
implementation and test author share a model lineage with the surrounding work.

## Reproducible interaction

```sh
cargo run --bin cuebox
cargo run --bin cuebox -- --dump --at=17500 --width=56 --height=24 --color=mono
cargo run --bin cuebox -- --dump --at=25000 --width=120 --height=32 --color=truecolor
cargo run --bin cuebox -- --auto --speed=20 --seconds=4 --color=mono
```

- Tab/Shift-Tab, Up/Down, or 1–8 select a performer.
- `p` toggles rehearsal play; `n`/Right steps to the next cue and holds; Left
  returns to the preceding cue boundary. `r` resets to the on-time preset, held.
- `d` selects the delayed entrance; `o` selects the on-time entrance. Choose
  before Q05 starts. Later choices are ignored until the transport rewinds or
  resets before that boundary.
- Enter/`i` opens the current cue card; Enter/`i` closes it. The modal captures
  focus, ignores performer/transport/branch commands, and restores the exact
  previously selected performer. Rehearsal time continues while the card is open.
- The frozen host owns Space pause and Esc/Ctrl-C exit. The app uses `p` and
  Enter/`i` so those bindings do not conflict.

The default on-time route enters Hana during Q05. The delayed route holds her
in the wing through Q05/Q06, enters during Q07, and subsequently reconverges.
Both routes have twelve semantic cue slots; Story represents Q05 with two
alternative graph nodes, so the authored graph has thirteen nodes. Cue IDs and
actual movement differ, rather than merely changing a branch label.

All actors use bracketed letters `[A]`–`[H]`, with `<A>`-style selection. Lights
use `/`, `\\`, `.`, and `:` fields plus L1–L4 on/off labels. Selected movement
paths use `+`; the throne is `###` and rostrum is `===`. These encodings survive
Mono color removal. At 56×24 and 80×24, inspection moves below the stage; at
120×32 and 160×40 it occupies a sidebar.

## Public API route and friction

The consumer owns performer identities, marks, movement timing, transport,
branch choice, and panel layout. Public `Story`/`Beat`/`StoryDirector` provide
cue progression, entry facts, and the delayed branch guard. Public `Scene` and
`SceneEntity` provide named performer layers with deterministic identities and
composition. `FocusRing::capture`/`release` supply modal ownership restoration.
The frozen `ui::App` runner owns terminal input/output, recording, and replay.

Story deliberately follows at most one transition per update. Passing arbitrary
large frame deltas would make progression depend on frame cadence. CUEBOX instead
reconstructs the director from a fixed 50 ms domain grid: a maximum of 900 updates
through 45 seconds. Frame sampling is pure and never advances stored application
state. Transport keys commit bounded tick/time anchors; branch choice is explicit
consumer state installed as a Story entry fact when sampling. The exact key log
reconstructs both this domain configuration and focus/transport state.

A first composition attempt painted the stage floor to the output and then
mounted a performer-only Scene. This failed: `ui::mount` realizes into an opaque
surface, so its blank background erased lights and obstacles. The initial focused
run had eight passing tests and one failing visual-content test, with the actual
56×24 frame showing actors on an empty floor. The safe consumer-level correction
was to put the floor raster into the same Scene at z=-1, beneath the eight actor
nodes. No helper rescue was required. A regression now checks the actual floor
cells, obstacles, performer cells, and labels after final composition.

Public Surface drawing is used for the floor, fixed stage furniture, path, cue
strip, and inspector. This is the lower-level public route allowed by the freeze;
no generic widget or semantic schema extension was needed. The only early build
blocker was a transient type-inference error in another concurrently edited module;
it disappeared when that owner fixed it. CUEBOX did not edit that module.

The runner's generic `--help` still mentions permission keys and omits the
consumer-specific bindings; CUEBOX displays its bindings in the frame. This is
lab ergonomics, not an established core expressiveness gap. Runner wall-clock
limits and the 45-second supplied application clock also bound how long a held
interactive session can continue; this is not an unbounded production transport.

## Validation evidence

On 2026-09-24:

```sh
rustfmt src/cuebox.rs src/bin/cuebox.rs tests/cuebox.rs
cargo test --test cuebox -- --nocapture
cargo clippy --bin cuebox --test cuebox -- -D warnings
```

The final focused suite passed **10 tests**; strict Clippy passed with exit 0.
The acceptance probes cover:

- Independently written expected cue orders for both routes and all eight
  initial/final marks; the expectations do not read the implementation cue table.
- A delayed/on-time counterfactual with different Hana positions, lighting,
  Q05 graph nodes, and final frame cells; later performer states reconverge.
- Modal focus capture, blocked commands, exact prior-performer restoration,
  normal focus traversal skipping the modal, and stable Scene/focus identities
  across all four reflows.
- Explicit play, hold, next/previous cue, and saturated end-of-score behavior.
- JSON key-log round-trip and exact application state, rehearsal state, and
  framebuffer equality at fifteen irregular sample times, including a key
  suppressed by modal capture. Every time is checked at 56×24, 80×24, 120×32,
  and 160×40 in TrueColor and Mono. Frames leave application state unchanged.
- Actual stage cells for Hana's branch-dependent position, slash/backslash
  light fields, and on/off lower-right lighting, as well as composed obstacles,
  all eight tokens, stage/cue labels, and modal content.
- Held frames at different timestamps produce **0 exact changed cells /
  0 affected cells / 0 compiled bytes** at every requested size and color mode.
- Zero/tiny/oversized dimensions, u64::MAX timestamps, a capped 900-step Story
  trace, a 240×80 maximum surface, rejected traces over 128 records,
  rejected decreasing timestamps/control-character traces, and ignored modified
  application commands.

Both listed dump commands ran successfully and their final plain cell output was
inspected. A real PTY run using
`TERM=xterm-256color cargo run --quiet --bin cuebox -- --auto --speed=20 --seconds=4 --color=mono`
completed with exit 0, reached the HOLD 45.0s curtain state, and executed terminal
restoration. This is a live runner smoke, not a human usability assessment.

## Claim limits

**Observed:** this externally authored consumer expresses the specified bounded
rehearsal, branching, rendering, capture, and replay behavior through the frozen
public APIs. The measured frozen-frame diff triple is 0/0/0 in the tested matrix.
The floor-erasure counterexample refuted the first composition attempt and is
retained in this record; it was a consumer composition mistake, not evidence
requiring a new core primitive.

**Not established:** general UI universality, arbitrary theater workflows,
accessibility certification, broad human usability, all terminal/emulator/font
behavior, performance at unbounded scale, real motion planning, or real light
physics. Fixed expected values provide a second implementation of the test
expectation, not an independently blinded human or model audit. The surviving
claim is limited to the frozen source map and stated probes; source correctness
and broader applicability do not follow solely from agreement among frame checks.
