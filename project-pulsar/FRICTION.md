# FRICTION.md — building Project Pulsar on LibGibson @ 068807d

Honest notes from building a five-representation instrument on the v0.5 `gibson::plot`
layer plus the rest of LibGibson. Ordered by how much each cost me. "Repro" points to a test in
`tests/plot_defects.rs` that **pins the behaviour observed at this revision** (if upstream changes it,
the test flips). Items marked *(source)* I read in the source but did not need to reproduce.

What worked well, so the rest is read in proportion: the compile/render seam is real and genuinely
useful (I place my own identity glyphs with the very transform the renderer used); `PlotReport`
receipts let me prove masked samples are counted and never bridged; `ExtremaPerColumn` and the Log10
axis do what they say; the headless `Context` + `UiRuntime` capture path made "look at the frame"
cheap; `AI_VISUAL_AUTHORING.md` + the atlas plan were the right route and I followed them.

## A. `gibson::plot`

1. **Axis labels, units and series labels are never drawn.** `AxisSpec { label, unit }` and
   `Series::label` are stored, copied into `PlotLayout` (`x_axis`, `y_axis`) and ignored by
   `render`. The doc's "axes/ticks/labels" reads as if they appear. Every consumer must re-implement
   axis captions and reserve the rows for them (I did: `views::common::{caption, rows, draw_chrome}`).
   Repro: `friction_axis_labels_and_series_labels_are_never_rendered`. This is the single biggest
   gap for a *scientific* plot: a figure without axis titles and units is not a figure.

2. **`VLine` at exactly `view.x.max()` and `HLine` at exactly `view.y.min()` are not drawn** (the
   opposite bounds are). `compile` accepts `u == 1.0` (`0.0..=1.0`) but then sets
   `col_px = round(u * px_w) = px_w`, one subpixel past the last canvas column; `row_px` has the same
   off-by-one at `v == 0`. A "now" cursor at the right edge of a scrolling window is the natural way
   to hit it; mine vanished and I draw it by hand. Real (if minor) bug.
   Repro: `friction_edge_annotations_vanish_at_upper_x_and_lower_y`.

3. **Tick marks and data disagree by one cell.** Tick cells use `u * (width - 1)`, data uses
   `u * 2 * width / 2`, so a datum exactly on a tick value can sit one cell right of its tick mark
   (several of the six ticks, depending on width). Cosmetic but visible in a precise instrument.
   Repro: `friction_tick_cell_and_data_cell_can_disagree`.

4. **No hit-testing / cell conversion API, although the doc promises it.** §2 says `unproject`
   and hit-testing "share the exact same transform", but `PlotLayout.transform` maps to
   *plot-rect-local Braille subpixels* (2x4 per cell) and the tick `cell`s are absolute: converting a
   data point to an absolute screen cell (to overlay a glyph) or a cursor to data needs
   `plot_rect` offsets, a `/2` and `/4`, and a flip. I wrote `cell_of` / `data_at` and a round-trip test
   (`the_plot_transform_round_trips_between_data_cells_and_the_cursor`). A `layout.cell_of(x, y)` and
   `layout.data_at(cell)` would remove the most error-prone part of any interactive plot.

5. **Ticks are not configurable and log axes are sparse.** Target counts are hard-coded (6 / 5); a
   Log10 axis labels only powers of ten, so a 1.3-decade axis gets two labels (`log10_minor_ticks`
   exists in `ticks.rs` and is never used by `compile`). No custom tick values, no label formatter, no
   way to say "no y labels". Repro: `friction_log_axis_has_only_decade_ticks`.

6. **Chrome silently disappears below fixed thresholds.** x axis + labels vanish under 8 rows,
   y labels under 24 columns, title under 6 rows, with no way to force or query them before
   `compile`. At 42x15 my plot areas are 6-9 rows, so I had to size rows around the 8-row cliff.
   Repro: `friction_chrome_vanishes_at_small_sizes`.

7. **Series differ only by colour; nothing exists for Mono/text.** There is no dash, marker, glyph or
   weight per series, and the renderer is blind to `ColorDepth::Mono`, so two series are identical in a
   text capture (and on a Mono terminal). I encoded "now vs earlier" as Line vs Scatter. A
   `Series::marker(char)` / `dash` would fix it. Repro: `friction_series_differ_only_by_colour`.

8. **`Annotation::Point` is a dot plus a label drawn to the right, clipped at the area edge** *(source)*,
   with no collision avoidance and no way to put the label elsewhere or use a glyph. Identity needs a
   per-cell glyph, so I drew my own overlays from the layout and never used `Point`.

9. **The plot `Surface` is opaque.** Blank cells are real spaces with the default style (`Surface::new`),
   so `blit_transparent` cannot composite a plot over a backdrop; I copy whole blocks
   cell by cell (`theme::blit_all`) and would need an ink-only copy to layer a plot over anything.

10. **`PlotReport` is per call, summed over series.** My trace plot has two series, so
    `nonfinite_rejected` double-counts masked samples; I report masked counts from my own mask and
    test the receipt with a single-series plot. A per-series breakdown would make the receipt directly
    usable for UI ("12 samples dropped in series *S1*").

