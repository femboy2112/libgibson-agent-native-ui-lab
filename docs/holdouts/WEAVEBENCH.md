# H3: Weavebench

**Verdict: PUBLIC API SUFFICIENT within the finite draft-editing contract.**

Status: implemented against the frozen public substrate. Consumer semantics,
rendered crossing topology, input replay, and responsive frames passed the
focused checks described below. This is a bounded synthetic weaving-draft
instrument, not a textile-mechanics model or evidence of production usability.

## Selection and independence

Weavebench was proposed in the explicitly recorded replacement-selection round.
Three initial concepts were rejected by the root's contamination screen; this
agent did not inspect that screen's dossier. Weavebench's purpose is to edit a
four-shaft loom draft, inspect over/under crossings, and find long cyclic floats.

The same agent designed and implemented Weavebench after two attempts to spawn a
fresh implementer were rejected by the four-thread limit. The weaker
designer/implementer independence was preregistered by the root at `f87adeb`.
The independent Boolean and interval-enumeration checks below are different
algorithms written in this same context. They are not an independently blinded
implementation or independent empirical evidence.

Substrate: `e5ede0a4ab8ff52caa567ee10d45824e42c1ebc6`.
Frozen helper commit: `1d1d272`.
The consumer owns only `src/weavebench.rs`, `src/bin/weavebench.rs`,
`tests/weavebench.rs`, and this document. The root supplied the module export.
No core API, frozen helper, Cargo manifest, or provisional schema was changed.

## Running and using it

```sh
cargo run --offline --bin weavebench
cargo run --offline --bin weavebench -- --color=mono
cargo run --offline --bin weavebench -- --dump --at=3600 --width=56 --height=24 --color=mono
cargo run --offline --bin weavebench -- --record=weave-keys.json
cargo run --offline --bin weavebench -- --replay=weave-keys.json
```

- `Tab` / reverse Tab cycle Cloth, Threading, Tie-up, and Treadling.
- Cloth: arrows select a crossing and pin the viewport to that selection.
  `f` resumes following the shuttle. The inspector retains the selected crossing
  even while the following viewport moves elsewhere.
- Threading: left/right choose a warp end; up/down or Enter change its shaft.
- Tie-up: arrows choose a shaft/treadle intersection; Enter toggles its lift.
- Treadling: up/down choose a pick; left/right or Enter change its treadle.
- `r` toggles one repeat versus a two-by-two repeated workspace. Selection in
  any repeated tile edits the same underlying draft coordinate.
- `w` pauses/resumes the consumer's weave clock; `n` completes the next row and
  pauses; `0` rewinds without changing whether the clock is playing.
- `c`, `t`, `u`, and `p` jump directly to the four editing modes.
- Space is the frozen runner's host pause. Use `w` when the pause must be an
  explicit recorded consumer action. Escape exits through the runner.

The cyan strand is warp and the amber strand is weft in TrueColor. Color is
supplementary: a continuous upper strand and an interrupted lower strand carry
the crossing topology in both TrueColor and Mono. The enlarged inspector shows
the selected crossing, shaft/treadle relationship, and cyclic float maxima.
`16+` means a whole strand remains on top throughout the repeat; that uninterrupted
float continues across repeated copies. The numerical model saturates this case
at 16 rather than inventing a finite maximum for an infinite periodic cloth.

Dim fabric is the planned unwoven draft. The shuttle moves left-to-right on even
picks and right-to-left on odd picks, turning that draft into woven presentation.
Changing the draft is an immediate what-if re-evaluation of the cloth; it does
not pretend that already manufactured cloth physically changes.

## Finite implementation and boundaries

- Four shafts, four treadles, 16 warp ends, 16 picks, and four four-bit tie-up
  masks. One shaft per end and one treadle per pick; multi-treadle simultaneous
  lifting, yarn tension, material behavior, and manufacturing export are outside
  this fixture.
- One 16-row weave takes 9,600 logical milliseconds, 600 ms per row. Sampling
  frequency never changes the model. Progress saturates after 16 rows; the
  frozen runner has a finite 18,000 ms end time.
- At most four repeated tiles exist in the display workspace. Primary crossings
  use at least three cells by two cells, or six by eight Braille samples.
  Smaller terminals scroll rather than collapse topology. The default viewport
  follows all 16 shuttle passes; arrows switch to following the selected crossing.
- Frames use bounded `Surface` and `BrailleCanvas` allocations. Dimensions clamp
  through the unchanged helper to 240 by 80 cells. The model retains no growing
  history. Input recording/replay uses the helper's 128-record bound.
- All geometry, selection routing, loom logic, and clock state are consumer-owned.
  The only terminal I/O is the frozen `ui::App` runner. No image protocol,
  generated native code, direct terminal ANSI, network call, or model call is used.

## Evidence and focused gates

`cargo test --offline --test weavebench` passes 12 tests. The tests cover:

1. A separate lifted-shaft-membership oracle over 192 draft variants and a known
   initial two-over/two-under row.
2. Cyclic float maxima checked by enumerating every start and interval length,
   including a wraparound run and fully exposed strands.
3. All 16 tie-up edits changing exactly the crossings whose shaft and treadle
   depend on that bit.
