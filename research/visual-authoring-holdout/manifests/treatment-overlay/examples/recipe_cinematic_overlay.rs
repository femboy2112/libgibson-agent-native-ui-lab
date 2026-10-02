//! RECIPE — CINEMATIC OVERLAY
//! Run:      cargo run --example recipe_cinematic_overlay
//! Capture:  cargo run --example recipe_cinematic_overlay -- --capture 120x40
//!           cargo run --example recipe_cinematic_overlay -- --capture 120x40:truecolor  (then press i in a real terminal)
//!
//! COMPOSITION LAW — context floats ABOVE a fixed world; it never reflows it.
//!
//! When the user asks to inspect something, the world must not jump. A weak
//! layout opens a detail pane by stealing columns from the world, shoving it
//! sideways and breaking the viewer's spatial lock. The strong move is an
//! OVERLAY: the world keeps its exact geometry, and the inspector floats over
//! it via `.overlay(modal(...))`. Toggle the modal and the world underneath is
//! byte-for-byte unchanged.

use gibson::raster::RgbRaster;
use gibson::ui::prelude::*;
use gibson::{Event, KeyCode, Node, Rect, Surface};

#[path = "recipes_support/mod.rs"]
mod recipes_support;

#[derive(Clone, Copy)]
struct Stage {
    /// Selected lane, 0..LANES. The inspector reports on this one.
    sel: usize,
    inspecting: bool,
}

const LANES: usize = 5;

/// The fixed world: LANES vertical ribbons of flowing energy. It is a pure
/// function of `sel` only — opening the inspector does not touch it.
fn paint_stage(rect: Rect, sel: usize) -> Surface {
    let w = rect.width.max(1);
    let h = rect.height.max(1);
    let mut px = RgbRaster::new(w, h.saturating_mul(2));
    let pw = px.width() as f32;
    let ph = px.height() as f32;
    let lane_w = pw / LANES as f32;
    for y in 0..px.height() as i32 {
        let t = y as f32 / ph;
        for x in 0..px.width() as i32 {
            let lane = (x as f32 / lane_w) as usize;
            let within = (x as f32 - lane as f32 * lane_w) / lane_w; // 0..1 across lane
            let edge = (within - 0.5).abs() * 2.0; // 0 center → 1 edge
            let body = (1.0 - edge).clamp(0.0, 1.0);
            let selected = lane == sel;
            let base = if selected { 1.0 } else { 0.4 };
            let r = (20.0 + 200.0 * body * base * (0.4 + 0.6 * t)) as u8;
            let g = (30.0 + 170.0 * body * base) as u8;
            let b = (50.0 + 120.0 * body * (1.0 - base * 0.5)) as u8;
            px.set(x, y, (r, g, b));
        }
    }
    px.to_surface()
}

fn view(st: &Stage, cx: &BuildCx) -> Element<()> {
    let sel = st.sel;
    // NOTE: the hero node is built identically whether or not we are inspecting.
    let world = raw(Node::canvas(move |rect| paint_stage(rect, sel))).grow(1.0);

    let status_row = row()
        .child(status("REACTOR").tone(Tone::Success))
        .child(spacer())
        .child(text(format!("lane {}/{}", sel + 1, LANES)))
        .height(1);

    let hint = text("</> select lane   i inspect (overlay)   Ctrl-C quit").height(1);

    let base = screen()
        .height(cx.environment.height)
        .child(status_row)
        .child(world)
        .child(hint);

    // The ONLY difference the inspector makes is an added floating layer.
    if st.inspecting {
        base.overlay(
            modal(format!("INSPECT · LANE {}", sel + 1))
                .child(text("flow      nominal"))
                .child(text("pressure  0.62"))
                .child(text("coupling  phase-locked"))
                .child(divider())
                .child(text("i or Esc to close")),
        )
    } else {
        base
    }
}

fn update(st: &mut Stage, event: AppEvent<()>) -> Control {
    if let AppEvent::Input(Event::Key(k)) = event {
        match k.code {
            KeyCode::Left => st.sel = (st.sel + LANES - 1) % LANES,
            KeyCode::Right => st.sel = (st.sel + 1) % LANES,
            KeyCode::Char('i') => st.inspecting = !st.inspecting,
            KeyCode::Esc => st.inspecting = false,
            _ => {}
        }
    }
    Control::Continue
}

fn main() -> std::io::Result<()> {
    recipes_support::present(
        skins::BLACK_ICE,
        Stage {
            sel: 2,
            inspecting: false,
        },
        update,
        view,
    )
}
