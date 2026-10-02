//! RECIPE — CONTINUOUS WORLD
//! Run:      cargo run --example recipe_continuous_world
//! Capture:  cargo run --example recipe_continuous_world -- --capture 120x40
//!
//! COMPOSITION LAW — one coordinate space, one camera. Scale is a camera move,
//! not a different screen.
//!
//! Every object lives at a fixed world coordinate. The user never switches
//! "pages" or "views": panning and zooming move a single camera over the SAME
//! space, and the renderer re-projects the same points. A weak composition
//! would build a separate dashboard per zoom level ("overview screen",
//! "detail screen") and swap between them, destroying the viewer's sense of
//! place. Here there is exactly one place, seen from different distances.

use gibson::ui::prelude::*;
use gibson::{Color, Event, KeyCode, Node, Rect, Style, Surface};

#[path = "recipes_support/mod.rs"]
mod recipes_support;

/// The world: fixed points in one coordinate space. (x, y, label.) These never
/// move; only the camera does.
const BODIES: [(f32, f32, &str); 7] = [
    (0.0, 0.0, "CORE"),
    (-14.0, -6.0, "ALPHA"),
    (12.0, -9.0, "BETA"),
    (20.0, 7.0, "GAMMA"),
    (-9.0, 11.0, "DELTA"),
    (-26.0, 2.0, "EPSILON"),
    (31.0, -2.0, "ZETA"),
];

#[derive(Clone, Copy)]
struct Cam {
    x: f32,
    y: f32,
    zoom: f32,
}

/// Paint the one world as seen by one camera. Pure function of the camera.
fn paint_world(rect: Rect, cam: Cam) -> Surface {
    let w = rect.width.max(1);
    let h = rect.height.max(1);
    let mut s = Surface::new(w, h);
    let hw = w as f32 / 2.0;
    let hh = h as f32 / 2.0;
    // One projection, applied to the whole space.
    let project = |wx: f32, wy: f32| -> (f32, f32) {
        let sx = (wx - cam.x) * cam.zoom + hw;
        let sy = (wy - cam.y) * cam.zoom * 0.5 + hh; // *0.5 → cells are ~2:1
        (sx, sy)
    };
    // Faint reference marks so the empty space still reads as a continuum.
    let dim = Style {
        fg: Some(Color::rgb(40, 54, 68)),
        ..Default::default()
    };
    let mut gx = -40.0;
    while gx <= 40.0 {
        let mut gy = -20.0;
        while gy <= 20.0 {
            let (sx, sy) = project(gx, gy);
            if sx >= 0.0 && sx < w as f32 && sy >= 0.0 && sy < h as f32 {
                s.print_str(sx as u16, sy as u16, "·", dim, None);
            }
            gy += 10.0;
        }
        gx += 10.0;
    }
    // The bodies, each at its true world coordinate, brighter and labelled when
    // the camera is close enough to read them.
    let show_labels = cam.zoom >= 1.4;
    for (wx, wy, name) in BODIES {
        let (sx, sy) = project(wx, wy);
        if sx < 0.0 || sx >= w as f32 || sy < 0.0 || sy >= h as f32 {
            continue;
        }
        let core = Style {
            fg: Some(Color::rgb(120, 230, 240)),
            ..Default::default()
        };
        s.print_str(sx as u16, sy as u16, "◉", core, None);
        if show_labels {
            let lx = (sx as u16).saturating_add(2);
            if lx < w {
                let label = Style {
                    fg: Some(Color::rgb(150, 170, 190)),
                    ..Default::default()
                };
                s.print_str(lx, sy as u16, name, label, None);
            }
        }
    }
    s
}

fn view(cam: &Cam, cx: &BuildCx) -> Element<()> {
    let cam = *cam;
    let world = raw(Node::canvas(move |rect| paint_world(rect, cam))).grow(1.0);

    let status_row = row()
        .child(status("CONTINUUM").tone(Tone::Info))
        .child(spacer())
        .child(text(format!(
            "x {:+.0}  y {:+.0}  zoom {:.2}x",
            cam.x, cam.y, cam.zoom
        )))
        .height(1);

    let hint = text("arrows pan   +/- zoom   Ctrl-C quit").height(1);

    screen()
        .height(cx.environment.height)
        .child(status_row)
        .child(world)
        .child(hint)
}

fn update(cam: &mut Cam, event: AppEvent<()>) -> Control {
    if let AppEvent::Input(Event::Key(k)) = event {
        let step = 4.0 / cam.zoom;
        match k.code {
            KeyCode::Left => cam.x -= step,
            KeyCode::Right => cam.x += step,
            KeyCode::Up => cam.y -= step,
            KeyCode::Down => cam.y += step,
            KeyCode::Char('+') | KeyCode::Char('=') => cam.zoom = (cam.zoom * 1.25).min(8.0),
            KeyCode::Char('-') | KeyCode::Char('_') => cam.zoom = (cam.zoom / 1.25).max(0.25),
            _ => {}
        }
    }
    Control::Continue
}

fn main() -> std::io::Result<()> {
    recipes_support::present(
        skins::BLACK_ICE,
        Cam {
            x: 0.0,
            y: 0.0,
            zoom: 1.6,
        },
        update,
        view,
    )
}