11. **Log10 + exact zeros.** `y <= 0` is a domain rejection and also breaks the line. Consistent with
    the contract, but a periodogram with exact zeros needs flooring by the caller.
    Repro: `friction_log_axis_zero_samples_break_the_line`.

12. **`FiniteRange::new` returns `Option`.** Fine for the type's contract, but every view range built
    from data needs a fallback (`views::common::range`). A `FiniteRange::around(centre, half)` or a
    `from_data_padded(&[f64])` would save a helper in every application.

13. **No envelope/band semantics.** On a noisy log spectrum `ExtremaPerColumn` fills each column from
    floor to peak, a solid wall of ink; there is no "min-max band + mean line". I brighten bins above
    8x floor with a second scatter series to make lines pop.

## B. Rasters, glyphs, and "can I see my frame?"

14. **`RgbRaster::to_surface()` writes `▀` into every cell, including black ones**, so a headless text
    capture of any raster is a wall of `▀` and the recipes' `--capture` cannot show a half-block image
    (their mono path is fine). **`to_mono_surface()` writes U+2800 (blank Braille), not a space**, so
    `to_visible_lines()` keeps it and any "is this cell ink?" test sees ink. I wrote a replacement
    (dark pairs -> space, honest `▀`/`▄`; U+2800 -> space) during development and then abandoned rasters
    altogether: my sky and phase-time map use cell-level Braille line art and a `░▒▓█` ramp, which read
    in text *and* in Mono.

15. **Text captures hide colour, and colour is half the design.** `last_frame_lines()` drops styles. I
    added a `:color` capture that dumps ANSI from the composed `Surface` (`Style::to_sgr` made that
    easy) and used a throw-away script to turn it into a PNG so I could actually *look* at hue,
    glow and ghosting. An official "capture with ANSI" in the recipes' harness would help.

16. **The capture harness is not API.** `examples/recipes_support/mod.rs::present` is where
    `Context::headless` + `UiRuntime::frame` + `last_frame_lines` are wired; applications must copy it
    (`app::render_through_runtime`). A public `ui::capture(view, model, WxH, depth)` would remove
    ~20 lines of load-bearing boilerplate from every app.

17. **Glyph capability is not threaded through custom drawing.** `UiEnvironment.glyph_mode` reaches
    `gibson::plot`, but my own `BrailleCanvas` line art must remember to call `glyph_at_mode(.., mode)`
    (it is easy to call `glyph_at` and silently ignore the capability axis; I did at first).

18. **East-Asian-ambiguous identity glyphs.** `● ◆ ▲ ■ ▼ ○ ◇ △ □ ▽` are width 1 for `unicode-width`
    defaults but ambiguous on CJK terminals; the glyph-capability axis has no notion of a "narrow-safe
    symbol set" for identity marks.

## C. UI runtime and input

19. **No mouse events.** `Event` has `Key`, `Paste`, `Resize`, `Tick`. The brief asks to *inspect
    coordinates*; the plot doc even lists "mouse hit-test UI" as deferred. I built a keyboard cursor
    (`hjkl`) that reads values through the plot transform. Fine, but a scientific viewer wants hover.

20. **`AppEvent::Tick` carries cumulative elapsed time**, not a delta, and arrives every loop
    iteration. I keep the previous value and subtract. Harmless once known; surprising the first time,
    and `Duration::saturating_sub` is needed to be safe.

21. **`Node::canvas` closures are `Fn + Send + Sync + 'static`** and the view is rebuilt every frame, so
    state reaches the paint by cloning the model into the closure each frame (fine here, ~1.5 ms/frame
    for everything). A `canvas_with(Arc<State>, |rect, &State| ..)` form would make the intended pattern
    explicit.

22. **`screen()` is natural-height** (documented, load-bearing) — I would have lost the hero without
    `.height(cx.environment.height)`; consider making `screen()` fill by default.

23. **The `gibson::ui` widgets did not help a dense instrument.** `status`/`text` rows are fine, but I
    needed cell-exact ledger columns, a scrubber with glyph marks and overlays that never reflow the
    hero, so the whole frame is one canvas and `gibson::ui` supplies only the runtime. That matches
    the authoring guide's advice, but it means the "sparse chrome" half of the stack went unused.

24. **Small API asymmetries.** `Surface::print_str` takes `(x, y, text, style, max_width)` while
    `SurfaceClip::print_str` takes `(x, y, text, style)`; `Glyph::new(&str)` vs `Glyph::from_char`;
    `plot` series/annotation colours are `(u8, u8, u8)` tuples while everything else uses `Color`.

## D. Process notes

* Floating-point only (no fast-math), seeded SplitMix64, no wall clock in paint paths: captures are
  byte-identical run to run. I could not verify bit-identity across platforms (libm differences), so
  the demo prints a digest.
* Dev profile `opt-level = 2` in `Cargo.toml`: the pipeline (FFT + folding for every epoch) runs at
  start-up and in tests; at `opt-level = 0` one world build takes ~7 s (measured: a single
  3-world test took 23 s) against ~0.9 s optimised, which would make the suite take minutes.
* One false start worth recording for other authors: the first sky/fold maps were half-block rasters;
  they looked fine in colour and were unreadable in a text capture and in Mono. Designing for the
  glyph first and colour second (the guide says so) was right and I should have started there.
