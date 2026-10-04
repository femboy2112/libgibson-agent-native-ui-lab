//! STREAM — raw voltage versus time, and the demodulated I/Q "lock monitor".
//!
//! Visual law: the signal is *buried*. The top lane is honest receiver voltage; any
//! structure the analysis claims to have found is overlaid in the identity's hue
//! (predicted pulse ticks along the top edge, the fitted waveform in the trace) so
//! you can see how small it is next to the noise. Below, the same identity as a
//! demodulated phasor: noise before lock, a steady in-phase level after.

use super::draw::*;
use super::{id_color, ViewIn, ViewOut};
use crate::analysis::{SignalProduct, Stage};
use crate::scenario::{SigId, CHUNK, FS, T_END};
use crate::session::Focus;
use gibson::plot::{Annotation, AxisScale, AxisSpec, PlotSpec, PlotView, Reduce, Series};
use gibson::Rect;
use std::f64::consts::TAU;

/// Seconds of raw voltage shown.
fn window_s(vin: &ViewIn) -> f64 {
    match (vin.st.focus, vin.st.zoom) {
        (Focus::Overview, _) => 24.0,
        (Focus::Signal, 0) => 12.0,
        (Focus::Signal, 1) => 4.0,
        _ => 1.2,
    }
}

/// Per-chunk demodulated `(t, I, Q)` of an identity, rotated so the mean phasor is
/// in-phase. Derived purely from the phasor walk the analysis publishes.
pub fn iq_series(p: &SignalProduct) -> Vec<(f64, f64, f64)> {
    if p.walk.len() < 2 {
        return Vec::new();
    }
    let last = p.walk[p.walk.len() - 1];
    let rot = last.1.atan2(last.0);
    let (c, s) = ((-rot).cos(), (-rot).sin());
    p.walk
        .windows(2)
        .enumerate()
        .map(|(j, w)| {
            let dx = w[1].0 - w[0].0;
            let dy = w[1].1 - w[0].1;
            (
                ((j + 1) * CHUNK) as f64 / FS,
                dx * c - dy * s,
                dx * s + dy * c,
            )
        })
        .collect()
}

/// The fitted waveform of an identity at station 0, evaluated at time `t`.
pub fn template_at(p: &SignalProduct, t: f64) -> f64 {
    let Some(m) = p.model else { return 0.0 };
    let phi = m.phase(t);
    p.harm
        .iter()
        .enumerate()
        .filter(|(h, _)| *h < p.nh)
        .map(|(h, &(a, th))| a * (TAU * (h + 1) as f64 * phi + th).cos())
        .sum()
}

/// Pulse arrival times (s) predicted by the fitted model inside `[t0, t1]`.
pub fn predicted_pulses(p: &SignalProduct, t0: f64, t1: f64) -> Vec<f64> {
    let (Some(m), Some(&(_, th))) = (p.model, p.harm.first()) else {
        return Vec::new();
    };
    if p.id == SigId::Beta || m.f0 <= 0.0 {
        return Vec::new();
    }
    // 2π φ(t) + θ = 2π m  ⇒  φ(t) = m − θ/2π
    let k0 = (m.phase(t0) + th / TAU).floor() as i64;
    let k1 = (m.phase(t1) + th / TAU).ceil() as i64;
    (k0..=k1)
        .map(|k| (k as f64 - th / TAU) / m.f0)
        .filter(|t| *t >= t0 && *t <= t1)
        .collect()
}

pub fn draw(vin: &ViewIn) -> ViewOut {
    let mut out = ViewOut::new(vin.rect);
    let r = vin.rect;
    if r.width < 6 || r.height < 3 {
        return out;
    }
    let n = vin.n_now;
    let t_now = n as f64 / FS;
    let overview = vin.st.focus == Focus::Overview;

    // lane split
    let tiny = r.height < 14;
    let (raw_h, n_iq) = if tiny {
        ((r.height * 3 / 5).max(3), 1)
    } else if overview {
        ((r.height * 2 / 5).max(5), 3)
    } else {
        ((r.height * 9 / 20).max(5), 1)
    };
    let raw_rect = Rect::new(r.x, r.y, r.width, raw_h);
    drawn_raw(vin, &mut out, raw_rect, t_now, n);

    let rest = r.height - raw_h;
    if rest >= 3 {
        let ids: Vec<SigId> = if n_iq == 3 {
            SigId::ALL.to_vec()
        } else {
            vec![vin.st.selected]
        };
        let lane_h = rest / ids.len() as u16;
        for (i, id) in ids.iter().enumerate() {
            let rect = Rect::new(
                r.x,
                r.y + raw_h + lane_h * i as u16,
                r.width,
                if i + 1 == ids.len() {
                    rest - lane_h * i as u16
                } else {
                    lane_h
                },
            );
            drawn_iq(vin, &mut out, rect, *id, t_now);
        }
    }
    out
}

