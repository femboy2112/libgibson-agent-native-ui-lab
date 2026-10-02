//! Headless rig: the same model/view/runtime the interactive loop uses, but with a virtual
//! terminal, a fixed presentation clock and no wall time. Everything deterministic tests,
//! captures, the matrix and the sustained run drive goes through here.

use crate::app::*;
use crate::ui::*;
use gibson::context::{Context, RenderMode};
use gibson::ui::prelude::*;
use gibson::{ColorDepth, Event, KeyCode, KeyEvent, KeyModifiers, SubcellGlyphMode};
use std::io;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameInfo {
    pub bytes: usize,
    pub exact_changed: usize,
    pub affected: usize,
    pub total_cells: usize,
    pub full_repaint: bool,
    pub gen_us: u64,
    pub write_us: u64,
    pub segments: u32,
    pub plotted: u64,
    pub build_us: u64,
}

pub struct Rig {
    pub ctx: Context,
    pub rt: UiRuntime<Action>,
    pub model: Model,
    pub depth: ColorDepth,
    pub glyphs: SubcellGlyphMode,
    pub motion: MotionPreference,
    pub dt: Duration,
    pub clock: Duration,
    pub total_bytes: u64,
    pub frames: u64,
    pub last: FrameInfo,
    pub last_screen: Option<Layout>,
    pub skin: Skin,
    /// Keep every emitted byte (for PNG previews and byte-level determinism tests).
    pub stream: Option<Vec<u8>>,
}

pub fn parse_depth(s: &str) -> Option<ColorDepth> {
    match s {
        "truecolor" | "true" | "24" => Some(ColorDepth::TrueColor),
        "ansi256" | "256" => Some(ColorDepth::Ansi256),
        "ansi16" | "16" => Some(ColorDepth::Ansi16),
        "mono" | "none" => Some(ColorDepth::Mono),
        _ => None,
    }
}

pub fn depth_name(d: ColorDepth) -> &'static str {
    match d {
        ColorDepth::TrueColor => "truecolor",
        ColorDepth::Ansi256 => "ansi256",
        ColorDepth::Ansi16 => "ansi16",
        ColorDepth::Mono => "mono",
    }
}

impl Rig {
    pub fn headless(
        w: u16,
        h: u16,
        depth: ColorDepth,
        glyphs: SubcellGlyphMode,
        mut options: Options,
    ) -> Rig {
        options.threaded_audio = false;
        if options.audio == AudioMode::Play {
            options.audio = AudioMode::Silent;
        }
        let mut ctx = Context::headless(RenderMode::Fullscreen, w, h);
        ctx.set_color_depth(depth);
        let mut model = Model::new(options);
        model.settle();
        Rig {
            ctx,
            rt: UiRuntime::new(skins::BLACK_ICE),
            model,
            depth,
            glyphs,
            motion: MotionPreference::None,
            dt: Duration::from_millis(33),
            clock: Duration::ZERO,
            total_bytes: 0,
            frames: 0,
            last: FrameInfo::default(),
            last_screen: None,
            skin: skins::BLACK_ICE,
            stream: None,
        }
    }

    pub fn env(&self) -> UiEnvironment {
        let (width, height) = self.ctx.session.terminal_size();
        UiEnvironment {
            width,
            height,
            color_depth: self.ctx.capabilities().color_depth,
            glyph_mode: self.glyphs,
            motion: self.motion,
        }
    }

    /// One frame: advance presentation time by `dt`, build, render, collect stats.
    pub fn frame(&mut self) -> io::Result<FrameInfo> {
        self.model.tick(self.dt);
        self.clock += self.dt;
        self.render_only()
    }

