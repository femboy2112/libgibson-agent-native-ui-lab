//! SYNESTHESIA — a playable terminal sequencer whose signal is the screen.

mod app;
mod audio;
mod cli;
mod engine;
mod input_pump;
mod profile;
#[cfg(test)]
mod unit_tests;
mod visual;

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use gibson::capability::{ColorDepth, TerminalCapabilities};
use gibson::context::{Context, RenderMode};
use gibson::surface::Surface;
use gibson::{compute_diff, Node};

use app::AppModel;
use cli::{ColorChoice, Options, HELP};
use input_pump::InputPump;
use profile::Profile;
use visual::ViewState;

fn main() {
    if let Err(error) = run() {
        eprintln!("synesthesia: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) if error.0 == "help" => {
            print!("{HELP}");
            return Ok(());
        }
        Err(error) => {
            eprintln!("{HELP}");
            return Err(Box::new(error));
        }
    };
    let glyph_mode =
        gibson::detect_glyph_mode_from_env(options.glyphs.as_deref()).map_err(io::Error::other)?;
    let mut model = AppModel::from_options(&options).map_err(io::Error::other)?;

    if options.headless() || (options.silent && !io::stdout().is_terminal()) {
        run_headless(options, model, glyph_mode)
    } else {
        run_interactive(options, &mut model, glyph_mode)
    }
}

fn run_headless(
    options: Options,
    mut model: AppModel,
    glyph_mode: gibson::SubcellGlyphMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let width = options.width.unwrap_or(120);
    let height = options.height.unwrap_or(40);
    let mut context = Context::headless(RenderMode::Fullscreen, width, height);
    configure_context(&mut context, options.color);
    let fixed_time = options.at_ms;
    let unattended_demo = options.silent
        && !io::stdout().is_terminal()
        && (options.demo || options.performance)
        && !options.dump;
    let count = if fixed_time.is_some() {
        1
    } else {
        options
            .frames
            .unwrap_or(if unattended_demo { 120 } else { 1 })
    };
    let interval = Duration::from_micros(1_000_000 / options.fps as u64);
    let mut profile = Profile::new(0);
    let mut previous_surface: Option<Surface> = None;
    let mut previous_stats = context.stats();
    let mut last_bytes = String::new();

    if let Some(at_ms) = fixed_time {
        visual::history_fill(&model.engine, at_ms, model.seed, &mut model.history);
    }

    for index in 0..count {
        let loop_start = Instant::now();
        // A 30 FPS interval is 33⅓ ms. Rounding it to 33 ms on every
        // synthetic frame would drift three seconds over a 9,000-frame run.
        let at_ms = fixed_time.unwrap_or_else(|| {
            ((index as u128 * 1_000) / options.fps as u128).min(u64::MAX as u128) as u64
        });
        let dsp_start = Instant::now();
        let frame = model.engine.render_at_ms(at_ms, model.seed);
        let dsp_time = dsp_start.elapsed();
        if fixed_time.is_none() {
            model.history.push(&frame.spectrum);
        }
        let modal = model.modal_label();
        let view = ViewState {
            playing: model.playing,
            performance: model.performance,
            focus: model.focus,
            selected_track: model.selected_track,
            selected_step: model.selected_step,
            glyph_mode,
            seed: model.seed,
            modal: modal.as_deref(),
            audio_status: "SILENT / deterministic simulation",
            fps: model.fps,
            events_seen: model.events_seen,
            key_events_seen: model.key_events_seen,
            input_profile: options.profile,
        };
        let scene = visual::build(&model.engine, &frame, &model.history, &view, width, height);
        let mut exact_changed = 0;
        if options.profile {
            exact_changed = measure_exact_delta(&scene.root, width, height, &mut previous_surface)?;
        }
        context.set_root(scene.root);
        context.render_now()?;
        let rendered_bytes = context.take_output();
        if options.dump {
            last_bytes = rendered_bytes;
        }
        let current_stats = context.stats();
        let bytes = current_stats
            .frame_bytes
            .saturating_sub(previous_stats.frame_bytes);
        let affected = current_stats
            .dirty_cells
            .saturating_sub(previous_stats.dirty_cells);
        let renderer_us = current_stats.last_render_duration_micros;
        if options.profile {
            profile.record(
                loop_start.elapsed(),
                dsp_time,
                scene.surface_us,
                renderer_us,
                bytes,
                exact_changed,
                affected,
                scene.generated_nodes,
                interval,
                current_stats
                    .skipped_frames
                    .saturating_sub(previous_stats.skipped_frames),
            );
        }
        previous_stats = current_stats;
    }

    if options.dump {
        io::stdout().write_all(last_bytes.as_bytes())?;
        io::stdout().flush()?;
    }
    if options.profile {
        profile.retained_history = model.history.len();
        eprintln!("{}", profile.summary(options.fps));
    }
    Ok(())
}

