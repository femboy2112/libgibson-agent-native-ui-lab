//! SKY: inferred source geometry. Custom half-block graphics (a plot cannot
//! express a sphere): an all-sky Hammer projection, or a local tangent view
//! when zoomed. Each locked identity is a posterior glow whose width is the
//! timing uncertainty; a planar array cannot tell a source from its mirror
//! image, so every glow has a ghost twin until station 4 collapses the pair.

use gibson::canvas::BrailleCanvas;
use gibson::{Cell, Color, Glyph, Rect, Style, Surface};

use super::{active_slots, effective_selection, Anchor, Ctx, ViewOut};
use crate::identity;
use crate::rng::Rng;
use crate::sim::{separation_deg, SLOTS};
use crate::theme::*;
use crate::track::{SkyFix, State};

const SKY_FILL: Rgb = (8, 11, 24);

#[derive(Clone, Copy, Debug)]
pub struct Geom {
    pub cx: f64,
    pub cy: f64,
    pub ew: f64,
    pub eh: f64,
    pub local: Option<(f64, f64, f64)>, // az0, el0, deg per pixel
}

fn wrap180(a: f64) -> f64 {
    let mut x = (a + 180.0).rem_euclid(360.0) - 180.0;
    if x <= -180.0 {
        x += 360.0;
    }
    x
}

impl Geom {
    pub fn all_sky(pw: f64, ph: f64) -> Geom {
        let ew = (pw - 2.0).min(2.0 * (ph - 2.0)).max(8.0);
        Geom {
            cx: pw / 2.0,
            cy: ph / 2.0,
            ew,
            eh: ew / 2.0,
            local: None,
        }
    }
    pub fn local(pw: f64, ph: f64, az0: f64, el0: f64, half_deg: f64) -> Geom {
        Geom {
            cx: pw / 2.0,
            cy: ph / 2.0,
            ew: pw,
            eh: ph,
            local: Some((az0, el0, 2.0 * half_deg / ph)),
        }
    }
    pub fn project(&self, az: f64, el: f64) -> Option<(f64, f64)> {
        if let Some((az0, el0, dpp)) = self.local {
            let dx = wrap180(az - az0) * el0.to_radians().cos().max(0.2);
            let dy = el - el0;
            return Some((self.cx + dx / dpp, self.cy - dy / dpp));
        }
        let lam = wrap180(az - 180.0).to_radians();
        let phi = el.to_radians();
        let d = (1.0 + phi.cos() * (lam / 2.0).cos()).sqrt();
        let x = 2.0 * std::f64::consts::SQRT_2 * phi.cos() * (lam / 2.0).sin() / d;
        let y = std::f64::consts::SQRT_2 * phi.sin() / d;
        Some((
            self.cx + x / (2.0 * std::f64::consts::SQRT_2) * (self.ew / 2.0),
            self.cy - y / std::f64::consts::SQRT_2 * (self.eh / 2.0),
        ))
    }
    pub fn unproject(&self, px: f64, py: f64) -> Option<(f64, f64)> {
        if let Some((az0, el0, dpp)) = self.local {
            let el = el0 - (py - self.cy) * dpp;
            if !(-90.0..=90.0).contains(&el) {
                return None;
            }
            let az = az0 + (px - self.cx) * dpp / el0.to_radians().cos().max(0.2);
            return Some((az.rem_euclid(360.0), el));
        }
        let xx = (px - self.cx) / (self.ew / 2.0) * 2.0 * std::f64::consts::SQRT_2;
        let yy = -(py - self.cy) / (self.eh / 2.0) * std::f64::consts::SQRT_2;
        let q = (xx / 4.0).powi(2) + (yy / 2.0).powi(2);
        // the Hammer ellipse is the region z^2 = 1 - q >= 1/2
        if q > 0.5 {
            return None;
        }
        let z = (1.0 - q).sqrt();
        let lam = 2.0 * (z * xx).atan2(2.0 * (2.0 * z * z - 1.0));
        let phi = (z * yy).clamp(-1.0, 1.0).asin();
        Some((
            (lam.to_degrees() + 180.0).rem_euclid(360.0),
            phi.to_degrees(),
        ))
    }
    fn inside(&self, px: f64, py: f64) -> bool {
        if self.local.is_some() {
            return true;
        }
        let a = (px - self.cx) / (self.ew / 2.0);
        let b = (py - self.cy) / (self.eh / 2.0);
        a * a + b * b <= 1.0
    }
}