    /// Render without advancing any clock (identical state => identical bytes).
    pub fn render_only(&mut self) -> io::Result<FrameInfo> {
        let env = self.env();
        let t0 = std::time::Instant::now();
        let screen = build_screen(&mut self.model, env);
        let build_us = t0.elapsed().as_micros() as u64;
        let frame = self
            .rt
            .frame(&screen.tree, env, Duration::from_secs_f32(self.model.time))
            .map_err(io::Error::other)?;
        self.ctx.set_root(frame.node);
        self.ctx.render_now()?;
        let rep = self.ctx.last_frame_report();
        let out = self.ctx.take_output();
        self.total_bytes += out.len() as u64;
        if let Some(st) = &mut self.stream {
            st.extend_from_slice(out.as_bytes());
        }
        self.frames += 1;
        self.last_screen = Some(screen.layout);
        let info = FrameInfo {
            bytes: rep.bytes_emitted,
            exact_changed: rep.exact_changed_cells,
            affected: rep.affected_cells,
            total_cells: rep.total_cells,
            full_repaint: rep.full_repaint,
            gen_us: rep.generation_duration.as_micros() as u64,
            write_us: rep.write_duration.as_micros() as u64,
            segments: screen.segments,
            plotted: screen.plotted,
            build_us,
        };
        self.last = info;
        Ok(info)
    }

    /// A *full repaint* of the current state, as the bytes a freshly attached terminal would see.
    /// (Renders through a throwaway headless `Context` + `UiRuntime`; leaves this rig untouched.)
    pub fn snapshot_ansi(&mut self) -> io::Result<Vec<u8>> {
        let (w, h) = self.ctx.session.terminal_size();
        let mut ctx = Context::headless(RenderMode::Fullscreen, w, h);
        ctx.set_color_depth(self.depth);
        let mut rt: UiRuntime<Action> = UiRuntime::new(self.skin);
        let env = self.env();
        let screen = build_screen(&mut self.model, env);
        let frame = rt
            .frame(&screen.tree, env, Duration::from_secs_f32(self.model.time))
            .map_err(io::Error::other)?;
        ctx.set_root(frame.node);
        ctx.render_now()?;
        Ok(ctx.take_output().into_bytes())
    }

    /// Run frames until the camera has settled and effects are quiescent.
    pub fn settle(&mut self) -> io::Result<()> {
        self.model.settle();
        for _ in 0..3 {
            self.frame()?;
        }
        Ok(())
    }

    pub fn route(&mut self, ev: &Event) {
        route_event(&mut self.model, &mut self.rt, ev);
    }

    pub fn key(&mut self, c: char) {
        self.route(&Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::empty(),
        )));
    }
    pub fn code(&mut self, k: KeyCode) {
        self.route(&Event::Key(KeyEvent::new(k, KeyModifiers::empty())));
    }

    pub fn resize(&mut self, w: u16, h: u16) {
        self.ctx.session.set_terminal_size(w, h);
    }

    /// The visible text of the last frame, one string per row.
    pub fn lines(&self) -> Vec<String> {
        self.ctx.last_frame_lines()
    }

    pub fn focus(&self) -> Option<String> {
        self.rt.focus().map(|k| k.to_string())
    }
}

/// Shared by the interactive loop and the rig: UI runtime first, then application keys.
pub fn route_event(model: &mut Model, rt: &mut UiRuntime<Action>, ev: &Event) {
    // any key hands control back from the guided demo
    if matches!(ev, Event::Key(_)) {
        model.demo = None;
    }
    // Esc with no modal open hands focus back to the viewport (so the arrows scrub again).
    if matches!(ev, Event::Key(k) if k.code == KeyCode::Esc) && model.modal == Modal::None {
        rt.set_focus(&Key::named("viewport"));
    }
    let out = rt.handle_event(ev);
    for a in out.actions {
        model.apply(a);
    }
    if !out.consumed {
        if let Some(c) = key_cmd(ev) {
            model.do_cmd(c);
        }
    }
}

