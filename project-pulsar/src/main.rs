//! Project Pulsar — command line.

use gibson::{ColorDepth, SubcellGlyphMode};
use pulsar::app::{self, RunOpts};
use pulsar::capture;
use pulsar::render::Env;
use pulsar::scenario::DEFAULT_SEED;
use pulsar::session::{parse_script, secs_to_samples, Cmd, Model, Session, View};
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
PROJECT PULSAR — a deterministic synthetic deep-space signal observatory

USAGE
  project-pulsar [OPTIONS]            interactive observatory (needs a terminal)

MODES (non-interactive modes never touch the terminal and always terminate)
  --demo                    play the built-in 12-step cue sheet to stdout
  --dump                    print one frame (use --at/--view/--script to set the state)
  --capture-dir DIR         write the full deterministic capture set into DIR
  --replay FILE             re-run a recorded session log; print the final frame
  --selftest                run the structural self-checks and exit 0/1
  --help

STATE
  --seed N                  universe seed (default 62; the same seed always gives the same universe)
  --at SECONDS              seek to a time (0-480)
  --view NAME|1-6           stream spectrum fold relation sky dossier
  --script \"a; b; c\"        extra commands: select a|b|c, focus, zoom 1, pin, compare,
                            inspect, cursor 3 0, truth, step 64, seek 3200 ...
REALISATION
  --size WxH                headless size (default 120x40)
  --mono                    no-colour realisation
  --glyphs MODE             braille | halfblock | block | ascii

INTERACTIVE
  --record FILE             write the exact command log of the session on exit
  --play                    start playing";

#[derive(Default)]
struct Args {
    seed: Option<u64>,
    at: Option<f64>,
    view: Option<View>,
    script: Option<String>,
    size: Option<(u16, u16)>,
    mono: bool,
    glyphs: Option<SubcellGlyphMode>,
    demo: bool,
    dump: bool,
    selftest: bool,
    capture_dir: Option<PathBuf>,
    replay: Option<PathBuf>,
    record: Option<PathBuf>,
    play: bool,
    help: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| it.next())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--seed" => a.seed = Some(value("--seed")?.parse().map_err(|e| format!("--seed: {e}"))?),
            "--at" => a.at = Some(value("--at")?.parse().map_err(|e| format!("--at: {e}"))?),
            "--view" => {
                let v = value("--view")?;
                a.view = Some(View::parse(&v).ok_or_else(|| format!("unknown view `{v}`"))?);
            }
            "--script" => a.script = Some(value("--script")?),
            "--size" => {
                let v = value("--size")?;
                let (w, h) = v.split_once('x').ok_or("--size wants WxH")?;
                a.size = Some((
                    w.parse().map_err(|e| format!("--size: {e}"))?,
                    h.parse().map_err(|e| format!("--size: {e}"))?,
                ));
            }
            "--mono" => a.mono = true,
            "--glyphs" => {
                let v = value("--glyphs")?;
                a.glyphs = Some(app::parse_glyphs(&v).ok_or_else(|| format!("unknown glyph mode `{v}`"))?);
            }
            "--demo" => a.demo = true,
            "--dump" => a.dump = true,
            "--selftest" => a.selftest = true,
            "--capture-dir" => a.capture_dir = Some(PathBuf::from(value("--capture-dir")?)),
            "--replay" => a.replay = Some(PathBuf::from(value("--replay")?)),
            "--record" => a.record = Some(PathBuf::from(value("--record")?)),
            "--play" => a.play = true,
            "--help" | "-h" => a.help = true,
            other => return Err(format!("unknown argument `{other}` (try --help)")),
        }
    }
    Ok(a)
}

fn start_commands(a: &Args) -> Result<Vec<Cmd>, String> {
    let mut v = Vec::new();
    if let Some(t) = a.at {
        v.push(Cmd::Seek(secs_to_samples(t)));
    }
    if let Some(view) = a.view {
        v.push(Cmd::SetView(view));
    }
    if let Some(s) = &a.script {
        v.extend(parse_script(s)?);
    }
    if a.play {
        v.push(Cmd::TogglePlay);
    }
    Ok(v)
}

