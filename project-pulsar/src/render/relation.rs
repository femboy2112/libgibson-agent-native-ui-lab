//! RELATION — relationships between measured quantities.
//!
//! Two plots of two different pairs of observables, both read straight from the
//! analysis: the **phasor walk** (cumulative demodulated Re vs Im — a coherent
//! signal walks in a straight line, noise diffuses, a wrong model curls) and the
//! **growth law** (S/N vs integration time on log–log axes — a real source climbs
//! along slope ½, a transient bends over).

use super::draw::*;
use super::{id_color, lane, Probe, ViewIn, ViewOut};
use crate::analysis::LineClass;
use crate::scenario::{SigId, T_END};
use crate::session::Focus;
use gibson::plot::{self, Annotation, AxisScale, AxisSpec, PlotSpec, PlotView, Series};
use gibson::Rect;

pub fn draw(vin: &ViewIn) -> ViewOut {
    let mut out = ViewOut::new(vin.rect);
    let r = vin.rect;
    if r.width < 8 || r.height < 4 {
        return out;
    }
    let side = r.width >= 90;
    let (ra, rb) = if side {
        let wa = r.width * 9 / 20;
        (
            Rect::new(0, 0, wa, r.height),
            Rect::new(wa, 0, r.width - wa, r.height),
        )
    } else {
        let ha = r.height / 2;
        (
            Rect::new(0, 0, r.width, ha),
            Rect::new(0, ha, r.width, r.height - ha),
        )
    };
    walk_plot(vin, &mut out, ra);
    growth_plot(vin, &mut out, rb);
    out
}

fn shown_ids(vin: &ViewIn) -> Vec<SigId> {
    if vin.st.focus == Focus::Overview {
        SigId::ALL.to_vec()
    } else {
        vec![vin.st.selected]
    }
}

fn line_style(c: LineClass) -> (Rgb, &'static str) {
    match c {
        LineClass::Terrestrial => (WARN, "✕"),
        LineClass::Transient => ((200, 170, 120), "~"),
        LineClass::Unassigned => (MUTED, "?"),
    }
}

fn walk_plot(vin: &ViewIn, out: &mut ViewOut, rect: Rect) {
    let st = vin.st;
    let cp = vin.cp;
    let area = lane(
        &mut out.surf,
        rect,
        &[
            ("PHASOR WALK".to_string(), style_b(INK, true, false)),
            (
                "  Σ demodulated chunks: straight = coherent, curl = wrong model".to_string(),
                style(MUTED),
            ),
        ],
    );
    let mut spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "Re Σ").unit("σ"),
        AxisSpec::new(AxisScale::Linear, "Im Σ").unit("σ"),
    );
    let mut rmax: f64 = 6.0;
    let mut ends: Vec<((f64, f64), String, Rgb)> = Vec::new();
    for id in shown_ids(vin) {
        let p = cp.signal(id);
        if p.walk.len() < 2 {
            continue;
        }
        let col = id_color(id, id == st.selected, st.focus);
        for &(x, y) in &p.walk {
            rmax = rmax.max(x.abs()).max(y.abs());
        }
        spec = spec.series(Series::line(p.walk.clone()).color(col).label(id.name()));
        ends.push((
            *p.walk.last().unwrap(),
            format!("{}{}", id.glyph(), id.letter()),
            col,
        ));
    }
    if st.focus == Focus::Overview {
        for l in &cp.lines {
            let (col, g) = line_style(l.class);
            if l.walk.len() < 2 {
                continue;
            }
            // Lines may be far brighter than any source (a terrestrial transmitter is);
            // they are allowed to run off the plot — the library counts the clipping —
            // rather than flatten every identity into a dot at the origin.
            if l.class != LineClass::Terrestrial {
                for &(x, y) in &l.walk {
                    rmax = rmax.max(x.abs()).max(y.abs());
                }
            }
            spec = spec.series(Series::line(l.walk.clone()).color(scale(col, 0.8)).label(&l.label));
            ends.push((*l.walk.last().unwrap(), format!("{g}{:.1}", l.f), col));
        }
    }
    for (xy, label, col) in ends {
        spec = spec.annotate(Annotation::Point {
            x: xy.0,
            y: xy.1,
            label,
            color: col,
        });
    }
    spec = spec
        .annotate(Annotation::HLine { y: 0.0, color: FAINT })
        .annotate(Annotation::VLine { x: 0.0, color: FAINT });
    let rr = rmax * 1.12;
    // equal data-scale on both axes: ask the compiler how many pixels the plot got
    let probe_view = PlotView::new(range(-rr, rr), range(-rr, rr));
    let aspect = plot::compile(&spec, &probe_view, Rect::new(0, 0, area.width, area.height))
        .map(|(l, _)| l.px_w as f64 / (l.px_h.max(1)) as f64)
        .unwrap_or(2.0)
        .clamp(0.5, 6.0);
    let view = PlotView::new(range(-rr * aspect, rr * aspect), range(-rr, rr));
    if let Some(pr) = draw_plot(&mut out.surf, area, "relation.walk", spec, view, vin.mode) {
        out.probes.push(Probe::Plot(pr));
    }
}

