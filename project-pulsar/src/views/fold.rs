//! FOLD: the data wrapped at the candidate's period. Top, the folded profile;
//! below, a phase-time map (every row folds one slab of the observation). When
//! the trial period is right the pulse forms a straight vertical ridge; detune
//! it and the ridge shears and the profile smears.

use gibson::plot::{Annotation, AxisScale, AxisSpec, PlotSpec, PlotView, Series};
use gibson::{Cell, Glyph, Rect, Surface};

use super::common::*;
use super::{effective_selection, Anchor, Ctx, ViewOut};
use crate::dsp::{self, Fold};
use crate::identity;
use crate::sim::{EPOCH_S, EPOCH_SAMPLES, FS};
use crate::theme::*;
use crate::track::State;

/// Frequency step of one "detune" key press: 0.12 of a cycle over the integration.
pub fn nudge_step(ep: usize) -> f64 {
    0.12 / (ep.max(1) as f64 * EPOCH_S)
}

/// The trial frequency shown for `slot`: the tracked estimate plus the user's detune.
pub fn trial_frequency(ctx: &Ctx, slot: usize) -> Option<f64> {
    let e = ctx.obs.timeline.epoch(ctx.ep)?;
    let sl = e.slots[slot];
    if !sl.state.is_active() || sl.f <= 0.0 {
        return None;
    }
    Some(sl.f + ctx.nudge as f64 * nudge_step(ctx.ep))
}

pub fn profile_for(
    ctx: &Ctx,
    slot: usize,
    ep: usize,
    bins: usize,
    nudge: i32,
) -> Option<(Fold, f64)> {
    let e = ctx.obs.timeline.epoch(ep)?;
    let sl = e.slots[slot];
    if !sl.state.is_active() || sl.f <= 0.0 {
        return None;
    }
    let f = sl.f + nudge as f64 * nudge_step(ep);
    Some((
        dsp::fold(&ctx.obs.cleans[0], 0, ep * EPOCH_SAMPLES, f, bins),
        f,
    ))
}

fn ridge_colour(z: f64, base: Rgb, mono: bool) -> Rgb {
    if mono {
        let v = (z / 3.2).clamp(0.0, 1.0) as f32;
        let g = (v * 255.0) as u8;
        return (g, g, g);
    }
    let v = (z / 3.2).clamp(0.0, 1.0) as f32;
    let bg = (6, 9, 16);
    if v < 0.6 {
        mix(bg, base, v / 0.6)
    } else {
        mix(base, (255, 255, 255), (v - 0.6) / 0.4)
    }
}

