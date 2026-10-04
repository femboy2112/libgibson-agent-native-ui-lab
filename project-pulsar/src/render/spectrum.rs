//! SPECTRUM — whitened power versus frequency (log power), with a dynamic spectrum.
//!
//! Visual law: *the same identity is a place on the frequency axis*. A pulse train
//! is a comb of teeth, a drifting carrier is a smear that the model path follows, a
//! line the analysis refuses to adopt is flagged as interference. The integrated
//! spectrum is the `gibson::plot` showpiece (`ExtremaPerColumn` keeps a one-bin
//! line alive across 16 000 bins squeezed into ~200 columns); the dynamic spectrum
//! below is custom graphics because `gibson::plot` has no heat-map.

use super::draw::*;
use super::{id_color, lane, Probe, RasterProbe, ViewIn, ViewOut};
use crate::analysis::chirp::B_BAND;
use crate::analysis::{LineClass, Stage, LINE_THR};
use crate::scenario::{SigId, CHUNK, FS};
use crate::session::Focus;
use gibson::plot::{Annotation, AxisScale, AxisSpec, PlotSpec, PlotView, Reduce, Series};
use gibson::Rect;

/// Frequency window (Hz) shown for the current focus / zoom.
pub fn freq_window(vin: &ViewIn) -> (f64, f64) {
    let st = vin.st;
    if st.focus == Focus::Overview {
        return (0.0, FS / 2.0);
    }
    let p = vin.cp.signal(st.selected);
    let t_now = vin.cp.t.max(1.0);
    let (lo, hi) = match (st.selected, p.model) {
        (SigId::Beta, Some(m)) => {
            let (a, b) = (m.freq(0.0), m.freq(t_now));
            let (fmin, fmax) = (a.min(b), a.max(b));
            match st.zoom {
                0 => (fmin - 0.6, fmax + 0.6),
                1 => (p.f_now - 0.3, p.f_now + 0.3),
                _ => (p.f_now - 0.1, p.f_now + 0.1),
            }
        }
        (SigId::Beta, None) => (B_BAND.0, B_BAND.1),
        (_, Some(m)) => {
            let f0 = m.f0.max(0.05);
            match st.zoom {
                0 => (0.0, 7.0 * f0),
                1 => (0.0, 3.5 * f0),
                _ => (0.5 * f0, 2.5 * f0),
            }
        }
        (id, None) => {
            let b = if id == SigId::Alpha {
                crate::analysis::BAND_A
            } else {
                crate::analysis::BAND_C
            };
            (0.0, 3.5 * b.1)
        }
    };
    (lo.max(0.0), hi.min(FS / 2.0).max(lo.max(0.0) + 0.05))
}

/// Series of the whitened spectrum restricted to bins whose frequency lies in any of
/// `windows`; windows are separated by a `(NaN, NaN)` gap so the line never bridges.
fn windowed(sp: &crate::analysis::spectral::Spectrum, windows: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for &(lo, hi) in windows {
        let j0 = sp.bin_of(lo.max(0.0)).max(1);
        let j1 = sp.bin_of(hi);
        if j1 < j0 {
            continue;
        }
        if !out.is_empty() {
            out.push((f64::NAN, f64::NAN));
        }
        for j in j0..=j1 {
            out.push((j as f64 * sp.df, sp.w[j] as f64));
        }
    }
    out
}

