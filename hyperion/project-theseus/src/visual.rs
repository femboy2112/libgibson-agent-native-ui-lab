//! Project Theseus — the visual machine.
//!
//! Every pixel here is computed from the *quotient state* and the *real cover
//! score*, never from a hidden copy of the reference. The centrepiece is an RGB
//! software raster (`RgbRaster`) post-processed by LibGibson's `RasterFx` chain
//! and realized to cells through `to_surface` / `to_mono_surface`.
//!
//! Design law: a preserved axis is a continuous, pulsing ring with a rotating
//! scan and a live beam to the core; a removed axis is a fractured, gapped ring
//! whose beam is gone. As axes are removed the core loses coherence and the
//! chromatic split / sine warp grow. There is no separate "hidden state".

use gibson::capability::ColorDepth;
use gibson::cell::{Cell, Color, Glyph, Style};
use gibson::field::plasma;
use gibson::glyph::{transcode_surface_glyphs, SubcellGlyphMode};
use gibson::particles::Particle;
use gibson::raster::RgbRaster;
use gibson::raster_fx::{metaballs, radial_glow};
use gibson::raster_fx::RasterFx;
use gibson::surface::{BorderType, Rect, Surface};

pub type Rgb = (u8, u8, u8);

/// Glyph family requested for realization. `Auto`/`HalfBlock` keep the vivid
/// RGB half-block path; the others degrade the *shape* channel while leaving the
/// colour axis to the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphMode {
    HalfBlock,
    Braille,
    Block,
    Ascii,
}

impl GlyphMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" | "half" | "halfblock" | "half-block" => Some(GlyphMode::HalfBlock),
            "braille" | "braille2x4" => Some(GlyphMode::Braille),
            "block" | "blocks" | "shade" => Some(GlyphMode::Block),
            "ascii" | "text" => Some(GlyphMode::Ascii),
            _ => None,
        }
    }

    pub fn subcell(self) -> SubcellGlyphMode {
        match self {
            GlyphMode::HalfBlock => SubcellGlyphMode::HalfBlock1x2,
            GlyphMode::Braille => SubcellGlyphMode::Braille2x4,
            GlyphMode::Block => SubcellGlyphMode::Block,
            GlyphMode::Ascii => SubcellGlyphMode::Ascii,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GlyphMode::HalfBlock => "halfblock",
            GlyphMode::Braille => "braille",
            GlyphMode::Block => "block",
            GlyphMode::Ascii => "ascii",
        }
    }
}

impl Default for GlyphMode {
    fn default() -> Self {
        GlyphMode::HalfBlock
    }
}

const TAU: f32 = std::f32::consts::TAU;
const PI: f32 = std::f32::consts::PI;

pub const AXIS_NAMES: [&str; 8] = [
    "MOTIF",
    "RIFF",
    "GROOVE",
    "HARM CONTOUR",
    "HARM LOOP",
    "FORM",
    "ORCHESTRATION",
    "BASS FIGURE",
];

pub const AXIS_DESC: [&str; 8] = [
    "lead identity line, exact relative pitches",
    "secondary hook line on the selected lane",
    "kick/snare pocket anchors",
    "root+quality sequence and metric landmarks",
    "cadence cycle / loop chart",
    "phrase bar spans and family values",
    "per-bar arrangement role vector",
    "sub-root bass line, own-octave normalized",
];

/// Distinct, vivid per-axis hues (cyan → magenta → amber → green → teal →
/// violet → rose → blue).
pub const AXIS_COLORS: [Rgb; 8] = [
    (0, 240, 255),
    (255, 60, 200),
    (255, 190, 40),
    (80, 255, 120),
    (60, 230, 200),
    (170, 120, 255),
    (255, 90, 120),
    (90, 140, 255),
];

#[derive(Clone, Copy, Debug)]
pub struct WorldPalette {
    pub name: &'static str,
    pub bg: Rgb,
    pub a: Rgb,
    pub b: Rgb,
    pub c: Rgb,
    pub accent: Rgb,
}