/// Tiny script language for captures and tests:
/// `end ; goto 120 ; step -5 ; jump next-decision ; fork replace2 ; branch 1 ; compare ;
///  ab ; collapse ; inspect ; resize 80x24 ; frames 3 ; play ; pause`.
pub fn run_script(rig: &mut Rig, script: &str) -> Result<(), String> {
    for raw in script.split(';') {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        let mut it = t.split_whitespace();
        let head = it.next().unwrap();
        let arg = it.next();
        let num = |a: Option<&str>| -> Result<i64, String> {
            a.ok_or("missing number")?
                .parse::<i64>()
                .map_err(|e| e.to_string())
        };
        match head {
            "end" => rig.model.do_cmd(Cmd::Jump(Jump::End)),
            "start" => rig.model.do_cmd(Cmd::Jump(Jump::Start)),
            "goto" => rig.model.do_cmd(Cmd::GoTo(num(arg)? as u32)),
            "step" => rig.model.do_cmd(Cmd::Step(num(arg)? as i32)),
            "play" => rig.model.do_cmd(Cmd::Play),
            "pause" => rig.model.do_cmd(Cmd::Pause),
            "jump" => {
                let j = match arg.ok_or("jump needs a target")? {
                    "next-landmark" => Jump::NextLandmark,
                    "prev-landmark" => Jump::PrevLandmark,
                    "next-decision" => Jump::NextDecision,
                    "prev-decision" => Jump::PrevDecision,
                    "next-epoch" => Jump::NextEpoch,
                    "prev-epoch" => Jump::PrevEpoch,
                    "next-input" => Jump::NextInput,
                    "prev-input" => Jump::PrevInput,
                    "next-checkpoint" => Jump::NextCheckpoint,
                    "prev-checkpoint" => Jump::PrevCheckpoint,
                    other => return Err(format!("unknown jump {other}")),
                };
                rig.model.do_cmd(Cmd::Jump(j))
            }
            "fork" => {
                use crate::history::Edit;
                let e = match arg.ok_or("fork needs an edit")? {
                    "drop" => Edit::DropCmd,
                    "replace2" => Edit::ReplaceCmd(2),
                    "replace5" => Edit::ReplaceCmd(5),
                    "insert2" => Edit::InsertCmd(2),
                    "insert5" => Edit::InsertCmd(5),
                    "insert1" => Edit::InsertCmd(1),
                    "decision" => {
                        let v = it
                            .next()
                            .ok_or("decision needs a value")?
                            .parse::<i32>()
                            .map_err(|e| e.to_string())?;
                        Edit::Override(v)
                    }
                    other => return Err(format!("unknown fork edit {other}")),
                };
                rig.model.do_cmd(Cmd::ForkEdit(e))
            }
            "branch" => rig.model.do_cmd(Cmd::SwitchBranch(num(arg)? as u16)),
            "compare" => rig.model.do_cmd(Cmd::ToggleCompare),
            "ab" => rig.model.do_cmd(Cmd::ToggleAb),
            "collapse" => rig.model.do_cmd(Cmd::ToggleCollapse),
            "turn" => rig.model.do_cmd(Cmd::Turn),
            "inside" => rig.model.do_cmd(Cmd::ToggleInside),
            "inspect" => rig.model.do_cmd(Cmd::Inspect),
            "openfork" => rig.model.do_cmd(Cmd::OpenFork),
            "help" => rig.model.do_cmd(Cmd::Help),
            "close" => rig.model.do_cmd(Cmd::CloseModal),
            "settle" => rig.settle().map_err(|e| e.to_string())?,
            "key" => rig.key(arg.ok_or("key needs a char")?.chars().next().unwrap()),
            "tab" => rig.code(KeyCode::Tab),
            "enter" => rig.code(KeyCode::Enter),
            "esc" => rig.code(KeyCode::Esc),
            "resize" => {
                let (w, h) = arg
                    .ok_or("resize WxH")?
                    .split_once('x')
                    .ok_or("resize WxH")?;
                rig.resize(
                    w.parse().map_err(|_| "bad width")?,
                    h.parse().map_err(|_| "bad height")?,
                );
            }
            "frames" => {
                for _ in 0..num(arg)? {
                    rig.frame().map_err(|e| e.to_string())?;
                }
            }
            other => return Err(format!("unknown script command `{other}`")),
        }
    }
    Ok(())
}
