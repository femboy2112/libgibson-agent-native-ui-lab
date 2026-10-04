//! Reproducers for suspected LIBRARY defects found while building Pulsar against
//! `libgibson@8d69c01`. Each test **pins the behaviour observed** (so this file stays
//! green) and states, in its doc comment, what the library's own documentation says
//! should happen instead. If the library is fixed, the pinned assertion fails — that is
//! the signal to delete the corresponding entry in FRICTION.md.
//!
//! None of these patch or work around the library here; they only measure it.

use gibson::plot::layout::Prims;
use gibson::plot::{
    compile, render, Annotation, AxisScale, AxisSpec, FiniteRange, PlotLayout, PlotSpec, PlotView,
    Reduce, Series,
};
use gibson::ui::prelude::*;
use gibson::{Context, Rect, RenderMode, SubcellGlyphMode};
use std::time::Duration;

fn fr(a: f64, b: f64) -> FiniteRange {
    FiniteRange::new(a, b).unwrap()
}

fn lin_spec() -> PlotSpec {
    PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "x"),
        AxisSpec::new(AxisScale::Linear, "y"),
    )
}

fn segments(l: &PlotLayout, series: usize) -> Vec<((i32, i32), (i32, i32))> {
    match &l.series[series].prims {
        Prims::Segments(s) => s.clone(),
        Prims::Scatter(_) => panic!("scatter"),
    }
}

/// DEFECT 1 — `Reduce::ExtremaPerColumn` lets *out-of-view* samples steal the
/// per-column envelope at the view edges, so an in-view spike disappears.
///
/// Doc (PLOT_OBSERVABLE_GEOMETRY §6, law J): "a one-sample spike between sampled
/// columns cannot vanish"; §3: "pan/zoom changes THIS [view], never the data".
/// Mechanism: `reduce_extrema` clamps every sample's column index into
/// `[0, num_cols-1]` — samples left of the view all land in column 0 (and right of it
/// in the last column), where their extreme values occupy the four first/min/max/last
/// slots; the real in-view samples of that column are discarded *before* clipping.
#[test]
fn defect_extrema_reducer_lets_out_of_view_samples_evict_an_in_view_spike() {
    let mut pts: Vec<(f64, f64)> = (0..2000).map(|i| (i as f64, 0.0)).collect();
    for (i, p) in pts.iter_mut().enumerate().take(500) {
        p.1 = if i % 2 == 0 { 100.0 } else { -100.0 }; // huge, entirely left of the view
    }
    pts[505].1 = 1.0; // the in-view spike, 5 samples inside the left edge
    let view = PlotView::new(fr(500.0, 2000.0), fr(-0.2, 1.2));
    let area = Rect::new(0, 0, 60, 16);
    let spec = |r| lin_spec().series(Series::line(pts.clone()).reduce(r));
    let near_top = |l: &PlotLayout| {
        segments(l, 0)
            .iter()
            .filter(|((_, y0), (_, y1))| *y0 < 6 || *y1 < 6)
            .count()
    };
    let (plain, _) = compile(&spec(Reduce::None), &view, area).unwrap();
    let (reduced, report) = compile(&spec(Reduce::ExtremaPerColumn), &view, area).unwrap();
    assert_eq!(report.reducers_declined, 0, "x is monotone: the reducer is applied");
    assert!(near_top(&plain) > 0, "without reduction the spike is drawn");
    assert_eq!(near_top(&reduced), 0, "OBSERVED: with ExtremaPerColumn the spike is gone");
}