pub fn palette_for(name: &str) -> WorldPalette {
    match name {
        "BLACK_ICE" => WorldPalette {
            name: "BLACK_ICE",
            bg: (4, 7, 14),
            a: (0, 170, 255),
            b: (120, 230, 255),
            c: (200, 255, 255),
            accent: (90, 255, 160),
        },
        "SWISS_SIGNAL" => WorldPalette {
            name: "SWISS_SIGNAL",
            bg: (12, 12, 12),
            a: (240, 20, 45),
            b: (255, 255, 255),
            c: (190, 190, 190),
            accent: (255, 214, 0),
        },
        _ => WorldPalette {
            name: "VAPOR95",
            bg: (16, 6, 34),
            a: (255, 60, 180),
            b: (90, 130, 255),
            c: (60, 255, 220),
            accent: (255, 184, 60),
        },
    }
}

#[derive(Clone, Debug)]
pub struct VisualParams {
    pub axes_on: [bool; 8],
    /// Known/Invariant per axis (false = Unknown/Free).
    pub axis_known: [bool; 8],
    /// Machine conformance per axis (None = not applicable).
    pub axis_conformance: [Option<bool>; 8],
    /// Axes involved in a refusal.
    pub refusal_axes: [bool; 8],
    pub refusal: bool,
    pub time: f32,
    pub seed: u64,
    pub palette: WorldPalette,
    pub color_depth: ColorDepth,
    pub glyph: GlyphMode,
    pub particles: Vec<Particle>,
    pub cover_notes: usize,
    pub source_notes: usize,
    pub cover_bars: Vec<(f32, f32, i32)>,
    pub source_bars: Vec<(f32, f32, i32)>,
    pub cover_beats: f32,
    pub source_beats: f32,
    pub envelope: Vec<f32>,
    pub playhead: f32,
    pub status: String,
    pub world_name: String,
    pub fidelity: String,
    pub theseus_pct: f32,
}

