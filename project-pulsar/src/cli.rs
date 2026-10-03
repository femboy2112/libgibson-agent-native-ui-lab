//! Command line: interactive run, deterministic headless capture, bounded demo,
//! and a developer report.

use std::sync::Arc;

use gibson::{ColorDepth, SubcellGlyphMode};

use crate::app;
use crate::compose;
use crate::observatory::{Observatory, DEFAULT_SEED};
use crate::rng::fnv1a;
use crate::sim::{DURATION, SLOTS};
use crate::state::{run_script, Model};
use crate::track::State;
use crate::views::View;

pub const USAGE: &str = "\
PROJECT PULSAR - a deterministic synthetic deep-space signal observatory

USAGE
  project-pulsar                         interactive (real terminal)
  project-pulsar --capture WxH[:mono|:color] [options]
                                         print ONE frame to stdout and exit
  project-pulsar --demo [WxH[:mono]]     headless bounded tour; prints a digest
  project-pulsar --report                the pipeline's timeline and truth check
  project-pulsar --help

CAPTURE OPTIONS (all deterministic)
  --at T            display time in mission seconds, 0..256       (default 0)
  --view V          trace|spectrum|fold|relate|sky  (or 1..5)     (default trace)
  --select S        focus candidate A|B|C|D|E
  --zoom N          0..2 (all-sky/local sky, spectrum, trace window)
  --detune N        fold trial-period nudge, integer steps
  --compare [T0]    pin an earlier state T0 and overlay it
  --cursor U,V      inspect cursor, normalised 0..1 inside the hero
  --from V --progress P   freeze an identity transport at P (0..1)
  --glyphs MODE     braille|halfblock|block|ascii                  (default braille)
  --script \"...\"    replay a key script first (the app prints one on quit)
  --seed N          world seed (decimal or 0x hex)
  --help-overlay    draw the help overlay

  :mono prints the Mono-fallback frame; :color prints the same frame with ANSI colour.
";

#[derive(Debug, Clone)]
pub struct Capture {
    pub w: u16,
    pub h: u16,
    pub mono: bool,
    pub color: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub capture: Option<Capture>,
    pub demo: Option<Capture>,
    pub report: bool,
    pub help: bool,
    pub at: Option<f64>,
    pub view: Option<View>,
    pub select: Option<usize>,
    pub zoom: Option<u8>,
    pub detune: Option<i32>,
    pub compare: Option<Option<f64>>,
    pub cursor: Option<(f64, f64)>,
    pub from: Option<View>,
    pub progress: Option<f32>,
    pub glyphs: Option<SubcellGlyphMode>,
    pub script: Option<String>,
    pub seed: Option<u64>,
    pub help_overlay: bool,
}

pub fn parse_size(spec: &str) -> Result<Capture, String> {
    let (size, tag) = spec.split_once(':').unwrap_or((spec, ""));
    let (w, h) = size
        .split_once('x')
        .ok_or_else(|| format!("expected WxH[:mono], got {spec:?}"))?;
    let w: u16 = w.parse().map_err(|_| format!("bad width in {spec:?}"))?;
    let h: u16 = h.parse().map_err(|_| format!("bad height in {spec:?}"))?;
    if w < 8 || h < 4 {
        return Err(format!("size {w}x{h} is too small (minimum 8x4)"));
    }
    match tag {
        "" => Ok(Capture {
            w,
            h,
            mono: false,
            color: false,
        }),
        "mono" => Ok(Capture {
            w,
            h,
            mono: true,
            color: false,
        }),
        "color" => Ok(Capture {
            w,
            h,
            mono: false,
            color: true,
        }),
        other => Err(format!(
            "unknown capture tag :{other} (use :mono or :color)"
        )),
    }
}

fn parse_seed(s: &str) -> Result<u64, String> {
    let r = if let Some(h) = s.strip_prefix("0x") {
        u64::from_str_radix(h, 16)
    } else {
        s.parse()
    };
    r.map_err(|_| format!("bad seed {s:?}"))
}