fn dir(az: f64, el: f64) -> (f64, f64) {
    let (a, e) = (az.to_radians(), el.to_radians());
    (e.cos() * a.cos(), e.cos() * a.sin())
}

/// Sample the boundary of the `k`-sigma ellipse of a sky fix as (az, el) points
/// in the hemisphere `sign` (+1 / -1).
pub fn ellipse_on_sky(fix: &SkyFix, k: f64, sign: f64, n: usize) -> Vec<(f64, f64)> {
    let (vx, vy, cxy) = (fix.cov[0], fix.cov[1], fix.cov[2]);
    let a = vx.max(1e-10).sqrt();
    let b = cxy / a;
    let c = (vy - b * b).max(1e-10).sqrt();
    (0..=n)
        .filter_map(|i| {
            let th = std::f64::consts::TAU * i as f64 / n as f64;
            let (ct, stt) = (th.cos(), th.sin());
            let sx = fix.sx + k * a * ct;
            let sy = fix.sy + k * (b * ct + c * stt);
            let r2 = sx * sx + sy * sy;
            if r2 >= 1.0 {
                return None;
            }
            let sz = (1.0 - r2).sqrt();
            let az = sy.atan2(sx).to_degrees().rem_euclid(360.0);
            Some((az, sign * sz.asin().to_degrees()))
        })
        .collect()
}

fn great_circle(a: (f64, f64), b: (f64, f64), n: usize) -> Vec<(f64, f64)> {
    let to_v = |az: f64, el: f64| {
        let (x, y) = dir(az, el);
        (x, y, el.to_radians().sin())
    };
    let (u, v) = (to_v(a.0, a.1), to_v(b.0, b.1));
    let dot = (u.0 * v.0 + u.1 * v.1 + u.2 * v.2).clamp(-1.0, 1.0);
    let om = dot.acos();
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let (s0, s1) = if om.abs() < 1e-6 {
                (1.0 - t, t)
            } else {
                (((1.0 - t) * om).sin() / om.sin(), (t * om).sin() / om.sin())
            };
            let p = (
                s0 * u.0 + s1 * v.0,
                s0 * u.1 + s1 * v.1,
                s0 * u.2 + s1 * v.2,
            );
            (
                p.1.atan2(p.0).to_degrees().rem_euclid(360.0),
                p.2.clamp(-1.0, 1.0).asin().to_degrees(),
            )
        })
        .collect()
}

struct Blob {
    slot: usize,
    fix: SkyFix,
    amp: f32,
    state: State,
    inv: [f64; 3], // inverse covariance (xx, yy, xy)
}

fn blob_for(ctx: &Ctx, ep: usize, slot: usize) -> Option<Blob> {
    let e = ctx.obs.timeline.epoch(ep)?;
    let sl = e.slots[slot];
    let fix = sl.sky?;
    if !sl.state.is_active() {
        return None;
    }
    let floor = 0.012f64.powi(2);
    let (vx, vy, cxy) = (fix.cov[0] + floor, fix.cov[1] + floor, fix.cov[2]);
    let det = (vx * vy - cxy * cxy).max(1e-14);
    let sig_eff = det.sqrt().sqrt();
    let mut amp = (0.25 / sig_eff).clamp(0.12, 1.0) as f32;
    amp *= ((sl.z / 8.0).clamp(0.3, 1.0)) as f32;
    if sl.state.is_rejected() {
        amp *= 0.45;
    }
    Some(Blob {
        slot,
        fix,
        amp,
        state: sl.state,
        inv: [vy / det, vx / det, -cxy / det],
    })
}

