//! The LibGibson application shell: the `gibson::ui` runtime owns the terminal
//! lifecycle; the whole observatory is one `Node::canvas` hero that paints the
//! composed frame at whatever size the layout engine assigns (so resize is free).

use std::io;
use std::time::Duration;

use gibson::ui::prelude::*;
use gibson::{ColorDepth, Context, Event, Node, RenderMode, Surface};

use crate::compose;
use crate::state::Model;

/// The `gibson::ui` view: a single full-frame canvas.
pub fn view(m: &Model, cx: &BuildCx) -> Element<()> {
    let mono = matches!(cx.environment.color_depth, ColorDepth::Mono);
    let glyphs = cx.environment.glyph_mode;
    let model = m.clone();
    let hero = raw(Node::canvas(move |rect| {
        compose::compose(&model, rect.width, rect.height, mono, glyphs).surface
    }))
    .grow(1.0);
    screen().height(cx.environment.height).child(hero)
}

pub fn update(m: &mut Model, event: AppEvent<()>) -> Control {
    match event {
        AppEvent::Input(Event::Key(k)) => m.key(k),
        AppEvent::Tick(d) => m.tick(d),
        _ => {}
    }
    if m.quit {
        Control::Quit
    } else {
        Control::Continue
    }
}

/// Run interactively in the real terminal. Returns the final model.
pub fn run_interactive(model: Model) -> io::Result<Model> {
    App::fullscreen()
        .skin(skins::BLACK_ICE)
        .motion(MotionPreference::None)
        .fps(30)
        .run(model, update, view)
}

/// Render one frame through the real UI runtime into a headless context and
/// return the visible text (exactly what the terminal would show).
pub fn render_through_runtime(
    m: &Model,
    w: u16,
    h: u16,
    depth: ColorDepth,
    glyph_mode: gibson::SubcellGlyphMode,
) -> io::Result<Vec<String>> {
    let env = UiEnvironment {
        width: w,
        height: h,
        color_depth: depth,
        glyph_mode,
        motion: MotionPreference::None,
    };
    let mut runtime: UiRuntime<()> = UiRuntime::new(skins::BLACK_ICE);
    let mut ctx = Context::headless(RenderMode::Fullscreen, w, h);
    let cx = runtime.build_cx(env, Duration::ZERO);
    let tree = view(m, &cx);
    let frame = runtime
        .frame(&tree, env, Duration::ZERO)
        .map_err(io::Error::other)?;
    ctx.set_root(frame.node);
    ctx.render_now()?;
    Ok(ctx.last_frame_lines())
}

/// ANSI (truecolor) dump of a composed surface, for eyeballing colour.
pub fn surface_to_ansi(s: &Surface) -> String {
    let mut out = String::new();
    for y in 0..s.height {
        let mut last = gibson::Style::new();
        for x in 0..s.width {
            if let Some(c) = s.get(x, y) {
                if c.is_continuation {
                    continue;
                }
                if c.style != last {
                    out.push_str("\x1b[0m");
                    out.push_str(&c.style.to_sgr());
                    last = c.style;
                }
                if c.glyph.is_empty() {
                    out.push(' ');
                } else {
                    out.push_str(c.glyph.grapheme.as_str());
                }
            }
        }
        out.push_str("\x1b[0m\n");
    }
    out
}