impl VisualParams {
    pub fn active_count(&self) -> usize {
        self.axes_on.iter().filter(|&&a| a).count()
    }
    pub fn entropy(&self) -> f32 {
        let removed = 8 - self.active_count();
        (removed as f32 / 8.0).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// low-level colour helpers
// ---------------------------------------------------------------------------

#[inline]
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

#[inline]
fn scale(a: Rgb, t: f32) -> Rgb {
    let f = |x: u8| (x as f32 * t).round().clamp(0.0, 255.0) as u8;
    (f(a.0), f(a.1), f(a.2))
}

#[inline]
fn add(a: Rgb, b: Rgb) -> Rgb {
    let f = |x: u8, y: u8| x.saturating_add(y);
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

#[inline]
fn hash2(x: i32, y: i32, seed: u64) -> f32 {
    let mut h = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((x as u64).wrapping_mul(0x100_0000_01b3))
        .wrapping_add((y as u64).wrapping_mul(0xff51_afd7_ed55_8ccd));
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 29;
    ((h >> 40) as f32) / ((1u32 << 24) as f32)
}

#[inline]
fn angle_dist(a: f32, b: f32) -> f32 {
    let mut d = (a - b).abs() % TAU;
    if d > PI {
        d = TAU - d;
    }
    d
}

#[inline]
fn put(s: &mut Surface, x: u16, y: u16, text: &str, style: Style) {
    if y < s.height && x < s.width {
        s.print_str(x, y, text, style, None);
    }
}

#[inline]
fn dim_style(c: Rgb) -> Style {
    Style::new().fg(Color::Rgb(c.0, c.1, c.2))
}

// ---------------------------------------------------------------------------
// THE IDENTITY REACTOR — the middle star
// ---------------------------------------------------------------------------

/// Build the raw RGB reactor. `h_cells` is the panel height in terminal cells;
/// the raster is `w × 2*h_cells` pixels realized through half-blocks.
pub fn identity_raster(w: u16, h_cells: u16, p: &VisualParams) -> RgbRaster {
    let w = w.max(1);
    let h_cells = h_cells.max(1);
    let mut r = RgbRaster::new(w, h_cells.saturating_mul(2));
    let pw = r.width() as i32;
    let ph = r.height() as i32;
    if pw < 2 || ph < 2 {
        return r;
    }
    let cx = pw as f32 / 2.0;
    let cy = ph as f32 / 2.0;
    let half = (pw.min(ph) as f32) * 0.5;
    let pal = p.palette;
    let t = p.time;
    let active = p.active_count() as f32;
    let energy = (active / 8.0).clamp(0.0, 1.0);
    let entropy = p.entropy();
    let danger: Rgb = (255, 40, 50);

    // three orbiting metaball sources, precomputed once
    let sources = [
        ((t * 0.9).cos() * 0.55, (t * 1.3).sin() * 0.55, 0.45),
        ((t * 1.7 + 2.0).cos() * 0.7, (t * 0.7 + 1.0).sin() * 0.7, 0.42),
        ((t * 0.5 + 4.0).cos() * 0.85, (t * 1.1 + 3.0).sin() * 0.85, 0.38),
    ];

    let core_r = 0.20 + 0.05 * energy;
    let r0 = 0.30f32;
    let step = 0.63 / 7.0;

    for y in 0..ph {
        for x in 0..pw {
            let dx = (x as f32 + 0.5 - cx) / half;
            let dy = (y as f32 + 0.5 - cy) / half;
            let rad = (dx * dx + dy * dy).sqrt();
            let ang = dy.atan2(dx);
            let u = x as f32 / pw as f32;
            let v = y as f32 / ph as f32;

            // --- background: world gradient + plasma + radial falloff ---
            let pl = plasma(u * 3.4, v * 2.6, t * 0.12, 11.0);
            let mut col = mix(pal.bg, scale(pal.a, 0.55), (1.0 - rad).max(0.0) * 0.30);
            col = mix(col, scale(pal.b, 0.9), pl * 0.10 * (0.4 + energy));
            let falloff = radial_glow(dx, dy, 0.9) * 0.10 * (0.3 + energy);
            col = add(col, scale(pal.c, falloff));

            // --- core reactor ---
            if rad < core_r {
                let m = metaballs(dx * 1.6, dy * 1.6, &sources).min(1.0);
                let inner = 1.0 - rad / core_r;
                let hot = (m * 0.7 + inner * 0.5).min(1.0);
                let mut core = mix(scale(pal.a, 0.35), pal.c, hot);
                core = mix(core, danger, entropy * 0.55 + if p.refusal { 0.3 } else { 0.0 });
                let gain = 0.55 + 0.75 * energy;
                col = add(col, scale(core, hot * gain));
            }

            // --- identity rings ---
            let ri = ((rad - r0) / step).round();
            if (0.0..=7.0).contains(&ri) {
                let radius = r0 + ri * step;
                let thickness = 0.026 + 0.010 * energy;
                if (rad - radius).abs() < thickness {
                    let i = ri as usize;
                    let axis = AXIS_COLORS[i];
                    if p.axes_on[i] {
                        let pulse = 0.62 + 0.38 * (t * 1.9 + i as f32 * 0.9).sin();
                        let scan = (t * (0.6 + i as f32 * 0.04) + i as f32 * 1.1).rem_euclid(TAU);
                        let hi = (-angle_dist(ang, scan).powi(2) / 0.045).exp();
                        let ring = mix(axis, pal.accent, 0.20);
                        let c = mix(scale(ring, pulse), (255, 255, 255), hi * 0.95);
                        col = mix(col, c, 0.92);
                    } else {
                        // fractured, gapped, glitch-displaced
                        let seg = (ang * 3.0 + t * 2.1 + i as f32 * 1.7).sin();
                        if seg > 0.12 {
                            // gap — leak a little ember
                            if hash2(x, y, p.seed ^ i as u64) > 0.86 {
                                col = add(col, scale(danger, 0.25));
                            }
                        } else {
                            let j = hash2(x, y, p.seed ^ (i as u64).wrapping_mul(31));
                            let tint = mix(scale(axis, 0.16 + 0.10 * j), danger, 0.45);
                            col = mix(col, tint, 0.6);
                        }
                    }
                }
            }

            // --- beams from core to each live ring ---
            if rad < r0 - step * 0.4 {
                for i in 0..8 {
                    if !p.axes_on[i] {
                        continue;
                    }
                    let bi = -PI / 2.0 + i as f32 * TAU / 8.0;
                    let d = angle_dist(ang, bi);
                    if d < 0.045 {
                        let beam = mix(AXIS_COLORS[i], pal.accent, 0.35);
                        let near = (1.0 - rad / (r0 - step * 0.3)).max(0.0);
                        col = add(col, scale(beam, 0.35 + 0.5 * near));
                    }
                }
            }

            r.set(x, y, col);
        }
    }

    // Beams drawn as crisp lines, then bloomed by the glow pass.
    for i in 0..8 {
        if !p.axes_on[i] {
            continue;
        }
        let a = -PI / 2.0 + i as f32 * TAU / 8.0;
        let radius = (r0 + i as f32 * step) * half;
        let x0 = cx;
        let y0 = cy;
        let x1 = cx + a.cos() * radius;
        let y1 = cy + a.sin() * radius;
        r.line(x0 as i32, y0 as i32, x1 as i32, y1 as i32, mix(AXIS_COLORS[i], pal.accent, 0.4));
        let prog = (t * 0.55 + i as f32 * 0.125).rem_euclid(1.0);
        let sx = x0 + (x1 - x0) * prog;
        let sy = y0 + (y1 - y0) * prog;
        r.disc(sx, sy, 1.4, (255, 255, 255));
    }

    // Severed particle cloud (deterministic, from the model's simulator).
    for pt in &p.particles {
        if !pt.x.is_finite() || !pt.y.is_finite() {
            continue;
        }
        let px = pt.x * pw as f32;
        let py = pt.y * ph as f32;
        let life = (pt.life / pt.max_life.max(0.001)).clamp(0.0, 1.0);
        let c = mix(pal.accent, (255, 255, 255), life);
        r.blend(px as i32, py as i32, c, life * pt.intensity);
    }

    // --- ordered RGB post-process: instability grows as identity is removed ---
    let mut fx = vec![
        RasterFx::Glow {
            radius: 2,
            threshold: 36,
            strength: 0.85 + entropy * 1.6,
        },
        RasterFx::Vignette { strength: 0.52 },
        RasterFx::Scanlines { strength: 0.14 },
    ];
    let split = (entropy * 3.0).round() as i16;
    if split > 0 {
        fx.insert(1, RasterFx::ChromaticSplit { offset: split });
    }
    if entropy > 0.05 {
        fx.push(RasterFx::SineWarp {
            amplitude: entropy * 2.4,
            frequency: 0.11,
            phase: t * 1.4,
        });
    }
    RasterFx::apply_chain(&mut r, &fx);
    r
}

/// Realize a raster for a capability and glyph family. Colored half-block keeps
/// the full RGB; a monochrome depth collapses to ordered Braille density; any
/// other glyph family degrades the shape channel through LibGibson's lossless
/// Braille transcode so a silhouette survives even a font without Braille.
pub fn realize(r: &RgbRaster, depth: ColorDepth, glyph: GlyphMode) -> Surface {
    match depth {
        ColorDepth::Mono => {
            let mut s = r.to_mono_surface();
            transcode_surface_glyphs(&mut s, glyph.subcell());
            s
        }
        _ => match glyph {
            GlyphMode::HalfBlock => r.to_surface(),
            other => {
                let mut s = r.to_mono_surface();
                transcode_surface_glyphs(&mut s, other.subcell());
                s
            }
        },
    }
}

/// The reactor panel surface with a HUD overlay drawn in cell space.
pub fn identity_panel(w: u16, h_cells: u16, p: &VisualParams) -> Surface {
    let raster = identity_raster(w, h_cells, p);
    let mut s = realize(&raster, p.color_depth, p.glyph);
    hud_overlay(&mut s, p);
    s
}

fn hud_overlay(s: &mut Surface, p: &VisualParams) {
    if s.width < 10 || s.height < 3 {
        return;
    }
    let pal = p.palette;
    let dim = dim_style(scale(pal.c, 0.7));
    let bright = Style::new().fg(Color::Rgb(pal.c.0, pal.c.1, pal.c.2)).bold();

    // top-left corner tag + live axis count
    put(s, 0, 0, "◈ IDENTITY QUOTIENT", bright);
    let count = format!("{}/8 AXES  {:.0}%", p.active_count(), p.theseus_pct);
    let cx = s.width.saturating_sub(count.chars().count() as u16 + 1);
    put(s, cx, 0, &count, bright);

    // bottom band: world / fidelity / status
    let band = s.height - 1;
    if p.refusal {
        let danger = Style::new().fg(Color::Rgb(255, 60, 70)).bold();
        put(s, 0, band, "⚡ LAWFUL REFUSAL — TARGET WORLD REJECTS THE BRIDGE", danger);
    } else {
        let tag = format!("{} · {}", p.world_name, p.fidelity);
        put(s, 0, band, &tag, dim);
    }
}

// ---------------------------------------------------------------------------
// PIANO ROLL (real score data, full RGB)
// ---------------------------------------------------------------------------

pub fn piano_roll_panel(
    w: u16,
    h_cells: u16,
    notes: &[(f32, f32, i32)],
    total_beats: f32,
    tint: Rgb,
    playhead: f32,
    pal: WorldPalette,
    depth: ColorDepth,
    glyph: GlyphMode,
) -> Surface {
    let w = w.max(2);
    let h_cells = h_cells.max(1);
    let mut r = RgbRaster::new(w, h_cells.saturating_mul(2));
    let pw = r.width() as i32;
    let ph = r.height() as i32;
    let beats = total_beats.max(1.0);

    // background + faint grid
    for y in 0..ph {
        for x in 0..pw {
            let v = y as f32 / ph as f32;
            let c = mix(pal.bg, scale(pal.b, 0.5), (1.0 - v) * 0.10);
            r.set(x, y, c);
        }
    }
    // horizontal octave lines
    for g in 0..=8 {
        let y = (g as f32 / 8.0 * (ph - 1) as f32).round() as i32;
        for x in 0..pw {
            let cur = r.get(x, y).unwrap_or((0, 0, 0));
            r.set(x, y, add(cur, (14, 14, 24)));
        }
    }
    // vertical beat lines
    let total_bars = (beats / 4.0).ceil().max(1.0) as i32;
    for b in 0..=total_bars {
        let x = (b as f32 / total_bars as f32 * (pw - 1) as f32).round() as i32;
        let strong = b % 4 == 0;
        for y in 0..ph {
            let cur = r.get(x, y).unwrap_or((0, 0, 0));
            r.set(x, y, add(cur, if strong { (20, 20, 34) } else { (10, 10, 18) }));
        }
    }

    if !notes.is_empty() {
        let min_p = notes.iter().map(|n| n.2).min().unwrap_or(48);
        let max_p = notes.iter().map(|n| n.2).max().unwrap_or(84);
        let span = (max_p - min_p).max(6) as f32;
        for &(at, dur, pitch) in notes {
            let x0 = ((at / beats) * (pw - 1) as f32).round() as i32;
            let x1 = (((at + dur) / beats) * (pw - 1) as f32).round() as i32;
            let x1 = x1.max(x0 + 1).min(pw);
            let ny = (pitch - min_p) as f32 / span;
            let y = (ph - 1) as f32 - ny * (ph - 2) as f32;
            // hue by pitch, blended toward the panel tint
            let hue = ((pitch.rem_euclid(12)) as f32 / 12.0 * TAU).sin() * 0.5 + 0.5;
            let note_col = mix(tint, pal.c, 0.35 + 0.3 * hue);
            for x in x0.max(0)..x1 {
                if x < 0 || x >= pw {
                    continue;
                }
                let top = y.round() as i32;
                r.set(x, top, note_col);
                if top + 1 < ph {
                    r.set(x, top + 1, scale(note_col, 0.7));
                }
                // glow halo
                if top - 1 >= 0 {
                    r.blend(x as i32, (top - 1) as i32, note_col, 0.30);
                }
            }
        }
    }

    // playhead
    let px = (playhead.clamp(0.0, 1.0) * (pw - 1) as f32).round() as i32;
    for y in 0..ph {
        r.blend(px, y, (255, 255, 255), 0.55);
    }
    RasterFx::apply_chain(
        &mut r,
        &[
            RasterFx::Glow { radius: 1, threshold: 60, strength: 0.7 },
            RasterFx::Vignette { strength: 0.35 },
        ],
    );
    realize(&r, depth, glyph)
}

// ---------------------------------------------------------------------------
// SPECTRUM / WAVEFORM RIBBON (real rendered audio envelope + pitch histogram)
// ---------------------------------------------------------------------------

pub fn spectrum_panel(
    w: u16,
    h_cells: u16,
    envelope: &[f32],
    notes: &[(f32, f32, i32)],
    _total_beats: f32,
    playhead: f32,
    pal: WorldPalette,
    depth: ColorDepth,
    glyph: GlyphMode,
    active: bool,
) -> Surface {
    let w = w.max(2);
    let h_cells = h_cells.max(1);
    let mut r = RgbRaster::new(w, h_cells.saturating_mul(2));
    let pw = r.width() as i32;
    let ph = r.height() as i32;
    let mid = ph / 2;

    // background gradient
    for y in 0..ph {
        for x in 0..pw {
            let u = x as f32 / pw as f32;
            let v = y as f32 / ph as f32;
            let mut c = mix(pal.bg, scale(pal.a, 0.5), (1.0 - v) * 0.16);
            c = mix(c, scale(pal.b, 0.8), u * 0.08);
            r.set(x, y, c);
        }
    }

    // pitch-class histogram as twelve spectral columns
    let mut hist = [0.0f32; 12];
    for &(_, dur, pitch) in notes {
        hist[(pitch.rem_euclid(12)) as usize] += dur.max(0.05);
    }
    let hmax = hist.iter().cloned().fold(1e-6, f32::max);
    let bw = pw as f32 / 12.0;
    for (pc, &wt) in hist.iter().enumerate() {
        let h = (wt / hmax) * (ph as f32 * 0.42);
        let x0 = (pc as f32 * bw) as i32;
        let x1 = (((pc + 1) as f32 * bw) as i32).min(pw);
        let col = mix(AXIS_COLORS[pc % 8], pal.c, 0.3);
        for x in x0..x1 {
            for y in 0..(h.round() as i32).min(ph) {
                r.blend(x, mid - 1 - y, col, 0.20);
            }
        }
    }

    // waveform from the real rendered envelope (mirrored)
    if !envelope.is_empty() {
        let n = envelope.len();
        for x in 0..pw {
            let idx = (x as usize * n) / pw as usize;
            let v = envelope[idx.min(n - 1)].clamp(0.0, 1.0);
            let amp = (v * (ph as f32 * 0.46)).round() as i32;
            let col = mix(pal.accent, (255, 255, 255), v);
            for d in 0..=amp {
                if mid - d >= 0 {
                    r.set(x, mid - d, col);
                }
                if mid + d < ph {
                    r.set(x, mid + d, col);
                }
            }
        }
    }

    // playhead
    let px = (playhead.clamp(0.0, 1.0) * (pw - 1) as f32).round() as i32;
    for y in 0..ph {
        r.blend(px, y, (255, 255, 255), 0.5);
    }
    RasterFx::apply_chain(
        &mut r,
        &[
            RasterFx::Glow { radius: 1, threshold: 50, strength: 0.6 },
            RasterFx::Scanlines { strength: 0.12 },
        ],
    );
    let mut s = realize(&r, depth, glyph);
    if s.height > 0 {
        let label = if active {
            "AUDIO ENVELOPE · COVER (offline render)"
        } else {
            "AUDIO ENVELOPE · REFERENCE (offline render)"
        };
        put(&mut s, 1, 0, label, Style::new().fg(Color::Rgb(pal.c.0, pal.c.1, pal.c.2)).bold());
    }
    s
}

// ---------------------------------------------------------------------------
// AXIS CONSOLE — the preservation dimensions, drawn as live bars
// ---------------------------------------------------------------------------

pub fn console_panel(w: u16, h_cells: u16, p: &VisualParams) -> Surface {
    let mut s = Surface::new(w.max(10), h_cells.max(3));
    let pal = p.palette;
    // header
    put(
        &mut s,
        0,
        0,
        "COVERMAP / PRESERVATION DIMENSIONS   (toggle 1-8 · the identity is what survives)",
        Style::new().fg(Color::Rgb(pal.c.0, pal.c.1, pal.c.2)).bold(),
    );
    let bar_w: usize = (s.width as usize).saturating_sub(34).clamp(6, 40);
    for i in 0..8 {
        let y = 1 + i as u16;
        if y >= s.height {
            break;
        }
        let ax_col = AXIS_COLORS[i];
        let on = p.axes_on[i];
        let known = p.axis_known[i];
        // key + name
        let keycol = if on { ax_col } else { (90, 90, 110) };
        put(
            &mut s,
            0,
            y,
            &format!("[{}] {:<14}", i + 1, AXIS_NAMES[i]),
            Style::new().fg(Color::Rgb(keycol.0, keycol.1, keycol.2)).bold(),
        );
        // bar
        let filled = if on { bar_w } else { 0 };
        let x0 = 17u16;
        for x in 0..bar_w {
            let ch = if x < filled { '█' } else if on { '█' } else { '░' };
            let col = if on {
                let t = x as f32 / bar_w.max(1) as f32;
                mix(ax_col, pal.c, t * 0.6)
            } else {
                (70, 40, 48)
            };
            let _ = ch;
            let _ = x0;
            let glyph = if on { "█" } else if x % 2 == 0 { "░" } else { " " };
            if x0 + x as u16 >= s.width {
                break;
            }
            s.set_cell(
                x0 + x as u16,
                y,
                Cell::new(Glyph::new(glyph), Style::new().fg(Color::Rgb(col.0, col.1, col.2))),
            );
        }
        // state column
        let sx = x0 + bar_w as u16 + 1;
        let (state, stcol) = if p.refusal && p.refusal_axes[i] {
            ("REFUSED", (255, 60, 70))
        } else if on && known {
            ("PRESERVED", ax_col)
        } else if !on && known {
            ("PURGED", (120, 110, 120))
        } else {
            ("UNKNOWN", (150, 130, 90))
        };
        put(
            &mut s,
            sx,
            y,
            state,
            Style::new().fg(Color::Rgb(stcol.0, stcol.1, stcol.2)).bold(),
        );
        if let Some(ok) = p.axis_conformance[i] {
            let (txt, c) = if ok { ("✓ conformance", (80, 240, 130)) } else { ("✗ conformance", (255, 80, 90)) };
            put(&mut s, sx + 10, y, txt, Style::new().fg(Color::Rgb(c.0, c.1, c.2)));
        }
    }
    // status line
    if s.height >= 10 {
        let y = s.height - 1;
        let msg = if p.status.is_empty() {
            "ready — press ? for the operator manual".to_string()
        } else {
            p.status.clone()
        };
        put(&mut s, 0, y, &format!("» {msg}"), Style::new().fg(Color::Rgb(pal.accent.0, pal.accent.1, pal.accent.2)));
    }
    s
}

// ---------------------------------------------------------------------------
// FRAME COMPOSITION
// ---------------------------------------------------------------------------

fn frame_box(s: &mut Surface, rect: Rect, title: &str, color: Rgb) {
    s.draw_border(rect, BorderType::Rounded, Style::new().fg(Color::Rgb(color.0, color.1, color.2)));
    if rect.width > 4 {
        put(
            s,
            rect.x + 2,
            rect.y,
            &format!(" {title} "),
            Style::new().fg(Color::Rgb(color.0, color.1, color.2)).bold(),
        );
    }
}

/// Compose the whole body canvas: ribbon, three panels, console, status.
pub fn build_frame(w: u16, h: u16, p: &VisualParams) -> Surface {
    let mut s = Surface::new(w.max(20), h.max(6));
    let pal = p.palette;

    // background fill
    s.fill_rect(
        Rect::new(0, 0, s.width, s.height),
        Cell::new(Glyph::space(), Style::new().bg(Color::Rgb(pal.bg.0, pal.bg.1, pal.bg.2))),
    );

    let status_h: u16 = 1;
    let gap: u16 = 1;
    let usable = s.height.saturating_sub(status_h + gap * 3);
    let ribbon_h: u16 = (usable / 5).clamp(2, 4);
    // The console wants ~11 rows to show its header, all eight axes and a status
    // line; give it what it can take, then let the reactor keep the rest.
    let console_h: u16 = usable.saturating_sub(7).clamp(3, 11);
    let main_h: u16 = usable.saturating_sub(ribbon_h + console_h).max(3);

    // ribbons and main band
    let ribbon_rect = Rect::new(0, 0, s.width, ribbon_h);
    let main_y = ribbon_h + gap;
    let wide = s.width >= 82;

    let main_band = Rect::new(0, main_y, s.width, main_h);

    if wide {
        let left_w = (s.width as f32 * 0.26).round() as u16;
        let right_w = (s.width as f32 * 0.26).round() as u16;
        let mid_w = s.width.saturating_sub(left_w + right_w + 2 * gap);
        let ref_rect = Rect::new(0, main_y, left_w, main_h);
        let mid_rect = Rect::new(left_w + 1, main_y, mid_w, main_h);
        let cov_rect = Rect::new(left_w + mid_w + 2, main_y, right_w, main_h);

        // reference
        let ref_inner = ref_rect.shrink(1);
        let ref_surface = piano_roll_panel(
            ref_inner.width.max(2),
            ref_inner.height.saturating_sub(2).max(1),
            &p.source_bars,
            p.source_beats,
            pal.c,
            0.5,
            pal,
            p.color_depth,
            p.glyph,
        );
        s.blit_transparent_clipped(&ref_surface, ref_inner.x as i32, ref_inner.y as i32, s.area());
        frame_box(&mut s, ref_rect, "1 · REFERENCE PERFORMANCE", pal.c);
        put(
            &mut s,
            ref_rect.x + 2,
            ref_rect.y + 1,
            &format!("{} notes · {:.0} beats", p.source_notes, p.source_beats),
            dim_style(pal.c),
        );

        // reactor (the star)
        let mid_inner = mid_rect.shrink(1);
        let reactor = identity_panel(mid_inner.width.max(4), mid_inner.height.saturating_sub(2).max(2), p);
        s.blit_transparent_clipped(&reactor, mid_inner.x as i32, mid_inner.y as i32, s.area());
        frame_box(
            &mut s,
            mid_rect,
            "2 · IDENTITY QUOTIENT / COVERMAP  ★",
            if p.refusal { (255, 60, 70) } else { pal.a },
        );

        // cover
        let cov_inner = cov_rect.shrink(1);
        let cover_surface = piano_roll_panel(
            cov_inner.width.max(2),
            cov_inner.height.saturating_sub(2).max(1),
            &p.cover_bars,
            p.cover_beats.max(1.0),
            if p.refusal { (255, 60, 70) } else { pal.accent },
            p.playhead,
            pal,
            p.color_depth,
            p.glyph,
        );
        s.blit_transparent_clipped(&cover_surface, cov_inner.x as i32, cov_inner.y as i32, s.area());
        if p.refusal {
            frame_box(&mut s, cov_rect, "3 · LAWFUL REFUSAL", (255, 60, 70));
        } else {
            frame_box(&mut s, cov_rect, "3 · FRESH COVER", pal.accent);
            put(
                &mut s,
                cov_rect.x + 2,
                cov_rect.y + 1,
                &format!("{} notes · from quotient only", p.cover_notes),
                dim_style(pal.accent),
            );
        }
    } else {
        // narrow: reactor only, with stats strip
        let reactor = identity_panel(main_band.width.saturating_sub(2).max(4), main_band.height.saturating_sub(2).max(3), p);
        s.blit_transparent_clipped(&reactor, 1, main_band.y as i32 + 1, s.area());
        frame_box(
            &mut s,
            main_band,
            "IDENTITY QUOTIENT / COVERMAP ★",
            if p.refusal { (255, 60, 70) } else { pal.a },
        );
    }

    // spectrum ribbon
    let sr = spectrum_panel(
        ribbon_rect.width,
        ribbon_rect.height,
        &p.envelope,
        if p.refusal { &p.source_bars } else { &p.cover_bars },
        if p.refusal { p.source_beats } else { p.cover_beats }.max(1.0),
        p.playhead,
        pal,
        p.color_depth,
        p.glyph,
        !p.refusal,
    );
    s.blit_transparent_clipped(&sr, 0, 0, s.area());

    // console
    let console_rect = Rect::new(0, main_y + main_h + gap, s.width, console_h);
    let console = console_panel(console_rect.width, console_rect.height, p);
    s.blit_transparent_clipped(&console, 0, console_rect.y as i32, s.area());
    frame_box(
        &mut s,
        console_rect,
        "4 · RECOMBINATION CONSOLE — COVERMAP / PRESERVATION DIMENSIONS",
        pal.b,
    );

    // status strip
    let status_y = console_rect.y + console_rect.height + gap;
    if status_y < s.height {
        put(
            &mut s,
            1,
            status_y,
            &format!(
                "THESEUS INDEX {:.0}%   AXES {}/8   {}   {}",
                p.theseus_pct,
                p.active_count(),
                p.world_name,
                p.fidelity
            ),
            Style::new().fg(Color::Rgb(pal.accent.0, pal.accent.1, pal.accent.2)).bold(),
        );
    }

    // global scan sweep for life
    let sweep = ((p.time * 0.20) % 1.0) * s.height as f32;
    let sy = sweep as u16;
    if sy < s.height {
        for x in 0..s.width {
            if let Some(cell) = s.get_mut(x, sy) {
                if !cell.transparent {
                    cell.style = cell.style.overlay(Style::new().fg(Color::Rgb(pal.c.0, pal.c.1, pal.c.2)).dim());
                }
            }
        }
    }
    s
}
