//! The interactive application and the headless realiser.
//!
//! One `view()` serves both: the live `gibson::ui::App` loop and the headless
//! `Context` used by captures, the demo and the tests. What you capture is exactly
//! what the interactive session paints.

use crate::render::{compose, Env};
use crate::scenario::FS;
use crate::session::{cmd_for_key, Cmd, Model, Session, SPEEDS};
use gibson::ui::prelude::*;
use gibson::{ColorDepth, Context, Event, Node, RenderMode, SubcellGlyphMode};
use std::io;
use std::sync::Arc;
use std::time::Duration;

/// Typed actions from semantic controls. Pulsar's own input is raw keys (the world
/// is the interface); the only semantic control is the help modal's dismissal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    CloseHelp,
}

pub const HELP: &[&str] = &[
    "VIEWS     1 stream  2 spectrum  3 fold  4 relation  5 sky  6 dossier",
    "TIME      ← →  ±8 s     , .  ±1 s     PgUp PgDn  ±60 s     Home End",
    "          g  go to t    n / N  next / previous analysis event",
    "          Space  play / pause     [ ]  speed (x1 x4 x16 x64 x256)",
    "IDENTITY  Tab / a b c  select ALPHA / BETA / GAMMA",
    "FOCUS     f  overview <-> selected signal      + / -  zoom",
    "COMPARE   p  pin the current time     m  compare pinned vs now",
    "INSPECT   i  cursor on/off     h j k l  move it (readout: coordinates)",
    "TRUTH     t  overlay what was actually injected",
    "OTHER     ?  this help     q  quit     Ctrl-C  quit",
];

/// The same keys, for terminals too narrow for the full table.
pub const HELP_SHORT: &[&str] = &[
    "1-6 views  Tab/a b c identity",
    "<- ->  +-8 s    , .  +-1 s",
    "PgUp/Dn +-60 s  g goto  n/N event",
    "f focus  +/- zoom  m compare",
    "i inspect (h j k l)  t truth",
    "Space play  [ ] speed  q quit",
];

pub fn env_of(cx: &BuildCx) -> Env {
    Env {
        width: cx.environment.width,
        height: cx.environment.height,
        depth: cx.environment.color_depth,
        glyphs: cx.environment.glyph_mode,
    }
}

/// The one view function.
pub fn view(model: &Model, cx: &BuildCx) -> Element<Act> {
    let env = env_of(cx);
    let frame = compose(model, &env);
    let mut tree: Element<Act> = screen()
        .height(env.height)
        .child(raw(Node::surface(Arc::new(frame.status))))
        .child(raw(Node::surface(Arc::new(frame.hero))))
        .child(raw(Node::surface(Arc::new(frame.hint))));
    if model.st.help {
        let lines: &[&str] = if env.width >= 76 { HELP } else { HELP_SHORT };
        let mut body = column();
        for l in lines {
            body = body.child(text(*l));
        }
        tree = tree.overlay(
            modal("PROJECT PULSAR — KEYS")
                .key("help")
                .width(env.width.saturating_sub(4).min(86))
                .height((lines.len() as u16 + 4).min(env.height.saturating_sub(2)))
                .on_dismiss(Act::CloseHelp)
                .child(body),
        );
    }
    tree
}

/// Text and ANSI of one headless frame.
pub struct Rendered {
    pub lines: Vec<String>,
    pub ansi: String,
}

/// Realise a model through the real `gibson::ui` pipeline into a headless terminal.
pub fn render_frame(model: &Model, env: &Env) -> io::Result<Rendered> {
    let uenv = UiEnvironment {
        width: env.width,
        height: env.height,
        color_depth: env.depth,
        glyph_mode: env.glyphs,
        motion: MotionPreference::None,
    };
    let mut rt: UiRuntime<Act> = UiRuntime::new(skins::BLACK_ICE);
    let mut ctx = Context::headless(RenderMode::Fullscreen, env.width, env.height);
    ctx.set_color_depth(env.depth);
    let cx = rt.build_cx(uenv, Duration::ZERO);
    let tree = view(model, &cx);
    let frame = rt.frame(&tree, uenv, Duration::ZERO).map_err(io::Error::other)?;
    ctx.set_root(frame.node);
    ctx.render_now()?;
    let lines = ctx.last_frame_lines();
    let ansi = ctx.take_output();
    Ok(Rendered { lines, ansi })
}

pub struct RunOpts {
    pub seed: u64,
    pub start: Vec<Cmd>,
    pub mono: bool,
    pub glyphs: Option<SubcellGlyphMode>,
}

struct Live {
    sess: Session,
    last: Option<Duration>,
    carry: f64,
}