fn parse_slot(s: &str) -> Result<usize, String> {
    let c = s.chars().next().map(|c| c.to_ascii_uppercase());
    crate::identity::IDENT
        .iter()
        .position(|i| Some(i.letter) == c)
        .ok_or_else(|| format!("unknown candidate {s:?} (A..E)"))
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut i = 0;
    let next = |i: &mut usize, what: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i)
            .cloned()
            .ok_or_else(|| format!("{what} needs a value"))
    };
    while i < args.len() {
        let a = args[i].as_str();
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (a.to_string(), None),
        };
        let val = |i: &mut usize, what: &str| -> Result<String, String> {
            match &inline {
                Some(v) => Ok(v.clone()),
                None => next(i, what),
            }
        };
        match flag.as_str() {
            "--capture" => o.capture = Some(parse_size(&val(&mut i, "--capture")?)?),
            "--demo" => {
                let spec = match &inline {
                    Some(v) => v.clone(),
                    None => match args.get(i + 1) {
                        Some(n) if !n.starts_with("--") => {
                            i += 1;
                            n.clone()
                        }
                        _ => "100x34".to_string(),
                    },
                };
                o.demo = Some(parse_size(&spec)?);
            }
            "--report" => o.report = true,
            "--help" | "-h" => o.help = true,
            "--help-overlay" => o.help_overlay = true,
            "--at" => {
                let v: f64 = val(&mut i, "--at")?
                    .parse()
                    .map_err(|_| "bad --at".to_string())?;
                if !v.is_finite() {
                    return Err("--at must be finite".into());
                }
                o.at = Some(v.clamp(0.0, DURATION));
            }
            "--view" => {
                let v = val(&mut i, "--view")?;
                o.view = Some(View::parse(&v).ok_or_else(|| format!("unknown view {v:?}"))?);
            }
            "--from" => {
                let v = val(&mut i, "--from")?;
                o.from = Some(View::parse(&v).ok_or_else(|| format!("unknown view {v:?}"))?);
            }
            "--progress" => {
                o.progress = Some(
                    val(&mut i, "--progress")?
                        .parse()
                        .map_err(|_| "bad --progress".to_string())?,
                )
            }
            "--select" => o.select = Some(parse_slot(&val(&mut i, "--select")?)?),
            "--zoom" => {
                let z: u8 = val(&mut i, "--zoom")?
                    .parse()
                    .map_err(|_| "bad --zoom".to_string())?;
                o.zoom = Some(z.min(2));
            }
            "--detune" => {
                o.detune = Some(
                    val(&mut i, "--detune")?
                        .parse()
                        .map_err(|_| "bad --detune".to_string())?,
                )
            }
            "--compare" => {
                let t = match &inline {
                    Some(v) => Some(v.parse::<f64>().map_err(|_| "bad --compare".to_string())?),
                    None => match args.get(i + 1) {
                        Some(n) if n.parse::<f64>().is_ok() => {
                            i += 1;
                            n.parse::<f64>().ok()
                        }
                        _ => None,
                    },
                };
                o.compare = Some(t);
            }
            "--cursor" => {
                let v = val(&mut i, "--cursor")?;
                let (u, w) = v.split_once(',').ok_or("--cursor needs U,V")?;
                o.cursor = Some((
                    u.parse::<f64>()
                        .map_err(|_| "bad cursor u")?
                        .clamp(0.0, 1.0),
                    w.parse::<f64>()
                        .map_err(|_| "bad cursor v")?
                        .clamp(0.0, 1.0),
                ));
            }
            "--glyphs" => {
                o.glyphs = Some(match val(&mut i, "--glyphs")?.as_str() {
                    "braille" => SubcellGlyphMode::Braille2x4,
                    "halfblock" => SubcellGlyphMode::HalfBlock1x2,
                    "block" => SubcellGlyphMode::Block,
                    "ascii" => SubcellGlyphMode::Ascii,
                    other => return Err(format!("unknown glyph mode {other:?}")),
                })
            }
            "--script" => o.script = Some(val(&mut i, "--script")?),
            "--seed" => o.seed = Some(parse_seed(&val(&mut i, "--seed")?)?),
            other => return Err(format!("unknown argument {other:?} (try --help)")),
        }
        i += 1;
    }
    Ok(o)
}

