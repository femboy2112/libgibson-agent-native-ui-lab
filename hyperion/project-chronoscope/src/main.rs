//! Project Chronoscope — a debugger for alternate histories.
//!
//! Interactive (needs a TTY):     chronoscope
//! Headless capture:              chronoscope --capture --size=120x40 --color=truecolor \
//!                                    --script="end; goto 50; fork replace2; goto 215; compare"
//! Sustained deterministic run:   chronoscope --sustained --frames=12000
//! Render branch audio to WAV:    chronoscope --wav-out=DIR --script="..."

use gibson::context::Context;
use gibson::glyph::GlyphChoice;
use gibson::ui::prelude::*;
use gibson::{ColorDepth, Event, KeyCode, KeyModifiers, SubcellGlyphMode};
use project_chronoscope::app::*;
use project_chronoscope::audio::*;
use project_chronoscope::bench::*;
use project_chronoscope::driver::*;
use project_chronoscope::ui::*;
use std::time::{Duration, Instant};

static SIGNALLED: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

extern "C" fn on_signal(sig: libc::c_int) {
    // async-signal-safe: only an atomic store; the main loop notices and unwinds normally
    SIGNALLED.store(sig, std::sync::atomic::Ordering::SeqCst);
}

/// LibGibson restores the terminal on explicit restore, Drop and panic — not on a signal's
/// default action. SIGTERM/SIGHUP (closing the window, `kill`) are turned into a normal exit.
fn install_signal_flag() {
    unsafe {
        libc::signal(
            libc::SIGTERM,
            on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGHUP,
            on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t,
        );
    }
}

/// `crossterm::event::poll` (which `Context::run_once` calls) never returns once the controlling
/// terminal has hung up *and the process survived SIGHUP*: it spins at ~65% CPU forever (see
/// docs/upstream/repro_hangup_spin.rs). A watchdog thread notices POLLHUP on stdin and exits.
fn spawn_hangup_watchdog() {
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let mut p = libc::pollfd {
            fd: 0,
            events: 0,
            revents: 0,
        };
        let r = unsafe { libc::poll(&mut p, 1, 0) };
        if r > 0 && p.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
            // the terminal is gone: there is nothing left to restore
            unsafe { libc::_exit(129) };
        }
    });
}

struct Args {
    raw: Vec<String>,
}

impl Args {
    fn has(&self, f: &str) -> bool {
        self.raw.iter().any(|a| a == f)
    }
    fn get(&self, p: &str) -> Option<String> {
        self.raw
            .iter()
            .find_map(|a| a.strip_prefix(p).map(|s| s.to_string()))
    }
}

fn usage() {
    println!(
        "chronoscope — a debugger for alternate histories (LibGibson v0.4.0 consumer)\n\n\
         interactive:  chronoscope [--world=black-ice|vapor95|swiss-signal] [--seed=N] [--mute | --no-audio]\n\
         \x20             [--audio-strategy=full|future] [--skin=black-ice|vapor95|swiss-signal]\n\
         \x20             [--color=truecolor|ansi256|ansi16|mono] [--glyphs=braille|halfblock|block|ascii]\n\
         guided demo:  chronoscope --demo   (run to catastrophe, rewind, change one input, compare, A/B)
         capture:      chronoscope --capture --size=WxH --script=\"...\" [--ansi=FILE] [--png]\n\
         sustained:    chronoscope --sustained [--frames=N] [--audio-every=K] [--out=FILE]\n\
         audio:        chronoscope --wav-out=DIR --script=\"...\"   (render every branch's performance)\n\n\
         script verbs: end start goto N step N play pause jump <next|prev>-<landmark|decision|epoch|input|checkpoint>\n\
         \x20             fork <drop|replace2|replace5|insert1|insert2|insert5|decision V> branch N compare ab collapse\n\
         \x20             turn inside inspect openfork help close settle key C tab enter esc resize WxH frames N"
    );
}

fn world_of(s: &str) -> gibson::audio::human_music::WorldId {
    use gibson::audio::human_music::WorldId::*;
    match s {
        "vapor95" => Vapor95,
        "swiss-signal" | "swiss" => SwissSignal,
        _ => BlackIce,
    }
}