pub fn render(ctx: &Ctx, rect: Rect) -> ViewOut {
    let (w, h) = (rect.width, rect.height);
    let mut surf = Surface::new(w, h);
    let rws = rows(h);
    let sel = effective_selection(ctx);
    let epoch = ctx.obs.timeline.epoch(ctx.ep);
    let xax = AxisSpec::new(AxisScale::Linear, "rotation phase").unit("cycles");
    let yax = AxisSpec::new(AxisScale::Linear, "folded amplitude").unit("\u{03C3}");

    let (Some(s), Some(e)) = (sel, epoch) else {
        let msg = "no candidate yet \u{2014} the fold needs a period to wrap the data at";
        put_clipped(&mut surf, 1, (h / 2) as i32, msg, st(DIM), w as usize);
        let readout = "integrating\u{2026}".to_string();
        draw_chrome(
            &mut surf,
            &rws,
            &Captions {
                y: &caption(&yax),
                x: &caption(&xax),
                describe: "FOLD",
                note: None,
                readout: &readout,
                plot_x: 0,
                plot_w: w,
            },
        );
        return ViewOut {
            surface: surf,
            anchors: vec![],
            readout,
            axes: vec![(caption(&xax), caption(&yax))],
            reports: vec![],
        };
    };
    let sl = e.slots[s];
    let col = identity::rgb(s, sl.state);

    // vertical split: profile on top, phase-time map below
    let body = rws.plot_h;
    let prof_h = if body >= 18 {
        (body as f32 * 0.38) as u16
    } else if body >= 9 {
        body * 4 / 10
    } else {
        body
    };
    let show_map = body >= 9;
    let map_h = if show_map { body - prof_h } else { 0 };

    let nb = 64usize;
    let (fold, f_trial) = profile_for(ctx, s, ctx.ep, nb, ctx.nudge).expect("active slot has fold");
    let ymax = fold.mean.iter().cloned().fold(f64::MIN, f64::max) / fold.sigma;
    let ymin = fold.mean.iter().cloned().fold(f64::MAX, f64::min) / fold.sigma;
    let pad = ((ymax - ymin) * 0.25).max(0.08);
    let yr = range(ymin - pad, ymax + pad);
    let xr = range(0.0, 1.0);
    let prof_pts: Vec<(f64, f64)> = (0..nb)
        .map(|b| ((b as f64 + 0.5) / nb as f64, fold.mean[b] / fold.sigma))
        .collect();
    let peak_phase = fold.peak_phase();
    let mut plot = PlotSpec::new(xax.clone(), yax.clone());
    if let Some(r) = ctx.ref_ep.filter(|r| *r >= 1 && *r < ctx.ep) {
        if let Some((fr, _)) = profile_for(ctx, s, r, nb, 0) {
            let ref_pts: Vec<(f64, f64)> = (0..nb)
                .map(|b| ((b as f64 + 0.5) / nb as f64, fr.mean[b] / fr.sigma))
                .collect();
            plot = plot.series(Series::scatter(ref_pts).color(DIM).label("earlier"));
        }
    }
    plot = plot
        .series(Series::line(prof_pts).color(col).label("now"))
        .annotate(Annotation::HLine {
            y: 0.0,
            color: FAINT,
        })
        .annotate(Annotation::VLine {
            x: peak_phase,
            color: scale(col, 0.45),
        });
    if let Some((u, _)) = ctx.cursor {
        plot = plot.annotate(Annotation::VLine {
            x: unproject_axis(AxisScale::Linear, xr, u),
            color: WARN,
        });
    }
    let po = draw_plot(&plot, &PlotView::new(xr, yr), w, prof_h, ctx.glyphs);
    crate::theme::blit_all(&mut surf, &po.surface, 0, rws.plot_y);
    let pr = po.layout.plot_rect;

    // identity glyph on the pulse peak
    let (pcx, pcy) = cell_of(&po.layout, peak_phase, ymax).unwrap_or((pr.x as i32, pr.y as i32));
    let ay = (rws.plot_y as i32 + pcy - 1).max(rws.plot_y as i32);
    put(
        &mut surf,
        pcx - 1,
        ay,
        &identity::tag(s, sl.state),
        identity::style(s, sl.state),
    );
    let mut anchors = vec![Anchor {
        slot: s,
        x: pcx - 1,
        y: ay,
    }];

    // other identities that are also active appear as their own peak phases
    for o in 0..crate::sim::SLOTS {
        if o == s {
            continue;
        }
        let os = e.slots[o];
        if matches!(os.state, State::Locked | State::Tracking | State::Candidate) && os.f > 0.0 {
            let c = (pr.x as i32 + (os.peak_phase * pr.width as f64) as i32)
                .min(pr.x as i32 + pr.width as i32 - 2);
            let y = rws.plot_y as i32 + pr.y as i32;
            put(
                &mut surf,
                c,
                y,
                identity::glyph(o, os.state),
                identity::style(o, os.state),
            );
            anchors.push(Anchor { slot: o, x: c, y });
        }
    }

    // ── phase-time map ──
    let mut map_info = String::new();
    if show_map && map_h >= 3 {
        let top = rws.plot_y + prof_h;
        let mw = pr.width.max(8);
        let mrows = map_h.saturating_sub(1).max(2); // leave a caption row
        let n = ctx.ep * EPOCH_SAMPLES;
        let clean = &ctx.obs.cleans[0];
        let cols = mw as usize;
        let base = {
            let mut v: Vec<f64> = fold.mean.clone();
            dsp::median(&mut v)
        };
        // one row per slab of the observation, one column per phase bin
        let mut ms = Surface::new(mw, mrows);
        for r in 0..mrows as usize {
            let a = n * r / mrows as usize;
            let b = n * (r + 1) / mrows as usize;
            let mut sum = vec![0.0; cols];
            let mut cnt = vec![0u32; cols];
            for j in a..b {
                if !clean.ok[j] {
                    continue;
                }
                let ph = f_trial * j as f64 / FS;
                let fr = ph - ph.floor();
                let c = ((fr * cols as f64) as usize).min(cols - 1);
                sum[c] += clean.x[j];
                cnt[c] += 1;
            }
            for c in 0..cols {
                // 3-column boxcar in phase
                let (mut sm, mut cn) = (0.0, 0u32);
                for dc in [cols - 1, 0, 1] {
                    let cc = (c + dc) % cols;
                    sm += sum[cc];
                    cn += cnt[cc];
                }
                let z = if cn > 0 {
                    (sm / cn as f64 - base) * (cn as f64).sqrt() / clean.sigma
                } else {
                    0.0
                };
                let glyph = if z > 3.6 {
                    "\u{2588}"
                } else if z > 2.8 {
                    "\u{2593}"
                } else if z > 2.0 {
                    "\u{2592}"
                } else if z > 1.3 {
                    "\u{2591}"
                } else {
                    continue;
                };
                let rgb = ridge_colour(z, col, ctx.mono);
                ms.set_cell(c as u16, r as u16, Cell::new(Glyph::new(glyph), st(rgb)));
            }
        }
        let px_rows = mrows as usize;
        blit_all(&mut surf, &ms, pr.x, top);
        // time axis on the left of the map
        let t_total = n as f64 / FS;
        put(&mut surf, 0, top as i32, "0 s", st(DIM));
        let lab = format!("{:.0} s", t_total);
        put(&mut surf, 0, (top + mrows) as i32 - 1, &lab, st(DIM));
        let mid = format!("{:.0}", t_total / 2.0);
        put(&mut surf, 0, (top + mrows / 2) as i32, &mid, st(DIM));
        // caption row under the map
        let cap = format!(
            "{} \u{00B7} phase-time map, {:.1} s per row \u{00B7} ridge {}",
            caption(&xax),
            t_total / px_rows as f64,
            if ctx.nudge == 0 {
                "straight = period right".to_string()
            } else {
                format!("sheared (detune {:+})", ctx.nudge)
            }
        );
        put_clipped(
            &mut surf,
            pr.x as i32,
            (top + mrows) as i32,
            &cap,
            st(DIM),
            (w - pr.x) as usize,
        );
        map_info = format!("map {}x{}", cols, px_rows);
    }
    let _ = map_info;

    let readout = if let Some((u, _)) = ctx.cursor {
        let ph = u.clamp(0.0, 0.999999);
        let b = ((ph * nb as f64) as usize).min(nb - 1);
        format!(
            "phase {:.3} cyc   folded {:+.3}\u{03C3}  ({} samples)   {}",
            ph,
            fold.mean[b] / fold.sigma,
            fold.count[b],
            identity::tag(s, sl.state)
        )
    } else {
        format!(
            "{} {}  P {:.5} s  f {:.5} Hz{}  peak @ {:.3} cyc  fold {:.1}\u{03C3}",
            identity::tag(s, sl.state),
            sl.state.label(),
            1.0 / f_trial,
            f_trial,
            if ctx.nudge != 0 {
                format!(" (detuned {:+})", ctx.nudge)
            } else {
                String::new()
            },
            peak_phase,
            fold.snr()
        )
    };
    draw_chrome(
        &mut surf,
        &Rows {
            title: rws.title,
            plot_y: rws.plot_y,
            plot_h: rws.plot_h,
            xcap: None,
            read: rws.read,
        },
        &Captions {
            y: &caption(&yax),
            x: &caption(&xax),
            describe: "FOLD \u{00B7} wrap the data at the period",
            note: super::compare_note(ctx, sel).as_deref(),
            readout: &readout,
            plot_x: pr.x,
            plot_w: pr.width,
        },
    );
    // x caption sits right under the profile plot (above the map)
    if rws.xcap.is_some() && !show_map {
        put_clipped(
            &mut surf,
            pr.x as i32,
            rws.xcap.unwrap_or(0) as i32,
            &caption(&xax),
            st(SOFT),
            w as usize,
        );
    }
    ViewOut {
        surface: surf,
        anchors,
        readout,
        axes: vec![(caption(&xax), caption(&yax))],
        reports: vec![po.report],
    }
}
