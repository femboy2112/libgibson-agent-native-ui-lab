//! RELATE: relationships between measured quantities. The main plot is the
//! delay plane (tau21, tau31): where every identity sits in the space of
//! measured inter-station delays. Physical sources can only live inside the
//! ellipse swept out by the unit disc of in-plane direction cosines; local
//! interference sits at the common-mode origin. The inset plots coherent
//! significance against integration time.

use gibson::plot::{Annotation, AxisScale, AxisSpec, PlotSpec, PlotView, Series};
use gibson::{Rect, Surface};

use super::common::*;
use super::{active_slots, effective_selection, Anchor, Ctx, ViewOut};
use crate::identity;
use crate::sim::{N_EPOCHS, SLOTS, STATION_POS};
use crate::theme::*;
use crate::track::{self, State};

pub const RANGE_MS: f64 = 70.0;

/// The physically allowed region of the delay plane, in ms.
pub fn allowed_region(n: usize) -> Vec<(f64, f64)> {
    let b2 = STATION_POS[1];
    let b3 = STATION_POS[2];
    (0..=n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            let (sx, sy) = (a.cos(), a.sin());
            (
                -(b2[0] * sx + b2[1] * sy) * 1000.0,
                -(b3[0] * sx + b3[1] * sy) * 1000.0,
            )
        })
        .collect()
}

/// Closed `k`-sigma covariance ellipse polyline.
pub fn cov_ellipse(
    cx: f64,
    cy: f64,
    vx: f64,
    vy: f64,
    cxy: f64,
    k: f64,
    n: usize,
) -> Vec<(f64, f64)> {
    let a = vx.max(1e-18).sqrt();
    let b = cxy / a;
    let c = (vy - b * b).max(1e-18).sqrt();
    (0..=n)
        .map(|i| {
            let th = std::f64::consts::TAU * i as f64 / n as f64;
            let (ct, stt) = (th.cos(), th.sin());
            (cx + k * a * ct, cy + k * (b * ct + c * stt))
        })
        .collect()
}