fn skin_of(s: &str) -> Skin {
    match s {
        "vapor95" => skins::VAPOR95,
        "swiss-signal" | "swiss" => skins::SWISS_SIGNAL,
        _ => skins::BLACK_ICE,
    }
}

fn options_from(a: &Args) -> Options {
    let mut o = Options::default();
    if let Some(w) = a.get("--world=") {
        o.world = world_of(&w);
    }
    if let Some(s) = a.get("--seed=").and_then(|s| s.parse().ok()) {
        o.seed = s;
    }
    if a.has("--no-audio") {
        o.audio = AudioMode::Off;
    } else if a.has("--mute") {
        o.audio = AudioMode::Silent;
    }
    match a.get("--audio-strategy=").as_deref() {
        Some("future") => o.strategy = Strategy::FutureOnly,
        Some("full") => o.strategy = Strategy::FullTrace,
        _ => {}
    }
    o
}

fn glyphs_from(a: &Args, default: SubcellGlyphMode) -> SubcellGlyphMode {
    match a.get("--glyphs=") {
        Some(g) => GlyphChoice::parse(&g)
            .map(|c| c.resolve(|k| std::env::var(k).ok()))
            .unwrap_or(default),
        None => default,
    }
}

fn size_from(a: &Args, default: (u16, u16)) -> (u16, u16) {
    a.get("--size=")
        .and_then(|s| {
            s.split_once('x').map(|(w, h)| {
                (
                    w.parse().unwrap_or(default.0),
                    h.parse().unwrap_or(default.1),
                )
            })
        })
        .unwrap_or(default)
}

fn capture(a: &Args) -> i32 {
    let (w, h) = size_from(a, (120, 40));
    let depth = a
        .get("--color=")
        .and_then(|s| parse_depth(&s))
        .unwrap_or(ColorDepth::TrueColor);
    let glyphs = glyphs_from(a, SubcellGlyphMode::Braille2x4);
    let mut rig = Rig::headless(w, h, depth, glyphs, options_from(a));
    rig.stream = Some(vec![]);
    if let Err(e) = run_script(&mut rig, &a.get("--script=").unwrap_or_default()) {
        eprintln!("script error: {e}");
        return 2;
    }
    if let Err(e) = rig.settle() {
        eprintln!("render error: {e}");
        return 1;
    }
    if let Some(p) = a.get("--ansi=") {
        std::fs::write(p, rig.stream.take().unwrap()).expect("write ansi");
    }
    for l in rig.lines() {
        println!("{l}");
    }
    0
}

fn wav_out(a: &Args, dir: &str) -> i32 {
    let mut o = options_from(a);
    o.threaded_audio = false;
    o.audio = AudioMode::Silent;
    let mut rig = Rig::headless(
        120,
        40,
        ColorDepth::TrueColor,
        SubcellGlyphMode::Braille2x4,
        o,
    );
    if let Err(e) = run_script(&mut rig, &a.get("--script=").unwrap_or_default()) {
        eprintln!("script error: {e}");
        return 2;
    }
    std::fs::create_dir_all(dir).expect("mkdir");
    let n = rig.model.hist.branches.len() as u16;
    for b in 0..n {
        let Model { hist, audio, .. } = &mut rig.model;
        audio.request(hist, b);
        let end = hist.branch(b).end();
        let total = audio.clock.sample_of(end) + 48_000;
        if let Some(pcm) = audio.fetch(hist, b, 0, total as usize) {
            let path =
                std::path::Path::new(dir).join(format!("branch-{}.wav", hist.branch(b).label));
            write_wav(&path, &pcm).expect("write wav");
            let p = audio.get(b).unwrap();
            println!(
                "{} → {} ({:.1}s, hash {:016x}, composed {:.0} ms, rendered {:.0} ms = {:.0}× realtime, peak {:.2})",
                hist.branch(b).label,
                path.display(),
                pcm.len() as f64 / 2.0 / 48_000.0,
                p.hash,
                p.stats.compose_ms,
                p.stats.render_ms,
                p.stats.rtf,
                p.stats.peak
            );
        }
    }
    0
}