fn env_of(a: &Args) -> Env {
    let (w, h) = a.size.unwrap_or((120, 40));
    Env {
        width: w,
        height: h,
        depth: if a.mono { ColorDepth::Mono } else { ColorDepth::TrueColor },
        glyphs: a.glyphs.unwrap_or(SubcellGlyphMode::Braille2x4),
    }
}

fn run(a: Args) -> Result<(), String> {
    let seed = a.seed.unwrap_or(DEFAULT_SEED);
    let err = |e: io::Error| e.to_string();
    if a.help {
        println!("{USAGE}");
        return Ok(());
    }
    if a.selftest {
        return selftest(seed);
    }
    if let Some(dir) = &a.capture_dir {
        let files = capture::write_capture_set(dir, seed).map_err(err)?;
        println!("wrote {} files under {}", files.len(), dir.display());
        return Ok(());
    }
    if a.demo {
        let n = capture::run_demo(seed, &env_of(&a), &mut io::stdout().lock()).map_err(err)?;
        eprintln!("demo complete: {n} steps");
        return Ok(());
    }
    if let Some(path) = &a.replay {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let cmds = parse_script(&text)?;
        let sess = Session::replay(seed, &cmds);
        let r = pulsar::app::render_frame(&sess.model, &env_of(&a)).map_err(err)?;
        for l in &r.lines {
            println!("{}", l.trim_end());
        }
        eprintln!("replayed {} commands; final t = {:.3} s; state = {:?}", cmds.len(), sess.model.t(), sess.model.st);
        return Ok(());
    }
    let start = start_commands(&a)?;
    if a.dump {
        let mut m = Model::new(seed);
        for c in start {
            m.apply(c);
        }
        m.apply(Cmd::Frame(255));
        let r = pulsar::app::render_frame(&m, &env_of(&a)).map_err(err)?;
        let mut out = io::stdout().lock();
        for l in &r.lines {
            writeln!(out, "{}", l.trim_end()).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    // interactive
    if !io::stdout().is_terminal() {
        return Err("stdout is not a terminal: use --dump, --demo or --capture-dir for headless output".into());
    }
    let sess = app::run_interactive(RunOpts {
        seed,
        start,
        mono: a.mono,
        glyphs: a.glyphs,
    })
    .map_err(err)?;
    if let Some(p) = a.record {
        std::fs::write(&p, sess.log_text()).map_err(|e| e.to_string())?;
        eprintln!("recorded {} commands to {}", sess.log.len(), p.display());
    }
    Ok(())
}

/// A quick self-check of the invariants the test-suite proves in depth.
fn selftest(seed: u64) -> Result<(), String> {
    let env = Env::new(80, 24);
    let mut m = Model::new(seed);
    m.apply(Cmd::Seek(secs_to_samples(200.0)));
    let a = pulsar::app::render_frame(&m, &env).map_err(|e| e.to_string())?;
    let mut m2 = Model::new(seed);
    for _ in 0..200 {
        m2.apply(Cmd::Advance(32));
    }
    let b = pulsar::app::render_frame(&m2, &env).map_err(|e| e.to_string())?;
    if a.lines != b.lines {
        return Err("FAIL: seek and run produced different frames".into());
    }
    println!("ok: direct seek to 200 s == running to 200 s (frames identical)");
    let c = pulsar::app::render_frame(&m, &env).map_err(|e| e.to_string())?;
    if a.lines != c.lines {
        return Err("FAIL: rendering the same state twice differs".into());
    }
    println!("ok: same state => same frame");
    Ok(())
}

fn main() -> ExitCode {
    match parse_args().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("project-pulsar: {e}");
            ExitCode::from(2)
        }
    }
}
