//! PROJECT CATHEDRAL — a deterministic simulated distributed-system incident whose
//! state is at once a living architectural structure and a HumanMusic score.
//!
//! External pressure test of the released `libgibson` **v0.4.0** public Rust API.
//! No path override, no patch, no upstream source change.

// The crate exposes a deliberately rich internal API: the interactive host, the
// headless runner, the capabilities harness, replay, and tests each use a different
// subset, so accessors that are unreached on one path are still load-bearing on another.
#![allow(dead_code)]

mod action;
mod app;
mod audio_out;
mod capability;
mod cli;
mod hash;
mod incident;
mod music;
mod profile;
mod replay;
mod rng;
mod scenario;
mod sim;
mod visual;

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use gibson::context::{Context, RenderMode};
use gibson::input::{Event, KeyEvent};
use gibson::surface::Surface;
use gibson::Node;

use app::App;
use capability::world_id;
use cli::{Options, HELP};
use profile::{rss_kib, FrameSample, Profile};
use replay::RecordFile;
use visual::{build_frame, Camera, Layout, Scale, ViewState};

fn main() {
    if let Err(error) = run() {
        eprintln!("cathedral: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(e) if e == "help" => {
            print!("{HELP}");
            return Ok(());
        }
        Err(e) => {
            eprintln!("{HELP}");
            return Err(e.into());
        }
    };

    if options.capability {
        return capability::run(&options);
    }

    if let Some(path) = &options.replay {
        let rec = replay::read_record(path).map_err(io::Error::other)?;
        let report =
            replay::run_replay(&rec, world_id(&options.world)).map_err(io::Error::other)?;
        println!("{report}");
        return Ok(());
    }

    let interactive = io::stdout().is_terminal()
        && !options.dump
        && !options.text
        && options.frames.is_none()
        && options.at.is_none();

    if interactive {
        run_interactive(&options)
    } else {
        run_headless(&options)
    }
}

fn build_view<'a>(
    app: &'a App,
    camera: &'a Camera,
    layout: &'a Layout,
    scale: Scale,
    now_playing: &'a str,
    music_state: &'a str,
) -> ViewState<'a> {
    ViewState {
        engine: &app.engine,
        layout,
        camera,
        phase: app.tracker.phase,
        scale,
        selected: app.selected,
        frame: app.frame,
        paused: app.paused,
        journal_len: app.journal.len(),
        incident_len: app.tracker.records.len(),
        now_playing,
        music_state,
        message: &app.message,
        color_depth: app.color_depth,
        show_help: app.show_help,
    }
}

fn music_state_string(app: &App) -> (String, String) {
    let now = app
        .music
        .take
        .as_ref()
        .map(|t| t.now_playing())
        .unwrap_or_else(|| "—".into());
    let (sem, _) = app.music.current_semantics();
    let s = format!(
        "T{:?} E{:?} D{:?} L{:?}",
        sem.tone, sem.emphasis, sem.density, sem.elevation
    );
    (now, s)
}

// ── headless / dump runner ───────────────────────────────────────────────────