/// Build the model a capture describes (script first, then explicit flags).
pub fn model_for(o: &Options, obs: Arc<Observatory>) -> Result<Model, String> {
    let mut m = Model::new(obs);
    if let Some(s) = &o.script {
        run_script(&mut m, s)?;
    }
    m.settle();
    if let Some(t) = o.at {
        m.seek(t);
    }
    if let Some(v) = o.view {
        m.view = v;
        m.from = None;
        m.trans = 1.0;
    }
    if let Some(s) = o.select {
        m.sel = Some(s);
    }
    if let Some(z) = o.zoom {
        m.zoom = z;
    }
    if let Some(d) = o.detune {
        m.nudge = d;
    }
    if let Some(c) = &o.compare {
        let t0 = c.unwrap_or_else(|| m.default_mark());
        m.mark = Some(t0.clamp(0.0, DURATION));
        m.compare = true;
    }
    if let Some(c) = o.cursor {
        m.cursor = c;
        m.cursor_on = true;
    }
    if let (Some(f), Some(p)) = (o.from, o.progress) {
        m.from = Some(f);
        m.trans = p.clamp(0.0, 1.0);
    }
    if o.help_overlay {
        m.help = true;
    }
    Ok(m)
}

pub fn capture_text(m: &Model, cap: &Capture, glyphs: SubcellGlyphMode) -> Result<String, String> {
    if cap.color {
        let f = compose::compose(m, cap.w, cap.h, false, glyphs);
        return Ok(app::surface_to_ansi(&f.surface));
    }
    let depth = if cap.mono {
        ColorDepth::Mono
    } else {
        ColorDepth::TrueColor
    };
    let lines =
        app::render_through_runtime(m, cap.w, cap.h, depth, glyphs).map_err(|e| e.to_string())?;
    let mut out = String::new();
    for l in lines {
        out.push_str(l.trim_end());
        out.push('\n');
    }
    Ok(out)
}

// ─────────────────────────────── bounded demo ──────────────────────────────────

/// One keyframe of the scripted tour: a label and the key script that reaches it
/// from the previous keyframe.
pub const TOUR: &[(&str, &str)] = &[
    ("00 noise only: nothing resolves yet", "1 @20"),
    ("01 a spectral line stands out above the floor", "2 @44"),
    (
        "02 a pulsar candidate appears; ambiguity everywhere",
        "@76 down",
    ),
    (
        "03 fold: the period makes the pulse a straight ridge",
        "3 @100",
    ),
    (
        "04 detune the period: the ridge shears, the pulse smears",
        ". . . .",
    ),
    (
        "05 back on period; the delay plane sees three clusters",
        "0 4 wait:0.3 @132",
    ),
    (
        "06 sky, planar array: every source has a mirror ghost",
        "5 wait:1.0 @136",
    ),
    (
        "07 station S4 comes online: ghosts begin to collapse",
        "@168",
    ),
    (
        "08 the time-domain comb of the same identities",
        "1 wait:1.0 z",
    ),
    (
        "09 compare: earlier uncertain state vs the final solution",
        "0 5 @256 c",
    ),
    ("10 the final interpretation", "wait:1.0 c"),
];

/// The bounded tour as text (its digest is the determinism witness).
pub fn demo_text(o: &Options, cap: &Capture, obs: Arc<Observatory>) -> Result<String, String> {
    use std::fmt::Write as _;
    let glyphs = o.glyphs.unwrap_or(SubcellGlyphMode::Braille2x4);
    let mut m = Model::new(obs);
    let mut digest_bytes: Vec<u8> = Vec::new();
    let mut out = String::new();
    let w = |out: &mut String, s: &str| {
        let _ = writeln!(out, "{s}");
    };
    w(
        &mut out,
        &format!(
            "PROJECT PULSAR demo \u{2014} {} keyframes at {}x{}{} (deterministic; terminates)",
            TOUR.len() + 1,
            cap.w,
            cap.h,
            if cap.mono { " mono" } else { "" }
        ),
    );
    for (i, (label, script)) in TOUR.iter().enumerate() {
        run_script(&mut m, script)?;
        m.settle();
        let text = capture_text(&m, cap, glyphs)?;
        w(
            &mut out,
            &format!(
                "\n=== {}/{}  {}  [t={:.1} view={} sel={}] ===",
                i + 1,
                TOUR.len() + 1,
                label,
                m.t,
                m.view.name(),
                m.sel
                    .map(|s| crate::identity::IDENT[s].letter.to_string())
                    .unwrap_or_else(|| "-".into())
            ),
        );
        out.push_str(&text);
        digest_bytes.extend_from_slice(text.as_bytes());
    }
    // mid-transport freeze: the identities travelling from FOLD to SKY
    let mut t = m.clone();
    t.from = Some(View::Fold);
    t.view = View::Sky;
    t.trans = 0.5;
    let text = capture_text(&t, cap, glyphs)?;
    w(
        &mut out,
        &format!(
            "\n=== {n}/{n}  identity transport FOLD \u{2192} SKY at 50% (the identities are in flight) ===",
            n = TOUR.len() + 1
        ),
    );
    out.push_str(&text);
    digest_bytes.extend_from_slice(text.as_bytes());
    w(&mut out, &format!("\nreplay script: {}", m.replay_script()));
    w(
        &mut out,
        &format!("demo digest: {:016x}", fnv1a(&digest_bytes)),
    );
    Ok(out)
}