pub fn draw(vin: &ViewIn) -> ViewOut {
    let mut out = ViewOut::new(vin.rect);
    let r = vin.rect;
    if r.width < 8 || r.height < 3 {
        return out;
    }
    let with_dyn = r.height >= 15 && vin.cp.dyn_rows > 1;
    let top_h = if with_dyn { (r.height * 11 / 20).max(8) } else { r.height };
    let top = Rect::new(0, 0, r.width, top_h);
    let st = vin.st;
    let cp = vin.cp;
    let (x0, x1) = freq_window(vin);

    let top = lane(
        &mut out.surf,
        top,
        &[
            (
                "INTEGRATED POWER SPECTRUM".to_string(),
                style_b(INK, true, false),
            ),
            (
                format!(
                    "  whitened, 3 stations · through {:.0} s",
                    cp.t
                ),
                style(MUTED),
            ),
        ],
    );
    let mut spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "frequency").unit("Hz"),
        AxisSpec::new(AxisScale::Log10, "power").unit("× floor"),
    );
    let mut ymax_data: f64 = 30.0;
    if let Some(sp) = &cp.spec {
        let wins = [(x0 - 2.0 * sp.df, x1 + 2.0 * sp.df)];
        let base = windowed(sp, &wins);
        for &(_, y) in &base {
            if y.is_finite() {
                ymax_data = ymax_data.max(y);
            }
        }
        spec = spec.series(
            Series::line(base)
                .color(NOISE)
                .reduce(Reduce::ExtremaPerColumn)
                .label("spectrum"),
        );
        let tol = (6.0 * sp.df).max(0.03);
        // identity overlays
        for id in SigId::ALL {
            let p = cp.signal(id);
            if p.stage < Stage::Candidate {
                continue;
            }
            let Some(m) = p.model else { continue };
            let col = id_color(id, id == st.selected, st.focus);
            let wins: Vec<(f64, f64)> = match id {
                SigId::Beta => {
                    let (a, b) = (m.freq(0.0), m.freq(cp.t));
                    vec![(a.min(b) - tol, a.max(b) + tol)]
                }
                _ => {
                    let hmax = if id == SigId::Alpha { 12 } else { 8 };
                    (1..=hmax)
                        .map(|h| h as f64 * m.f0)
                        .filter(|f| *f < FS / 2.0 - 0.1)
                        .map(|f| (f - tol, f + tol))
                        .collect()
                }
            };
            let ov = windowed(sp, &wins);
            spec = spec.series(Series::line(ov).color(col).label(id.name()));
            // direct labels
            let label_at = |f: f64, text: String, spec: PlotSpec| -> PlotSpec {
                if f < x0 || f > x1 {
                    return spec;
                }
                let w = sp.at(f).max(1.0);
                spec.annotate(Annotation::Point {
                    x: f,
                    y: w.max(0.5),
                    label: text,
                    color: col,
                })
            };
            match id {
                SigId::Beta => {
                    let f = p.f_now;
                    spec = label_at(f, format!("{}{}", id.glyph(), id.letter()), spec);
                }
                _ => {
                    for h in 1..=3usize {
                        let f = h as f64 * m.f0;
                        let t = if h == 1 {
                            format!("{}{}", id.glyph(), id.letter())
                        } else {
                            format!("{h}")
                        };
                        spec = label_at(f, t, spec);
                    }
                }
            }
        }
        // interference / unassociated lines
        for l in &cp.lines {
            let (col, glyph) = match l.class {
                LineClass::Terrestrial => (WARN, "✕"),
                LineClass::Transient => ((200, 170, 120), "~"),
                LineClass::Unassigned => (MUTED, "?"),
            };
            if l.f < x0 || l.f > x1 {
                continue;
            }
            let ov = windowed(sp, &[(l.f - tol, l.f + tol)]);
            spec = spec.series(Series::line(ov).color(col).label(&l.label));
            spec = spec.annotate(Annotation::Point {
                x: l.f,
                y: sp.at(l.f).max(0.5),
                label: format!("{glyph}{:.1}", l.f),
                color: col,
            });
        }
        spec = spec.annotate(Annotation::HLine {
            y: LINE_THR,
            color: FAINT,
        });
    }
    let ytop = 10f64.powf((ymax_data * 1.6).log10().ceil()).max(100.0);
    let view = PlotView::new(range(x0, x1), log_range(0.3, ytop));
    let probe = draw_plot(&mut out.surf, top, "spectrum", spec, view, vin.mode);

    // ---- dynamic spectrum (custom raster) ----
    if with_dyn {
        if let Some(pr) = &probe {
            let rect = Rect::new(0, top_h, r.width, r.height - top_h);
            let rect = lane(
                &mut out.surf,
                rect,
                &[
                    ("DYNAMIC SPECTRUM".to_string(), style_b(INK, true, false)),
                    (
                        "  32 s segments, time flows down · ● model path of BETA".to_string(),
                        style(MUTED),
                    ),
                ],
            );
            dynamic_spectrum(vin, &mut out, rect, pr);
        }
    }
    if let Some(p) = probe {
        out.probes.insert(0, Probe::Plot(p));
    }
    out
}

