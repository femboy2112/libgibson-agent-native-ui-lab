//! RECIPE — HERO WITH HUD
//! Run:      cargo run --example recipe_hero_with_hud
//! Capture:  cargo run --example recipe_hero_with_hud -- --capture 120x40
//!
//! COMPOSITION LAW — the world owns the frame; chrome is sparse and subordinate.
//!
//!     tiny status ───────────────────────────────
//!              CUSTOM WORLD                 │ rail
//!              CUSTOM WORLD                 │
//!     ────────────────────────── minimal controls
//!
//! The dominant visual object (a continuous energy field with one focal core)
//! is painted by hand into a `Surface` via `Node::canvas`, which hands the
//! painter the exact `Rect` it was given and takes a finished `Surface` back.
//! `gibson::ui` supplies ONLY the thin chrome: a one-line status, a one-line
//! control hint, and a narrow contextual rail. The world `.grow(1.0)`s to eat
//! everything the chrome does not claim. Contrast a dashboard, where the frame
//! is sliced into co-equal boxes and no single object is the subject.

use gibson::raster::RgbRaster;
use gibson::ui::prelude::*;
use gibson::{Event, KeyCode, Node, Rect, Surface};

#[path = "recipes_support/mod.rs"]
mod recipes_support;

#[derive(Clone)]
struct World {
    /// Focal position across the field, 0.0 ..= 1.0.
    focus: f32,
    /// How hot the core burns, 0.0 ..= 1.0.
    intensity: f32,
}

/// The hero. A pure function of state → geometry: the focal core sits where
/// `focus` says, and the field's color and reach follow `intensity`. No widgets
/// live in here — this is a coordinate space, not a panel.
fn paint_field(rect: Rect, focus: f32, intensity: f32) -> Surface {
    let w = rect.width.max(1);
    let h = rect.height.max(1);
    // Half-block rendering packs two vertical pixels per cell row, so the raster
    // is twice as tall as the cell grid.
    let mut px = RgbRaster::new(w, h.saturating_mul(2));
    let pw = px.width() as f32;
    let ph = px.height() as f32;
    let cx = focus.clamp(0.0, 1.0) * pw;
    let cy = ph * 0.5;
    let reach = (0.18 + 0.32 * intensity) * pw; // the field grows as it heats
    for y in 0..px.height() as i32 {
        for x in 0..px.width() as i32 {
            let dx = x as f32 - cx;
            // Cells are ~2:1, so scale y to keep the field round, not squashed.
            let dy = (y as f32 - cy) * 2.0;
            let dist = (dx * dx + dy * dy).sqrt();
            let fall = (1.0 - (dist / reach)).clamp(0.0, 1.0);
            let e = fall * fall; // sharpen the core
                                 // Cool teal in the body, flaring to hot amber at the core.
            let r = (30.0 + 225.0 * e * intensity) as u8;
            let g = (40.0 + 150.0 * e) as u8;
            let b = (60.0 + 120.0 * (e * (1.0 - intensity))) as u8;
            px.set(x, y, (r, g, b));
        }
    }
    px.to_surface()
}

fn view(m: &World, cx: &BuildCx) -> Element<()> {
    let wide = cx.environment.width >= 90;
    let focus = m.focus;
    let intensity = m.intensity;

    // The hero: deferred, size-aware paint. It receives whatever Rect the layout
    // leaves it and fills every pixel of it.
    let hero = raw(Node::canvas(move |rect| {
        paint_field(rect, focus, intensity)
    }))
    .grow(1.0);

    // Sparse chrome #1 — a one-line status strip across the top.
    let status_row = row()
        .child(status("FIELD").tone(Tone::Success))
        .child(spacer())
        .child(text(format!("intensity {:>3.0}%", intensity * 100.0)))
        .height(1);

    // The world fills the middle. A thin contextual rail rides beside it only
    // when there is room — chrome appears because it fits, not because the
    // layout was carved into boxes first.
    let stage = if wide {
        row().grow(1.0).child(hero).child(
            column()
                .width(16)
                .child(status("READOUT").tone(Tone::Info))
                .child(text(format!("x  {focus:.2}")))
                .child(text(format!("E  {intensity:.2}")))
                .child(spacer())
                .child(text("< > move"))
                .child(text("^ v power")),
        )
    } else {
        row().grow(1.0).child(hero)
    };

    // Sparse chrome #2 — one line of controls.
    let hint = text("</> move core   ^/v power   Ctrl-C quit").height(1);

    // `screen()` is natural-height: without pinning it to the terminal height,
    // `.grow(1.0)` has nothing to expand into and the world collapses to a
    // single row. Pin it, and the hero fills everything the two chrome lines
    // and the rail do not. (This one line is load-bearing — see the research
    // audit; a one-call "mount a full-frame hero" helper is proposed upstream.)
    screen()
        .height(cx.environment.height)
        .child(status_row)
        .child(stage)
        .child(hint)
}

fn update(m: &mut World, event: AppEvent<()>) -> Control {
    if let AppEvent::Input(Event::Key(k)) = event {
        match k.code {
            KeyCode::Left => m.focus = (m.focus - 0.05).max(0.0),
            KeyCode::Right => m.focus = (m.focus + 0.05).min(1.0),
            KeyCode::Up => m.intensity = (m.intensity + 0.05).min(1.0),
            KeyCode::Down => m.intensity = (m.intensity - 0.05).max(0.0),
            _ => {}
        }
    }
    Control::Continue
}

fn main() -> std::io::Result<()> {
    recipes_support::present(
        skins::BLACK_ICE,
        World {
            focus: 0.5,
            intensity: 0.6,
        },
        update,
        view,
    )
}