/// DEFECT 2 — `Reduce::ExtremaPerColumn` buckets samples *linearly in x* even when the
/// x axis is `Log10`, so it is not a per-*device*-column envelope there.
///
/// Doc: "Per device column it keeps a first / min / max / last envelope". On a log-x
/// axis the low-x device columns are far narrower than the linear buckets, so many
/// columns share one bucket's four points and the waveform between them is lost.
#[test]
fn defect_extrema_reducer_on_a_log_x_axis_does_not_preserve_per_column_envelopes() {
    let pts: Vec<(f64, f64)> = (0..20_000)
        .map(|i| {
            let x = 1.0 + i as f64 * 0.05;
            (x, (x * 6.0).sin())
        })
        .collect();
    let spec = |r| {
        PlotSpec::new(
            AxisSpec::new(AxisScale::Log10, "x"),
            AxisSpec::new(AxisScale::Linear, "y"),
        )
        .series(Series::line(pts.clone()).reduce(r))
    };
    let view = PlotView::new(fr(1.0, 1000.0), fr(-1.2, 1.2));
    let area = Rect::new(0, 0, 80, 20);
    let (plain, _) = compile(&spec(Reduce::None), &view, area).unwrap();
    let (reduced, rep) = compile(&spec(Reduce::ExtremaPerColumn), &view, area).unwrap();
    assert_eq!((rep.reducers_requested, rep.reducers_declined), (1, 0));
    let extent = |l: &PlotLayout| {
        let mut e = vec![(i32::MAX, i32::MIN); l.px_w as usize];
        for ((x0, y0), (x1, y1)) in segments(l, 0) {
            for (x, y) in [(x0, y0), (x1, y1)] {
                let c = &mut e[x as usize];
                c.0 = c.0.min(y);
                c.1 = c.1.max(y);
            }
        }
        e
    };
    let (a, b) = (extent(&plain), extent(&reduced));
    let differing = a
        .iter()
        .zip(&b)
        .filter(|(p, q)| p.0 != i32::MAX && p != q)
        .count();
    assert!(
        differing > 50,
        "OBSERVED: {differing} of {} device columns have a different vertical extent after reduction (expected 0)",
        a.len()
    );
}

/// DEFECT 3a — a y tick label that does not fit is *truncated from the right*, which
/// silently changes the number it shows: "-1000000" is drawn as "-100000".
///
/// Doc §7: "Responsive rendering may drop tick labels as space shrinks, but never moves
/// a semantic tick value" — it should drop the label, not print a different number.
#[test]
fn defect_truncated_y_tick_label_shows_a_different_number() {
    let view = PlotView::new(fr(0.0, 1.0), fr(-1.2e6, 1.2e6));
    let (l, _) = compile(&lin_spec(), &view, Rect::new(0, 0, 24, 12)).unwrap();
    assert!(l.y_ticks.iter().any(|t| t.value == -1_000_000.0 && t.label == "-1000000"));
    let lines = render(&l, SubcellGlyphMode::Braille2x4).to_visible_lines();
    let text = lines.join("\n");
    assert!(text.contains("-100000┤"), "OBSERVED: the tick for -1e6 is drawn as -100000:\n{text}");
    assert!(!text.contains("-1000000┤"));
}

/// DEFECT 3b — x tick labels that collide are printed against each other instead of
/// being dropped: the six ticks 0.0 … 1.0 on a 24-column plot render as
/// `0.00.2 0.4 0.6 0.81.0` — "0.0" and "0.2" (and "0.8", "1.0") run together into
/// unreadable numbers ("0.00.2" reads as 0.00 and .2). On a narrower plot the
/// labels overwrite each other outright.
#[test]
fn defect_colliding_x_tick_labels_run_together() {
    let view = PlotView::new(fr(0.0, 1.0), fr(0.0, 1.0));
    let (l, _) = compile(&lin_spec(), &view, Rect::new(0, 0, 24, 12)).unwrap();
    assert_eq!(l.x_ticks.len(), 6);
    let lines = render(&l, SubcellGlyphMode::Braille2x4).to_visible_lines();
    let row = lines.iter().find(|r| r.contains("0.4") && r.contains("1.0")).unwrap();
    assert!(row.contains("0.00.2"), "OBSERVED: labels run together: {row:?}");
    assert!(row.contains("0.81.0"), "OBSERVED: labels run together: {row:?}");
    // …and with a wide y gutter (long y labels) the loss is worse: the 0.4 label is gone
    let view = PlotView::new(fr(0.0, 1.0), fr(-1.2e6, 1.2e6));
    let (l, _) = compile(&lin_spec(), &view, Rect::new(0, 0, 24, 12)).unwrap();
    let lines = render(&l, SubcellGlyphMode::Braille2x4).to_visible_lines();
    let row = lines.iter().find(|r| r.contains("1.0")).unwrap();
    assert!(!row.contains("0.4"), "OBSERVED: 0.4 destroyed in {row:?}");
}