/// A Braille line-art layer with one colour (dot resolution 2x4 per cell).
struct Layer {
    canvas: BrailleCanvas,
    rgb: Rgb,
}

impl Layer {
    fn new(w: u16, h: u16, rgb: Rgb) -> Layer {
        Layer {
            canvas: BrailleCanvas::new(w, h),
            rgb,
        }
    }

    /// Stroke a polyline of (az, el) points through `geom`; `dotted` strokes every third dot.
    fn stroke(&mut self, geom: &Geom, pts: &[(f64, f64)], dotted: bool) {
        let mut prev: Option<(f64, f64)> = None;
        let mut n = 0usize;
        for &(az, el) in pts {
            if let Some((x, y)) = geom.project(az, el) {
                if let Some((x0, y0)) = prev {
                    if (x - x0).abs() < geom.ew * 0.6 && (y - y0).abs() < geom.eh * 1.2 {
                        if dotted {
                            let steps =
                                ((x - x0).abs().max((y - y0).abs())).ceil().max(1.0) as usize;
                            for i in 0..=steps {
                                n += 1;
                                if n.is_multiple_of(4) {
                                    let t = i as f64 / steps as f64;
                                    self.canvas.set(
                                        (x0 + (x - x0) * t).round() as i32,
                                        (y0 + (y - y0) * t).round() as i32,
                                    );
                                }
                            }
                        } else {
                            self.canvas.line(
                                x0.round() as i32,
                                y0.round() as i32,
                                x.round() as i32,
                                y.round() as i32,
                            );
                        }
                    }
                }
                prev = Some((x, y));
            } else {
                prev = None;
            }
        }
    }
}

const SHADES: [&str; 4] = ["\u{2591}", "\u{2592}", "\u{2593}", "\u{2588}"];