fn run_interactive(
    options: Options,
    model: &mut AppModel,
    glyph_mode: gibson::SubcellGlyphMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut context = Context::fullscreen()?;
    configure_context(&mut context, options.color);
    context.session.enter_interactive()?;
    let mut input_pump = InputPump::start();
    let audio = if options.silent {
        None
    } else {
        match audio::AudioOutput::try_new(model.seed) {
            Ok(output) => Some(output),
            Err(error) => {
                eprintln!("audio unavailable; continuing in silent simulation: {error}");
                None
            }
        }
    };
    let audio_status = if options.silent {
        "SILENT / simulation".to_owned()
    } else {
        audio
            .as_ref()
            .map(|a| a.status().to_owned())
            .unwrap_or_else(|| "SILENT / no output device".into())
    };
    if let Some(output) = &audio {
        output.update(&model.engine, model.playing);
    }

    let interval = Duration::from_micros(1_000_000 / options.fps as u64);
    context.set_max_fps(options.fps);
    context.set_animation_interval(interval);
    let mut next_frame = Instant::now();
    let mut profile = Profile::new(0);
    let mut previous_surface: Option<Surface> = None;
    let mut previous_stats = context.stats();
    let mut application_late = 0_u64;

    while !model.should_quit {
        for _ in 0..128 {
            let Ok(event) = input_pump.try_recv() else {
                break;
            };
            model.handle_event(&event, Instant::now());
            if let Some(output) = &audio {
                output.update(&model.engine, model.playing);
            }
        }
        if model.should_quit {
            break;
        }
        let now = Instant::now();
        if now < next_frame {
            let wait = next_frame.saturating_duration_since(now);
            if let Some(event) = input_pump.recv_timeout(wait) {
                model.handle_event(&event, Instant::now());
                if let Some(output) = &audio {
                    output.update(&model.engine, model.playing);
                }
            }
            continue;
        }

        let loop_start = Instant::now();
        let music_ms = model.music_time_ms(loop_start);
        let dsp_start = Instant::now();
        let frame = model.engine.render_at_ms(music_ms, model.seed);
        let dsp_time = dsp_start.elapsed();
        // HOLD freezes the spectral timeline; an edit while held still adds
        // one new row when it changes the actual signal.
        if model.playing || model.history.age(0) != Some(&frame.spectrum) {
            model.history.push(&frame.spectrum);
        }
        let modal = model.modal_label();
        let status = if let Some(output) = &audio {
            let timing = output.timing();
            if output.backend_errors() > 0 {
                format!(
                    "{} / PCM max {:.2} / backend errors {}",
                    output.status(),
                    timing.peak_sample,
                    output.backend_errors()
                )
            } else {
                format!("{} / PCM max {:.2}", output.status(), timing.peak_sample)
            }
        } else {
            audio_status.clone()
        };
        let (actual_width, actual_height) = context.session.terminal_size();
        let width = options.width.unwrap_or(actual_width);
        let height = options.height.unwrap_or(actual_height);
        let view = ViewState {
            playing: model.playing,
            performance: model.performance,
            focus: model.focus,
            selected_track: model.selected_track,
            selected_step: model.selected_step,
            glyph_mode,
            seed: model.seed,
            modal: modal.as_deref(),
            audio_status: &status,
            fps: model.fps,
            events_seen: model.events_seen,
            key_events_seen: model.key_events_seen,
            input_profile: options.profile,
        };
        let scene = visual::build(&model.engine, &frame, &model.history, &view, width, height);
        let exact_changed = if options.profile {
            measure_exact_delta(
                &scene.root,
                actual_width,
                actual_height,
                &mut previous_surface,
            )?
        } else {
            0
        };
        context.set_root(scene.root);
        context.render_now()?;
        let current_stats = context.stats();
        let bytes = current_stats
            .frame_bytes
            .saturating_sub(previous_stats.frame_bytes);
        let affected = current_stats
            .dirty_cells
            .saturating_sub(previous_stats.dirty_cells);
        let renderer_us = current_stats.last_render_duration_micros;
        let frame_time = loop_start.elapsed();
        let now = Instant::now();
        let late_steps = if now > next_frame + interval {
            (now.duration_since(next_frame).as_nanos() / interval.as_nanos()).min(u64::MAX as u128)
                as u64
        } else {
            0
        };
        application_late += late_steps;
        if options.profile {
            profile.record(
                frame_time,
                dsp_time,
                scene.surface_us,
                renderer_us,
                bytes,
                exact_changed,
                affected,
                scene.generated_nodes,
                interval,
                current_stats
                    .skipped_frames
                    .saturating_sub(previous_stats.skipped_frames)
                    + late_steps,
            );
        }
        previous_stats = current_stats;
        next_frame += interval * (late_steps + 1).max(1) as u32;

        // Audio state is published on edits and transport changes above. Its
        // callback owns its clock and never waits for a visual frame.
    }

    input_pump.stop();
    if let Some(output) = &audio {
        eprintln!("audio backend stream errors: {}", output.backend_errors());
        let timing = output.timing();
        eprintln!(
            "audio callback timing: callbacks {} nonzero {} peak {:.4} mean {:.2} us max {:.2} us over-budget {} backend errors {}",
            timing.callbacks,
            timing.nonzero_callbacks,
            timing.peak_sample,
            timing.mean_duration_ns as f64 / 1_000.0,
            timing.max_duration_ns as f64 / 1_000.0,
            timing.over_budget_callbacks,
            timing.backend_errors,
        );
    }
    eprintln!("input backend errors: {}", input_pump.errors());
    eprintln!("application input events received: {}", model.events_seen);
    eprintln!("application key events received: {}", model.key_events_seen);
    eprintln!(
        "application resize events received: {}",
        model.resize_events_seen
    );
    context.restore()?;
    if options.profile {
        profile.retained_history = model.history.len();
        eprintln!("{}", profile.summary(options.fps));
        eprintln!("application deadline misses: {application_late}");
    }
    Ok(())
}

fn configure_context(context: &mut Context, choice: ColorChoice) {
    match choice {
        ColorChoice::TrueColor => context.set_color_depth(ColorDepth::TrueColor),
        ColorChoice::Ansi256 => context.set_color_depth(ColorDepth::Ansi256),
        ColorChoice::Ansi16 => context.set_color_depth(ColorDepth::Ansi16),
        ColorChoice::Mono => context.set_color_depth(ColorDepth::Mono),
        ColorChoice::Auto => context.set_capabilities(TerminalCapabilities::detect_from_env()),
    }
}

/// Rebuilds the same public Node -> layout -> painter surface the renderer uses,
/// then asks the release API for exact semantic cell changes (including erasures).
fn measure_exact_delta(
    root: &Node,
    width: u16,
    height: u16,
    previous: &mut Option<Surface>,
) -> io::Result<usize> {
    let mut measured_root = root.clone();
    gibson::compute_layout(&mut measured_root, width, height).map_err(io::Error::other)?;
    let mut current = Surface::new(width, height);
    gibson::painter::paint(&measured_root, &mut current);
    let count = compute_diff(previous.as_ref(), &current).exact_changed_cell_count();
    *previous = Some(current);
    Ok(count)
}