fn drawn_raw(vin: &ViewIn, out: &mut ViewOut, rect: Rect, t_now: f64, n: usize) {
    let w = window_s(vin);
    let x0 = (t_now - w).max(0.0);
    let x1 = x0 + w;
    let st = vin.st;
    let x = vin.eng.rx.station(0);
    let lo = ((x0 * FS).floor() as usize).min(n);
    let pts: Vec<(f64, f64)> = (lo..n).map(|i| (i as f64 / FS, x[i] as f64)).collect();
    let rect = super::lane(
        &mut out.surf,
        rect,
        &[
            (
                format!(
                    "RECEIVER VOLTAGE · station 0 · last {w:.0} s{}",
                    if n == 0 { " · (no data yet)" } else { "" }
                ),
                style_b(INK, true, false),
            ),
            ("   buried signal = coloured ticks / trace".to_string(), style(MUTED)),
        ],
    );
    let mut spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "time").unit("s"),
        AxisSpec::new(AxisScale::Linear, "voltage"),
    );
    spec = spec.series(
        Series::line(pts)
            .color(NOISE)
            .reduce(Reduce::ExtremaPerColumn)
            .label("station 0"),
    );
    // identity overlays
    let ids: Vec<SigId> = if st.focus == Focus::Overview {
        SigId::ALL.to_vec()
    } else {
        vec![st.selected]
    };
    for id in ids {
        let p = vin.cp.signal(id);
        if p.stage < Stage::Candidate {
            continue;
        }
        let sel = id == st.selected;
        let col = id_color(id, sel, st.focus);
        // fitted waveform (only for the selected identity in overview: keep the trace legible)
        if sel {
            let a = lo as f64 / FS;
            let m = ((x1 - a) * FS).ceil() as usize + 1;
            let tpl: Vec<(f64, f64)> = (0..m.min(4000))
                .map(|i| {
                    let t = a + i as f64 / FS;
                    (t, template_at(p, t))
                })
                .filter(|(t, _)| *t <= t_now)
                .collect();
            spec = spec.series(Series::line(tpl).color(col).label("fitted waveform"));
        }
        let ticks = predicted_pulses(p, x0, x1);
        let ylab = 4.6 - 0.5 * id.idx() as f64;
        for (i, t) in ticks.iter().enumerate() {
            spec = spec.annotate(Annotation::Point {
                x: *t,
                y: ylab,
                label: if i == 0 { id.glyph().to_string() } else { String::new() },
                color: col,
            });
        }
    }
    let view = PlotView::new(range(x0, x1.max(x0 + 1e-6)), range(-5.0, 5.0));
    if let Some(p) = draw_plot(&mut out.surf, rect, "stream.raw", spec, view, vin.mode) {
        out.probes.push(super::Probe::Plot(p));
    }
}

fn drawn_iq(vin: &ViewIn, out: &mut ViewOut, rect: Rect, id: SigId, t_now: f64) {
    let p = vin.cp.signal(id);
    let st = vin.st;
    let sel = id == st.selected;
    let col = id_color(id, sel, st.focus);
    let iq = iq_series(p);
    let ipts: Vec<(f64, f64)> = iq.iter().map(|&(t, i, _)| (t, i)).collect();
    let qpts: Vec<(f64, f64)> = iq.iter().map(|&(t, _, q)| (t, q)).collect();
    let ymax = iq
        .iter()
        .map(|&(_, i, q)| i.abs().max(q.abs()))
        .fold(3.0f64, f64::max)
        * 1.15;
    let rect = super::lane(
        &mut out.surf,
        rect,
        &[
            (
                format!("{} {}", id.glyph(), id.name()),
                style_b(col, true, !sel && st.focus == Focus::Signal),
            ),
            (
                format!("  demodulated I/Q per 8 s chunk · {}", p.stage.label()),
                style(MUTED),
            ),
        ],
    );
    let mut spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "time").unit("s"),
        AxisSpec::new(AxisScale::Linear, "demod").unit("σ"),
    )
    .series(Series::line(qpts).color(NOISE).label("Q"))
    .series(Series::line(ipts).color(col).label("I"))
    .annotate(Annotation::HLine { y: 0.0, color: FAINT })
    .annotate(Annotation::VLine {
        x: t_now.clamp(0.0, T_END),
        color: INK,
    });
    if let Some(k) = p.lock_cp {
        let t = (k * CHUNK) as f64 / FS;
        spec = spec.annotate(Annotation::Point {
            x: t,
            y: ymax * 0.85,
            label: format!("{} lock", id.letter()),
            color: col,
        });
    }
    let view = PlotView::new(range(0.0, T_END), range(-ymax, ymax));
    let name = format!("stream.iq.{}", id.letter());
    if let Some(pr) = draw_plot(&mut out.surf, rect, &name, spec, view, vin.mode) {
        out.probes.push(super::Probe::Plot(pr));
    }
}
