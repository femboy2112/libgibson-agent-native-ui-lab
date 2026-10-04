# FRICTION — building Project Pulsar against `libgibson@8d69c01`

Honest notes on what was confusing, missing, surprising or broken, **ordered by how much it cost
me** (time lost, or design forced). Items marked **DEFECT** are suspected library bugs with a
reproducer in `tests/library_defects.rs`; those tests *pin the observed behaviour* (so the suite
stays green) and fail loudly if the library changes. I did not patch or work around the library
inside those tests. Items marked **GAP** are missing capabilities, **FRICTION** is ergonomics or
documentation. I looked at everything through rendered frames, ANSI → PNG renders, a PTY, and a
terminal emulator crate, but never on a real Linux VT or a real emulator.

## 1. GAP — the plot layer stops where the app starts (cost: most of the custom graphics)

`gibson::plot` is a good *x–y* layer, and the doc is upfront that heat-maps, polar/orthographic
projection and legends are deferred. In a signal-analysis app that leaves out three of the six
representations I needed (dynamic spectrum, phase–time waterfall, the all-sky map), so they are
custom `Surface`/`BrailleCanvas` code (several hundred lines). Two follow-on costs worth stating:

* **No identity channel except colour.** `Series` has a colour and nothing else — no dash, no
  marker, no per-series glyph — and `Series::label` is stored but **never rendered** (no legend;
  `observation_series_label_is_not_rendered`). In Mono (colour dropped by the quantizer) three
  series are indistinguishable. I had to build identity out of redundancy *outside* the library:
  direct `Annotation::Point` labels (`◆A`, `●B`), one lane per identity, lane headers, and the rail.
  A per-series marker glyph or stroke style would have saved a day.
* Aligning a custom raster under a plot worked well only because `PlotLayout::plot_rect` and
  `PlotTransform2D` are public — that part of the design is excellent and I leaned on it hard.

## 2. DEFECT — tick labels: truncation shows a different number; neighbours overprint (cost: noticed late)

At narrow widths the axis chrome degrades **into wrong information** rather than dropping labels,
which contradicts doc §7 ("may drop tick labels as space shrinks, but never moves a semantic tick value").

* `defect_truncated_y_tick_label_shows_a_different_number` — a y tick of −1 000 000 on a
  24-column plot is truncated from the right and drawn as `-100000` (a different number, ×0.1).
* `defect_colliding_x_tick_labels_run_together` — six x ticks on a 24-column plot print as
  `0.00.2 0.4 0.6 0.81.0`; with a wide y gutter the 0.4 label is destroyed outright.

Reproducer (also in the test):
```rust
let view = PlotView::new(FiniteRange::new(0.0,1.0).unwrap(), FiniteRange::new(-1.2e6,1.2e6).unwrap());
let (l, _) = compile(&spec, &view, Rect::new(0,0,24,12)).unwrap();
// l.y_ticks contains label "-1000000", but the rendered gutter shows "-100000┤"
```
Impact on Pulsar: none visible in my captures (at 42×15 the lanes are too short to get x labels at all, at 80×24 the labels fit), so I found this by probing, not by looking — any app that lets a user squeeze a plot to 24–40 columns will hit it, and I did not mitigate it.

## 3. DEFECT — `Reduce::ExtremaPerColumn` is not the per-device-column envelope the doc promises, in two situations

This is the library's flagship feature (it is why one bright spectral bin survives 16 000 bins
squeezed into ~200 columns, and my tests confirm that in the ordinary case:
`a_one_bin_line_survives_the_extrema_reducer_in_the_drawn_frame`). It breaks at the edges:

* **3a — out-of-view samples evict in-view ones** (`defect_extrema_reducer_lets_out_of_view_samples_evict_an_in_view_spike`).
  `reduce_extrema` clamps each sample's column index into `[0, num_cols-1]`, so all data left of
  the view land in column 0 and all data right of it in the last column, and their extreme values
  fill the four first/min/max/last slots *before* clipping. Zoom into a region next to a larger
  excursion and a real spike at the edge vanishes (law J violated under pan/zoom, which §3 says
  must "never" change the data). Reproducer: 2000 samples, huge alternating values for x < 500,
  one in-view spike of 1.0 at x = 505, view `x ∈ [500, 2000]`: 5 segments reach the top row
  unreduced, **0** reduced. *My workaround:* the app slices its data to the visible window before
  handing it over (the stream and spectrum views do), which defeats the library's purpose of
  taking the full series and a view.
* **3b — log-x axes** (`defect_extrema_reducer_on_a_log_x_axis_does_not_preserve_per_column_envelopes`).
  Buckets are linear in x but device columns on a `Log10` axis are not, so low-x columns share one
  bucket's four points: on a sine over 1…1000 in an 80-column area (148 sub-pixel columns), 102 columns have a
  different vertical extent after reduction and the ink drops from 891 to 607 cells.

## 4. DEFECT — a `Log10` axis with no power of ten in view has no ticks (cost: shaped a design choice)

`compile` only calls `major_ticks`; `log10_minor_ticks` is exported but unused, although doc §7 says
"minors (2..9 × 10^k) only if space supports them" (`defect_log_axis_without_a_power_of_ten_in_view_has_no_ticks`).
A log axis over 2…8 has **no** labels; over 2…60 it has one ("10"). I chose my log ranges so each
contains a power of ten, which is exactly the constraint a zoom feature cannot respect.

## 5. DEFECT (gibson::ui) — `modal` silently shows at most nine rows (cost: ~20 min, found by looking at a capture)

