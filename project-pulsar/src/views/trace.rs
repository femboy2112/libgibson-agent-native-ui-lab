//! TRACE: station-1 receiver amplitude versus time. The noise hides the pulses;
//! the rows of identity markers above the trace are the predicted pulse epochs
//! of every candidate the pipeline is tracking, so the *same* identity that was
//! a spectral peak reads here as a periodic comb of arrival times.

use gibson::plot::{Annotation, AxisScale, AxisSpec, PlotSpec, PlotView, Reduce, Series};
use gibson::{Rect, Surface};

use super::common::*;
use super::{effective_selection, Anchor, Ctx, ViewOut};
use crate::identity;
use crate::sim::{FS, N, SLOTS};
use crate::theme::*;
use crate::track::State;

pub const WINDOWS: [f64; 3] = [24.0, 8.0, 3.0];

pub fn window_for(zoom: u8) -> f64 {
    WINDOWS[(zoom as usize).min(2)]
}

/// Predicted pulse arrival times (s) of a slot inside `[lo, hi]` at station 1.
pub fn pulse_epochs(f: f64, peak_phase: f64, lo: f64, hi: f64) -> Vec<f64> {
    if f <= 0.0 {
        return Vec::new();
    }
    let m0 = (lo * f - peak_phase).floor() as i64;
    let m1 = (hi * f - peak_phase).ceil() as i64;
    (m0..=m1)
        .map(|m| (peak_phase + m as f64) / f)
        .filter(|t| *t >= lo && *t <= hi)
        .collect()
}

/// The S1 samples in `[x_lo, x_hi]` as plot points in units of sigma. A masked
/// (invalid or glitched) sample becomes a NaN gap and is never bridged.
pub fn trace_points(clean: &crate::dsp::Clean, x_lo: f64, x_hi: f64) -> Vec<(f64, f64)> {
    let j0 = (x_lo * FS).floor().max(0.0) as usize;
    let j1 = ((x_hi * FS).ceil() as usize).min(N);
    (j0..j1)
        .map(|j| {
            let t = j as f64 / FS;
            if clean.ok[j] {
                (t, clean.x[j] / clean.sigma)
            } else {
                (t, f64::NAN)
            }
        })
        .collect()
}

/// 0.1 s running mean of the valid samples, scaled x3 for visibility.
pub fn smoothed_points(clean: &crate::dsp::Clean, x_lo: f64, x_hi: f64) -> Vec<(f64, f64)> {
    let j0 = (x_lo * FS).floor().max(0.0) as usize;
    let j1 = ((x_hi * FS).ceil() as usize).min(N);
    let sm = ((0.10 * FS) as usize).max(2);
    let mut out = Vec::new();
    for j in j0..j1 {
        if j + sm <= j1 && j >= sm {
            let (mut s, mut c) = (0.0, 0);
            for q in j - sm / 2..j + sm / 2 {
                if q < N && clean.ok[q] {
                    s += clean.x[q];
                    c += 1;
                }
            }
            out.push((
                j as f64 / FS,
                if c > 0 {
                    s / c as f64 / clean.sigma * 3.0
                } else {
                    f64::NAN
                },
            ));
        }
    }
    out
}

/// Number of masked (invalid or glitched) samples of a cleaned stream in `[t0, t1)`.
pub fn masked_in(clean: &crate::dsp::Clean, t0: f64, t1: f64) -> usize {
    let a = (t0 * FS).floor().max(0.0) as usize;
    let b = ((t1 * FS).ceil() as usize).min(clean.ok.len());
    (a..b).filter(|j| !clean.ok[*j]).count()
}

