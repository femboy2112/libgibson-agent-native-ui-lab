//! Small external drawing/runner helpers, not an interaction router.
use gibson::input::{Event, KeyCode, KeyEvent, KeyModifiers};
use gibson::{Color, ColorDepth, Context, Node, Rect, Style, Surface};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputKey {
    Tab,
    BackTab,
    Left,
    Right,
    Up,
    Down,
    Enter,
    Char(char),
}
impl InputKey {
    pub fn event(&self) -> KeyEvent {
        KeyEvent::new(
            match *self {
                Self::Tab => KeyCode::Tab,
                Self::BackTab => KeyCode::BackTab,
                Self::Left => KeyCode::Left,
                Self::Right => KeyCode::Right,
                Self::Up => KeyCode::Up,
                Self::Down => KeyCode::Down,
                Self::Enter => KeyCode::Enter,
                Self::Char(c) => KeyCode::Char(c),
            },
            KeyModifiers::empty(),
        )
    }
    fn from_event(key: KeyEvent) -> Option<Self> {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        Some(match key.code {
            KeyCode::Tab => Self::Tab,
            KeyCode::BackTab => Self::BackTab,
            KeyCode::Left => Self::Left,
            KeyCode::Right => Self::Right,
            KeyCode::Up => Self::Up,
            KeyCode::Down => Self::Down,
            KeyCode::Enter => Self::Enter,
            KeyCode::Char(c) if !c.is_control() => Self::Char(c),
            _ => return None,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputRecord {
    pub at_ms: u64,
    pub key: InputKey,
}
pub fn validate_inputs(inputs: &[InputRecord]) -> Result<(), String> {
    if inputs.len() > 128 {
        return Err("input trace exceeds 128 records".into());
    }
    if inputs.windows(2).any(|p| p[0].at_ms > p[1].at_ms) {
        return Err("input timestamps must be ordered".into());
    }
    if inputs
        .iter()
        .any(|r| matches!(r.key,InputKey::Char(c) if c.is_control()))
    {
        return Err("control character in input trace".into());
    }
    Ok(())
}
/// Reconstructs presentation-local focus as well as emitted semantic actions.
/// Configuration (mode/metaphor) belongs to the supplied fresh app.
pub fn replay<A: App>(mut app: A, inputs: &[InputRecord], at_ms: u64) -> Result<A, String> {
    validate_inputs(inputs)?;
    for input in inputs.iter().take_while(|r| r.at_ms <= at_ms) {
        app.key(input.at_ms, input.key.event());
    }
    Ok(app)
}

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
        println!("External LibGibson consumer. Tab focus; arrows select; Enter act; Y/N permission; Space pause; Esc/Ctrl-C exit.\n--auto --at=MS --speed=1..20 --seconds=N --color=truecolor|ansi16|mono --dump --cells --width=120 --height=32\n--record=PATH records explicit application keys; --replay=PATH replays them. Pause/exit are host controls.");
        return Ok(());
    }
    let depth = match val("--color=").unwrap_or("truecolor") {
        "mono" => ColorDepth::Mono,
        "ansi16" => ColorDepth::Ansi16,
        "truecolor" => ColorDepth::TrueColor,
        other => return Err(format!("unsupported color {other}").into()),
    };
    let mut at: u64 = val("--at=").unwrap_or("0").parse()?;
    let replay_inputs: Option<Vec<InputRecord>> = val("--replay=")
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            let file = std::fs::File::open(path)?;
            use std::io::Read;
            let mut bytes = Vec::new();
            file.take(32_769).read_to_end(&mut bytes)?;
            if bytes.len() > 32_768 {
                return Err("input trace exceeds byte budget".into());
            }
            let input: Vec<InputRecord> = serde_json::from_slice(&bytes)?;
            validate_inputs(&input)?;
            Ok(input)
        })
        .transpose()?;
    let mut replay_index = 0;
    let mut recorded = Vec::new();
    if let Some(inputs) = &replay_inputs {
        while replay_index < inputs.len() && inputs[replay_index].at_ms <= at {
            let r = &inputs[replay_index];
            app.key(r.at_ms, r.key.event());
            replay_index += 1;
        }
    }
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
        if let Some(inputs) = &replay_inputs {
            while replay_index < inputs.len() && inputs[replay_index].at_ms <= at {
                let r = &inputs[replay_index];
                app.key(r.at_ms, r.key.event());
                replay_index += 1;
            }
        }
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
                _ if replay_inputs.is_none() => {
                    if let Some(input) = InputKey::from_event(key) {
                        if recorded.len() >= 128 {
                            ctx.restore()?;
                            return Err("input recording budget exhausted; session stopped".into());
                        }
                        recorded.push(InputRecord {
                            at_ms: at,
                            key: input,
                        });
                        app.key(at, key);
                    }
                }
                _ => {}
            }
        }
    }
    ctx.restore()?;
    if let Some(path) = val("--record=") {
        std::fs::write(path, serde_json::to_vec_pretty(&recorded)?)?;
    }
    Ok(())
}
