//! Small external drawing/runner helpers, not an interaction router.
use gibson::input::{Event, KeyCode, KeyEvent, KeyModifiers};
use gibson::{Color, ColorDepth, Context, Node, Rect, Style, Surface};
use std::time::{Duration, Instant};

pub const MAX_WIDTH: u16 = 240;
pub const MAX_HEIGHT: u16 = 80;

pub fn dimensions(w: u16, h: u16) -> (u16, u16) {
    (w.min(MAX_WIDTH), h.min(MAX_HEIGHT))
}
pub fn accent(index: usize) -> Color {
    [Color::Cyan, Color::Yellow, Color::Magenta, Color::Green][index % 4]
}
pub fn realize(mut node: Node, w: u16, h: u16) -> Surface {
    let (w, h) = dimensions(w, h);
    let mut surface = Surface::new(w, h);
    if w > 0 && h > 0 {
        gibson::layout::compute_layout(&mut node, w, h).expect("bounded lab layout");
        gibson::painter::paint(&node, &mut surface);
    }
    surface
}
pub fn text(s: impl Into<String>, color: Color) -> Node {
    Node::text_wrapped(s, Style::new().fg(color), gibson::WrapMode::WordWrap)
}
pub fn put(out: &mut Surface, x: u16, y: u16, s: &str, style: Style) {
    out.print_str(x, y, s, style, Some(out.width.saturating_sub(x)));
}
pub fn mount(out: &mut Surface, node: Node, rect: Rect) {
    if rect.is_empty() {
        return;
    }
    let surface = realize(
        node.width(rect.width as f32).height(rect.height as f32),
        rect.width,
        rect.height,
    );
    out.blit_transparent_clipped(&surface, rect.x as i32, rect.y as i32, rect);
}
pub fn finish(mut s: Surface, depth: ColorDepth) -> Surface {
    for cell in &mut s.cells {
        cell.style = gibson::capability::quantize_style(cell.style, depth);
    }
    s
}
pub fn plain(s: &Surface) -> String {
    (0..s.height)
        .map(|y| {
            (0..s.width)
                .filter_map(|x| {
                    let c = s.get(x, y)?;
                    (!c.is_continuation).then(|| c.glyph.grapheme.to_string())
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Implementers retain all semantic state and explicit event routing.
pub trait App {
    fn frame(&self, at_ms: u64, w: u16, h: u16, depth: ColorDepth) -> Surface;
    fn key(&mut self, at_ms: u64, key: KeyEvent);
    fn end_ms(&self) -> u64 {
        18_000
    }
}

pub fn run(mut app: impl App) -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let val = |p: &str| args.iter().find_map(|a| a.strip_prefix(p));
    let has = |s: &str| args.iter().any(|a| a == s);
    if has("--help") {
        println!("External LibGibson consumer. Tab focus; arrows select; Enter act; Y/N permission; Space pause; Esc/Ctrl-C exit.\n--auto --at=MS --speed=1..20 --seconds=N --color=truecolor|ansi16|mono --dump --cells --width=120 --height=32");
        return Ok(());
    }
    let depth = match val("--color=").unwrap_or("truecolor") {
        "mono" => ColorDepth::Mono,
        "ansi16" => ColorDepth::Ansi16,
        "truecolor" => ColorDepth::TrueColor,
        other => return Err(format!("unsupported color {other}").into()),
    };
    let mut at: u64 = val("--at=").unwrap_or("0").parse()?;
    if has("--dump") || has("--cells") {
        let w = val("--width=").unwrap_or("120").parse()?;
        let h = val("--height=").unwrap_or("32").parse()?;
        let s = app.frame(at, w, h, depth);
        if has("--cells") {
            let cells:Vec<_>=s.cells.iter().map(|c|serde_json::json!({"glyph":c.glyph.grapheme.as_str(),"fg":c.style.fg.and_then(Color::resolve_rgb),"bg":c.style.bg.and_then(Color::resolve_rgb),"reverse":c.style.reverse})).collect();
            println!(
                "{}",
                serde_json::json!({"width":s.width,"height":s.height,"cells":cells})
            );
        } else {
            println!("{}", plain(&s));
        }
        return Ok(());
    }
    let speed = val("--speed=").unwrap_or("1").parse::<u64>()?.clamp(1, 20);
    let seconds = val("--seconds=")
        .map(str::parse::<u64>)
        .transpose()?
        .unwrap_or(120)
        .min(600);
    let mut ctx = Context::fullscreen()?;
    ctx.set_color_depth(depth);
    let mut paused = false;
    let start = Instant::now();
    let mut last = start;
    loop {
        let now = Instant::now();
        if !paused {
            let dt = if has("--deterministic") {
                33
            } else {
                now.duration_since(last).as_millis().min(250) as u64
            };
            at = at.saturating_add(dt * speed).min(app.end_ms());
        }
        last = now;
        let (w, h) = ctx.session.terminal_size();
        ctx.set_root(Node::raster(app.frame(at, w, h, depth)));
        ctx.render_now()?;
        if (has("--auto") && at >= app.end_ms()) || start.elapsed().as_secs() >= seconds {
            break;
        }
        if let Some(Event::Key(key)) = ctx.poll_event(Duration::from_millis(33))? {
            match key.code {
                KeyCode::Esc => break,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                KeyCode::Char(' ') => paused = !paused,
                _ => app.key(at, key),
            }
        }
    }
    ctx.restore()?;
    Ok(())
}
