//! Minimal reproducers for surprises found in `gibson::plot` (see FRICTION.md).
//!
//! These tests PIN the behaviour observed at the pinned LibGibson revision
//! (068807d). They are not claims that the behaviour is intended: if upstream
//! changes it, the corresponding test flips and tells us the friction is gone.

use gibson::plot::{
    self, Annotation, AxisScale, AxisSpec, FiniteRange, PlotSpec, PlotView, Series,
};
use gibson::{Rect, SubcellGlyphMode};

fn fr(a: f64, b: f64) -> FiniteRange {
    FiniteRange::new(a, b).expect("range")
}

fn blank_spec() -> PlotSpec {
    PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "frequency").unit("Hz"),
        AxisSpec::new(AxisScale::Linear, "power").unit("dB"),
    )
}

fn braille_cells(s: &gibson::Surface, rect: Rect) -> usize {
    let mut n = 0;
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            if let Some(c) = s.get(x, y) {
                if c.glyph
                    .grapheme
                    .chars()
                    .any(|ch| ('\u{2801}'..='\u{28FF}').contains(&ch))
                {
                    n += 1;
                }
            }
        }
    }
    n
}

/// F1 - `AxisSpec::label`/`unit` and `Series::label` are stored and returned in
/// `PlotLayout`, but `render` never draws them: the caller must caption axes.
#[test]
fn friction_axis_labels_and_series_labels_are_never_rendered() {
    let spec = blank_spec()
        .title("")
        .series(Series::line(vec![(0.0, 0.0), (1.0, 1.0)]).label("UNIQUE-SERIES-LABEL"));
    let view = PlotView::new(fr(0.0, 1.0), fr(0.0, 1.0));
    let (surface, _) = plot::plot(
        &spec,
        &view,
        Rect::new(0, 0, 80, 24),
        SubcellGlyphMode::Braille2x4,
    );
    let text = surface.to_visible_lines().join("\n");
    assert!(
        !text.contains("frequency"),
        "axis label unexpectedly rendered"
    );
    assert!(!text.contains("Hz"), "axis unit unexpectedly rendered");
    assert!(
        !text.contains("UNIQUE-SERIES-LABEL"),
        "series label unexpectedly rendered"
    );
    // ...yet the layout does carry them:
    let (layout, _) = plot::compile(&spec, &view, Rect::new(0, 0, 80, 24));
    assert_eq!(layout.x_axis.label, "frequency");
    assert_eq!(layout.y_axis.unit.as_deref(), Some("dB"));
}

/// F2 - A `VLine` at exactly the view's upper x bound, or an `HLine` at exactly
/// the lower y bound, passes the `0..=1` containment test but lands one subpixel
/// outside the canvas and is not drawn. The opposite bounds ARE drawn.
#[test]
fn friction_edge_annotations_vanish_at_upper_x_and_lower_y() {
    let view = PlotView::new(fr(0.0, 10.0), fr(0.0, 10.0));
    let draw = |ann: Annotation| {
        let spec = blank_spec().annotate(ann);
        let (layout, _) = plot::compile(&spec, &view, Rect::new(0, 0, 60, 20));
        let s = plot::render(&layout, SubcellGlyphMode::Braille2x4);
        braille_cells(&s, layout.plot_rect)
    };
    let c = (255, 255, 255);
    assert!(
        draw(Annotation::VLine { x: 0.0, color: c }) > 0,
        "VLine at x.min is drawn"
    );
    assert!(
        draw(Annotation::VLine { x: 5.0, color: c }) > 0,
        "VLine inside is drawn"
    );
    assert_eq!(
        draw(Annotation::VLine { x: 10.0, color: c }),
        0,
        "VLine at x.max is NOT drawn"
    );
    assert!(
        draw(Annotation::HLine { y: 10.0, color: c }) > 0,
        "HLine at y.max is drawn"
    );
    assert_eq!(
        draw(Annotation::HLine { y: 0.0, color: c }),
        0,
        "HLine at y.min is NOT drawn"
    );
}