fn run_headless(options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    let (w, h) = (options.width.unwrap_or(160), options.height.unwrap_or(50));
    let mut ctx = Context::headless(RenderMode::Fullscreen, w, h);
    ctx.set_color_depth(options.color.depth());

    let mut app = App::new(options.seed, world_id(&options.world), options.wtf);
    app.enable_music = options.music;
    app.color_depth = options.color.depth();

    let frames = options.at.unwrap_or_else(|| options.frames.unwrap_or(1));
    let mut profile = Profile::new();
    profile.color_depth = options.color.depth();
    let mut checkpointer = replay::Checkpointer::new(200);
    let mut prev_stats = ctx.stats();
    let mut last_out = String::new();
    let mut last_text = String::new();

    for _ in 0..frames {
        let t0 = profile::now_us();
        app.aim(w as f32, (h as f32 - 3.0).max(1.0));
        let outcome = app.step();
        for entry in outcome.scrollback {
            if std::env::var_os("CATHEDRAL_TRACE").is_some() {
                eprintln!("[t={}] {}", app.frame, entry.text);
            }
            ctx.insert_text_before_live(&entry.text)?;
            profile.scrollback_entries += 1;
        }
        checkpointer.sample(&app);
        let t1 = profile::now_us();
        let (now_playing, music_state) = music_state_string(&app);
        let surface = render_surface(&app, w, h, &now_playing, &music_state);
        if options.text {
            last_text = surface.to_visible_lines().join("\n");
        }
        let t2 = profile::now_us();
        ctx.set_root(Node::raster(surface));
        ctx.render_now()?;
        let stats = ctx.stats();
        let out = ctx.take_output();
        if options.dump || options.at.is_some() {
            last_out = out.clone();
        }
        let bytes = out.len() as u64;
        let dirty = stats.dirty_cells.saturating_sub(prev_stats.dirty_cells);
        let exact = ctx.last_frame_report().exact_changed_cells as u64;
        profile.record(FrameSample {
            frame_time_us: profile::now_us().saturating_sub(t0),
            render_us: stats.last_render_duration_micros,
            surface_build_us: t2.saturating_sub(t1),
            bytes,
            dirty_cells: dirty,
            exact_changed: exact,
            history_insertions: 0,
            rss_kib: rss_kib(),
        });
        profile.absorb_stats(&stats, &prev_stats);
        prev_stats = stats;
    }

    profile.music_builds = app.music.rebuilds;
    profile.music_last_us = app.music.last_cost.as_micros() as u64;
    profile.music_total_us = app.music.cumulative_cost.as_micros() as u64;

    // Music artifact + hash. `--play` implies an export to DIR or the cwd.
    let export_dir = options
        .music_out
        .clone()
        .or_else(|| options.play.then(|| ".".to_string()));
    let mut wav_sha = None;
    if let Some(dir) = &export_dir {
        if app.music.take.is_some() {
            std::fs::create_dir_all(dir)?;
            let path = std::path::Path::new(dir).join("cathedral.wav");
            let (out, pcm_sha) = app.music.export(
                &path,
                gibson::audio::SampleRate::STUDIO,
                512,
                &app.music.world,
            )?;
            wav_sha = Some(pcm_sha.clone());
            println!(
                "WAV {}  pcm_sha256={}  rebuilds={}  checked={}",
                path.display(),
                pcm_sha,
                app.music.rebuilds,
                app.music
                    .take
                    .as_ref()
                    .map(|t| t.receipt_ok)
                    .unwrap_or(false)
            );
            if options.play {
                let _ = io::stdout().flush();
                match audio_out::LiveAudio::start(&app) {
                    Ok(mut live) => {
                        let sr = gibson::audio::SampleRate::STUDIO.get();
                        let full = out.audio.frames() as f64 / sr as f64;
                        let secs = options
                            .play_seconds
                            .map(|s| (s as f64).min(full))
                            .unwrap_or(full);
                        eprintln!(
                            "cathedral: streaming to the default output device at {} Hz \
                             for ~{:.0}s (device backend, no external player; Ctrl-C to stop)",
                            live.sample_rate(),
                            secs
                        );
                        std::thread::sleep(Duration::from_secs_f64(secs.max(0.0)));
                        live.stop();
                        eprintln!(
                            "cathedral: playback stopped (underruns={}); WAV at {}",
                            live.underruns(),
                            path.display()
                        );
                    }
                    Err(reason) => eprintln!(
                        "cathedral: no audio device ({reason}); the WAV is at {}",
                        path.display()
                    ),
                }
            }
        }
    }

    // Record the deterministic run.
    if let Some(path) = &options.record {
        let rec = RecordFile {
            app: "project-cathedral".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            libgibson_commit: LIBSIBSON_COMMIT.into(),
            journal: app.journal.clone(),
            frames,
            final_digest: app.digest(),
            final_frame: app.frame,
            checkpoints: checkpointer.samples.clone(),
            checkpoint_every: checkpointer.every,
            enable_music: app.enable_music,
            wav_sha256: wav_sha.clone(),
        };
        replay::write_record(path, &rec)?;
        println!("record written: {path}");
    }

    if options.text {
        io::stdout().write_all(last_text.as_bytes())?;
        if !last_text.ends_with('\n') {
            io::stdout().write_all(b"\n")?;
        }
        io::stdout().flush()?;
    } else if options.dump || options.at.is_some() {
        io::stdout().write_all(last_out.as_bytes())?;
        io::stdout().flush()?;
    }

    if options.frames.unwrap_or(0) > 1 || options.at.is_some() {
        eprintln!("{}", profile.summary(options.fps));
        println!(
            "CATHEDRAL_METRICS {}",
            serde_json::json!({
                "frames": profile.frames,
                "services": app.engine.len(),
                "edges": app.engine.fixture.edges().len(),
                "clusters": app.engine.fixture.clusters.len(),
                "mean_frame_us": profile.mean_frame_us(),
                "p95_frame_us": profile.percentile_us(0.95),
                "max_frame_us": profile.max_frame_us(),
                "mean_render_us": profile.mean_render_us(),
                "mean_bytes": profile.mean_bytes(),
                "total_bytes": profile.total_bytes,
                "mean_dirty_cells": profile.mean_dirty(),
                "mean_exact_changed": profile.mean_exact(),
                "history_insertions": profile.total_history_insertions,
                "max_history_per_frame": profile.max_history_insertions_per_frame,
                "scrollback_entries": profile.scrollback_entries,
                "rss_start_kib": profile.rss_start_kib,
                "rss_end_kib": profile.rss_end_kib,
                "rss_peak_kib": profile.rss_peak_kib,
                "music_builds": profile.music_builds,
                "music_rejections": app.music.rejections.len(),
                "music_last_us": profile.music_last_us,
                "music_total_us": profile.music_total_us,
                "final_digest": app.digest(),
                "journal_digest": app.journal.digest(),
                "actions": app.journal.len(),
                "incident_events": app.tracker.records.len(),
                "phase": app.tracker.phase.label(),
                "phases": app
                    .tracker
                    .records
                    .iter()
                    .map(|r| serde_json::json!([r.frame, r.phase.label()]))
                    .collect::<Vec<_>>(),
                "music_checked": app.music.take.as_ref().map(|t| t.receipt_ok),
                "wav_sha256": wav_sha,
            })
        );
    }
    Ok(())
}