pub fn render(ctx: &Ctx, rect: Rect) -> ViewOut {
    let (w, h) = (rect.width, rect.height);
    let mut surf = Surface::new(w, h);
    let rws = rows(h);
    let tl = &ctx.obs.timeline;
    let epoch = tl.epoch(ctx.ep);
    let sel = effective_selection(ctx);
    let xax = AxisSpec::new(AxisScale::Linear, "delay \u{03C4}21").unit("ms");
    let yax = AxisSpec::new(AxisScale::Linear, "delay \u{03C4}31").unit("ms");
    let xr = range(-RANGE_MS, RANGE_MS);
    let yr = range(-RANGE_MS, RANGE_MS);
    // wide layouts give the delay plane the left ~62 % and the evidence the rest
    let two_pane = w >= 100 && rws.plot_h >= 18;
    let lw = if two_pane { w * 62 / 100 } else { w };

    let mut plot = PlotSpec::new(xax.clone(), yax.clone())
        .series(
            Series::line(allowed_region(120))
                .color((84, 96, 126))
                .label("allowed"),
        )
        .annotate(Annotation::VLine {
            x: 0.0,
            color: FAINT,
        })
        .annotate(Annotation::HLine {
            y: 0.0,
            color: FAINT,
        });
    let slots = active_slots(ctx);
    for &s in &slots {
        let state = epoch.map(|e| e.slots[s].state).unwrap_or(State::Quiet);
        let rgb = identity::rgb(s, state);
        // convergence trail: every epoch's estimate so far
        let trail: Vec<(f64, f64)> = (1..=ctx.ep)
            .filter_map(|k| {
                let d = tl.epoch(k)?.slots[s].delays?;
                (d.tau[0].is_finite() && d.tau[1].is_finite())
                    .then_some((d.tau[0] * 1000.0, d.tau[1] * 1000.0))
            })
            .collect();
        if !trail.is_empty() {
            plot = plot.series(
                Series::scatter(trail)
                    .color(scale(rgb, 0.55))
                    .label("trail"),
            );
        }
        if let Some(d) = epoch.and_then(|e| e.slots[s].delays) {
            if d.tau[0].is_finite() && d.tau[1].is_finite() {
                let (v2, v3) = ((d.sigma[0] * 1000.0).powi(2), (d.sigma[1] * 1000.0).powi(2));
                let c23 = 0.5 * (v2 * v3).sqrt();
                plot = plot.series(
                    Series::line(cov_ellipse(
                        d.tau[0] * 1000.0,
                        d.tau[1] * 1000.0,
                        v2,
                        v3,
                        c23,
                        2.0,
                        48,
                    ))
                    .color(rgb)
                    .label("2 sigma"),
                );
            }
        }
    }
    if let Some(r) = ctx.ref_ep.filter(|r| *r >= 1 && *r < ctx.ep) {
        for &s in &slots {
            if let Some(d) = tl.epoch(r).and_then(|e| e.slots[s].delays) {
                if d.tau[0].is_finite() && d.tau[1].is_finite() {
                    let (v2, v3) = ((d.sigma[0] * 1000.0).powi(2), (d.sigma[1] * 1000.0).powi(2));
                    let c23 = 0.5 * (v2 * v3).sqrt();
                    plot = plot.series(
                        Series::scatter(cov_ellipse(
                            d.tau[0] * 1000.0,
                            d.tau[1] * 1000.0,
                            v2,
                            v3,
                            c23,
                            2.0,
                            24,
                        ))
                        .color(DIM)
                        .label("earlier 2 sigma"),
                    );
                }
            }
        }
    }
    if let Some((u, v)) = ctx.cursor {
        plot = plot
            .annotate(Annotation::VLine {
                x: unproject_axis(AxisScale::Linear, xr, u),
                color: WARN,
            })
            .annotate(Annotation::HLine {
                y: unproject_axis(AxisScale::Linear, yr, v),
                color: WARN,
            });
    }
    let po = draw_plot(&plot, &PlotView::new(xr, yr), lw, rws.plot_h, ctx.glyphs);
    blit_all(&mut surf, &po.surface, 0, rws.plot_y);
    let pr = po.layout.plot_rect;

    // common-mode origin marker
    if let Some((ox, oy)) = cell_of(&po.layout, 0.0, 0.0) {
        let y = rws.plot_y as i32 + oy;
        if pr.width > 40 {
            put(
                &mut surf,
                ox + 2,
                y + 1,
                "common-mode (local) origin",
                st(DIM),
            );
        }
        put(&mut surf, ox, y, "+", st(SOFT));
    }
    put(
        &mut surf,
        pr.x as i32 + 1,
        rws.plot_y as i32 + pr.y as i32,
        "allowed region = unit disc of in-plane directions",
        st(scale(DIM, 0.9)),
    );

    // identity tags at the current estimates
    let mut anchors = Vec::new();
    for &s in &slots {
        let sl = epoch.map(|e| e.slots[s]).expect("active slot has epoch");
        let (cx, cy) = match sl.delays {
            Some(d) if d.tau[0].is_finite() && d.tau[1].is_finite() => {
                cell_of(&po.layout, d.tau[0] * 1000.0, d.tau[1] * 1000.0)
                    .unwrap_or((pr.x as i32, pr.y as i32))
            }
            _ => (pr.x as i32 + 1, pr.y as i32 + 1),
        };
        let x = cx.clamp(pr.x as i32, pr.x as i32 + pr.width as i32 - 3);
        let y = (rws.plot_y as i32 + cy).clamp(
            rws.plot_y as i32 + pr.y as i32,
            rws.plot_y as i32 + pr.y as i32 + pr.height as i32 - 1,
        );
        put(
            &mut surf,
            x,
            y,
            &identity::tag(s, sl.state),
            identity::style(s, sl.state),
        );
        anchors.push(Anchor { slot: s, x, y });
    }

    // right pane: significance against integration time, then the evidence ledger
    let mut reports = vec![po.report];
    let mut axes = vec![(caption(&xax), caption(&yax))];
    if two_pane {
        let (ix, iw) = (lw + 3, w - lw - 4);
        let ih = (rws.plot_h * 50 / 100).max(9);
        let iy = rws.plot_y;
        let zx = AxisSpec::new(AxisScale::Linear, "integration").unit("s");
        let zy = AxisSpec::new(AxisScale::Linear, "coherent significance").unit("\u{03C3}");
        let mut zp = PlotSpec::new(zx.clone(), zy.clone())
            .annotate(Annotation::HLine {
                y: track::Z_TRACK,
                color: FAINT,
            })
            .annotate(Annotation::HLine {
                y: track::Z_LOCK,
                color: scale(GOOD, 0.6),
            });
        for &s in &slots {
            let pts: Vec<(f64, f64)> = (1..=ctx.ep)
                .filter_map(|k| {
                    let e = tl.epoch(k)?;
                    let sl = e.slots[s];
                    sl.state.is_active().then_some((e.t, sl.z.max(0.0)))
                })
                .collect();
            let state = epoch.map(|e| e.slots[s].state).unwrap_or(State::Quiet);
            if pts.len() >= 2 {
                zp = zp.series(Series::line(pts).color(identity::rgb(s, state)).label("Z"));
            }
        }
        let zv = PlotView::new(range(0.0, crate::sim::DURATION), range(0.0, 26.0));
        let io = draw_plot(&zp, &zv, iw, ih, ctx.glyphs);
        blit_all(&mut surf, &io.surface, ix, iy + 1);
        for yy in 0..rws.plot_h {
            set_char(
                &mut surf,
                lw as i32 + 1,
                (rws.plot_y + yy) as i32,
                '\u{2502}',
                st(FAINT),
            );
        }
        put_clipped(
            &mut surf,
            ix as i32,
            iy as i32,
            &caption(&zy),
            st(SOFT),
            iw as usize,
        );
        let xc = caption(&zx);
        put_clipped(
            &mut surf,
            ix as i32 + (iw as i32 - xc.chars().count() as i32) / 2,
            (iy + 1 + ih) as i32,
            &xc,
            st(SOFT),
            iw as usize,
        );
        // identity tags at the right end of each curve
        for &s in &slots {
            if let Some(sl) = epoch.map(|e| e.slots[s]) {
                let (tx, ty) = (
                    cell_of(
                        &io.layout,
                        ctx.ep as f64 * crate::sim::EPOCH_S,
                        sl.z.clamp(0.0, 25.0),
                    ),
                    0,
                );
                let _ = ty;
                if let Some((cx, cy)) = tx {
                    put(
                        &mut surf,
                        (ix as i32 + cx - 3).max(ix as i32),
                        iy as i32 + 1 + cy,
                        &identity::tag(s, sl.state),
                        identity::style(s, sl.state),
                    );
                }
            }
        }
        put(
            &mut surf,
            (ix + iw) as i32 - 13,
            iy as i32 + 2,
            "lock \u{2265} 8\u{03C3}",
            st(scale(GOOD, 0.75)),
        );
        axes.push((caption(&zx), caption(&zy)));
        reports.push(io.report);

        // evidence ledger for the selected identity
        let ly = (iy + ih + 3) as i32;
        let ly = ly.min((rws.plot_y + rws.plot_h) as i32 - 8);
        if let (Some(s), Some(sl)) = (sel, sel.and_then(|s| epoch.map(|e| e.slots[s]))) {
            let ok = |b: bool| if b { "\u{2713}" } else { "\u{2717}" };
            let head = format!(
                "EVIDENCE for {} {}",
                identity::tag(s, sl.state),
                sl.state.label()
            );
            put_clipped(
                &mut surf,
                ix as i32,
                ly,
                &head,
                identity::style(s, sl.state),
                iw as usize,
            );
            let (sg, sd) = match sl.delays {
                Some(d) => (d.sigma[0] * 1000.0, d.tau[0].abs() * 1000.0),
                None => (f64::NAN, f64::NAN),
            };
            let rows_: [(bool, String); 6] = [
                (
                    sl.z >= track::Z_LOCK,
                    format!(
                        "coherent power  Z {:.1}\u{03C3}  (\u{2265} {:.0})",
                        sl.z,
                        track::Z_LOCK
                    ),
                ),
                (
                    sl.pulsy >= track::PULSY_MIN,
                    format!(
                        "pulse-like      {:.2}  (\u{2265} {:.2}; sine = 0)",
                        sl.pulsy,
                        track::PULSY_MIN
                    ),
                ),
                (
                    sl.persist >= 0.5,
                    format!("persistent      {:.2}  (\u{2265} 0.50)", sl.persist),
                ),
                (sl.stationary, "both halves of the data agree".to_string()),
                (
                    !sl.common_mode,
                    format!(
                        "not common-mode |\u{03C4}21| {:.0} ms, \u{03C3} {:.0}",
                        sd, sg
                    ),
                ),
                (
                    sl.sky
                        .map(|k| k.resolved_by_s4 && k.confidence_of_best() > 0.9)
                        .unwrap_or(false),
                    match sl.sky {
                        Some(k) => format!(
                            "mirror resolved {:.0}% {}",
                            k.confidence_of_best() * 100.0,
                            if k.resolved_by_s4 {
                                "(S4)"
                            } else {
                                "(planar array: 50/50)"
                            }
                        ),
                        None => "mirror resolved \u{2014}".to_string(),
                    },
                ),
            ];
            for (i, (good, text)) in rows_.iter().enumerate() {
                let y = ly + 1 + i as i32;
                if y >= (rws.plot_y + rws.plot_h) as i32 {
                    break;
                }
                put(
                    &mut surf,
                    ix as i32,
                    y,
                    ok(*good),
                    st(if *good { GOOD } else { WARN }),
                );
                put_clipped(&mut surf, ix as i32 + 2, y, text, st(SOFT), iw as usize - 2);
            }
        }
    }

    let readout = if let Some((u, v)) = ctx.cursor {
        let t2 = unproject_axis(AxisScale::Linear, xr, u);
        let t3 = unproject_axis(AxisScale::Linear, yr, v);
        let (sx, sy) = track::solve_inplane(t2 / 1000.0, t3 / 1000.0);
        let r = sx.hypot(sy);
        if r <= 1.0 {
            let az = sy.atan2(sx).to_degrees().rem_euclid(360.0);
            let el = (1.0 - r * r).sqrt().asin().to_degrees();
            format!(
                "\u{03C4}21 {:+.1} ms  \u{03C4}31 {:+.1} ms  \u{2192} az {:.1}\u{00B0}  el \u{00B1}{:.1}\u{00B0} (mirror pair)",
                t2, t3, az, el
            )
        } else {
            format!(
                "\u{03C4}21 {:+.1} ms  \u{03C4}31 {:+.1} ms  \u{2192} outside the allowed region (unphysical)",
                t2, t3
            )
        }
    } else {
        match (sel, epoch) {
            (Some(s), Some(e)) => {
                let sl = e.slots[s];
                match sl.delays {
                    Some(d) if d.tau[0].is_finite() => format!(
                        "{}  \u{03C4}21 {:+.1}\u{00B1}{:.1} ms  \u{03C4}31 {:+.1}\u{00B1}{:.1} ms{}",
                        identity::tag(s, sl.state),
                        d.tau[0] * 1000.0,
                        d.sigma[0] * 1000.0,
                        d.tau[1] * 1000.0,
                        d.sigma[1] * 1000.0,
                        if sl.common_mode { "  COMMON-MODE" } else { "" }
                    ),
                    _ => format!("{} no delay solution yet", identity::tag(s, sl.state)),
                }
            }
            _ => "no delay measurements yet".to_string(),
        }
    };
    draw_chrome(
        &mut surf,
        &rws,
        &Captions {
            y: &caption(&yax),
            x: &caption(&xax),
            describe: "RELATE \u{00B7} the delay plane",
            note: super::compare_note(ctx, sel).as_deref(),
            readout: &readout,
            plot_x: pr.x,
            plot_w: pr.width,
        },
    );
    let _ = (N_EPOCHS, SLOTS);
    ViewOut {
        surface: surf,
        anchors,
        readout,
        axes,
        reports,
    }
}
