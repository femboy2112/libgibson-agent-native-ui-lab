//! Rendering: pure functions from `(State, Engine, size, capability)` to cells.
//!
//! One semantic model (the engine's checkpoints) is read by every representation.
//! Each view draws into its own `Surface` and reports `Probe`s describing exactly
//! what was drawn where, so tests and the inspect cursor can read the geometry back.

pub mod chrome;
pub mod dossier;
pub mod draw;
pub mod fold;
pub mod relation;
pub mod sky;
pub mod spectrum;
pub mod stream;

use crate::analysis::{Checkpoint, Engine};
use crate::scenario::{SigId, CHUNK, FS};
use crate::session::{Focus, Model, State, View};
use draw::*;
use gibson::plot::PlotLayout;
use gibson::{ColorDepth, Rect, SubcellGlyphMode, Surface};
use std::sync::Arc;

pub use draw::PlotProbe;

/// Terminal capability and size the frame is realised for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Env {
    pub width: u16,
    pub height: u16,
    pub depth: ColorDepth,
    pub glyphs: SubcellGlyphMode,
}

impl Env {
    pub fn new(width: u16, height: u16) -> Env {
        Env {
            width,
            height,
            depth: ColorDepth::TrueColor,
            glyphs: SubcellGlyphMode::Braille2x4,
        }
    }
    pub fn mono(mut self) -> Env {
        self.depth = ColorDepth::Mono;
        self
    }
    pub fn is_mono(&self) -> bool {
        self.depth == ColorDepth::Mono
    }
}

/// Hit-testing information for a custom (non-plot) raster region.
#[derive(Clone, Debug)]
pub struct RasterProbe {
    pub name: String,
    pub origin: (u16, u16),
    pub size: (u16, u16),
    /// Data coordinates at the left/right and top/bottom edges of the region.
    pub x_range: (f64, f64),
    pub y_range: (f64, f64),
    pub x_label: &'static str,
    pub y_label: &'static str,
    pub z_label: &'static str,
    /// `z[row][col]` sampled at the drawn resolution.
    pub z: Vec<Vec<f32>>,
}

impl RasterProbe {
    pub fn at(&self, cell: (u16, u16)) -> Option<(f64, f64, f64)> {
        let (cx, cy) = (
            cell.0 as i32 - self.origin.0 as i32,
            cell.1 as i32 - self.origin.1 as i32,
        );
        if cx < 0 || cy < 0 || cx >= self.size.0 as i32 || cy >= self.size.1 as i32 {
            return None;
        }
        let u = (cx as f64 + 0.5) / self.size.0 as f64;
        let v = (cy as f64 + 0.5) / self.size.1 as f64;
        let x = self.x_range.0 + u * (self.x_range.1 - self.x_range.0);
        let y = self.y_range.0 + v * (self.y_range.1 - self.y_range.0);
        let z = self
            .z
            .get(cy as usize)
            .and_then(|r| r.get(cx as usize))
            .copied()
            .unwrap_or(0.0) as f64;
        Some((x, y, z))
    }
}

/// Orthographic sky window: pixel ↔ direction-cosine mapping.
#[derive(Clone, Debug)]
pub struct SkyProbe {
    pub origin: (u16, u16),
    pub size: (u16, u16),
    /// Window centre (l, m) and half-extent (in direction-cosine units, vertical).
    pub center: (f64, f64),
    pub half: f64,
    /// Subpixel scale: pixels per unit direction cosine.
    pub px_per_unit: f64,
    pub center_px: (f64, f64),
    /// Named things drawn on the sky: label, position (l, m), 1σ radius.
    pub marks: Vec<(String, (f64, f64), f64)>,
}

impl SkyProbe {
    pub fn lm_to_px(&self, l: f64, m: f64) -> (f64, f64) {
        (
            self.center_px.0 + (l - self.center.0) * self.px_per_unit,
            self.center_px.1 - (m - self.center.1) * self.px_per_unit,
        )
    }
    pub fn px_to_lm(&self, px: f64, py: f64) -> (f64, f64) {
        (
            self.center.0 + (px - self.center_px.0) / self.px_per_unit,
            self.center.1 - (py - self.center_px.1) / self.px_per_unit,
        )
    }
    pub fn lm_at(&self, cell: (u16, u16)) -> Option<(f64, f64)> {
        let (cx, cy) = (
            cell.0 as i32 - self.origin.0 as i32,
            cell.1 as i32 - self.origin.1 as i32,
        );
        if cx < 0 || cy < 0 || cx >= self.size.0 as i32 || cy >= self.size.1 as i32 {
            return None;
        }
        Some(self.px_to_lm(cx as f64 * 2.0 + 1.0, cy as f64 * 4.0 + 2.0))
    }
}

#[derive(Clone, Debug)]
pub enum Probe {
    Plot(PlotProbe),
    Raster(RasterProbe),
    Sky(SkyProbe),
}