/// DEFECT 4 — a `Log10` axis whose view spans less than one decade (or contains no
/// power of ten) gets **no tick labels at all**; `log10_minor_ticks` exists and is
/// exported but `compile` never calls it.
///
/// Doc §7: "Log10 — majors at powers of ten; minors (2..9 × 10^k) only if space
/// supports them."
#[test]
fn defect_log_axis_without_a_power_of_ten_in_view_has_no_ticks() {
    let spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "x"),
        AxisSpec::new(AxisScale::Log10, "y"),
    );
    let (l, _) = compile(&spec, &PlotView::new(fr(0.0, 1.0), fr(2.0, 8.0)), Rect::new(0, 0, 40, 12)).unwrap();
    assert!(l.y_ticks.is_empty(), "OBSERVED: no y ticks on a log axis spanning 2..8");
    let (l, _) = compile(&spec, &PlotView::new(fr(0.0, 1.0), fr(2.0, 60.0)), Rect::new(0, 0, 40, 12)).unwrap();
    let labels: Vec<_> = l.y_ticks.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, ["10"], "OBSERVED: a single tick on a 2..60 log axis (no minors)");
    // the helper exists and would have produced them
    let minors = gibson::plot::log10_minor_ticks(fr(2.0, 60.0));
    assert!(!minors.is_empty());
}

/// OBSERVATION 5 — `Series::label` is stored but never drawn (there is no legend), so
/// an application must label its own lines. `PlotSpec::new(..).series(Series::line(..)
/// .label("x"))` is accepted without complaint.
#[test]
fn observation_series_label_is_not_rendered() {
    let spec = lin_spec().series(Series::line(vec![(0.0, 0.0), (1.0, 1.0)]).label("SECRET_SERIES_LABEL"));
    let view = PlotView::new(fr(0.0, 1.0), fr(0.0, 1.0));
    let (l, _) = compile(&spec, &view, Rect::new(0, 0, 60, 20)).unwrap();
    let text = render(&l, SubcellGlyphMode::Braille2x4).to_visible_lines().join("\n");
    assert!(!text.contains("SECRET_SERIES_LABEL"));
}

/// OBSERVATION 6 — an `Annotation` outside the view is dropped without any receipt
/// counter (receipts count samples, not annotations). Plotting "never silently eats
/// data" holds for samples only.
#[test]
fn observation_out_of_view_annotations_vanish_without_a_receipt() {
    let spec = lin_spec()
        .series(Series::line(vec![(0.0, 0.0), (1.0, 1.0)]))
        .annotate(Annotation::Point { x: 50.0, y: 50.0, label: "far".into(), color: (1, 2, 3) })
        .annotate(Annotation::VLine { x: -7.0, color: (1, 2, 3) });
    let view = PlotView::new(fr(0.0, 1.0), fr(0.0, 1.0));
    let (l, rep) = compile(&spec, &view, Rect::new(0, 0, 40, 12)).unwrap();
    assert!(l.annotations.is_empty());
    assert_eq!(rep.points_clipped, 0);
    assert_eq!(rep.segments_clipped, 0);
}

/// DEFECT 7 (gibson::ui) — a `modal` shows at most **nine** rows of content whatever
/// the screen height or the number of children; excess children are silently clipped.
/// Giving the modal an explicit `.height(..)` is the only way to see them.
///
/// Doc (UI_LAYER): "modals center through ordinary Taffy constraints"; nothing says its
/// content is capped, and there is no scroll.
#[test]
fn defect_modal_content_is_silently_capped_at_nine_rows() {
    fn visible(screen_h: u16, children: usize, explicit: Option<u16>) -> usize {
        let env = UiEnvironment {
            width: 60,
            height: screen_h,
            motion: MotionPreference::None,
            ..UiEnvironment::default()
        };
        let mut rt: UiRuntime<()> = UiRuntime::new(skins::BLACK_ICE);
        let mut ctx = Context::headless(RenderMode::Fullscreen, 60, screen_h);
        let mut body = column();
        for i in 0..children {
            body = body.child(text(format!("line {i:02}")));
        }
        let mut m = modal("TITLE").key("m").child(body);
        if let Some(h) = explicit {
            m = m.height(h);
        }
        let tree: Element<()> = screen().height(screen_h).child(text("base")).overlay(m);
        let f = rt.frame(&tree, env, Duration::ZERO).unwrap();
        ctx.set_root(f.node);
        ctx.render_now().unwrap();
        ctx.last_frame_lines().iter().filter(|l| l.contains("line ")).count()
    }
    assert_eq!(visible(24, 12, None), 9, "OBSERVED: 12 children, 9 visible");
    assert_eq!(visible(40, 30, None), 9, "OBSERVED: 30 children on a 40-row screen, still 9");
    assert_eq!(visible(24, 12, Some(16)), 12, "an explicit height shows them all");
}
