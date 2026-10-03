//! SPECTRUM: normalised power versus frequency on a log axis. Harmonic combs and
//! identity tags are projected onto the same axes the data uses.

use gibson::plot::{Annotation, AxisScale, AxisSpec, PlotSpec, PlotView, Reduce, Series};
use gibson::{Rect, Surface};

use super::common::*;
use super::{active_slots, effective_selection, Anchor, Ctx, ViewOut};
use crate::identity;
use crate::theme::*;
use crate::track::State;

pub fn x_range(ctx: &Ctx, sel_f: Option<f64>) -> (f64, f64) {
    match (ctx.zoom, sel_f) {
        (1, Some(f)) => ((f - 0.6).max(0.05), f + 0.6),
        (z, Some(f)) if z >= 2 => ((f - 0.12).max(0.02), f + 0.12),
        _ => (0.3, 5.5),
    }
}

pub fn render(ctx: &Ctx, rect: Rect) -> ViewOut {
    let (w, h) = (rect.width, rect.height);
    let mut surf = Surface::new(w, h);
    let rws = rows(h);
    let ep = ctx.ep;
    let tl = &ctx.obs.timeline;
    let sel = effective_selection(ctx);
    let epoch = tl.epoch(ep);
    let sel_f = sel
        .and_then(|s| epoch.map(|e| e.slots[s].f))
        .filter(|f| *f > 0.0);
    let (x_lo, x_hi) = x_range(ctx, sel_f);

    let spec_now = ctx.obs.spectrum(ep.max(1));
    let df = spec_now.df;
    let k0 = ((x_lo / df).floor() as usize).min(spec_now.power.len().saturating_sub(1));
    let k1 = (((x_hi / df).ceil() as usize) + 1).min(spec_now.power.len());
    let pts: Vec<(f64, f64)> = if ep == 0 {
        Vec::new()
    } else {
        (k0..k1)
            .map(|k| (spec_now.freq(k), spec_now.power[k]))
            .collect()
    };
    let ymax = pts.iter().map(|p| p.1).fold(1.0f64, f64::max);
    let y_hi = 10f64.powf((ymax * 2.0).log10().ceil()).max(100.0);
    let xr = range(x_lo, x_hi);
    let yr = range(0.3, y_hi);

    let xax = AxisSpec::new(AxisScale::Linear, "frequency").unit("Hz");
    let yax = AxisSpec::new(AxisScale::Log10, "power").unit("\u{00D7} floor");
    let mut plot = PlotSpec::new(xax.clone(), yax.clone());
    if let Some(r) = ctx.ref_ep.filter(|r| *r >= 1 && *r < ep) {
        let spec_ref = ctx.obs.spectrum(r);
        let ref_pts: Vec<(f64, f64)> = (k0..k1.min(spec_ref.power.len()))
            .map(|k| (spec_ref.freq(k), spec_ref.power[k]))
            .collect();
        plot = plot.series(
            Series::scatter(ref_pts)
                .color((190, 170, 120))
                .label("earlier"),
        );
    }
    plot = plot
        .series(
            Series::line(pts)
                .color(TRACE)
                .reduce(Reduce::ExtremaPerColumn)
                .label("now"),
        )
        .annotate(Annotation::HLine {
            y: 1.0,
            color: FAINT,
        });
    // a faint guide from every identity tag down to its spectral line
    if let Some(e) = epoch {
        for sl_i in active_slots(ctx) {
            let sl = e.slots[sl_i];
            if sl.f > x_lo && sl.f < x_hi {
                plot = plot.annotate(Annotation::VLine {
                    x: sl.f,
                    color: scale(identity::rgb(sl_i, sl.state), 0.32),
                });
            }
        }
    }
    if let (Some(s), Some(f)) = (sel, sel_f) {
        let col = identity::rgb(s, epoch.map(|e| e.slots[s].state).unwrap_or(State::Quiet));
        for hmn in 1..=4 {
            let fh = f * hmn as f64;
            if fh > x_lo && fh < x_hi {
                plot = plot.annotate(Annotation::VLine {
                    x: fh,
                    color: scale(col, 0.55),
                });
            }
        }
    }
    if let Some((u, v)) = ctx.cursor {
        let cx = unproject_axis(AxisScale::Linear, xr, u);
        let cy = unproject_axis(AxisScale::Log10, yr, v);
        plot = plot
            .annotate(Annotation::VLine {
                x: cx,
                color: ACCENT,
            })
            .annotate(Annotation::HLine {
                y: cy,
                color: scale(ACCENT, 0.5),
            });
    }
    let po = draw_plot(&plot, &PlotView::new(xr, yr), w, rws.plot_h, ctx.glyphs);
    crate::theme::blit_all(&mut surf, &po.surface, 0, rws.plot_y);

    // identity tags on the axes the data uses
    let mut anchors = Vec::new();
    let mut readout_near = String::new();
    let cursor_f = ctx
        .cursor
        .map(|(u, _)| unproject_axis(AxisScale::Linear, xr, u));
    if let Some(e) = epoch {
        for (rank, s) in active_slots(ctx).into_iter().enumerate() {
            let sl = e.slots[s];
            if sl.f <= 0.0 {
                continue;
            }
            let (cx, _) = cell_of(&po.layout, sl.f, 1.0).unwrap_or((-1, 0));
            let row_off = (rank % 3) as i32;
            let y = rws.plot_y as i32 + po.layout.plot_rect.y as i32 + row_off;
            let in_view = sl.f >= x_lo && sl.f <= x_hi;
            let (ax, ay) = if in_view {
                (cx, y)
            } else if sl.f < x_lo {
                (po.layout.plot_rect.x as i32, y)
            } else {
                (
                    (po.layout.plot_rect.x + po.layout.plot_rect.width) as i32 - 3,
                    y,
                )
            };
            let tag = identity::tag(s, sl.state);
            let sty = identity::style(s, sl.state);
            if in_view {
                put(&mut surf, ax, ay, &tag, sty);
            } else {
                let arrow = if sl.f < x_lo { "\u{25C0}" } else { "\u{25B6}" };
                put(&mut surf, ax, ay, &format!("{arrow}{tag}"), sty);
            }
            anchors.push(Anchor {
                slot: s,
                x: ax,
                y: ay,
            });
            if let Some(cf) = cursor_f {
                if (cf - sl.f).abs() < (3.0 * df).max(0.02 * (x_hi - x_lo) / 4.0) {
                    readout_near =
                        format!("  \u{2248} {} f={:.4}", identity::tag(s, sl.state), sl.f);
                }
            }
        }
    }

    let readout = if let Some(cf) = cursor_f {
        let k = spec_now.bin(cf);
        let p = spec_now.power.get(k).copied().unwrap_or(0.0);
        format!(
            "f {:.4} Hz   P {:.1}\u{00D7} floor ({:.1} dB){}",
            cf,
            p,
            10.0 * p.max(1e-9).log10(),
            readout_near
        )
    } else {
        match (sel, epoch) {
            (Some(s), Some(e)) => {
                let sl = e.slots[s];
                format!(
                    "sel {} {}  f {:.5} Hz  Z {:.1}\u{03C3}   res {:.4} Hz",
                    identity::tag(s, sl.state),
                    sl.state.label(),
                    sl.f,
                    sl.z,
                    1.0 / (ep.max(1) as f64 * crate::sim::EPOCH_S)
                )
            }
            _ => format!(
                "integrating {:.0} s \u{00B7} resolution {:.4} Hz \u{00B7} no candidate above the floor yet",
                ep as f64 * crate::sim::EPOCH_S,
                1.0 / (ep.max(1) as f64 * crate::sim::EPOCH_S)
            ),
        }
    };
    draw_chrome(
        &mut surf,
        &rws,
        &Captions {
            y: &caption(&yax),
            x: &caption(&xax),
            describe: "SPECTRUM \u{00B7} harmonic-summed periodogram",
            note: super::compare_note(ctx, sel).as_deref(),
            readout: &readout,
            plot_x: po.layout.plot_rect.x,
            plot_w: po.layout.plot_rect.width,
        },
    );
    ViewOut {
        surface: surf,
        anchors,
        readout,
        axes: vec![(caption(&xax), caption(&yax))],
        reports: vec![po.report],
    }
}