/// Run the interactive observatory until the user quits; returns the session
/// (whose `log` is the exact replayable record).
pub fn run_interactive(opts: RunOpts) -> io::Result<Session> {
    let mut ctx = Context::fullscreen()?;
    if opts.mono {
        ctx.set_color_depth(ColorDepth::Mono);
    }
    if let Some(g) = opts.glyphs {
        std::env::set_var("LIBGIBSON_GLYPHS", glyph_name(g));
    }
    let mut sess = Session::new(opts.seed);
    sess.model.engine.prewarm();
    for c in opts.start {
        sess.apply(c);
    }
    let live = Live {
        sess,
        last: None,
        carry: 0.0,
    };
    let result = App::fullscreen()
        .skin(skins::BLACK_ICE)
        .motion(MotionPreference::None)
        .fps(30)
        .run_with_context(
            &mut ctx,
            live,
            |s, ev, _ctx| {
                match ev {
                    AppEvent::Action(Act::CloseHelp) => s.sess.apply(Cmd::ToggleHelp),
                    AppEvent::Input(Event::Key(k)) => {
                        if let Some(c) = cmd_for_key(&s.sess.model.st, k) {
                            s.sess.apply(c);
                        }
                    }
                    AppEvent::Input(_) => {}
                    AppEvent::Tick(now) => {
                        let dt = match s.last {
                            Some(prev) => now.saturating_sub(prev),
                            None => Duration::ZERO,
                        };
                        s.last = Some(now);
                        if s.sess.model.st.playing {
                            // wall time enters only here, already quantised to samples,
                            // and is recorded as an exact `Advance`
                            s.carry += dt.as_secs_f64()
                                * SPEEDS[s.sess.model.st.speed as usize] as f64
                                * FS;
                            let k = s.carry.floor();
                            if k >= 1.0 {
                                s.carry -= k;
                                s.sess.apply(Cmd::Advance(k as u32));
                            }
                        }
                        if s.sess.model.st.xfade.is_some() {
                            s.sess.apply(Cmd::Frame(1));
                        }
                    }
                }
                Ok(if s.sess.model.st.quit {
                    Control::Quit
                } else {
                    Control::Continue
                })
            },
            |s, cx| view(&s.sess.model, cx),
        );
    let restored = ctx.restore();
    let live = result?;
    restored?;
    Ok(live.sess)
}

/// Play the built-in cue sheet on a real terminal (bounded: it stops after the last
/// step, or on `q` / Ctrl-C). Step changes are scheduled from the loop's own elapsed
/// time, but each step's *frame* is a pure function of the commands applied — so the
/// picture at every step is identical to the headless `--demo` frame.
pub fn run_demo_live(seed: u64, hold: Duration, mono: bool) -> io::Result<()> {
    use crate::capture::DEMO;
    use crate::session::parse_script;
    let mut ctx = Context::fullscreen()?;
    if mono {
        ctx.set_color_depth(ColorDepth::Mono);
    }
    struct Demo {
        sess: Session,
        step: usize,
        started: Option<Duration>,
    }
    let d = Demo {
        sess: Session::new(seed),
        step: 0,
        started: None,
    };
    d.sess.model.engine.prewarm();
    let result = App::fullscreen()
        .skin(skins::BLACK_ICE)
        .motion(MotionPreference::None)
        .fps(20)
        .run_with_context(
            &mut ctx,
            d,
            |d, ev, _ctx| {
                match ev {
                    AppEvent::Input(Event::Key(k)) => {
                        if matches!(k.code, gibson::KeyCode::Char('q') | gibson::KeyCode::Esc) {
                            return Ok(Control::Quit);
                        }
                    }
                    AppEvent::Tick(now) => {
                        let due = match d.started {
                            None => true,
                            Some(t0) => now.saturating_sub(t0) >= hold,
                        };
                        if due {
                            if d.step >= DEMO.len() {
                                return Ok(Control::Quit);
                            }
                            if let Ok(cmds) = parse_script(DEMO[d.step].script) {
                                for c in cmds {
                                    d.sess.apply(c);
                                }
                            }
                            d.sess.apply(Cmd::Frame(255));
                            d.step += 1;
                            d.started = Some(now);
                        }
                    }
                    _ => {}
                }
                Ok(Control::Continue)
            },
            |d, cx| view(&d.sess.model, cx),
        );
    let restored = ctx.restore();
    result?;
    restored
}

pub fn glyph_name(g: SubcellGlyphMode) -> &'static str {
    match g {
        SubcellGlyphMode::Braille2x4 => "braille",
        SubcellGlyphMode::HalfBlock1x2 => "halfblock",
        SubcellGlyphMode::Block => "block",
        SubcellGlyphMode::Ascii => "ascii",
        _ => "braille",
    }
}

pub fn parse_glyphs(s: &str) -> Option<SubcellGlyphMode> {
    match s {
        "braille" => Some(SubcellGlyphMode::Braille2x4),
        "halfblock" | "half" => Some(SubcellGlyphMode::HalfBlock1x2),
        "block" => Some(SubcellGlyphMode::Block),
        "ascii" => Some(SubcellGlyphMode::Ascii),
        _ => None,
    }
}
