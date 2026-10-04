//! The five representations of the same three signals.
//!
//!   TRACE    raw receiver amplitude vs time              (gibson::plot)
//!   SPECTRUM power vs frequency, log axis                (gibson::plot)
//!   FOLD     folded profile + phase-time map             (gibson::plot + custom raster)
//!   RELATE   delay plane (tau21, tau31) + significance   (gibson::plot)
//!   SKY      inferred source geometry on the celestial sphere (custom raster)
//!
//! Every view reads the same `Observatory`, draws every active identity with the
//! same glyph / letter / hue, and reports an `Anchor` for it, so the compositor
//! can transport identities between representations.

pub mod common;
pub mod fold;
pub mod relate;
pub mod sky;
pub mod spectrum;
pub mod trace;

use gibson::plot::PlotReport;
use gibson::{Rect, SubcellGlyphMode, Surface};

use crate::observatory::Observatory;
use crate::sim::SLOTS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum View {
    Trace,
    Spectrum,
    Fold,
    Relate,
    Sky,
}

impl View {
    pub const ALL: [View; 5] = [
        View::Trace,
        View::Spectrum,
        View::Fold,
        View::Relate,
        View::Sky,
    ];
    pub fn name(self) -> &'static str {
        match self {
            View::Trace => "TRACE",
            View::Spectrum => "SPECTRUM",
            View::Fold => "FOLD",
            View::Relate => "RELATE",
            View::Sky => "SKY",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            View::Trace => "TRACE",
            View::Spectrum => "SPECT",
            View::Fold => "FOLD",
            View::Relate => "RELATE",
            View::Sky => "SKY",
        }
    }
    pub fn key(self) -> char {
        match self {
            View::Trace => '1',
            View::Spectrum => '2',
            View::Fold => '3',
            View::Relate => '4',
            View::Sky => '5',
        }
    }
    pub fn index(self) -> usize {
        View::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }
    pub fn next(self) -> View {
        View::ALL[(self.index() + 1) % View::ALL.len()]
    }
    pub fn prev(self) -> View {
        View::ALL[(self.index() + View::ALL.len() - 1) % View::ALL.len()]
    }
    pub fn parse(s: &str) -> Option<View> {
        match s.to_ascii_lowercase().as_str() {
            "1" | "trace" | "time" => Some(View::Trace),
            "2" | "spectrum" | "spect" | "spec" => Some(View::Spectrum),
            "3" | "fold" | "phase" => Some(View::Fold),
            "4" | "relate" | "delay" | "relation" => Some(View::Relate),
            "5" | "sky" | "source" => Some(View::Sky),
            _ => None,
        }
    }
}

/// Where an identity lands in a representation (hero-local cell coordinates).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub slot: usize,
    pub x: i32,
    pub y: i32,
}

/// Everything a representation needs to know to draw one frame.
#[derive(Clone)]
pub struct Ctx<'a> {
    pub obs: &'a Observatory,
    /// Analysis epoch shown (0 = nothing integrated yet).
    pub ep: usize,
    /// Exact display time (s), `>= ep * EPOCH_S`.
    pub t: f64,
    pub sel: Option<usize>,
    /// Analysis epoch of the pinned earlier state, when comparing.
    pub ref_ep: Option<usize>,
    pub zoom: u8,
    pub nudge: i32,
    /// Inspect cursor, normalised to the plot area (u right, v up).
    pub cursor: Option<(f64, f64)>,
    pub mono: bool,
    pub glyphs: SubcellGlyphMode,
}

pub struct ViewOut {
    pub surface: Surface,
    pub anchors: Vec<Anchor>,
    pub readout: String,
    /// (x caption, y caption) of every `gibson::plot` drawn.
    pub axes: Vec<(String, String)>,
    pub reports: Vec<PlotReport>,
}

impl ViewOut {
    pub fn anchor(&self, slot: usize) -> Option<Anchor> {
        self.anchors.iter().copied().find(|a| a.slot == slot)
    }
}

pub fn render(view: View, ctx: &Ctx, rect: Rect) -> ViewOut {
    match view {
        View::Trace => trace::render(ctx, rect),
        View::Spectrum => spectrum::render(ctx, rect),
        View::Fold => fold::render(ctx, rect),
        View::Relate => relate::render(ctx, rect),
        View::Sky => sky::render(ctx, rect),
    }
}

/// Slots that currently have a presence (anything other than Quiet).
pub fn active_slots(ctx: &Ctx) -> Vec<usize> {
    match ctx.obs.timeline.epoch(ctx.ep) {
        Some(e) => (0..SLOTS)
            .filter(|s| e.slots[*s].state.is_active())
            .collect(),
        None => Vec::new(),
    }
}

/// The selected slot, or a sensible default: the strongest active candidate.
pub fn effective_selection(ctx: &Ctx) -> Option<usize> {
    let e = ctx.obs.timeline.epoch(ctx.ep)?;
    if let Some(s) = ctx.sel {
        if e.slots[s].state.is_active() {
            return Some(s);
        }
    }
    (0..SLOTS)
        .filter(|s| e.slots[*s].state.is_active() && !e.slots[*s].state.is_rejected())
        .max_by(|a, b| e.slots[*a].z.total_cmp(&e.slots[*b].z))
        .or_else(|| {
            (0..SLOTS)
                .filter(|s| e.slots[*s].state.is_active())
                .max_by(|a, b| e.slots[*a].z.total_cmp(&e.slots[*b].z))
        })
}

/// One-line "then versus now" comparison of the focused identity, shown while
/// an earlier uncertain state is pinned (`c`).
pub fn compare_note(ctx: &Ctx, sel: Option<usize>) -> Option<String> {
    let r = ctx.ref_ep?;
    let s = sel?;
    let tl = &ctx.obs.timeline;
    let then = tl.epoch(r)?.slots[s];
    let now = tl.epoch(ctx.ep)?.slots[s];
    let tr = r as f64 * crate::sim::EPOCH_S;
    let tag = crate::identity::IDENT[s].letter;
    if !then.state.is_active() {
        return Some(format!(
            "COMPARE {tag}: not yet detected at t={tr:.0}s \u{2192} Z {:.1}\u{03C3} now",
            now.z
        ));
    }
    let sky = |e: &crate::track::SlotEpoch| match e.sky {
        Some(k) => format!(
            "\u{00B1}{:.0}\u{00B0} mirror {:.0}%",
            k.sigma_deg,
            k.confidence_of_best() * 100.0
        ),
        None => "no sky fix".to_string(),
    };
    Some(format!(
        "COMPARE {tag} t={tr:.0}s\u{2192}{:.0}s  Z {:.1}\u{2192}{:.1}\u{03C3}  {} \u{2192} {}",
        ctx.ep as f64 * crate::sim::EPOCH_S,
        then.z,
        now.z,
        sky(&then),
        sky(&now)
    ))
}
