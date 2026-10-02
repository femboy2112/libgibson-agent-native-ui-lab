//! RECIPE — SEMANTIC ZOOM (responsive)
//! Run:      cargo run --example recipe_semantic_zoom
//! Capture:  cargo run --example recipe_semantic_zoom -- --capture 120x40
//!           cargo run --example recipe_semantic_zoom -- --capture 80x24
//!           cargo run --example recipe_semantic_zoom -- --capture 42x15
//!
//! COMPOSITION LAW — shrinking means dropping secondary meaning, not squeezing
//! every box into fewer columns.
//!
//!   wide   :  status + HERO + context rail + timeline + controls
//!   medium :  status + HERO + timeline + compact controls
//!   tiny   :  status + HERO + essential controls
//!
//! The hero is present and dominant at EVERY size. What changes is which
//! SECONDARY chrome earns its space. The anti-pattern is keeping all six
//! regions and letting each collapse into an illegible sliver.

use gibson::raster::RgbRaster;
use gibson::ui::prelude::*;
use gibson::{Node, Rect, Surface};

#[path = "recipes_support/mod.rs"]
mod recipes_support;

#[derive(Clone, Copy)]
struct Scene {
    phase: f32,
}

/// The hero world: a banded gradient horizon. Always the subject.
fn paint_scene(rect: Rect, phase: f32) -> Surface {
    let w = rect.width.max(1);
    let h = rect.height.max(1);
    let mut px = RgbRaster::new(w, h.saturating_mul(2));
    let ph = px.height() as f32;
    let horizon = ph * (0.45 + 0.1 * phase);
    for y in 0..px.height() as i32 {
        let t = y as f32 / ph;
        for x in 0..px.width() as i32 {
            if (y as f32) < horizon {
                // Sky: deep indigo fading up.
                let k = 1.0 - t;
                px.set(x, y, (20 + (30.0 * k) as u8, 24 + (40.0 * k) as u8, 60));
            } else {
                // Ground: warm band keyed to phase.
                let g = ((y as f32 - horizon) / (ph - horizon)).clamp(0.0, 1.0);
                px.set(
                    x,
                    y,
                    (60 + (120.0 * g * phase) as u8, 40 + (60.0 * g) as u8, 30),
                );
            }
        }
    }
    px.to_surface()
}

fn view(s: &Scene, cx: &BuildCx) -> Element<()> {
    let w = cx.environment.width;
    let phase = s.phase;
    let hero = raw(Node::canvas(move |rect| paint_scene(rect, phase))).grow(1.0);

    // Status is ESSENTIAL — present at every size.
    let status_row = row()
        .child(status("HORIZON").tone(Tone::Success))
        .child(spacer())
        .child(text(format!("phase {:.0}%", phase * 100.0)))
        .height(1);

    // Build the body around the hero, adding secondary chrome only as size
    // allows. The hero is added first and grows; nothing shrinks it.
    let stage = if w >= 100 {
        // WIDE: hero + a contextual rail beside it.
        row().grow(1.0).child(hero).child(
            column()
                .width(18)
                .child(status("CONTEXT").tone(Tone::Info))
                .child(text("sun    rising"))
                .child(text("wind   calm"))
                .child(text("tide   low"))
                .child(spacer())
                .child(text("space  advance")),
        )
    } else {
        // MEDIUM / TINY: hero alone in the stage; the rail is dropped, not squeezed.
        row().grow(1.0).child(hero)
    };

    let mut screen_el = screen()
        .height(cx.environment.height)
        .child(status_row)
        .child(stage);

    // Timeline is SECONDARY — present at medium and wide, dropped when tiny.
    if w >= 60 {
        screen_el = screen_el.child(text("├────●────────────────┤  dawn → noon → dusk").height(1));
    }

    // Controls degrade from verbose → essential rather than vanishing entirely.
    let controls = if w >= 100 {
        "space advance phase    r reset    Ctrl-C quit"
    } else if w >= 60 {
        "space advance   r reset   Ctrl-C quit"
    } else {
        "space  Ctrl-C"
    };
    screen_el.child(text(controls).height(1))
}

fn update(s: &mut Scene, event: AppEvent<()>) -> Control {
    use gibson::{Event, KeyCode};
    if let AppEvent::Input(Event::Key(k)) = event {
        match k.code {
            KeyCode::Char(' ') => s.phase = (s.phase + 0.1).min(1.0),
            KeyCode::Char('r') => s.phase = 0.0,
            _ => {}
        }
    }
    Control::Continue
}

fn main() -> std::io::Result<()> {
    recipes_support::present(skins::BLACK_ICE, Scene { phase: 0.3 }, update, view)
}