// The exact v0.4.0 commit this crate's lockfile resolves to. Kept as a literal so a
// binary can report it without reading Cargo.lock at runtime.
const LIBSIBSON_COMMIT: &str = "c2f6483d92fe2b351e6cd50936a97d8cdf73cb79";

fn render_surface(app: &App, w: u16, h: u16, now_playing: &str, music_state: &str) -> Surface {
    let layout = &app.layout;
    let camera = &app.camera;
    let view = build_view(app, camera, layout, app.scale, now_playing, music_state);
    build_frame(&view, w, h)
}

// ── interactive runner ───────────────────────────────────────────────────────

fn run_interactive(options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = Context::fullscreen()?;
    ctx.set_color_depth(options.color.depth());
    ctx.set_sync_updates(true);
    ctx.session.enter_interactive()?;

    let mut app = App::new(options.seed, world_id(&options.world), options.wtf);
    app.enable_music = options.music;
    app.color_depth = options.color.depth();

    let tick = Duration::from_micros(1_000_000 / options.fps.max(1) as u64);
    let mut next = Instant::now();
    let mut profile = Profile::new();
    profile.color_depth = options.color.depth();
    let mut prev_stats = ctx.stats();
    let mut checkpointer = replay::Checkpointer::new(200);
    // In-process device playback (libgibson `audio-cpal` → ALSA on Linux).
    let mut live: Option<audio_out::LiveAudio> = None;
    let mut autoplayed = false;

    while !app.should_quit {
        // Drain pending input without blocking the animation clock.
        loop {
            let wait = next
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(8));
            match ctx.poll_event(wait)? {
                Some(Event::Key(k)) => {
                    app.handle_key(k);
                }
                Some(Event::Resize(_, _)) => {}
                Some(_) => {}
                None => break,
            }
        }
        if app.should_quit {
            break;
        }
        let now = Instant::now();
        if now < next {
            std::thread::sleep(next - now);
            continue;
        }

        let t0 = profile::now_us();
        let (w, h) = ctx.session.terminal_size();
        app.aim(w as f32, (h as f32 - 3.0).max(1.0));
        let outcome = app.step();
        for entry in outcome.scrollback {
            if std::env::var_os("CATHEDRAL_TRACE").is_some() {
                eprintln!("[t={}] {}", app.frame, entry.text);
            }
            ctx.insert_text_before_live(&entry.text)?;
            profile.scrollback_entries += 1;
        }
        checkpointer.sample(&app);
        let t1 = profile::now_us();
        let (now_playing, music_state) = music_state_string(&app);
        let surface = render_surface(&app, w, h, &now_playing, &music_state);
        let t2 = profile::now_us();
        ctx.set_root(Node::raster(surface));
        ctx.render_now()?;
        let stats = ctx.stats();
        let out = ctx.take_output();
        let _ = out;
        let bytes = stats.frame_bytes.saturating_sub(prev_stats.frame_bytes);
        let dirty = stats.dirty_cells.saturating_sub(prev_stats.dirty_cells);
        let exact = ctx.last_frame_report().exact_changed_cells as u64;
        profile.record(FrameSample {
            frame_time_us: profile::now_us().saturating_sub(t0),
            render_us: stats.last_render_duration_micros,
            surface_build_us: t2.saturating_sub(t1),
            bytes,
            dirty_cells: dirty,
            exact_changed: exact,
            history_insertions: 0,
            rss_kib: rss_kib(),
        });
        profile.absorb_stats(&stats, &prev_stats);
        prev_stats = stats;
        let _ = (t1, t0);

        if app.request_export {
            app.request_export = false;
            let dir = options.music_out.clone().unwrap_or_else(|| ".".into());
            if app.music.take.is_some() {
                std::fs::create_dir_all(&dir)?;
                let path = std::path::Path::new(&dir).join("cathedral.wav");
                let (_o, sha) = app.music.export(
                    &path,
                    gibson::audio::SampleRate::STUDIO,
                    512,
                    &app.music.world,
                )?;
                if let Some(mut prev) = live.take() {
                    prev.stop();
                }
                let short = &sha[..12.min(sha.len())];
                match audio_out::LiveAudio::start(&app) {
                    Ok(l) => {
                        let rate = l.sample_rate();
                        live = Some(l);
                        app.message = format!("playing @ {rate} Hz (W restarts, Q stops)");
                        ctx.insert_text_before_live(&format!(
                            "♪ PLAY  {}  pcm_sha256={}  device={} Hz",
                            path.display(),
                            short,
                            rate
                        ))?;
                    }
                    Err(reason) => {
                        app.message = format!("wrote {} — no device", path.display());
                        ctx.insert_text_before_live(&format!(
                            "♪ EXPORT {}  pcm_sha256={}  device=unavailable ({reason})",
                            path.display(),
                            short
                        ))?;
                    }
                }
                profile.scrollback_entries += 1;
            } else {
                app.message = "no performance to export yet".into();
            }
        }

        // `--play`: start the first available checked take, once.
        if options.play && !autoplayed && app.music.take.is_some() {
            let dir = options.music_out.clone().unwrap_or_else(|| ".".into());
            std::fs::create_dir_all(&dir)?;
            let path = std::path::Path::new(&dir).join("cathedral.wav");
            let (_o, sha) = app.music.export(
                &path,
                gibson::audio::SampleRate::STUDIO,
                512,
                &app.music.world,
            )?;
            let short = &sha[..12.min(sha.len())];
            match audio_out::LiveAudio::start(&app) {
                Ok(l) => {
                    let rate = l.sample_rate();
                    live = Some(l);
                    ctx.insert_text_before_live(&format!(
                        "♪ AUTOPLAY {}  pcm_sha256={}  device={} Hz  (Q stops)",
                        path.display(),
                        short,
                        rate
                    ))?;
                }
                Err(reason) => {
                    ctx.insert_text_before_live(&format!(
                        "♪ EXPORTED {}  pcm_sha256={}  device=unavailable ({reason})",
                        path.display(),
                        short
                    ))?;
                }
            }
            profile.scrollback_entries += 1;
            autoplayed = true;
        }

        next += tick;
        // Do not accumulate an unbounded catch-up storm after a stall.
        if Instant::now() > next + tick * 4 {
            next = Instant::now() + tick;
        }
    }

    if let Some(mut l) = live.take() {
        l.stop();
    }
    ctx.restore()?;

    profile.music_builds = app.music.rebuilds;
    profile.music_total_us = app.music.cumulative_cost.as_micros() as u64;
    if let Some(path) = &options.record {
        let rec = RecordFile {
            app: "project-cathedral".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            libgibson_commit: LIBSIBSON_COMMIT.into(),
            journal: app.journal.clone(),
            frames: app.frame as u64,
            final_digest: app.digest(),
            final_frame: app.frame,
            checkpoints: checkpointer.samples.clone(),
            checkpoint_every: checkpointer.every,
            enable_music: app.enable_music,
            wav_sha256: None,
        };
        replay::write_record(path, &rec)?;
    }
    eprintln!("{}", profile.summary(options.fps));
    Ok(())
}

#[allow(dead_code)]
fn _unused(_: KeyEvent) {}