pub fn render(ctx: &Ctx, rect: Rect) -> ViewOut {
    let (w, h) = (rect.width, rect.height);
    let mut surf = Surface::new(w, h);
    let rws = rows(h);
    let now = ctx.t.clamp(0.0, N as f64 / FS);
    let win = window_for(ctx.zoom);
    let (x_lo, x_hi) = if now < win {
        (0.0, win)
    } else {
        (now - win, now)
    };
    let xr = range(x_lo, x_hi);
    let yr = range(-4.0, 6.0);
    let clean = &ctx.obs.cleans[0];

    let trace = trace_points(clean, x_lo, now.min(x_hi));
    let smooth = smoothed_points(clean, x_lo, now.min(x_hi));

    let xax = AxisSpec::new(AxisScale::Linear, "receiver time").unit("s");
    let yax = AxisSpec::new(AxisScale::Linear, "S1 amplitude").unit("\u{03C3}");
    let mut plot = PlotSpec::new(xax.clone(), yax.clone())
        .series(
            Series::line(trace)
                .color(scale(TRACE, 0.8))
                .reduce(Reduce::ExtremaPerColumn)
                .label("S1"),
        )
        .series(Series::line(smooth).color(ACCENT).label("0.1 s mean x3"))
        .annotate(Annotation::HLine {
            y: 0.0,
            color: FAINT,
        });
    if let Some(r) = ctx.ref_ep {
        let tr = r as f64 * crate::sim::EPOCH_S;
        if tr >= x_lo && tr <= x_hi {
            plot = plot.annotate(Annotation::VLine { x: tr, color: DIM });
        }
    }
    if let Some((u, _)) = ctx.cursor {
        plot = plot.annotate(Annotation::VLine {
            x: unproject_axis(AxisScale::Linear, xr, u),
            color: WARN,
        });
    }
    let po = draw_plot(&plot, &PlotView::new(xr, yr), w, rws.plot_h, ctx.glyphs);
    crate::theme::blit_all(&mut surf, &po.surface, 0, rws.plot_y);

    // Predicted pulse epochs, one marker row per tracked identity.
    let mut anchors = Vec::new();
    let epoch = ctx.obs.timeline.epoch(ctx.ep);
    let pr = po.layout.plot_rect;
    // the "now" cursor: drawn by hand because `Annotation::VLine` at exactly the
    // view's upper bound lands one subpixel outside the canvas and vanishes
    // (see FRICTION.md)
    if let Some((nx, _)) = cell_of(&po.layout, now.min(x_hi), 0.0) {
        let nx = nx.clamp(pr.x as i32, (pr.x + pr.width) as i32 - 1);
        for yy in 0..pr.height {
            set_char(
                &mut surf,
                nx,
                (rws.plot_y + pr.y + yy) as i32,
                '\u{2502}',
                st(scale(ACCENT, 0.75)),
            );
        }
    }
    let mut rank = 0i32;
    let mut cursor_note = String::new();
    if let Some(e) = epoch {
        for s in 0..SLOTS {
            let sl = e.slots[s];
            if !matches!(sl.state, State::Tracking | State::Locked | State::Candidate) {
                continue;
            }
            let y = rws.plot_y as i32 + pr.y as i32 + rank;
            rank += 1;
            let tag = identity::tag(s, sl.state);
            let sty = identity::style(s, sl.state);
            let epochs = if sl.state == State::Candidate {
                Vec::new() // a bare spectral candidate has no timing solution yet
            } else {
                pulse_epochs(sl.f, sl.peak_phase_recent, x_lo, now.min(x_hi))
            };
            let cols: Vec<i32> = epochs
                .iter()
                .filter_map(|t| cell_of(&po.layout, *t, 0.0).map(|c| c.0))
                .collect();
            let spacing = if cols.len() >= 2 {
                (cols[cols.len() - 1] - cols[0]) as f64 / (cols.len() - 1) as f64
            } else {
                99.0
            };
            let mark = identity::glyph(s, sl.state);
            for c in &cols {
                if spacing >= 3.0 {
                    put(&mut surf, *c, y, mark, sty);
                } else {
                    put(&mut surf, *c, y, "\u{2575}", sty);
                }
            }
            // row legend at the left edge, on top of the markers
            put(&mut surf, pr.x as i32, y, &tag, sty);
            anchors.push(Anchor {
                slot: s,
                x: pr.x as i32,
                y,
            });
            if let Some((u, _)) = ctx.cursor {
                let tc = x_lo + u * (x_hi - x_lo);
                if sl.f > 0.0 && sl.state != State::Candidate {
                    let ph = (sl.f * tc - sl.peak_phase_recent).rem_euclid(1.0);
                    let ph = if ph > 0.5 { ph - 1.0 } else { ph };
                    cursor_note.push_str(&format!(
                        "  {} {:+.2}cyc",
                        identity::tag(s, sl.state),
                        ph
                    ));
                }
            }
        }
    }

    let readout = if let Some((u, v)) = ctx.cursor {
        let tc = x_lo + u * (x_hi - x_lo);
        let j = (tc * FS).round() as usize;
        let val = if tc > now || j >= N {
            "(not yet received)".to_string()
        } else if clean.ok[j] {
            format!("{:+.2}\u{03C3}", clean.x[j] / clean.sigma)
        } else {
            "MASKED (gap)".to_string()
        };
        let _ = v;
        format!("t {:.2} s   S1 {}{}", tc, val, cursor_note)
    } else {
        format!(
            "window {:.0} s \u{00B7} {} masked samples shown as gaps \u{00B7} markers = predicted pulse epochs",
            win,
            masked_in(clean, x_lo, now.min(x_hi))
        )
    };
    draw_chrome(
        &mut surf,
        &rws,
        &Captions {
            y: &caption(&yax),
            x: &caption(&xax),
            describe: "TRACE \u{00B7} the pulses are buried",
            note: super::compare_note(ctx, effective_selection(ctx)).as_deref(),
            readout: &readout,
            plot_x: pr.x,
            plot_w: pr.width,
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