fn growth_plot(vin: &ViewIn, out: &mut ViewOut, rect: Rect) {
    let st = vin.st;
    let cp = vin.cp;
    let area = lane(
        &mut out.surf,
        rect,
        &[
            ("GROWTH LAW".to_string(), style_b(INK, true, false)),
            (
                "  S/N vs integration time: a source climbs slope ½, a transient bends".to_string(),
                style(MUTED),
            ),
        ],
    );
    let mut spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Log10, "integration time").unit("s"),
        AxisSpec::new(AxisScale::Log10, "coherent S/N"),
    );
    let mut ymax: f64 = 8.0;
    let mut ends: Vec<((f64, f64), String, Rgb)> = Vec::new();
    let mut guide_from: Option<(f64, f64)> = None;
    for id in shown_ids(vin) {
        let p = cp.signal(id);
        if p.growth.is_empty() {
            continue;
        }
        let col = id_color(id, id == st.selected, st.focus);
        for &(_, y) in &p.growth {
            ymax = ymax.max(y);
        }
        spec = spec.series(Series::line(p.growth.clone()).color(col).label(id.name()));
        let last = *p.growth.last().unwrap();
        ends.push((last, format!("{}{}", id.glyph(), id.letter()), col));
        if id == st.selected && last.1 > 0.0 {
            guide_from = Some(last);
        }
    }
    if st.focus == Focus::Overview {
        for l in &cp.lines {
            let (col, g) = line_style(l.class);
            if l.growth.is_empty() {
                continue;
            }
            for &(_, y) in &l.growth {
                ymax = ymax.max(y);
            }
            spec = spec.series(Series::line(l.growth.clone()).color(scale(col, 0.8)).label(&l.label));
            let last = *l.growth.last().unwrap();
            ends.push((last, g.to_string(), col));
        }
    }
    // the slope-½ reference through the selected identity's current point
    if let Some((t, s)) = guide_from {
        let c = s / t.sqrt();
        let g: Vec<(f64, f64)> = [8.0, T_END].iter().map(|&x| (x, c * x.sqrt())).collect();
        for &(_, y) in &g {
            ymax = ymax.max(y.min(ymax * 4.0));
        }
        spec = spec.series(Series::line(g).color(FAINT).label("slope ½"));
        spec = spec.annotate(Annotation::Point {
            x: 8.0 * 1.3,
            y: c * (8.0f64 * 1.3).sqrt(),
            label: "slope ½".into(),
            color: FAINT,
        });
    }
    for (xy, label, col) in ends {
        if xy.1 > 0.0 {
            spec = spec.annotate(Annotation::Point {
                x: xy.0,
                y: xy.1,
                label,
                color: col,
            });
        }
    }
    let ytop = 10f64.powf((ymax * 1.5).log10().ceil()).max(10.0);
    let view = PlotView::new(log_range(8.0, T_END), log_range(0.3, ytop));
    if let Some(pr) = draw_plot(&mut out.surf, area, "relation.growth", spec, view, vin.mode) {
        out.probes.push(Probe::Plot(pr));
    }
}
