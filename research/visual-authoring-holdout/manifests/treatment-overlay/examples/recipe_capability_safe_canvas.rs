//! RECIPE — CAPABILITY-SAFE CANVAS
//! Run:      cargo run --example recipe_capability_safe_canvas
//! Capture:  cargo run --example recipe_capability_safe_canvas -- --capture 120x40:truecolor
//!           cargo run --example recipe_capability_safe_canvas -- --capture 120x40:mono
//!
//! COMPOSITION LAW — carry meaning in SHAPE, POSITION and LUMINANCE, so the
//! object survives when color does not.
//!
//! Terminals degrade: TrueColor → ANSI256 → ANSI16 → Mono, and some can't even
//! render fancy glyphs. A world that encodes its meaning ONLY in hue becomes an
//! undifferentiated smear in Mono. The fix is to make the structure legible
//! from a glyph/luminance ramp alone; color then merely *enhances* a picture
//! that already reads. Capture this recipe at `:truecolor` and `:mono` — the
//! object is recognizable in both, because a text capture sees exactly what a
//! Mono terminal sees: the ramp, not the color.

use gibson::ui::prelude::*;
use gibson::{Color, Event, KeyCode, Node, Rect, Style, Surface};

#[path = "recipes_support/mod.rs"]
mod recipes_support;

/// Dark → bright luminance ramp. This is what carries the shape when color is
/// gone. Index by intensity.
const RAMP: &[char] = &[' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

#[derive(Clone, Copy)]
struct Orb {
    radius: f32, // 0.2 ..= 0.9 of the half-extent
}

/// Paint a single recognizable object (a cored orb with a bright rim) using the
/// glyph ramp for structure and color only as enhancement. The SAME call works
/// at every capability; LibGibson quantizes the color for the environment, and
/// when that collapses to nothing the ramp still draws the orb.
fn paint_orb(rect: Rect, radius: f32) -> Surface {
    let w = rect.width.max(1);
    let h = rect.height.max(1);
    let mut s = Surface::new(w, h);
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let half = (w.min(h * 2) as f32) * 0.5 * radius;
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = (y as f32 - cy) * 2.0; // correct for ~2:1 cell aspect
            let dist = (dx * dx + dy * dy).sqrt() / half.max(1.0);
            // Bright rim near dist≈1, dim core, empty outside.
            let lum = if dist > 1.05 {
                0.0
            } else {
                let rim = 1.0 - (dist - 0.85).abs() * 3.0;
                (0.25 + 0.75 * rim.clamp(0.0, 1.0)).clamp(0.0, 1.0)
            };
            if lum <= 0.001 {
                continue;
            }
            let gi = ((lum * (RAMP.len() - 1) as f32).round() as usize).min(RAMP.len() - 1);
            let glyph = RAMP[gi];
            // Color is pure enhancement: teal body, amber rim. Stripped in Mono;
            // the ramp above already made the orb legible.
            let style = Style {
                fg: Some(Color::rgb(
                    (40.0 + 215.0 * lum) as u8,
                    (80.0 + 120.0 * lum) as u8,
                    (120.0 * (1.0 - lum)) as u8 + 40,
                )),
                ..Default::default()
            };
            s.print_str(x, y, &glyph.to_string(), style, None);
        }
    }
    s
}

fn view(orb: &Orb, cx: &BuildCx) -> Element<()> {
    let radius = orb.radius;
    let depth = cx.environment.color_depth;
    let hero = raw(Node::canvas(move |rect| paint_orb(rect, radius))).grow(1.0);

    let status_row = row()
        .child(status("ORB").tone(Tone::Info))
        .child(spacer())
        .child(text(format!("depth {depth:?}   radius {radius:.2}")))
        .height(1);

    let hint = text("+/- resize   (recognizable in TrueColor…Mono)   Ctrl-C quit").height(1);

    screen()
        .height(cx.environment.height)
        .child(status_row)
        .child(hero)
        .child(hint)
}

fn update(orb: &mut Orb, event: AppEvent<()>) -> Control {
    if let AppEvent::Input(Event::Key(k)) = event {
        match k.code {
            KeyCode::Char('+') | KeyCode::Char('=') => orb.radius = (orb.radius + 0.05).min(0.9),
            KeyCode::Char('-') | KeyCode::Char('_') => orb.radius = (orb.radius - 0.05).max(0.2),
            _ => {}
        }
    }
    Control::Continue
}

fn main() -> std::io::Result<()> {
    recipes_support::present(skins::BLACK_ICE, Orb { radius: 0.7 }, update, view)
}
