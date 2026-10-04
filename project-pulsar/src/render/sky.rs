//! SKY — inferred source geometry.
//!
//! Visual law: **uncertainty is diffusion; confidence is sharpness.** Each identity
//! is a cloud of 160 posterior samples. Before the analysis has a measurement the
//! cloud is the prior (diffuse over the whole sky); as coherent integration
//! accumulates, the *same* samples are drawn through a shrinking covariance, so the
//! cloud visibly contracts onto the source and the 1σ/2σ ellipses close around it.
//! Nothing is redrawn at random between frames: sample `i` of identity `s` is a pure
//! function of `(seed, s, i)`.
//!
//! Custom graphics (Braille canvas layers + glyph markers), because a sky is not an
//! x–y graph: `gibson::plot` has no polar/orthographic projection (deferred in its
//! own design doc), and the posterior is not a series.

use super::draw::*;
use super::{azel, id_color, Probe, SkyProbe, ViewIn, ViewOut};
use crate::analysis::coherent::{chol2, ellipse};
use crate::analysis::{LineClass, SignalProduct, Stage};
use crate::rng::{gauss, hash3, unit};
use crate::scenario::SigId;
use crate::session::Focus;
use gibson::{BrailleCanvas, Style, SubcellGlyphMode, Surface};
use std::f64::consts::{PI, TAU};

pub const CLOUD_N: usize = 160;

/// Deterministic posterior sample `i` of identity `id` in direction cosines.
/// Prior (uniform over the visible sky) until a position measurement exists.
pub fn sample(seed: u64, id: SigId, p: &SignalProduct, i: usize) -> (f64, f64) {
    match (&p.sky, p.stage >= Stage::Candidate) {
        (Some(sk), true) => {
            let z0 = gauss(seed, 700 + id.idx() as u64, 2 * i as u64);
            let z1 = gauss(seed, 700 + id.idx() as u64, 2 * i as u64 + 1);
            let l = chol2(sk.cov);
            let (x, y) = (
                sk.lm.0 + l[0][0] * z0,
                sk.lm.1 + l[1][0] * z0 + l[1][1] * z1,
            );
            let r = (x * x + y * y).sqrt();
            if r > 0.995 {
                (x * 0.995 / r, y * 0.995 / r)
            } else {
                (x, y)
            }
        }
        _ => {
            let u1 = unit(hash3(seed, 900 + id.idx() as u64, 2 * i as u64));
            let u2 = unit(hash3(seed, 900 + id.idx() as u64, 2 * i as u64 + 1));
            let r = u1.sqrt() * 0.995;
            (r * (TAU * u2).cos(), r * (TAU * u2).sin())
        }
    }
}

/// Points of the `k`-σ covariance ellipse, in direction cosines.
pub fn ellipse_points(sk: &crate::analysis::coherent::SkyFit, k: f64, n: usize) -> Vec<(f64, f64)> {
    let (a, b, ang) = ellipse(sk.cov);
    let (ca, sa) = (ang.cos(), ang.sin());
    (0..=n)
        .map(|j| {
            let t = TAU * j as f64 / n as f64;
            let (x, y) = (k * a * t.cos(), k * b * t.sin());
            (sk.lm.0 + x * ca - y * sa, sk.lm.1 + x * sa + y * ca)
        })
        .collect()
}

struct Layer {
    canvas: BrailleCanvas,
    style: Style,
}

fn paint_layer(surf: &mut Surface, layer: &Layer, origin: (u16, u16), mode: SubcellGlyphMode) {
    for cy in 0..layer.canvas.height {
        for cx in 0..layer.canvas.width {
            if let Some(g) = layer.canvas.glyph_at_mode(cx, cy, mode) {
                let (x, y) = (origin.0 + cx, origin.1 + cy);
                if let Some(cell) = surf.get_mut(x, y) {
                    let bg = cell.style.bg;
                    cell.glyph = gibson::Glyph::from_char(g);
                    cell.style = layer.style;
                    cell.style.bg = bg;
                    cell.is_continuation = false;
                }
            }
        }
    }
}