impl Probe {
    fn shift(&mut self, dx: u16, dy: u16) {
        match self {
            Probe::Plot(p) => {
                p.origin.0 += dx;
                p.origin.1 += dy;
            }
            Probe::Raster(r) => {
                r.origin.0 += dx;
                r.origin.1 += dy;
            }
            Probe::Sky(s) => {
                s.origin.0 += dx;
                s.origin.1 += dy;
            }
        }
    }
}

pub struct ViewIn<'a> {
    pub st: &'a State,
    pub eng: &'a Engine,
    pub cp: &'a Checkpoint,
    /// Samples received "now" (the stream view reads raw samples up to here).
    pub n_now: usize,
    pub rect: Rect,
    pub mode: SubcellGlyphMode,
    pub depth: ColorDepth,
    /// The view to draw (normally `st.view`).
    pub view: View,
}

pub struct ViewOut {
    pub surf: Surface,
    pub probes: Vec<Probe>,
}

impl ViewOut {
    pub fn new(rect: Rect) -> ViewOut {
        ViewOut {
            surf: Surface::new(rect.width.max(1), rect.height.max(1)),
            probes: Vec::new(),
        }
    }
}

pub fn draw_view(vin: &ViewIn) -> ViewOut {
    match vin.view {
        View::Stream => stream::draw(vin),
        View::Spectrum => spectrum::draw(vin),
        View::Fold => fold::draw(vin),
        View::Relation => relation::draw(vin),
        View::Sky => sky::draw(vin),
        View::Dossier => dossier::draw(vin),
    }
}

/// Identity colour with selection/focus emphasis.
pub fn id_color(id: SigId, selected: bool, focus: Focus) -> Rgb {
    let c = id.color();
    if focus == Focus::Signal && !selected {
        scale(c, 0.42)
    } else {
        c
    }
}

/// Draw a one-row lane header and return the remaining rectangle for the plot.
pub fn lane(surf: &mut Surface, rect: Rect, spans: &[(String, gibson::Style)]) -> Rect {
    let mut x = rect.x as i32 + 1;
    for (text, st) in spans {
        let room = (rect.x + rect.width) as i32 - x;
        if room <= 0 {
            break;
        }
        put_max(surf, x, rect.y as i32, text, *st, room as u16);
        x += text.chars().count() as i32;
    }
    Rect::new(rect.x, rect.y + 1, rect.width, rect.height.saturating_sub(1))
}

/// Azimuth (deg, from north through east) and elevation (deg) of direction cosines.
pub fn azel(lm: (f64, f64)) -> (f64, f64) {
    let r2 = lm.0 * lm.0 + lm.1 * lm.1;
    if r2 > 1.0 {
        return (lm.0.atan2(lm.1).to_degrees().rem_euclid(360.0), f64::NAN);
    }
    let el = (1.0 - r2).sqrt().asin().to_degrees();
    (lm.0.atan2(lm.1).to_degrees().rem_euclid(360.0), el)
}

// ---------------------------------------------------------------------------
// frame composition
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub width: u16,
    pub hero_h: u16,
    /// Compact identity strip row instead of the side rail?
    pub strip: bool,
    pub rail_w: u16,
    pub main: Rect,
    pub timeline_row: u16,
}

pub fn layout(env: &Env) -> Layout {
    let w = env.width;
    let hero_h = env.height.saturating_sub(2).max(1);
    let strip = w < 60;
    let rail_w = if w >= 110 {
        28
    } else if w >= 78 {
        22
    } else {
        0
    };
    let top = if strip { 1 } else { 0 };
    let tl = hero_h.saturating_sub(1);
    let gap = if rail_w > 0 { 1 } else { 0 };
    let main = Rect::new(
        rail_w + gap,
        top,
        w.saturating_sub(rail_w + gap),
        tl.saturating_sub(top),
    );
    Layout {
        width: w,
        hero_h,
        strip,
        rail_w,
        main,
        timeline_row: tl,
    }
}

pub struct Frame {
    pub status: Surface,
    pub hero: Surface,
    pub hint: Surface,
    pub probes: Vec<Probe>,
    pub readout: Option<String>,
    pub layout: Layout,
}