A modal with 12 text children inside `screen().height(24)` shows 9; with 30 children on a 40-row
screen it still shows 9; there is no scroll and no warning
(`defect_modal_content_is_silently_capped_at_nine_rows`). Setting an explicit `.height(n)` shows
them all, which is how the help overlay works. The doc says modals "center through ordinary Taffy
constraints" and mentions no cap.

## 6. FRICTION — `App` and the deterministic world (cost: design time)

* `App::run` owns `Context::new` and offers no way to force a colour depth, so a **Mono mode** needs
  `Context::fullscreen()` + `set_color_depth` + `run_with_context`; and the **glyph mode** can only
  be chosen through the `LIBGIBSON_GLYPHS` environment variable (the loop calls
  `detect_glyph_mode_from_env(None)` itself), so `--glyphs` sets the variable in-process.
* `AppEvent::Tick` carries *elapsed since start*, not a delta, and nothing says so; a deterministic
  app has to turn it into quantised, **recorded** `Advance(n)` commands itself (I did; the docs'
  "explicit time" discipline is great, but the loop that feeds it is wall-clock).
* `Node::surface` takes its size from the surface, so "fill the parent" has to be computed from
  `UiEnvironment` by the application; `screen()` is natural-height and needs `.height(env.height)`
  (documented in the authoring guide — the guide saved me here).
* The runtime consumes **nothing** I did not ask for: with no focusable controls Tab/arrows/letters
  all arrive as `AppEvent::Input`, and a modal swallows everything but Esc (documented, tested in the PTY).

## 7. FRICTION — documentation and API surface

* PLOT doc §3 lists `Series { kind, points, color, reduce }`; the code also has `label` (rendered nowhere).
* `gibson::plot::layout::Prims`, `ProjectedSeries`, `ProjAnnotation` are reachable only through
  `gibson::plot::layout::…` while `PlotLayout`'s *public fields* use them; `gibson::plot::Prims` does not exist.
* `PlotLayout`, `PlotSpec`, `Series` have no `PartialEq`; "same input ⇒ same geometry" tests compare `Debug` strings.
* `SubcellGlyphMode` is `#[non_exhaustive]`, so a harmless `match` over its four variants needs a wildcard arm.
* `gibson::plot::plot` on a 0×0 area returns a 1×1 blank surface (doc: "valid-but-empty area is Ok with a blank surface") — fine, but my wrapper had to special-case it to avoid blitting a ghost cell.
* `Annotation`s outside the view vanish with no receipt counter (`observation_out_of_view_annotations_vanish_without_a_receipt`). Receipts count *samples*; "plotting must never silently eat data" is true of samples only.
* `Annotation::Point` labels always extend rightwards from the point and overlap freely; no leader lines, no collision avoidance. Fine for a handful of labels, a limit for a spectrum comb.
* PLOT doc links `docs/research/SAI_ARCHITECTURAL_CROSSOVER.md` and AI_VISUAL_AUTHORING links `docs/research/AI_VISUAL_AUTHORING_AUDIT.md`; neither is in the reference copy (the reference is stated to be pruned, so this may be an artefact of the canary).
* The first `cargo build` of the pinned git dependency took ≈ 35 s (fine).

## 8. What worked unusually well (so it is not lost)

* **The receipt is real.** `PlotReport` let me assert, for every plot in every view at three sizes,
  `samples_seen = Σ series lens`, `finite + nonfinite = seen`, the line-conservation law, and that
  every `NaN` gap in my series was *counted*. That turned "does the plot eat data?" from a feeling
  into a test.
* **`compile`/`render` seam.** Geometry tests never depend on glyphs; I proved capability naturality
  (colour depth and glyph mode do not move geometry) by comparing `PlotLayout` across `Env`s.
* **`PlotTransform2D::unproject` is a correct hit-test.** I round-tripped *every* cell of *every*
  plot in four views, including log axes (`inspect_readout_matches_the_plot_transform_and_round_trips`),
  and found no off-by-one — the one-last-index convention in doc §5 holds.
* **`Context::headless` + `last_frame_lines`** make captures byte-deterministic and let a PTY test
  compare a live screen with a replayed one.
* **The visual-authoring guide** (Panel Farm, atlas, hero-with-HUD, `--capture` as a run mode) shaped
  the whole architecture and kept me honest.

## Defects and bugs in *this* application (found, fixed, or still open)

* *Fixed:* first key after start took ~300 ms because the background pre-warm held the engine lock
  during DSP. Now readers of computed checkpoints never wait. Seeking past the pre-warm front still
  blocks until the chain is computed (≤ ≈ 3 s once at startup) — a frame must be a pure function of
  the model, so I did not show "analysis catching up".
* *Fixed:* a high-SNR test first failed for GAMMA — not a bug: its position is statistically
  unconstrained (0.53 Hz on a 56 ms baseline); the test now judges it against its own σ.
* *Fixed:* an early version asserted the terrestrial line's delays are unphysical; they alias
  (66 ms > half the 75 ms period), so only the amplitude test rejects it. The scenario comment and
  the test were corrected rather than the physics.
* *Open:* the dynamic spectrum cannot show BETA's ridge (per-segment S/N ≈ 2); it shows the *model*
  path instead. A matched-filter-integrated ridge would be the honest picture.
* *Open:* delay phases are assumed unambiguous (constrained by the generator, undetected otherwise).
* *Open:* at 42×15 the lanes are too short for the library to draw axis labels (its thresholds: x labels need ≥ 8 rows, titles ≥ 6, captions ≥ 12), so those graphs are readable only through the inspect cursor and the lane headers I add myself.
* *Open:* PTY tests are timing-based (generous timeouts, polling); they passed repeatedly here but
  are the first thing I would expect to flake on a loaded machine.