pub fn run_demo(o: &Options, cap: &Capture, obs: Arc<Observatory>) -> Result<(), String> {
    print!("{}", demo_text(o, cap, obs)?);
    Ok(())
}

// ──────────────────────────────── report ──────────────────────────────────────

pub fn run_report(obs: &Observatory) {
    println!(
        "world seed {:#x}  data digest {:016x}",
        obs.seed,
        obs.digest()
    );
    println!("\nPIPELINE TIMELINE (every 4th epoch)");
    for e in obs.timeline.epochs.iter().filter(|e| e.k % 4 == 0) {
        print!("t={:5.0}s ", e.t);
        for s in 0..SLOTS {
            let sl = e.slots[s];
            if sl.state == State::Quiet {
                print!(" {}:--------- ", crate::identity::IDENT[s].letter);
            } else {
                print!(
                    " {}:{:<9.9} Z{:>5.1} ",
                    crate::identity::IDENT[s].letter,
                    sl.state.label(),
                    sl.z
                );
            }
        }
        println!();
    }
    println!("\nEVENTS");
    for ev in &obs.timeline.events {
        let who = ev
            .slot
            .map(|s| crate::identity::IDENT[s].letter.to_string())
            .unwrap_or_else(|| "-".into());
        println!("  t={:5.0}s  {}  {}", ev.t, who, ev.describe());
    }
    println!("\nRECOVERY (final epoch) vs injected truth");
    let last = obs.timeline.epochs.last().expect("epochs");
    for (i, tr) in obs.truth().iter().enumerate() {
        let sl = last.slots[i];
        let pos = sl
            .sky
            .map(|k| {
                let b = k.best();
                format!(
                    "az {:6.1} el {:+6.1}  (truth az {:6.1} el {:+6.1}, off {:.1} deg, sigma {:.1})",
                    b.0,
                    b.1,
                    tr.az,
                    tr.el,
                    crate::sim::separation_deg(b, (tr.az, tr.el)),
                    k.sigma_deg
                )
            })
            .unwrap_or_else(|| "no sky fix".into());
        println!(
            "  {} {:<9} f {:.5} (truth {:.5}, err {:+.1e})  {}",
            crate::identity::IDENT[i].letter,
            sl.state.label(),
            sl.f,
            tr.f,
            sl.f - tr.f,
            pos
        );
    }
}

pub fn main_with(args: &[String]) -> Result<i32, String> {
    let o = parse_args(args)?;
    if o.help {
        print!("{USAGE}");
        return Ok(0);
    }
    let seed = o.seed.unwrap_or(DEFAULT_SEED);
    let obs = Arc::new(Observatory::new(seed));
    if o.report {
        run_report(&obs);
        return Ok(0);
    }
    if let Some(cap) = &o.demo {
        run_demo(&o, cap, obs)?;
        return Ok(0);
    }
    if let Some(cap) = &o.capture {
        let m = model_for(&o, obs)?;
        let text = capture_text(&m, cap, o.glyphs.unwrap_or(SubcellGlyphMode::Braille2x4))?;
        print!("{text}");
        return Ok(0);
    }
    // interactive
    let m = model_for(&o, obs)?;
    let done = app::run_interactive(m).map_err(|e| e.to_string())?;
    let script = done.replay_script();
    if !script.is_empty() {
        println!("replay this session exactly:");
        println!("  project-pulsar --capture 120x40 --script \"{script}\"");
    }
    Ok(0)
}
