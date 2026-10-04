//! FOLD — the same data folded at the fitted period.
//!
//! Visual law: *coherence is sharpness*. A noise profile is a flat fuzz with wide
//! error bars; a locked pulse is a crisp peak with the bars closed around it. The
//! rival fold (twice the period) is drawn in grey — if it shows two matching pulses
//! the period is confirmed. The waterfall (custom raster: `gibson::plot` has no
//! heat-map) stacks sub-integrations: a locked signal is a straight stripe, a wrong
//! model is a slanted one.

use super::draw::*;
use super::{id_color, lane, Probe, RasterProbe, ViewIn, ViewOut};
use crate::analysis::{SignalProduct, Stage, NB_FOLD, NB_WF};
use crate::scenario::{SigId, T_END};
use crate::session::Focus;
use gibson::plot::{Annotation, AxisScale, AxisSpec, PlotSpec, PlotView, Series};
use gibson::Rect;

/// Peak-over-median signal-to-noise of a folded profile.
pub fn profile_snr(p: &SignalProduct) -> f64 {
    if p.profile.is_empty() || p.profile_err <= 0.0 {
        return 0.0;
    }
    let mut v: Vec<f64> = p.profile.iter().map(|&x| x as f64).collect();
    let max = v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let med = crate::dsp::median(&mut v);
    ((max - med) / p.profile_err as f64).max(0.0)
}

/// Pearson correlation between the two halves of the 2P (rival) fold: two matching
/// pulses ⇒ the adopted period is right.
pub fn rival_halves_match(p: &SignalProduct) -> Option<f64> {
    let r = p.rival.as_ref()?;
    if r.profile.len() < 4 {
        return None;
    }
    let h = r.profile.len() / 2;
    let (a, b) = (&r.profile[..h], &r.profile[h..2 * h]);
    let ma = a.iter().map(|&x| x as f64).sum::<f64>() / h as f64;
    let mb = b.iter().map(|&x| x as f64).sum::<f64>() / h as f64;
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for i in 0..h {
        let (x, y) = (a[i] as f64 - ma, b[i] as f64 - mb);
        sab += x * y;
        saa += x * x;
        sbb += y * y;
    }
    if saa <= 0.0 || sbb <= 0.0 {
        return None;
    }
    Some(sab / (saa * sbb).sqrt())
}

pub fn period_ms(p: &SignalProduct) -> f64 {
    match p.model {
        Some(m) if p.f_now.is_finite() && p.f_now > 0.0 => {
            let _ = m;
            1000.0 / p.f_now
        }
        _ => f64::NAN,
    }
}

pub fn draw(vin: &ViewIn) -> ViewOut {
    let mut out = ViewOut::new(vin.rect);
    let r = vin.rect;
    if r.width < 8 || r.height < 3 {
        return out;
    }
    let st = vin.st;
    if st.focus == Focus::Overview {
        let lane_h = r.height / 3;
        for (i, id) in SigId::ALL.iter().enumerate() {
            let rect = Rect::new(
                0,
                lane_h * i as u16,
                r.width,
                if i == 2 { r.height - lane_h * 2 } else { lane_h },
            );
            profile_lane(vin, &mut out, rect, *id, 1.0, false);
        }
    } else {
        let big = r.height >= 15;
        let top_h = if big { r.height * 11 / 20 } else { r.height };
        profile_lane(vin, &mut out, Rect::new(0, 0, r.width, top_h), st.selected, 2.0, true);
        if big {
            let rect = Rect::new(0, top_h, r.width, r.height - top_h);
            waterfall(vin, &mut out, rect, st.selected);
        }
    }
    out
}