/// Window: `(center, half_extent)` in direction cosines for the current focus.
pub fn window(vin: &ViewIn) -> ((f64, f64), f64) {
    let st = vin.st;
    if st.focus == Focus::Overview {
        return ((0.0, 0.0), 1.0);
    }
    let p = vin.cp.signal(st.selected);
    match (&p.sky, p.stage >= Stage::Candidate) {
        (Some(sk), true) => {
            let s = sk.sig_major.max(1e-3);
            let half = match st.zoom {
                0 => (4.0 * s).max(0.35),
                1 => (2.5 * s).max(0.15),
                _ => (1.2 * s).max(0.05),
            }
            .min(1.0);
            (sk.lm, half)
        }
        _ => ((0.0, 0.0), 1.0),
    }
}

pub fn draw(vin: &ViewIn) -> ViewOut {
    let mut out = ViewOut::new(vin.rect);
    let r = vin.rect;
    if r.width < 10 || r.height < 4 {
        return out;
    }
    let st = vin.st;
    let cp = vin.cp;
    let seed = st.seed;
    let mode = vin.mode;

    // ---- geometry: where does the disc go? ----
    let legend_side = r.width >= 90;
    let legend_rows: u16 = if !legend_side && r.height >= 20 { 6 } else { 0 };
    let legend_w: u16 = if legend_side { 40.min(r.width / 2) } else { 0 };
    let avail_w = r.width - legend_w;
    let avail_h = r.height - legend_rows;
    let disc_h = avail_h.saturating_sub(2).min((avail_w.saturating_sub(4)) / 2).max(3);
    let disc_w = (disc_h * 2).min(avail_w);
    let ox = (avail_w - disc_w) / 2;
    let oy = (avail_h - disc_h) / 2;
    let origin = (ox, oy);
    let pw = disc_w as f64 * 2.0;
    let ph = disc_h as f64 * 4.0;
    let (center, half) = window(vin);
    let px_per_unit = (ph.min(pw) / 2.0 - 1.0) / half;
    let probe_base = SkyProbe {
        origin,
        size: (disc_w, disc_h),
        center,
        half,
        px_per_unit,
        center_px: (pw / 2.0, ph / 2.0),
        marks: Vec::new(),
    };
    let to_px = |l: f64, m: f64| -> (i32, i32) {
        let (x, y) = probe_base.lm_to_px(l, m);
        (x.round() as i32, y.round() as i32)
    };

    // ---- layer 0: sky grid ----
    let mut grid = Layer {
        canvas: BrailleCanvas::new(disc_w, disc_h),
        style: style(FAINT),
    };
    let ring = |c: &mut BrailleCanvas, rad: f64, step: usize| {
        let n = 360;
        for j in (0..n).step_by(step) {
            let a = TAU * j as f64 / n as f64;
            let (x, y) = to_px(rad * a.cos(), rad * a.sin());
            c.set(x, y);
        }
    };
    ring(&mut grid.canvas, 1.0, 1); // horizon, solid
    ring(&mut grid.canvas, 0.866, 3); // 30° elevation
    ring(&mut grid.canvas, 0.5, 3); // 60° elevation
    for k in 0..8 {
        let a = PI / 4.0 * k as f64;
        for s in 0..=40 {
            if s % 3 != 0 {
                continue;
            }
            let rr = 0.05 + 0.95 * s as f64 / 40.0;
            let (x, y) = to_px(rr * a.sin(), rr * a.cos());
            grid.canvas.set(x, y);
        }
    }
    // zoomed views get a local cross-hair grid so scale is readable
    if half < 0.9 {
        let step = if half > 0.25 { 0.1 } else if half > 0.1 { 0.05 } else { 0.02 };
        let k0 = ((center.0 - half * 2.0) / step).floor() as i64;
        let k1 = ((center.0 + half * 2.0) / step).ceil() as i64;
        for k in k0..=k1 {
            let l = k as f64 * step;
            let (x, _) = to_px(l, 0.0);
            for yy in (0..ph as i32).step_by(6) {
                grid.canvas.set(x, yy);
            }
        }
        let k0 = ((center.1 - half * 2.0) / step).floor() as i64;
        let k1 = ((center.1 + half * 2.0) / step).ceil() as i64;
        for k in k0..=k1 {
            let m = k as f64 * step;
            let (_, y) = to_px(0.0, m);
            for xx in (0..pw as i32).step_by(6) {
                grid.canvas.set(xx, y);
            }
        }
    }
    paint_layer(&mut out.surf, &grid, origin, mode);

    // ---- layers per identity: cloud, ellipses ----
    let mut marks: Vec<(String, (f64, f64), f64)> = Vec::new();
    for id in SigId::ALL {
        let p = cp.signal(id);
        let sel = id == st.selected;
        let col = id_color(id, sel, st.focus);
        let mut cloud = Layer {
            canvas: BrailleCanvas::new(disc_w, disc_h),
            style: style_b(
                if p.stage >= Stage::Candidate { col } else { scale(col, 0.5) },
                false,
                p.stage < Stage::Candidate,
            ),
        };
        for i in 0..CLOUD_N {
            let (l, m) = sample(seed, id, p, i);
            let (x, y) = to_px(l, m);
            cloud.canvas.set(x, y);
        }
        paint_layer(&mut out.surf, &cloud, origin, mode);
        if let (Some(sk), true) = (&p.sky, p.stage >= Stage::Candidate) {
            let mut e = Layer {
                canvas: BrailleCanvas::new(disc_w, disc_h),
                style: style_b(col, sel, false),
            };
            for (k, step) in [(1.0, 1usize), (2.0, 2usize)] {
                let pts = ellipse_points(sk, k, 96);
                for (j, &(l, m)) in pts.iter().enumerate() {
                    if j % step == 0 && l * l + m * m <= 1.05 {
                        let (x, y) = to_px(l, m);
                        e.canvas.set(x, y);
                    }
                }
            }
            paint_layer(&mut out.surf, &e, origin, mode);
            marks.push((id.name().to_string(), sk.lm, sk.sig_major));
        }
    }
    // interference seen by the array: only a *physical* direction is placed on the sky
    for l in &cp.lines {
        if let Some(sk) = &l.sky {
            if l.class != LineClass::Terrestrial && sk.physical {
                let (x, y) = to_px(sk.lm.0, sk.lm.1);
                let (cx, cy) = (x / 2, y / 4);
                if cx >= 0 && cy >= 0 && (cx as u16) < disc_w && (cy as u16) < disc_h {
                    let g = if l.class == LineClass::Transient { "~" } else { "?" };
                    put(&mut out.surf, (origin.0 as i32) + cx, (origin.1 as i32) + cy, g, style_b(MUTED, true, false));
                    marks.push((format!("{} {:.1} Hz", l.class.label().to_ascii_lowercase(), l.f), sk.lm, sk.sig_major));
                }
            }
        }
    }

    // ---- truth overlay ----
    if st.truth {
        for id in SigId::ALL {
            let t = vin.eng.rx.scn.lm(id);
            let (x, y) = to_px(t.0, t.1);
            let (cx, cy) = (x / 2, y / 4);
            if cx >= 0 && cy >= 0 && (cx as u16) < disc_w && (cy as u16) < disc_h {
                let g = ["◇", "○", "△"][id.idx()];
                put(
                    &mut out.surf,
                    origin.0 as i32 + cx,
                    origin.1 as i32 + cy,
                    g,
                    style_b(id.color(), true, false),
                );
            }
        }
    }

    // ---- markers + names ----
    for id in SigId::ALL {
        let p = cp.signal(id);
        let (Some(sk), true) = (&p.sky, p.stage >= Stage::Candidate) else { continue };
        let sel = id == st.selected;
        let col = id_color(id, sel, st.focus);
        let (x, y) = to_px(sk.lm.0, sk.lm.1);
        let (cx, cy) = (x / 2, y / 4);
        if cx < 0 || cy < 0 || cx as u16 >= disc_w || cy as u16 >= disc_h {
            continue;
        }
        let (gx, gy) = (origin.0 as i32 + cx, origin.1 as i32 + cy);
        put(&mut out.surf, gx, gy, id.glyph(), style_b(col, true, false));
        let name = if disc_w >= 30 { id.name().to_string() } else { id.letter().to_string() };
        let label = if sel && disc_w >= 30 {
            let (az, el) = azel(sk.lm);
            format!("{name} {az:.0}°/{el:.0}°")
        } else {
            name
        };
        let right = gx + 2 + label.chars().count() as i32 <= (origin.0 + disc_w) as i32 + legend_w as i32 / 2;
        let lx = if right { gx + 2 } else { gx - 1 - label.chars().count() as i32 };
        put(&mut out.surf, lx, gy, &label, style_b(col, sel, false));
    }

    // ---- compass ----
    let ccx = origin.0 as i32 + disc_w as i32 / 2;
    let ccy = origin.1 as i32 + disc_h as i32 / 2;
    if half >= 0.9 {
        let st_c = style(MUTED);
        put(&mut out.surf, ccx, origin.1 as i32 - 1, "N", st_c);
        put(&mut out.surf, ccx, (origin.1 + disc_h) as i32, "S", st_c);
        put(&mut out.surf, origin.0 as i32 - 2, ccy, "W", st_c);
        put(&mut out.surf, (origin.0 + disc_w) as i32 + 1, ccy, "E", st_c);
        // zenith and elevation labels along the north spoke
        put(&mut out.surf, ccx, ccy, "+", style(FAINT));
        if disc_h >= 18 {
            for (rad, lab) in [(0.866, "30°"), (0.5, "60°")] {
                let (_, py) = to_px(0.0, rad);
                put(&mut out.surf, ccx + 1, origin.1 as i32 + py / 4, lab, style(FAINT));
            }
        }
    } else {
        put(
            &mut out.surf,
            origin.0 as i32,
            origin.1 as i32,
            &format!("zoom: centre l={:+.3} m={:+.3}  half-width {:.3}", center.0, center.1, half),
            style(MUTED),
        );
    }

    // ---- legend ----
    let lx0 = if legend_side { (avail_w) as i32 + 1 } else { 1 };
    let ly0 = if legend_side { 1 } else { (avail_h) as i32 };
    let mut y = ly0;
    let lw = if legend_side { legend_w as i32 - 2 } else { r.width as i32 - 2 };
    let maxy = r.height as i32;
    let mut line = |surf: &mut Surface, text: String, st: Style| {
        if y < maxy {
            put_max(surf, lx0, y, &text, st, lw.max(1) as u16);
        }
        y += 1;
    };
    if legend_side || legend_rows > 0 {
        let stage_note = |p: &SignalProduct| p.stage.label();
        for id in SigId::ALL {
            let p = cp.signal(id);
            let sel = id == st.selected;
            let col = id_color(id, sel, st.focus);
            line(
                &mut out.surf,
                format!("{} {:<6} {}", id.glyph(), id.name(), stage_note(p)),
                style_b(col, true, false),
            );
            match (&p.sky, p.stage >= Stage::Candidate) {
                (Some(sk), true) => {
                    let (az, el) = azel(sk.lm);
                    let (a, b, _) = ellipse(sk.cov);
                    if legend_side {
                        line(
                            &mut out.surf,
                            if sk.physical {
                                format!("  az {az:5.1}°  el {el:4.1}°")
                            } else if sk.sig_major > 0.3 {
                                "  position unconstrained".to_string()
                            } else {
                                "  no far-field solution".to_string()
                            },
                            style(INK),
                        );
                        line(
                            &mut out.surf,
                            format!("  1σ {a:.3} × {b:.3}  ({:.1}°)", a.to_degrees()),
                            style(MUTED),
                        );
                        if st.truth {
                            let t = vin.eng.rx.scn.lm(id);
                            let d = ((t.0 - sk.lm.0).powi(2) + (t.1 - sk.lm.1).powi(2)).sqrt();
                            line(
                                &mut out.surf,
                                format!("  truth miss {d:.3} ({:.1}σ)", d / a.max(1e-9)),
                                style(WARN),
                            );
                        }
                    } else {
                        line(
                            &mut out.surf,
                            if sk.physical {
                                format!("  az {az:.0}° el {el:.0}°  1σ {a:.2}×{b:.2}")
                            } else {
                                format!("  unconstrained  1σ {a:.2}×{b:.2}")
                            },
                            style(INK),
                        );
                    }
                }
                _ => {
                    line(
                        &mut out.surf,
                        "  position unconstrained (prior: all sky)".to_string(),
                        style(FAINT),
                    );
                }
            }
        }
        for l in cp.interference().into_iter().filter(|l| l.class == LineClass::Terrestrial) {
            if legend_side {
                let why = match &l.sky {
                    Some(sk) if !sk.physical => "delays fit no far-field direction".to_string(),
                    _ => format!("amplitude differs by station ({:.1}σ)", l.amp_z),
                };
                line(
                    &mut out.surf,
                    format!("✕ {:.2} Hz terrestrial", l.f),
                    style_b(WARN, true, false),
                );
                line(&mut out.surf, format!("  {why}"), style(WARN));
            }
        }
    }

    let mut probe = probe_base;
    probe.marks = marks;
    out.probes.push(Probe::Sky(probe));
    out
}
