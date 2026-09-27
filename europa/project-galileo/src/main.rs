//! PROJECT GALILEO
//! A REAL-TIME JOVIAN MISSION OPERATIONS / SCIENTIFIC VISUALIZATION ENVIRONMENT
//!
//! Consumes released LibGibson v0.2.0 API (`libgibson = { git = "...", tag = "v0.2.0" }`).
//! Presents a seamless multi-scale physical world across Jovian system, Europa orbit,
//! Europa surface, and subsurface ice shell tomography.

pub mod demo;
pub mod profile;
pub mod render;
pub mod sim;
pub mod ui;

use std::env;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use gibson::capability::ColorDepth;
use gibson::input::{Event, KeyCode, KeyModifiers, TextInputState};
use gibson::ui::prelude::*;
use gibson::{Context, RenderMode};

use crate::demo::DemoScript;
use crate::profile::Profiler;
use crate::ui::command::ZoomAction;
use crate::ui::dashboard::{ActiveView, DashboardAction, DashboardState};

#[derive(Debug, Clone)]
pub struct CliOptions {
    pub demo: bool,
    pub at_ms: Option<u64>,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub color_depth: Option<ColorDepth>,
    pub initial_view: Option<ActiveView>,
    pub profile: bool,
    pub headless: bool,
    pub fps: u32,
    pub max_frames: Option<u64>,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            demo: false,
            at_ms: None,
            width: None,
            height: None,
            color_depth: None,
            initial_view: None,
            profile: false,
            headless: false,
            fps: 30,
            max_frames: None,
        }
    }
}

pub fn parse_args() -> CliOptions {
    let mut opts = CliOptions::default();
    let args: Vec<String> = env::args().collect();

    for arg in args.iter().skip(1) {
        if arg == "--demo" {
            opts.demo = true;
        } else if let Some(val) = arg.strip_prefix("--at-ms=") {
            opts.at_ms = val.parse::<u64>().ok();
        } else if let Some(val) = arg.strip_prefix("--width=") {
            opts.width = val.parse::<u16>().ok();
        } else if let Some(val) = arg.strip_prefix("--height=") {
            opts.height = val.parse::<u16>().ok();
        } else if arg == "--truecolor" {
            opts.color_depth = Some(ColorDepth::TrueColor);
        } else if arg == "--ansi16" {
            opts.color_depth = Some(ColorDepth::Ansi16);
        } else if arg == "--mono" {
            opts.color_depth = Some(ColorDepth::Mono);
        } else if let Some(val) = arg.strip_prefix("--view=") {
            opts.initial_view = match val.to_lowercase().as_str() {
                "system" | "sys" | "1" => Some(ActiveView::System),
                "trajectory" | "traj" | "2" => Some(ActiveView::Trajectory),
                "surface" | "surf" | "3" => Some(ActiveView::Surface),
                "tomography" | "tomo" | "radar" | "4" => Some(ActiveView::Tomography),
                "missioncontrol" | "ops" | "control" | "5" => Some(ActiveView::MissionControl),
                _ => None,
            };
        } else if arg == "--profile" {
            opts.profile = true;
        } else if arg == "--headless" {
            opts.headless = true;
        } else if let Some(val) = arg.strip_prefix("--fps=") {
            opts.fps = val.parse::<u32>().unwrap_or(30);
        } else if let Some(val) = arg.strip_prefix("--frames=") {
            opts.max_frames = val.parse::<u64>().ok();
        }
    }

    opts
}