pub fn render(ctx: &Ctx, rect: Rect) -> ViewOut {
    let (w, h) = (rect.width, rect.height);
    let mut surf = Surface::new(w, h);
    let title = h >= 13;
    let sky_y: u16 = if title { 1 } else { 0 };
    let sky_h = h.saturating_sub(sky_y + 1).max(1);
    // dot space: braille is 2 x 4 dots per cell and its dots are ~square
    let (pw, ph) = (w as f64 * 2.0, sky_h as f64 * 4.0);
    let epoch = ctx.obs.timeline.epoch(ctx.ep);
    let sel = effective_selection(ctx);
    let slots = active_slots(ctx);

    let sel_pos = sel
        .and_then(|s| epoch.and_then(|e| e.slots[s].sky))
        .map(|f| f.best());
    let geom = match (ctx.zoom, sel_pos) {
        (0, _) | (_, None) => Geom::all_sky(pw, ph),
        (1, Some(p)) => Geom::local(pw, ph, p.0, p.1, 45.0),
        (_, Some(p)) => Geom::local(pw, ph, p.0, p.1, 12.0),
    };
    let cell_dot = |cx: u16, cy: u16| (cx as f64 * 2.0 + 1.0, cy as f64 * 4.0 + 2.0);

    // ── layers: line art in Braille, one colour each ──
    let grid_rgb = (44, 56, 84);
    let mut grid = Layer::new(w, sky_h, grid_rgb);
    let mut plane = Layer::new(w, sky_h, (78, 104, 150));
    let (az_step, el_step) = match (geom.local, ctx.zoom) {
        (None, _) => (60.0, 30.0),
        (Some(_), 1) => (10.0, 10.0),
        _ => (4.0, 4.0),
    };
    let mut az = 0.0;
    while az < 360.0 {
        let pts: Vec<(f64, f64)> = (-90..=90).step_by(2).map(|e| (az, e as f64)).collect();
        grid.stroke(&geom, &pts, true);
        az += az_step;
    }
    let mut el: f64 = -90.0 + el_step;
    while el < 90.0 {
        if el.abs() > 1e-9 {
            let pts: Vec<(f64, f64)> = (0..=360).step_by(2).map(|a| (a as f64, el)).collect();
            grid.stroke(&geom, &pts, true);
        }
        el += el_step;
    }
    // the array plane (el = 0): the great circle a planar array cannot tell sides of
    let eq: Vec<(f64, f64)> = (0..=360).step_by(2).map(|a| (a as f64, 0.0)).collect();
    plane.stroke(&geom, &eq, false);
    if geom.local.is_none() {
        for i in 0..1440 {
            let a = std::f64::consts::TAU * i as f64 / 1440.0;
            plane.canvas.set(
                (geom.cx + a.cos() * geom.ew / 2.0).round() as i32,
                (geom.cy + a.sin() * geom.eh / 2.0).round() as i32,
            );
        }
    }
    let mut layers: Vec<Layer> = vec![grid, plane];

    // ── posterior glow per cell (shade ramp, identity colour) ──
    let blobs: Vec<Blob> = slots
        .iter()
        .filter_map(|s| blob_for(ctx, ctx.ep, *s))
        .collect();
    let mut glow: Vec<(f32, (f32, f32, f32))> =
        vec![(0.0, (0.0, 0.0, 0.0)); w as usize * sky_h as usize];
    for cy in 0..sky_h {
        for cx in 0..w {
            let (dx_, dy_) = cell_dot(cx, cy);
            let Some((az, el)) = geom.unproject(dx_, dy_) else {
                continue;
            };
            let (sx, sy) = dir(az, el);
            let mut total = 0.0f32;
            let mut col = (0.0f32, 0.0f32, 0.0f32);
            for b in &blobs {
                let (dx, dy) = (sx - b.fix.sx, sy - b.fix.sy);
                let d2 = b.inv[0] * dx * dx + b.inv[1] * dy * dy + 2.0 * b.inv[2] * dx * dy;
                if d2 > 18.0 {
                    continue;
                }
                let wgt = if el >= 0.0 {
                    b.fix.w_plus
                } else {
                    1.0 - b.fix.w_plus
                } as f32;
                let a = b.amp * wgt * (-0.5 * d2 as f32).exp();
                total += a;
                let (r, g, bb) = identity::IDENT[b.slot].rgb;
                col.0 += r as f32 * a;
                col.1 += g as f32 * a;
                col.2 += bb as f32 * a;
            }
            if total > 0.0 {
                glow[cy as usize * w as usize + cx as usize] =
                    (total, (col.0 / total, col.1 / total, col.2 / total));
            }
        }
    }

    // ── 1/2-sigma contours of both mirror images, weighted ──
    for b in &blobs {
        let col = identity::rgb(b.slot, b.state);
        for (sign, wgt) in [(1.0, b.fix.w_plus), (-1.0, 1.0 - b.fix.w_plus)] {
            if wgt < 0.04 {
                continue;
            }
            let c = scale(col, (0.5 + 0.5 * wgt) as f32);
            let mut one = Layer::new(w, sky_h, c);
            one.stroke(&geom, &ellipse_on_sky(&b.fix, 1.0, sign, 40), wgt < 0.5);
            let mut two = Layer::new(w, sky_h, scale(c, 0.55));
            two.stroke(&geom, &ellipse_on_sky(&b.fix, 2.0, sign, 40), true);
            layers.push(two);
            layers.push(one);
        }
    }
    // earlier uncertain state (compare)
    if let Some(r) = ctx.ref_ep.filter(|r| *r >= 1 && *r < ctx.ep) {
        let mut ghost = Layer::new(w, sky_h, (176, 184, 204));
        for &s in &slots {
            if let Some(bl) = blob_for(ctx, r, s) {
                for (sign, wgt) in [(1.0, bl.fix.w_plus), (-1.0, 1.0 - bl.fix.w_plus)] {
                    if wgt >= 0.1 {
                        ghost.stroke(&geom, &ellipse_on_sky(&bl.fix, 2.0, sign, 40), true);
                    }
                }
            }
        }
        layers.push(ghost);
    }

    // ── the final interpretation: arcs between resolved, locked sources ──
    let locked: Vec<(usize, (f64, f64))> = slots
        .iter()
        .filter_map(|s| {
            let e = epoch?;
            let sl = e.slots[*s];
            (sl.state == State::Locked)
                .then(|| {
                    sl.sky
                        .filter(|k| k.resolved_by_s4 && k.confidence_of_best() > 0.9)
                })
                .flatten()
                .map(|k| (*s, k.best()))
        })
        .collect();
    let mut seps = Vec::new();
    if locked.len() >= 2 {
        let mut arcs = Layer::new(w, sky_h, (150, 162, 196));
        for i in 0..locked.len() {
            for j in i + 1..locked.len() {
                arcs.stroke(&geom, &great_circle(locked[i].1, locked[j].1, 48), true);
                seps.push((
                    locked[i].0,
                    locked[j].0,
                    separation_deg(locked[i].1, locked[j].1),
                ));
            }
        }
        layers.push(arcs);
    }

    // ── compose cells: fill, stars, glow, then line art ──
    let mut rng = Rng::new(0x57A2_0001);
    let mut stars: std::collections::HashMap<(u16, u16), (&'static str, u8)> =
        std::collections::HashMap::new();
    for _ in 0..(if geom.local.is_some() { 300 } else { 170 }) {
        let az = rng.uniform() * 360.0;
        let el = (2.0 * rng.uniform() - 1.0).asin().to_degrees();
        let b = rng.uniform();
        if let Some((x, y)) = geom.project(az, el) {
            if x >= 0.0 && y >= 0.0 && geom.inside(x, y) {
                let (cx, cy) = ((x / 2.0) as u16, (y / 4.0) as u16);
                if cx < w && cy < sky_h {
                    let g = if b > 0.93 {
                        "+"
                    } else if b > 0.6 {
                        "\u{00B7}"
                    } else {
                        "."
                    };
                    stars.insert((cx, cy), (g, (60.0 + 120.0 * b) as u8));
                }
            }
        }
    }
    for cy in 0..sky_h {
        for cx in 0..w {
            let (dx_, dy_) = cell_dot(cx, cy);
            let inside = geom.inside(dx_, dy_);
            let mut glyph: String = " ".into();
            let mut style = Style::new();
            if inside && !ctx.mono {
                style.bg = Some(Color::rgb(SKY_FILL.0, SKY_FILL.1, SKY_FILL.2));
            }
            if inside {
                if let Some((g, v)) = stars.get(&(cx, cy)) {
                    glyph = (*g).to_string();
                    style = style.fg(Color::rgb(*v, *v, (*v as f32 * 1.12).min(255.0) as u8));
                }
            }
            let (tot, col) = glow[cy as usize * w as usize + cx as usize];
            if tot > 0.10 {
                let lvl = if tot > 0.85 {
                    3
                } else if tot > 0.55 {
                    2
                } else if tot > 0.28 {
                    1
                } else {
                    0
                };
                glyph = SHADES[lvl].to_string();
                let k = (0.55 + 0.45 * tot.min(1.0)).min(1.0);
                style = style.fg(Color::rgb(
                    (col.0 * k).min(255.0) as u8,
                    (col.1 * k).min(255.0) as u8,
                    (col.2 * k).min(255.0) as u8,
                ));
                if !ctx.mono {
                    style.bg = Some(Color::rgb(
                        (col.0 * 0.22 * tot.min(1.0)) as u8 + SKY_FILL.0,
                        (col.1 * 0.22 * tot.min(1.0)) as u8 + SKY_FILL.1,
                        (col.2 * 0.22 * tot.min(1.0)) as u8 + SKY_FILL.2,
                    ));
                }
            }
            for l in &layers {
                if let Some(g) = l.canvas.glyph_at_mode(cx, cy, ctx.glyphs) {
                    glyph = g.to_string();
                    style = style.fg(Color::rgb(l.rgb.0, l.rgb.1, l.rgb.2));
                }
            }
            if glyph != " " || style.bg.is_some() {
                surf.set_cell(cx, sky_y + cy, Cell::new(Glyph::new(&glyph), style));
            }
        }
    }

    // ── identity markers ──
    let mut anchors = Vec::new();
    let cell_of = |az: f64, el: f64| -> Option<(i32, i32)> {
        let (x, y) = geom.project(az, el)?;
        Some((
            (x / 2.0).floor() as i32,
            sky_y as i32 + (y / 4.0).floor() as i32,
        ))
    };
    for &s in &slots {
        let Some(e) = epoch else { break };
        let sl = e.slots[s];
        let (ax, ay, placed) = match sl.sky.and_then(|k| cell_of(k.best().0, k.best().1)) {
            Some((x, y))
                if x >= 0 && x < w as i32 && y >= sky_y as i32 && y < (sky_y + sky_h) as i32 =>
            {
                (x, y, true)
            }
            _ => (1, sky_y as i32 + 1, false),
        };
        let tagged = identity::tag(s, sl.state);
        let sty = identity::style(s, sl.state);
        let is_sel = sel == Some(s);
        if placed {
            if is_sel {
                put(&mut surf, ax - 1, ay, "[", st(INK));
                put(&mut surf, ax, ay, &tagged, sty);
                put(
                    &mut surf,
                    ax + tagged.chars().count() as i32,
                    ay,
                    "]",
                    st(INK),
                );
            } else {
                put(&mut surf, ax, ay, &tagged, sty);
            }
            if let Some(k) = sl.sky {
                let wo = 1.0 - k.confidence_of_best();
                let (gaz, gel) = (k.best().0, -k.best().1);
                if wo >= 0.04 {
                    if let Some((gx, gy)) = cell_of(gaz, gel) {
                        if gx >= 0
                            && gx < w as i32 - 6
                            && gy >= sky_y as i32
                            && gy < (sky_y + sky_h) as i32
                        {
                            let ghost = format!(
                                "{}{}\u{00B7}{:.0}%",
                                identity::IDENT[s].hollow,
                                identity::IDENT[s].letter,
                                wo * 100.0
                            );
                            put(
                                &mut surf,
                                gx,
                                gy,
                                &ghost,
                                st(scale(identity::rgb(s, State::Candidate), 0.95)),
                            );
                        }
                    }
                }
            }
        } else {
            put(&mut surf, ax, ay, &tagged, sty);
        }
        anchors.push(Anchor {
            slot: s,
            x: ax,
            y: ay,
        });
    }

    // ── captions & the interpretation, only where they cannot collide with the sphere ──
    if title {
        let note = super::compare_note(ctx, sel);
        let view_name = if geom.local.is_some() {
            "(local view)"
        } else {
            "(all-sky, Hammer)"
        };
        let head = if note.is_some() {
            format!("\u{25B4} sky {view_name}")
        } else {
            format!(
                "\u{25B4} sky {view_name} \u{00B7} az 0\u{00B0}\u{2192}360\u{00B0}  el \u{00B1}90\u{00B0}  (dotted = array plane)"
            )
        };
        put_clipped(&mut surf, 0, 0, &head, st(SOFT), w as usize);
        if let Some(note) = note {
            let n = note.chars().count() as i32;
            if (w as i32) > head.chars().count() as i32 + n + 2 {
                put(&mut surf, w as i32 - n, 0, &note, st(WARN));
            }
        }
    }
    if geom.local.is_none() && !locked.is_empty() {
        // left corner: the geometry; right corner: the verdict
        let mut left: Vec<String> = vec!["INTERPRETATION".into()];
        for (a, b, d) in &seps {
            left.push(format!(
                "{}\u{2013}{} {:.1}\u{00B0}",
                identity::IDENT[*a].letter,
                identity::IDENT[*b].letter,
                d
            ));
        }
        let mut right: Vec<String> = vec![format!(
            "{} persistent source{} localised",
            locked.len(),
            if locked.len() == 1 { "" } else { "s" }
        )];
        if let Some(e) = epoch {
            for s in 0..SLOTS {
                match e.slots[s].state {
                    State::RejectedCw => right.push(format!(
                        "{} rejected: local CW interference",
                        identity::IDENT[s].letter
                    )),
                    State::RejectedTransient => right.push(format!(
                        "{} rejected: transient, not persistent",
                        identity::IDENT[s].letter
                    )),
                    _ => {}
                }
            }
        }
        let fits = |i: usize, len: usize, from_right: bool| -> bool {
            let dy = (i as f64) * 4.0 + 2.0;
            let half = (geom.ew / 2.0)
                * (1.0 - ((dy - geom.cy) / (geom.eh / 2.0)).powi(2))
                    .max(0.0)
                    .sqrt();
            let room = if from_right {
                ((pw - geom.cx - half) / 2.0 - 1.0).floor()
            } else {
                ((geom.cx - half) / 2.0 - 1.0).floor()
            };
            room >= len as f64
        };
        for (i, l) in left.iter().enumerate() {
            if fits(i, l.chars().count(), false) {
                put(
                    &mut surf,
                    0,
                    sky_y as i32 + i as i32,
                    l,
                    st(if i == 0 { INK } else { SOFT }),
                );
            }
        }
        for (i, l) in right.iter().enumerate() {
            let n = l.chars().count();
            if fits(i, n, true) {
                put(
                    &mut surf,
                    w as i32 - n as i32,
                    sky_y as i32 + i as i32,
                    l,
                    st(if i == 0 { GOOD } else { SOFT }),
                );
            }
        }
    }

    // ── cursor & readout ──
    let mut cursor_note = String::new();
    let readout = if let Some((u, v)) = ctx.cursor {
        let (cxp, cyp) = (u * pw, (1.0 - v) * ph);
        let ccx = ((cxp / 2.0) as i32).clamp(0, w as i32 - 1);
        let ccy =
            (sky_y as i32 + (cyp / 4.0) as i32).clamp(sky_y as i32, (sky_y + sky_h) as i32 - 1);
        put(&mut surf, ccx, ccy, "+", st_bold(WARN));
        match geom.unproject(cxp, cyp) {
            Some((az, el)) => {
                let el = if el.abs() < 0.05 { 0.0 } else { el };
                let mut best: Option<(usize, f64)> = None;
                for &s in &slots {
                    if let Some(k) = epoch.and_then(|e| e.slots[s].sky) {
                        let d = separation_deg((az, el), k.best());
                        if best.map(|b| d < b.1).unwrap_or(true) {
                            best = Some((s, d));
                        }
                    }
                }
                if let Some((s, d)) = best {
                    let st_ = epoch.map(|e| e.slots[s].state).unwrap_or(State::Quiet);
                    cursor_note = format!("   nearest {} {:.1}\u{00B0}", identity::tag(s, st_), d);
                }
                format!("az {:.1}\u{00B0}  el {:+.1}\u{00B0}{}", az, el, cursor_note)
            }
            None => "outside the sky".to_string(),
        }
    } else {
        match (sel, epoch) {
            (Some(s), Some(e)) => {
                let sl = e.slots[s];
                match sl.sky {
                    Some(k) => {
                        let (az, el) = k.best();
                        format!(
                            "{} {}  az {:.1}\u{00B0} el {:+.1}\u{00B0} \u{00B1}{:.1}\u{00B0}   mirror {:.0}/{:.0}%{}",
                            identity::tag(s, sl.state),
                            sl.state.label(),
                            az,
                            el,
                            k.sigma_deg,
                            k.w_plus * 100.0,
                            (1.0 - k.w_plus) * 100.0,
                            if k.resolved_by_s4 { "  (S4)" } else { "  (planar: ambiguous)" }
                        )
                    }
                    None => format!("{} no sky solution yet", identity::tag(s, sl.state)),
                }
            }
            _ => "no localisable candidate yet".to_string(),
        }
    };
    put_clipped(&mut surf, 0, h as i32 - 1, &readout, st(INK), w as usize);
    let _ = SLOTS;
    ViewOut {
        surface: surf,
        anchors,
        readout,
        axes: vec![],
        reports: vec![],
    }
}