fn dynamic_spectrum(vin: &ViewIn, out: &mut ViewOut, rect: Rect, pr: &PlotProbe) {
    let cp = vin.cp;
    let dy = &vin.eng.dynspec;
    let rows = cp.dyn_rows.min(dy.rows.len());
    let pl = pr.layout.plot_rect;
    let (cx0, cw) = (pl.x, pl.width);
    if rows == 0 || cw == 0 || rect.height < 2 {
        return;
    }
    let disp = rect.height as usize;
    let t_of = |j: usize| (j * dy.hop + dy.seg_len / 2) as f64 / FS;
    let tr = pr.layout.transform.as_ref();
    let Some(tr) = tr else { return };
    // The vertical axis is the whole observation (time flows down): rows not yet
    // observed stay blank, so the picture fills in as the run advances.
    let t_total = crate::scenario::T_END;
    let row_of_t = |t: f64| (((t / t_total) * disp as f64).floor() as usize).min(disp - 1);
    // frequency of each cell column
    let fcol: Vec<(f64, f64)> = (0..cw)
        .map(|c| {
            let a = tr.unproject(c as f64 * 2.0, 0.0).0;
            let b = tr.unproject(c as f64 * 2.0 + 2.0, 0.0).0;
            (a.min(b), a.max(b))
        })
        .collect();
    let mut z = vec![vec![0.0f32; cw as usize]; disp];
    for j in 0..rows {
        let d = row_of_t(t_of(j));
        for (c, zc) in z[d].iter_mut().enumerate() {
            let (fa, fb) = fcol[c];
            let b0 = ((fa / dy.df).floor().max(0.0)) as usize;
            let b1 = (((fb / dy.df).ceil()) as usize).min(dy.nbins - 1);
            for b in b0..=b1.max(b0) {
                if b < dy.nbins {
                    *zc = zc.max(dy.rows[j][b]);
                }
            }
        }
    }
    for (d, zr) in z.iter().enumerate() {
        for (c, &v) in zr.iter().enumerate() {
            let level = ((v.max(1.0) as f64).ln() / 40f64.ln()).clamp(0.0, 1.0);
            if level < 0.38 {
                continue; // below ~4× floor: not drawn (noise speckle)
            }
            let ch = ramp_char((level - 0.3) / 0.7, vin.mode);
            let col = mix((50, 70, 110), INK, level);
            put(
                &mut out.surf,
                (rect.x + cx0 + c as u16) as i32,
                (rect.y + d as u16) as i32,
                &ch.to_string(),
                style(col),
            );
        }
    }
    // BETA's fitted path, drawn over the ridge it is supposed to follow
    let pb = cp.signal(SigId::Beta);
    if pb.stage >= Stage::Candidate {
        if let Some(m) = pb.model {
            let col = id_color(SigId::Beta, SigId::Beta == vin.st.selected, vin.st.focus);
            for j in 0..rows {
                let f = m.freq(t_of(j));
                if let Some((px, _)) = tr.project(f, 1.0) {
                    let c = (px / 2.0).floor() as i32;
                    if c >= 0 && (c as u16) < cw {
                        put(
                            &mut out.surf,
                            (rect.x + cx0) as i32 + c,
                            (rect.y as usize + row_of_t(t_of(j))) as i32,
                            "●",
                            style_b(col, true, false),
                        );
                    }
                }
            }
        }
    }
    let f_lo = tr.unproject(0.0, 0.0).0;
    let f_hi = tr.unproject(cw as f64 * 2.0, 0.0).0;
    out.probes.push(Probe::Raster(RasterProbe {
        name: "dynspec".into(),
        origin: (rect.x + cx0, rect.y),
        size: (cw, disp as u16),
        x_range: (f_lo, f_hi),
        y_range: (0.0, t_total),
        x_label: "frequency [Hz]",
        y_label: "time [s]",
        z_label: "power [× floor]",
        z,
    }));
    let _ = CHUNK;
}