/// Compose the whole frame for a model at a size/capability. Pure.
pub fn compose(model: &Model, env: &Env) -> Frame {
    let st = &model.st;
    let lay = layout(env);
    let cp = model.cp();
    let mut hero = Surface::new(lay.width.max(1), lay.hero_h.max(1));
    let mut probes: Vec<Probe> = Vec::new();

    // ---- main region (one view, or a compare split) ----
    let main = lay.main;
    let draw_one = |view: View, cp: &Checkpoint, n: usize, rect: Rect| -> ViewOut {
        draw_view(&ViewIn {
            st,
            eng: &model.engine,
            cp,
            n_now: n,
            rect: Rect::new(0, 0, rect.width, rect.height),
            mode: env.glyphs,
            depth: env.depth,
            view,
        })
    };
    if st.compare && main.width >= 8 && main.height >= 6 {
        let pin_n = st.pin.unwrap_or(0);
        let pin_cp = model.engine.checkpoint_at_sample(pin_n);
        let horizontal = main.width >= 100;
        let (ra, rb) = if horizontal {
            let wa = main.width / 2;
            (
                Rect::new(main.x, main.y, wa, main.height),
                Rect::new(main.x + wa, main.y, main.width - wa, main.height),
            )
        } else {
            let ha = main.height / 2;
            (
                Rect::new(main.x, main.y, main.width, ha),
                Rect::new(main.x, main.y + ha, main.width, main.height - ha),
            )
        };
        for (i, (rect, label, ccp, n)) in [
            (ra, "EARLIER", pin_cp.clone(), pin_n),
            (rb, "NOW", cp.clone(), st.n),
        ]
        .into_iter()
        .enumerate()
        {
            let head = format!(
                "{} {} t={:.0}s · {}",
                if i == 0 { "◀" } else { "▶" },
                label,
                n as f64 / FS,
                stage_line(&ccp, st.selected)
            );
            let hr = Rect::new(rect.x, rect.y, rect.width, 1);
            put_max(
                &mut hero,
                hr.x as i32,
                hr.y as i32,
                &format!(" {head}"),
                style_b(if i == 0 { WARN } else { GOOD }, true, false),
                hr.width,
            );
            let body = Rect::new(rect.x, rect.y + 1, rect.width, rect.height.saturating_sub(1));
            let mut o = draw_one(st.view, &ccp, n, body);
            blit(&mut hero, &o.surf, body.x, body.y);
            for p in o.probes.iter_mut() {
                p.shift(body.x, body.y);
            }
            probes.extend(o.probes);
            // divider
            if horizontal && i == 1 {
                for y in rect.y..rect.y + rect.height {
                    put(&mut hero, rect.x as i32, y as i32, "│", style(FAINT));
                }
            }
        }
    } else if main.width >= 4 && main.height >= 2 {
        let mut o = draw_one(st.view, &cp, st.n, main);
        if let Some((from, k)) = st.xfade {
            let old = draw_one(from, &cp, st.n, main);
            let p = (k as f64 + 1.0) / crate::session::XFADE_FRAMES as f64;
            dissolve(&mut o.surf, &old.surf, p);
        }
        blit(&mut hero, &o.surf, main.x, main.y);
        for p in o.probes.iter_mut() {
            p.shift(main.x, main.y);
        }
        probes.extend(o.probes);
    }

    // ---- identity rail / strip, timeline ----
    chrome::draw_identity(&mut hero, &lay, &cp, st, env);
    chrome::draw_timeline(&mut hero, &lay, model, &cp, env);

    // ---- inspect cursor ----
    let mut readout = None;
    if let Some((u, v)) = st.cursor {
        if main.width > 0 && main.height > 0 {
            let cx = main.x + ((u * main.width as f32) as u16).min(main.width - 1);
            let cy = main.y + ((v * main.height as f32) as u16).min(main.height - 1);
            readout = chrome::readout_at(&probes, (cx, cy), st);
            put(&mut hero, cx as i32, cy as i32, "┼", style_b(INK, true, false));
        }
    }

    let status = chrome::status_row(model, &cp, env);
    let hint = chrome::hint_row(model, env, readout.as_deref());
    Frame {
        status,
        hero,
        hint,
        probes,
        readout,
        layout: lay,
    }
}

/// One-line stage summary of the identities at a checkpoint.
pub fn stage_line(cp: &Checkpoint, sel: SigId) -> String {
    let p = cp.signal(sel);
    format!("{} {}", sel.name(), p.stage.label())
}

/// Hash-ordered cell dissolve from `old` to `new`: progress `p ∈ [0,1]`.
pub fn dissolve(new: &mut Surface, old: &Surface, p: f64) {
    for y in 0..new.height.min(old.height) {
        for x in 0..new.width.min(old.width) {
            let h = crate::rng::hash3(x as u64, y as u64, 0xD155_017E);
            let u = crate::rng::unit(h);
            if u >= p {
                if let Some(c) = old.get(x, y) {
                    if !c.is_continuation {
                        let cell = c.clone();
                        new.set_cell(x, y, cell);
                    }
                }
            }
        }
    }
}

/// The checkpoint nearest to "now" (convenience for tests).
pub fn checkpoint_for(model: &Model) -> Arc<Checkpoint> {
    model.cp()
}

pub fn layout_of(l: &PlotLayout) -> String {
    format!("{l:?}")
}

pub const CHUNK_S: f64 = CHUNK as f64 / FS;