4. Every threading and treadling assignment location compared with independently
   constructed expected cloth, with unrelated rows/columns unchanged.
5. All 16 shuttle row boundaries, subrow progress, pause, step, rewind, saturation,
   and a rendered visible-shuttle witness for every pass at all eight size/mode
   combinations.
6. Focus traversal, repeated-coordinate identity, selection-pinned scrolling,
   and restoring shuttle following.
7. Full consumer-state equality and full rendered-frame equality during replay
   of a 27-key trace at irregular timestamps, including JSON round-trip; repeated
   resize/render calls do not mutate any state.
8. Acceptance of 128 records and rejection of 129 records or unordered timestamps.
9. Decoded Braille dots in both the PRIMARY cloth and enlarged inspector: the
   upper strand is continuous, both lower ends remain, and the lower strand has
   gaps beside the intersection. Reversing strand order changes actual glyphs,
   including in Mono; this check does not rely on foreground colors or labels.
10. Four required sizes (56x24, 80x24, 120x32, 160x40), TrueColor and Mono,
    all four editing modes, retained labels and dense cloth geometry, no color
    attributes in Mono, and frozen-frame exact changes / affected cells / emitted
    bytes of **0 / 0 / 0**. Tiny/zero dimensions and maximum bounds are also checked.

`cargo clippy --offline --bin weavebench --test weavebench -- -D warnings` passes.
Only assigned Rust files were formatted with standalone `rustfmt --edition 2021`.

A real 80x24 pseudo-terminal smoke ran the frozen runner in Mono, sent the ASCII
keyboard sequence `t`, Enter, `u`, Enter, `p`, Enter, `c`, `r`, `w`, `n`, `f`,
and exited successfully: exit code 0, 10,740 output bytes, title observed, and
0.926 seconds wall time. The capture was bounded at 512 KiB and five seconds.
That smoke checks the live runtime path and key delivery; semantic assertions
come from the deterministic tests, not from interpreting captured terminal bytes.

The root separately reported structural inspection of TrueColor 120-column and
Mono 56-column frames and a passing PTY focus/resize/restore check. These are
root-reported observations, not tests independently rerun by this agent. The
frame viewer does not accurately capture terminal font dimming. Visual inspection
therefore supports structural legibility only, not terminal-emulator or font
certification. No user usability evaluation was conducted.

## Friction, failed attempts, and corrections

1. The first primary-cloth geometry used four by four samples and gaps only at
   the shared intersection. Root review identified that repainting the upper
   strand recreated that dot, collapsing both crossing orders to the same Mono
   glyph union. This was a consumer presentation defect, not a substrate wall.
   Minimum primary tiles were enlarged to six by eight samples, actual gaps now
   extend past the intersection, and the PRIMARY-cloth topology oracle was added.
   The original bad version was identified by inspection; no claim is made that
   an old-version negative-control execution was performed.
2. The first integration-test compile failed with E0451 because struct-update
   syntax touched private clock fields. Tests now construct the default app and
   assign public edit state; no clock visibility or core API was loosened.
3. The first strict Clippy run rejected `needless_range_loop` in the independent
   float oracle. Iterating rows with `enumerate` removed the warning while keeping
   the interval-enumeration algorithm distinct from the implementation's scan.
4. Cargo briefly waited for the shared build-directory lock during concurrent
   consumer work. Commands completed; no test failure was recategorized as green.
5. Keeping all 16 rows compressed into the smallest frame conflicted with visible
   crossing topology. The final small layout exposes a scrolling window and an
   enlarged inspector. A follow-shuttle control ensures later passes remain
   observable rather than silently running below that window.

No frozen API/helper blocker was encountered. Passing these bounded fixtures
does not establish a general loom editor, arbitrary-terminal portability, or
independent post-freeze replication.

## Tool and source-access journal

Design orientation read the frozen public API guide, `src/lib.rs`, the first
260 lines of `src/node.rs`, and public declarations in `canvas`, `raster`,
`raster3d`, `scene`, `story`, `input`, `focus`, `surface`, `raster_fx`, and
`surface_fx` under the substrate `src` directory. The replacement-selection
round read no additional files and wrote none.

Implementation read only the assigned source scaffold, frozen `src/ui.rs`, and
public API portions of `cell.rs`, `node.rs`, `input.rs`, `canvas.rs`, `diff.rs`,
`ansi.rs`, and `surface.rs`. `cargo metadata --no-deps --format-version 1` supplied
the crate name, pinned dependency revision, and target metadata; it exposed target
names but no other consumer contents. No examples, other consumers, holdout
implementations, research dossier, tests outside this holdout, or memories were read.

Commands/tool operations were: bounded `cat`/`sed`/`rg` source orientation;
assigned-path `git status`; Cargo metadata; `apply_patch` edits to the four owned
files; `cargo check --offline --bin weavebench`; standalone assigned-file
`rustfmt`; the focused test command (initial compile failure, then passing runs);
the focused Clippy command (initial lint failure, then pass); a 56x24 Mono dump;
and a bounded Python standard-library PTY harness invoking the real consumer.
Tool-session polling only collected output from those commands. No commit or
push was made by this agent.
