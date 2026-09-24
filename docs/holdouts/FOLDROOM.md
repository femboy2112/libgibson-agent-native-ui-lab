# FOLDROOM — post-freeze external holdout

**Verdict: PUBLIC API SUFFICIENT within the finite rigid-panel contract.** FOLDROOM is an interactive
prescribed folding-carton workbench built after the API freeze, with no changes to
LibGibson, frozen helpers, dependencies, or provisional IR. This is finite geometry
and terminal-interaction evidence, not a material, collision, or manufacturability
claim.

Authorship context received only the task, `docs/FROZEN_API_GUIDE.md`,
`docs/API_FREEZE.md`, frozen `src/ui.rs`, and public LibGibson source. It did not read
other consumers, research, or conversations. This is context separation within a
shared model family and filesystem, not process-isolated independence. The author
observed lab HEAD `69f06c15861ba9763ce3469b92c0d5a4bf75763c` on arrival. The
consumer targets frozen LibGibson
`e5ede0a4ab8ff52caa567ee10d45824e42c1ebc6` and frozen helper/IR revision
`1d1d27286b50c752527021697c14fd8a32ccd0ea`.

## Task and behavior

The net contains six rectangular faces and four side tabs. Its base is 2 × 1.5
units, walls are 1.2 units high, and tabs are 0.3 units wide. The lid hinges from
the back wall. Ten stable panel IDs connect the rendered label, selected highlight,
and inspector, including when the selected face is hidden by another face.

- **AROUND** raises front, left, back, right, tabs, then lid.
- **PAIRED** raises tabs, front/back, left/right, then lid.
- Left/Right scrub five degrees; `[` and `]` move ninety degrees along the sequence.
- `f` returns to flat; `c` closes the net; `s` changes sequence while retaining the
  integer fraction of total sequence progress.
- Tab/Up/Down select panels; keys `1`–`9`, `0` select IDs directly.
- `a`/`d` orbit; `w`/`x` tilt, including an underside view.
- Enter or `e` separates only the selected panel for inspection. This presentation
  translation never changes the authored hinge geometry. `r` resets everything.

No timer drives folding. The clock can advance while all semantic state and every
rendered cell remain unchanged. The frozen runner owns terminal I/O, recording,
replay, pause, exit, and the 128-input session limit. Its generic `--help` text was
left unchanged; the application footer gives the actual FOLDROOM controls.

Run from the lab:

```sh
cargo run --bin foldroom
cargo run --bin foldroom -- --dump --width=56 --height=24 --color=mono
cargo run --bin foldroom -- --record=/tmp/foldroom-inputs.json --seconds=30
cargo run --bin foldroom -- --replay=/tmp/foldroom-inputs.json --at=15000 --dump
cargo test --test foldroom
cargo clippy --test foldroom --bin foldroom -- -D warnings
```

## Public implementation route and bounds

Consumer-owned hinge definitions and rigid rotations produce ten quadrilateral
panels. Each panel submits two triangles through public
`gibson::raster3d::Rasterizer::draw_triangle`: **20 submitted triangles**, below the
96-triangle ceiling. Public depth-tested `line` draws the same panel boundaries;
`Camera::project` and `Rasterizer::depth` control label visibility. Public
`RgbRaster::to_surface` makes TrueColor half blocks. Mono deliberately uses neutral
panel intensities and `RgbRaster::to_mono_surface` for density glyphs, preserving
silhouettes without foreground/background colors. Both feed ordinary `Surface`
composition and frozen `ui::finish`/`ui::App`.

Raster dimensions are capped at **160 × 80**. Frame dimensions use the frozen
240 × 80 terminal bound. State is private, integer-bounded, serializable for
comparison, and reconstructed from validated input records rather than an
unchecked state deserializer. Geometry, display separation, camera state, and
viewport realization are separate.

No missing public API prevented this holdout. Application-owned work includes
hinge topology, stage routing, responsive inspector layout, and center-label depth
sampling. The public renderer has no panel-identity buffer; this consumer needs
stable keyboard selection, not pixel picking, so its explicit IDs suffice. Edges
use a 0.012-unit eye-directed offset to reduce coplanar raster artifacts. Labels
use a 0.12-unit center-depth tolerance. These are bounded presentation heuristics,
not exact visible-area measurement. At full closure some zero-thickness tabs
coincide with side walls; coplanar overlap is not interpreted as material contact
or an unambiguous visible object identity.

The first compile failed on an ambiguous consumer `map_or(...into())` expression,
with an unused-import warning. Both were corrected in `src/foldroom.rs`; no helper
or API changes were used. No geometric test failure was hidden or reclassified.

## Evidence and limits

Six focused tests pass:

1. All 40 flat vertices and all 40 closed vertices match explicit coordinate
   oracles, including four tabs, under both sequences.
2. Independent distance calculations verify edge lengths, diagonals, finite
   vertices, and both sides of every shared hinge at five-degree samples through
   each complete sequence. These are sampled invariants, not a formal proof.
3. Counterfactual angle/stage/sequence controls change geometry; camera controls
   change pixels while preserving geometry; separation changes exactly one display
   panel and preserves the physical model.
4. A closed lid actually hides the base's label while the inspector retains BASE.
   Opposite camera poses swap front/back label visibility; an underside view
   exposes the base and hides the lid. Selection survives all those occlusions.
5. A JSON input recording is reconstructed at 17 irregular times. State and every
   frame cell agree with sequential live event dispatch at **56 × 24, 80 × 24,
   120 × 32, and 160 × 40**, each in **TrueColor and Mono**. Resize leaves semantic
   state unchanged. Each unchanged state also produces **0 exact changed cells,
   0 affected cells, and 0 compiled wire bytes** after time advances.
6. Resource bounds, tiny/zero/oversized viewports, reset, camera/control clamps,
   the valid 128-input boundary, and rejection of 129 or unordered inputs pass.

The final strict focused Clippy run passes with `-D warnings`. The 56 × 24 Mono
executable dump was inspected and contains a filled labeled net, all six inspector
rows, and controls.

A real 120 × 32 Mono PTY session also ran the unmodified runner, exited with code
0, and emitted 11,805 terminal bytes. The supplied keys were `6 ] d e s c`; the
runner wrote all six input records, at 118, 276, 429, 663, 908, and 1122 ms. This
confirms the live input path in addition to pure replay tests. These timings are
observations from that run, not fixed benchmark requirements. The recorded file
was an ephemeral `/tmp` smoke artifact; the durable irregular replay fixture is
in `tests/foldroom.rs`.

Scope of the result: the frozen public substrate supports this finite folding
workbench through its lower-level raster and Surface path. No claim is made about
arbitrary nets, folding feasibility, collision-free sequences, paper thickness,
material mechanics, production tooling, terminal diversity beyond the checked
routes, or promotion of a new generic core primitive.