fn main() -> io::Result<()> {
    let opts = parse_args();

    let width = opts.width.unwrap_or(120);
    let height = opts.height.unwrap_or(40);
    let color_depth = opts.color_depth.unwrap_or(ColorDepth::TrueColor);
    let mono_mode = color_depth == ColorDepth::Mono;

    let mut state = DashboardState::new(mono_mode);
    if let Some(view) = opts.initial_view {
        state.active_view = view;
        state.scale_coordinator.set_view(match view {
            ActiveView::System => crate::render::scale::PrimaryView::System,
            ActiveView::Trajectory => crate::render::scale::PrimaryView::Trajectory,
            ActiveView::Surface => crate::render::scale::PrimaryView::Surface,
            ActiveView::Tomography => crate::render::scale::PrimaryView::Tomography,
            ActiveView::MissionControl => crate::render::scale::PrimaryView::System,
        });
    }

    let demo = DemoScript::new();
    let mut profiler = Profiler::new(width, height);
    let mut runtime = UiRuntime::new(state.resolved_skin());

    // 1. Fixed-time capture mode (--at-ms=)
    if let Some(at_ms) = opts.at_ms {
        if opts.demo || opts.initial_view.is_none() {
            demo.apply_at_ms(&mut state, at_ms);
        } else {
            state.update(at_ms as f32 / 1000.0);
        }

        let mut context = Context::headless(RenderMode::Fullscreen, width, height);
        context.set_color_depth(color_depth);

        let now = Duration::from_millis(at_ms);
        let env = UiEnvironment {
            width,
            height,
            color_depth,
            motion: gibson::ui::MotionPreference::None,
            ..Default::default()
        };

        let cx = runtime.build_cx(env, now);
        let tree = state.view(&cx);
        let frame = runtime.frame(&tree, env, now).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidInput, format!("UiError: {:?}", e))
        })?;

        context.set_root(frame.node);
        context.render_now()?;

        let bytes = context.rendered_bytes();
        if opts.profile {
            profiler.record_frame(
                bytes,
                Duration::from_millis(1),
                runtime.active_animation_count(),
                runtime.retained_key_count(),
            );
            eprintln!("{}", profiler.summary());
        }

        if let Err(e) = io::stdout().write_all(bytes) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                return Err(e);
            }
        }
        let _ = io::stdout().flush();
        return Ok(());
    }

    // 2. Headless mode (--headless or demo headless capture)
    if opts.headless {
        let mut context = Context::headless(RenderMode::Fullscreen, width, height);
        context.set_color_depth(color_depth);

        let max_frames = opts.max_frames.unwrap_or(if opts.demo { 120 } else { 10 });
        let frame_dt = Duration::from_micros((1_000_000.0 / opts.fps as f64) as u64);

        for f in 0..max_frames {
            let t_ms = (f * 1000) / (opts.fps as u64);
            let frame_start = Instant::now();

            if opts.demo {
                demo.apply_at_ms(&mut state, t_ms);
            } else {
                state.update(frame_dt.as_secs_f32());
            }

            let now = Duration::from_millis(t_ms);
            let env = UiEnvironment {
                width,
                height,
                color_depth,
                ..Default::default()
            };

            let cx = runtime.build_cx(env, now);
            let tree = state.view(&cx);
            let frame = runtime.frame(&tree, env, now).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidInput, format!("UiError: {:?}", e))
            })?;

            context.set_root(frame.node);
            context.render_now()?;

            let render_dur = frame_start.elapsed();
            let bytes = context.rendered_bytes();

            if opts.profile {
                profiler.record_frame(
                    bytes,
                    render_dur,
                    runtime.active_animation_count(),
                    runtime.retained_key_count(),
                );
            }
        }

        if opts.profile {
            eprintln!("{}", profiler.summary());
        }
        return Ok(());
    }

    // 3. Fullscreen Interactive Mode
    let mut context = Context::fullscreen()?;
    context.set_color_depth(color_depth);

    let (term_w, term_h) = context.session.terminal_size();
    let _width = opts.width.unwrap_or(term_w);
    let _height = opts.height.unwrap_or(term_h);

    let start_time = Instant::now();
    let frame_interval = Duration::from_micros((1_000_000.0 / opts.fps as f64) as u64);
    let mut last_tick = Instant::now();
    let mut frame_count: u64 = 0;

    loop {
        let frame_start = Instant::now();
        let elapsed = start_time.elapsed();
        let dt = last_tick.elapsed().as_secs_f32();
        last_tick = Instant::now();

        if opts.demo {
            demo.apply_at_ms(&mut state, elapsed.as_millis() as u64);
        } else {
            state.update(dt);
        }

        let (cur_w, cur_h) = context.session.terminal_size();
        let render_w = opts.width.unwrap_or(cur_w);
        let render_h = opts.height.unwrap_or(cur_h);

        let env = UiEnvironment {
            width: render_w,
            height: render_h,
            color_depth,
            ..Default::default()
        };

        // Ensure runtime uses selected skin
        runtime.set_skin(state.resolved_skin());

        let cx = runtime.build_cx(env, elapsed);
        let tree = state.view(&cx);
        let frame = match runtime.frame(&tree, env, elapsed) {
            Ok(f) => f,
            Err(e) => {
                state.log_console(format!("UI FRAME ERROR: {:?}", e));
                continue;
            }
        };

        context.set_root(frame.node);
        context.render_now()?;

        let render_duration = frame_start.elapsed();
        frame_count += 1;

        if opts.profile {
            profiler.record_frame(
                context.rendered_bytes(),
                render_duration,
                runtime.active_animation_count(),
                runtime.retained_key_count(),
            );
        }

        if let Some(limit) = opts.max_frames {
            if frame_count >= limit {
                break;
            }
        }

        // Process Input Events
        let poll_dur = frame_interval.saturating_sub(frame_start.elapsed());
        if let Some(event) = context.poll_event(poll_dur)? {
            // Check for Ctrl+C exit immediately
            if matches!(&event, Event::Key(k) if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL))
            {
                break;
            }

            // Workaround for LibGibson Issue #35:
            // TextInputState unconditionally consumes Alt+<char> chords.
            // Intercept Alt key shortcuts BEFORE passing to runtime.handle_event!
            let mut alt_handled = false;
            if let Event::Key(k) = &event {
                if k.modifiers.contains(KeyModifiers::ALT) {
                    match k.code {
                        KeyCode::Char('1') => {
                            state.handle_action(DashboardAction::SwitchView(ActiveView::System));
                            alt_handled = true;
                        }
                        KeyCode::Char('2') => {
                            state
                                .handle_action(DashboardAction::SwitchView(ActiveView::Trajectory));
                            alt_handled = true;
                        }
                        KeyCode::Char('3') => {
                            state.handle_action(DashboardAction::SwitchView(ActiveView::Surface));
                            alt_handled = true;
                        }
                        KeyCode::Char('4') => {
                            state
                                .handle_action(DashboardAction::SwitchView(ActiveView::Tomography));
                            alt_handled = true;
                        }
                        KeyCode::Char('5') => {
                            state.handle_action(DashboardAction::SwitchView(
                                ActiveView::MissionControl,
                            ));
                            alt_handled = true;
                        }
                        KeyCode::Char('s') => {
                            state.handle_action(DashboardAction::CycleSkin);
                            alt_handled = true;
                        }
                        _ => {}
                    }
                }
            }

            if alt_handled {
                continue;
            }

            // Let UiRuntime handle focus, buttons, and text input edits
            let outcome = runtime.handle_event(&event);
            for action in outcome.actions {
                state.handle_action(action);
            }

            // If not consumed by focused control, dispatch global shortcuts
            if !outcome.consumed {
                if let Event::Key(k) = &event {
                    match k.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            if !state.command_input.text.is_empty() {
                                state.command_input = TextInputState::new();
                            } else {
                                break;
                            }
                        }
                        KeyCode::Char('1') => {
                            state.handle_action(DashboardAction::SwitchView(ActiveView::System))
                        }
                        KeyCode::Char('2') => {
                            state.handle_action(DashboardAction::SwitchView(ActiveView::Trajectory))
                        }
                        KeyCode::Char('3') => {
                            state.handle_action(DashboardAction::SwitchView(ActiveView::Surface))
                        }
                        KeyCode::Char('4') => {
                            state.handle_action(DashboardAction::SwitchView(ActiveView::Tomography))
                        }
                        KeyCode::Char('5') => state
                            .handle_action(DashboardAction::SwitchView(ActiveView::MissionControl)),
                        KeyCode::Char('z') => {
                            state.handle_action(DashboardAction::Zoom(ZoomAction::In))
                        }
                        KeyCode::Char('x') => {
                            state.handle_action(DashboardAction::Zoom(ZoomAction::Out))
                        }
                        KeyCode::Char('n') => state.handle_action(DashboardAction::NextTarget),
                        KeyCode::Char('p') => state.handle_action(DashboardAction::PrevTarget),
                        KeyCode::Char('b') => {
                            state.handle_action(DashboardAction::ExecuteBurn(25.0))
                        }
                        KeyCode::Char('r') => state.handle_action(DashboardAction::ToggleRadarBand),
                        KeyCode::Char('a') => {
                            state.handle_action(DashboardAction::AcknowledgeAlert)
                        }
                        KeyCode::Char('s') => state.handle_action(DashboardAction::CycleSkin),
                        KeyCode::Char(' ') => state.handle_action(DashboardAction::TogglePause),
                        KeyCode::Char('w') | KeyCode::Char('t') => {
                            state.handle_action(DashboardAction::CycleWarpRate)
                        }
                        KeyCode::Char('[') => {
                            state.handle_action(DashboardAction::ScrubTimeline(-5.0))
                        }
                        KeyCode::Char(']') => {
                            state.handle_action(DashboardAction::ScrubTimeline(5.0))
                        }
                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            state.handle_action(DashboardAction::AdjustGain(5.0))
                        }
                        KeyCode::Char('-') | KeyCode::Char('_') => {
                            state.handle_action(DashboardAction::AdjustGain(-5.0))
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            state.handle_action(DashboardAction::Rotate(0.0, 1.0))
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            state.handle_action(DashboardAction::Rotate(0.0, -1.0))
                        }
                        KeyCode::Left | KeyCode::Char('h') => {
                            state.handle_action(DashboardAction::Rotate(-1.0, 0.0))
                        }
                        KeyCode::Right | KeyCode::Char('l') => {
                            state.handle_action(DashboardAction::Rotate(1.0, 0.0))
                        }
                        KeyCode::Char('m') => {
                            state.execute_command(crate::ui::command::Command::ToggleMagnetic)
                        }
                        KeyCode::Char('g') => {
                            state.execute_command(crate::ui::command::Command::ToggleThermal)
                        }
                        KeyCode::Char('o') => {
                            state.execute_command(crate::ui::command::Command::ToggleAutoOrbit)
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if opts.profile {
        eprintln!("{}", profiler.summary());
    }

    Ok(())
}