/// F3 - Tick marks are placed with a `(width - 1)` scale but data with a `width`
/// scale, so a datum exactly on a tick value can land one cell to the side of
/// its tick mark (here: at least one of the six ticks of a 0..1 axis).
#[test]
fn friction_tick_cell_and_data_cell_can_disagree() {
    let view = PlotView::new(fr(0.0, 1.0), fr(0.0, 1.0));
    let mut disagreements = Vec::new();
    for width in 90u16..=130 {
        let spec = blank_spec();
        let (layout, _) = plot::compile(&spec, &view, Rect::new(0, 0, width, 30));
        let t = layout.transform.expect("transform");
        for tick in &layout.x_ticks {
            let (px, _) = t.project(tick.value, 0.5).expect("projects");
            let data_cell =
                layout.plot_rect.x + ((px / 2.0).floor() as u16).min(layout.plot_rect.width - 1);
            let d = data_cell as i32 - tick.cell as i32;
            assert!(d.abs() <= 1, "never more than one cell apart");
            if d != 0 {
                disagreements.push((width, tick.value, d));
            }
        }
    }
    assert!(
        !disagreements.is_empty(),
        "if this fails upstream made tick cells and data cells agree"
    );
}

/// F4 - Tick counts are fixed (6 x, 5 y) and a log axis labels powers of ten
/// only: a 1.3-decade axis gets two labelled ticks. `log10_minor_ticks` exists
/// but `compile` never uses it.
#[test]
fn friction_log_axis_has_only_decade_ticks() {
    let spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "x"),
        AxisSpec::new(AxisScale::Log10, "power"),
    );
    let view = PlotView::new(fr(0.0, 1.0), fr(0.3, 20.0));
    let (layout, _) = plot::compile(&spec, &view, Rect::new(0, 0, 80, 24));
    let labels: Vec<&str> = layout.y_ticks.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, vec!["1", "10"]);
    assert!(
        !plot::log10_minor_ticks(fr(0.3, 20.0)).is_empty(),
        "minor ticks exist..."
    );
}

/// F5 - Below 8 rows the x axis (line AND labels) disappears entirely; below 24
/// columns the y labels do. There is no way to ask for them.
#[test]
fn friction_chrome_vanishes_at_small_sizes() {
    let spec = blank_spec().series(Series::line(vec![(0.0, 0.0), (1.0, 1.0)]));
    let view = PlotView::new(fr(0.0, 1.0), fr(0.0, 1.0));
    let (tall, _) = plot::compile(&spec, &view, Rect::new(0, 0, 40, 8));
    let (short, _) = plot::compile(&spec, &view, Rect::new(0, 0, 40, 7));
    assert!(tall.show_x_labels && !short.show_x_labels);
    let (wide, _) = plot::compile(&spec, &view, Rect::new(0, 0, 24, 12));
    let (narrow, _) = plot::compile(&spec, &view, Rect::new(0, 0, 23, 12));
    assert!(wide.show_y_labels && !narrow.show_y_labels);
}

/// F6 - The plot's `Surface` is opaque (blanks are real spaces) and the `Mono`
/// colour depth is invisible to the renderer, so two series differ ONLY by
/// colour: in a text capture they are indistinguishable.
#[test]
fn friction_series_differ_only_by_colour() {
    let view = PlotView::new(fr(0.0, 1.0), fr(0.0, 1.0));
    let pts: Vec<(f64, f64)> = (0..=20)
        .map(|i| (i as f64 / 20.0, i as f64 / 20.0))
        .collect();
    let red = blank_spec().series(Series::line(pts.clone()).color((255, 0, 0)));
    let blue = blank_spec().series(Series::line(pts).color((0, 0, 255)));
    let a = plot::plot(
        &red,
        &view,
        Rect::new(0, 0, 50, 16),
        SubcellGlyphMode::Braille2x4,
    )
    .0;
    let b = plot::plot(
        &blue,
        &view,
        Rect::new(0, 0, 50, 16),
        SubcellGlyphMode::Braille2x4,
    )
    .0;
    assert_eq!(a.to_visible_lines(), b.to_visible_lines());
}

/// F7 - `reduce_extrema` keeps a one-sample spike (law J)... but a Log10 axis
/// with the zero-valued floor of a spectrum counts every zero as a domain
/// rejection AND breaks the line there. Callers must floor their data.
#[test]
fn friction_log_axis_zero_samples_break_the_line() {
    let spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "f"),
        AxisSpec::new(AxisScale::Log10, "p"),
    )
    .series(Series::line(vec![
        (0.0, 1.0),
        (1.0, 2.0),
        (2.0, 0.0),
        (3.0, 2.0),
        (4.0, 1.0),
    ]));
    let view = PlotView::new(fr(0.0, 4.0), fr(0.1, 10.0));
    let (_, rep) = plot::compile(&spec, &view, Rect::new(0, 0, 60, 20));
    assert_eq!(rep.scale_domain_rejected, 1);
    assert_eq!(
        rep.segments_considered, 2,
        "3 valid segments minus the 2 broken by the zero"
    );
}