fn profile_lane(vin: &ViewIn, out: &mut ViewOut, rect: Rect, id: SigId, cycles: f64, detail: bool) {
    let st = vin.st;
    let p = vin.cp.signal(id);
    let sel = id == st.selected;
    let col = id_color(id, sel, st.focus);
    let pms = period_ms(p);
    let snr = profile_snr(p);
    let mut head = vec![
        (
            format!("{} {}", id.glyph(), id.name()),
            style_b(col, true, !sel && st.focus == Focus::Signal),
        ),
        (
            if pms.is_finite() {
                format!("  fold @ P = {pms:.2} ms")
            } else {
                "  no model yet".to_string()
            },
            style(INK),
        ),
        (format!(" · profile S/N {snr:.1} · {}", p.stage.label()), style(MUTED)),
    ];
    if detail {
        if let Some(rm) = rival_halves_match(p) {
            let verdict = if rm > 0.6 && p.stage >= Stage::Candidate {
                "2P halves match: period confirmed"
            } else {
                "2P halves differ: period ambiguous"
            };
            head.push((format!(" · r={rm:.2} {verdict}"), style(if rm > 0.6 { GOOD } else { WARN })));
        }
    }
    let area = lane(&mut out.surf, rect, &head);
    if p.profile.is_empty() {
        put(
            &mut out.surf,
            area.x as i32 + 2,
            area.y as i32 + area.height as i32 / 2,
            "no data folded yet",
            style(MUTED),
        );
        return;
    }
    let nb = p.profile.len();
    let reps = cycles.round() as usize;
    let mut mean_pts = Vec::new();
    let mut hi = Vec::new();
    let mut lo = Vec::new();
    let e = p.profile_err as f64;
    for c in 0..reps {
        for (b, &v) in p.profile.iter().enumerate() {
            let x = c as f64 + (b as f64 + 0.5) / nb as f64;
            mean_pts.push((x, v as f64));
            hi.push((x, v as f64 + e));
            lo.push((x, v as f64 - e));
        }
    }
    let mut ymin = f64::INFINITY;
    let mut ymax = f64::NEG_INFINITY;
    for &(_, y) in hi.iter().chain(lo.iter()) {
        ymin = ymin.min(y);
        ymax = ymax.max(y);
    }
    let mut spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "phase").unit("cycles"),
        AxisSpec::new(AxisScale::Linear, "amplitude"),
    );
    if let (true, Some(rv)) = (detail || st.focus == Focus::Overview, p.rival.as_ref()) {
        let pts: Vec<(f64, f64)> = rv
            .profile
            .iter()
            .enumerate()
            .filter(|(b, _)| (*b as f64 + 0.5) / nb as f64 <= cycles)
            .map(|(b, &v)| ((b as f64 + 0.5) / nb as f64, v as f64))
            .collect();
        for &(_, y) in &pts {
            ymin = ymin.min(y);
            ymax = ymax.max(y);
        }
        if detail || reps >= 2 {
            spec = spec.series(Series::line(pts).color(FAINT).label("2P fold"));
        }
    }
    spec = spec
        .series(Series::line(hi).color(scale(col, 0.45)).label("+1σ"))
        .series(Series::line(lo).color(scale(col, 0.45)).label("-1σ"))
        .series(Series::line(mean_pts).color(col).label("profile"));
    spec = spec.annotate(Annotation::HLine { y: 0.0, color: FAINT });
    let span = (ymax - ymin).max(4.0 * e).max(1e-6);
    let view = PlotView::new(
        range(0.0, cycles),
        range(ymin - 0.08 * span, ymax + 0.08 * span),
    );
    let name = format!("fold.{}", id.letter());
    if let Some(pr) = draw_plot(&mut out.surf, area, &name, spec, view, vin.mode) {
        out.probes.push(Probe::Plot(pr));
    }
}

fn waterfall(vin: &ViewIn, out: &mut ViewOut, rect: Rect, id: SigId) {
    let p = vin.cp.signal(id);
    let col = id.color();
    let area = lane(
        &mut out.surf,
        rect,
        &[
            ("WATERFALL".to_string(), style_b(INK, true, false)),
            (
                format!(
                    "  {} {} · 16 s sub-integrations, time flows down · stripe straight = locked",
                    id.glyph(),
                    id.name()
                ),
                style(MUTED),
            ),
        ],
    );
    if area.height < 2 || area.width < 8 || p.waterfall.is_empty() {
        return;
    }
    // align columns with the profile plot above: same left margin is not available
    // (plots size their own gutter), so use the full width with a small left gutter
    let gx = 6u16.min(area.width / 4);
    let cw = area.width - gx - 1;
    let disp = area.height as usize;
    let rows = p.waterfall.len();
    let row_s = 16.0; // 2 chunks
    let mut z = vec![vec![0.0f32; cw as usize]; disp];
    for (j, row) in p.waterfall.iter().enumerate() {
        let t = (j as f64 + 0.5) * row_s;
        let d = (((t / T_END) * disp as f64).floor() as usize).min(disp - 1);
        for c in 0..cw as usize {
            // two cycles across the width
            let ph = (c as f64 + 0.5) / cw as f64 * 2.0;
            let b = ((ph.fract()) * NB_WF as f64) as usize % NB_WF;
            z[d][c] = z[d][c].max(row[b]);
        }
    }
    let _ = rows;
    for (d, zr) in z.iter().enumerate() {
        for (c, &v) in zr.iter().enumerate() {
            let level = ((v as f64) / 4.0).clamp(0.0, 1.0);
            if level < 0.3 {
                continue;
            }
            let ch = ramp_char((level - 0.2) / 0.8, vin.mode);
            put(
                &mut out.surf,
                (area.x + gx + c as u16) as i32,
                (area.y + d as u16) as i32,
                &ch.to_string(),
                style(mix(scale(col, 0.4), col, level)),
            );
        }
    }
    // axis hints
    put(&mut out.surf, area.x as i32, area.y as i32, "0s", style(FAINT));
    put(&mut out.surf, area.x as i32, (area.y + area.height - 1) as i32, "480", style(FAINT));
    out.probes.push(Probe::Raster(RasterProbe {
        name: "waterfall".into(),
        origin: (area.x + gx, area.y),
        size: (cw, disp as u16),
        x_range: (0.0, 2.0),
        y_range: (0.0, T_END),
        x_label: "phase [cycles]",
        y_label: "time [s]",
        z_label: "excess [σ]",
        z,
    }));
    let _ = NB_FOLD;
}