/// Run the guided demo headlessly and write a key frame (`.ansi`) at each beat of the story.
fn storyboard(a: &Args, dir: &str) -> i32 {
    let (w, h) = size_from(a, (120, 40));
    let depth = a
        .get("--color=")
        .and_then(|s| parse_depth(&s))
        .unwrap_or(ColorDepth::TrueColor);
    let mut o = options_from(a);
    o.audio = AudioMode::Silent;
    let mut rig = Rig::headless(w, h, depth, SubcellGlyphMode::Braille2x4, o);
    rig.motion = MotionPreference::Full;
    std::fs::create_dir_all(dir).expect("mkdir");
    rig.model.demo = Some(project_chronoscope::demo::Demo::new());
    let mut shots: Vec<(String, u32)> = vec![];
    // key frames: (label, predicate on the model)
    type Mark = (&'static str, Box<dyn Fn(&Model) -> bool>);
    let marks: Vec<Mark> = vec![
        (
            "01-recording",
            Box::new(|m: &Model| m.cursor >= 20 && m.cur_b == 0 && !m.facing_back),
        ),
        (
            "02-escalation",
            Box::new(|m: &Model| m.cur_b == 0 && m.cursor >= 275 && !m.facing_back),
        ),
        (
            "03-catastrophe",
            Box::new(|m: &Model| m.cur_b == 0 && m.cursor >= 344),
        ),
        (
            "04-rewinding",
            Box::new(|m: &Model| m.cur_b == 0 && m.facing_back && m.cursor <= 200),
        ),
        (
            "05-the-fork",
            Box::new(|m: &Model| {
                m.cur_b == 1 && m.bloom.map(|(_, age)| age > 0.3).unwrap_or(false)
            }),
        ),
        (
            "06-second-future",
            Box::new(|m: &Model| m.cur_b == 1 && m.cursor >= 300 && !m.facing_back),
        ),
        (
            "07-compare",
            Box::new(|m: &Model| m.compare.is_some() && m.cursor >= 215),
        ),
        (
            "08-compare-late",
            Box::new(|m: &Model| m.compare.is_some() && m.cursor >= 330),
        ),
    ];
    let mut done = vec![false; marks.len()];
    for frame in 0..6000u32 {
        rig.frame().expect("frame");
        for (i, (label, pred)) in marks.iter().enumerate() {
            if !done[i] && pred(&rig.model) {
                // let the visual state settle for a few frames, then dump a full repaint
                for _ in 0..8 {
                    rig.frame().expect("frame");
                }
                let bytes = rig.snapshot_ansi().expect("snapshot");
                std::fs::write(format!("{dir}/{label}.ansi"), bytes).expect("write");
                shots.push((label.to_string(), frame));
                done[i] = true;
            }
        }
        if rig.model.demo.is_none() && done.iter().all(|d| *d) {
            break;
        }
        if rig.model.demo.is_none() && frame > 200 {
            break;
        }
    }
    for (l, f) in &shots {
        println!("{l} @ frame {f}");
    }
    0
}

fn sustained(a: &Args) -> i32 {
    let mut cfg = SustainedConfig::default();
    if let Some(n) = a.get("--frames=").and_then(|s| s.parse().ok()) {
        cfg.frames = n;
    }
    if let Some(n) = a.get("--audio-every=").and_then(|s| s.parse().ok()) {
        cfg.audio_every = n;
    }
    if let Some(d) = a.get("--color=").and_then(|s| parse_depth(&s)) {
        cfg.depth = d;
    }
    let rep = run_sustained(&cfg);
    let md = rep.to_markdown();
    print!("{md}");
    if let Some(p) = a.get("--out=") {
        std::fs::write(p, md).expect("write report");
    }
    0
}

fn interactive(a: &Args) -> i32 {
    let mut ctx = match Context::fullscreen() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("chronoscope: cannot open the terminal: {e}");
            return 1;
        }
    };
    if !ctx.is_interactive() {
        let _ = ctx.restore();
        eprintln!("chronoscope: stdin/stdout is not an interactive terminal. Use --capture for a headless frame.");
        return 2;
    }
    if let Some(d) = a.get("--color=").and_then(|s| parse_depth(&s)) {
        ctx.set_color_depth(d);
    }
    install_signal_flag();
    spawn_hangup_watchdog();
    ctx.set_max_fps(30);
    let skin = a
        .get("--skin=")
        .map(|s| skin_of(&s))
        .unwrap_or(skins::BLACK_ICE);
    let mut rt: UiRuntime<Action> = UiRuntime::new(skin);
    let mut model = Model::new(options_from(a));
    model.settle();
    if a.has("--demo") {
        model.demo = Some(project_chronoscope::demo::Demo::new());
    }
    let glyphs = match a.get("--glyphs=") {
        Some(g) => GlyphChoice::parse(&g)
            .map(|c| c.resolve(|k| std::env::var(k).ok()))
            .unwrap_or(SubcellGlyphMode::Braille2x4),
        None => {
            gibson::glyph::detect_glyph_mode_from_env(None).unwrap_or(SubcellGlyphMode::Braille2x4)
        }
    };
    let motion = if a.has("--no-motion") {
        MotionPreference::None
    } else {
        MotionPreference::Full
    };
    let start = Instant::now();
    let mut last = Duration::ZERO;
    // optional diagnostics: every input event and every frame's accounting, with timestamps
    let mut trace = std::env::var("CHRONO_TRACE")
        .ok()
        .and_then(|p| std::fs::File::create(p).ok());
    let result = (|| -> std::io::Result<()> {
        loop {
            let now = start.elapsed();
            model.tick(now.saturating_sub(last).min(Duration::from_millis(250)));
            last = now;
            let (width, height) = ctx.session.terminal_size();
            let env = UiEnvironment {
                width,
                height,
                color_depth: ctx.capabilities().color_depth,
                glyph_mode: glyphs,
                motion,
            };
            let screen = build_screen(&mut model, env);
            let frame = rt
                .frame(&screen.tree, env, Duration::from_secs_f32(model.time))
                .map_err(std::io::Error::other)?;
            ctx.set_root(frame.node);
            let polled = ctx.run_once(Duration::from_millis(16))?;
            if let Some(f) = trace.as_mut() {
                use std::io::Write;
                let r = ctx.last_frame_report();
                let _ = writeln!(f, "{:>8} frame size={}x{} bytes={} gen_us={} write_us={} changed={} full={} missed={}", start.elapsed().as_millis(), width, height, r.bytes_emitted, r.generation_duration.as_micros(), r.write_duration.as_micros(), r.exact_changed_cells, r.full_repaint, ctx.missed_periods_total());
                if let Some(e) = &polled {
                    let _ = writeln!(f, "{:>8} event {:?}", start.elapsed().as_millis(), e);
                }
            }
            if let Some(ev) = polled {
                if matches!(&ev, Event::Key(k) if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL))
                {
                    break;
                }
                route_event(&mut model, &mut rt, &ev);
            }
            if model.quit || SIGNALLED.load(std::sync::atomic::Ordering::SeqCst) != 0 {
                break;
            }
        }
        Ok(())
    })();
    model.player.stop();
    let restored = ctx.restore();
    for line in &model.journal {
        println!("{line}");
    }
    let sig = SIGNALLED.load(std::sync::atomic::Ordering::SeqCst);
    match (result, restored) {
        (Ok(()), Ok(())) if sig != 0 => 128 + sig,
        (Ok(()), Ok(())) => 0,
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("chronoscope: {e}");
            1
        }
    }
}

fn main() {
    let a = Args {
        raw: std::env::args().skip(1).collect(),
    };
    if a.has("--help") || a.has("-h") {
        usage();
        return;
    }
    let code = if a.has("--sustained") {
        sustained(&a)
    } else if let Some(dir) = a.get("--wav-out=") {
        wav_out(&a, &dir)
    } else if let Some(kind) = a.get("--evidence=") {
        match kind.as_str() {
            "audio" => print!("{}", project_chronoscope::evidence::audio_evidence()),
            "matrix" => print!("{}", project_chronoscope::evidence::matrix_evidence()),
            "causal" => print!("{}", project_chronoscope::evidence::causal_evidence()),
            other => eprintln!("unknown evidence kind {other}"),
        }
        0
    } else if let Some(dir) = a.get("--storyboard=") {
        storyboard(&a, &dir)
    } else if a.has("--capture") {
        capture(&a)
    } else {
        interactive(&a)
    };
    std::process::exit(code);
}
