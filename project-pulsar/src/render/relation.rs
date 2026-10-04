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

/// Coherence of a phasor walk: `|ΣZ| / Σ|Z|` — 1 for a perfectly straight ray, about
/// `0.89/√N` for the random walk of pure noise.
pub fn coherence(walk: &[(f64, f64)]) -> Option<(f64, f64)> {
    if walk.len() < 3 {
        return None;
    }
    let steps = walk.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1));
    let total: f64 = steps.sum();
    let last = walk.last()?;
    if total <= 0.0 {
        return None;
    }
    let n = (walk.len() - 1) as f64;
    Some((last.0.hypot(last.1) / total, 0.886 / n.sqrt()))
}

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
    // In the side-by-side layout the walk is as tall as it is *wide* in pixels (equal
    // scales need a roughly square plot); the rows beneath carry the coherence table.
    let walk_h = if side {
        (ra.width / 2 + 4).min(ra.height).max(8.min(ra.height))
    } else {
        ra.height
    };
    walk_plot(vin, &mut out, Rect::new(ra.x, ra.y, ra.width, walk_h));
    if side && ra.height >= walk_h + 6 {
        coherence_table(vin, &mut out, Rect::new(ra.x, ra.y + walk_h, ra.width, ra.height - walk_h));
    }
    growth_plot(vin, &mut out, rb);
    out
}

fn coherence_table(vin: &ViewIn, out: &mut ViewOut, rect: Rect) {
    let cp = vin.cp;
    let st = vin.st;
    let mut y = rect.y as i32 + 1;
    put(
        &mut out.surf,
        rect.x as i32 + 1,
        y,
        "COHERENCE  |ΣZ| / Σ|Z|",
        style_b(INK, true, false),
    );
    y += 1;
    let ids = shown_ids(vin);
    for id in ids {
        if y >= (rect.y + rect.height) as i32 {
            break;
        }
        let col = id_color(id, id == st.selected, st.focus);
        let p = cp.signal(id);
        let txt = match coherence(&p.walk) {
            Some((c, base)) => format!(
                "{} {:<5} {} {:.2}  noise {:.2} ({:.0}×)",
                id.glyph(),
                id.name(),
                meter(c, 10, vin.mode),
                c,
                base,
                c / base.max(1e-9)
            ),
            None => format!("{} {:<5} no walk yet", id.glyph(), id.name()),
        };
        put_max(&mut out.surf, rect.x as i32 + 1, y, &txt, style_b(col, id == st.selected, false), rect.width.saturating_sub(2));
        y += 1;
    }
    for l in cp.interference().into_iter().take(2) {
        if y >= (rect.y + rect.height) as i32 {
            break;
        }
        if let Some((c, base)) = coherence(&l.walk) {
            let (col, g) = line_style(l.class);
            put_max(
                &mut out.surf,
                rect.x as i32 + 1,
                y,
                &format!("{g} {:>5.2}Hz {} {:.2}  noise {:.2}", l.f, meter(c, 10, vin.mode), c, base),
                style(col),
                rect.width.saturating_sub(2),
            );
            y += 1;
        }
    }
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
    // Scale the axes to the *second-largest* identity so one bright source cannot flatten
    // the others into the origin: the brightest runs off the plot (the library clips it
    // and counts the clip); its label sits where it leaves.
    let ids = shown_ids(vin);
    let mut finals: Vec<f64> = ids
        .iter()
        .filter_map(|id| cp.signal(*id).walk.last().map(|p| p.0.hypot(p.1)))
        .collect();
    finals.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let rmax: f64 = match (ids.len(), finals.as_slice()) {
        (n, [_, second, ..]) if n > 1 => (1.25 * second).max(6.0),
        (_, [only]) => (1.15 * only).max(6.0),
        _ => 6.0,
    };
    let aspect = {
        let probe_spec = PlotSpec::new(
            AxisSpec::new(AxisScale::Linear, "Re Σ").unit("σ"),
            AxisSpec::new(AxisScale::Linear, "Im Σ").unit("σ"),
        );
        let pv = PlotView::new(range(-rmax, rmax), range(-rmax, rmax));
        plot::compile(&probe_spec, &pv, Rect::new(0, 0, area.width, area.height))
            .map(|(l, _)| l.px_w as f64 / (l.px_h.max(1)) as f64)
            .unwrap_or(2.0)
            .clamp(0.5, 6.0)
    };
    let (bx, by) = (rmax * aspect, rmax);
    let inside = |p: &(f64, f64)| p.0.abs() <= bx && p.1.abs() <= by;
    let mut ends: Vec<((f64, f64), String, Rgb)> = Vec::new();
    for id in ids {
        let p = cp.signal(id);
        if p.walk.len() < 2 {
            continue;
        }
        let col = id_color(id, id == st.selected, st.focus);
        spec = spec.series(Series::line(p.walk.clone()).color(col).label(id.name()));
        // label at the last point still on the plot
        let at = p.walk.iter().rev().find(|q| inside(q)).copied().unwrap_or((0.0, 0.0));
        ends.push((at, format!("{}{}", id.glyph(), id.letter()), col));
    }
    if st.focus == Focus::Overview {
        for l in &cp.lines {
            let (col, g) = line_style(l.class);
            if l.walk.len() < 2 {
                continue;
            }
            // Lines may be far brighter than any source; they run off the plot too.
            spec = spec.series(Series::line(l.walk.clone()).color(scale(col, 0.8)).label(&l.label));
            let at = l.walk.iter().rev().find(|q| inside(q)).copied().unwrap_or((0.0, 0.0));
            ends.push((at, format!("{g}{:.1}", l.f), col));
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
    let view = PlotView::new(range(-bx, bx), range(-by, by));
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
