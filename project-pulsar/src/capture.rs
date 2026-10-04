//! Deterministic captures, the keyframe sequences and the bounded demo.
//!
//! Every capture is `f(seed, scripted commands, size, capability)`: nothing reads a
//! clock, so the same invocation always yields the same bytes.

use crate::app::{render_frame, Rendered};
use crate::render::Env;
use crate::scenario::SigId;
use crate::session::{parse_script, secs_to_samples, Cmd, Model, View};
use gibson::{ColorDepth, SubcellGlyphMode};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const SIZES: [(u16, u16); 3] = [(120, 40), (80, 24), (42, 15)];

/// Keyframe times (seconds): uncertainty → candidates → locks → fixes → final.
pub const KEYFRAME_TIMES: [f64; 9] = [8.0, 32.0, 56.0, 80.0, 104.0, 152.0, 200.0, 300.0, 480.0];

pub fn model_with(seed: u64, cmds: &[Cmd]) -> Model {
    let mut m = Model::new(seed);
    for c in cmds {
        m.apply(c.clone());
    }
    // settle any view dissolve so a capture is the settled frame
    m.apply(Cmd::Frame(255));
    m
}

pub fn text_of(r: &Rendered) -> String {
    let mut s = String::new();
    for l in &r.lines {
        s.push_str(l.trim_end());
        s.push('\n');
    }
    s
}

pub fn render_text(model: &Model, env: &Env) -> io::Result<String> {
    Ok(text_of(&render_frame(model, env)?))
}

fn env_for(w: u16, h: u16, mono: bool, glyphs: SubcellGlyphMode) -> Env {
    Env {
        width: w,
        height: h,
        depth: if mono { ColorDepth::Mono } else { ColorDepth::TrueColor },
        glyphs,
    }
}

/// One scene = a script that sets up state, applied on top of "seek to t".
pub struct Scene {
    pub name: &'static str,
    pub view: View,
    pub script: &'static str,
}

pub const SCENES: [Scene; 6] = [
    Scene { name: "stream", view: View::Stream, script: "select alpha" },
    Scene { name: "spectrum", view: View::Spectrum, script: "select alpha" },
    Scene { name: "fold", view: View::Fold, script: "select alpha; focus" },
    Scene { name: "relation", view: View::Relation, script: "select alpha" },
    Scene { name: "sky", view: View::Sky, script: "select alpha" },
    Scene { name: "dossier", view: View::Dossier, script: "select alpha" },
];

pub fn scene_model(seed: u64, scene: &Scene, t: f64) -> Model {
    let mut cmds = vec![Cmd::Seek(secs_to_samples(t)), Cmd::SetView(scene.view)];
    cmds.extend(parse_script(scene.script).expect("scene script parses"));
    model_with(seed, &cmds)
}

/// A manifest line describing how to reproduce a capture.
fn repro(seed: u64, t: f64, scene: &Scene, w: u16, h: u16, mono: bool) -> String {
    format!(
        "project-pulsar --seed {seed} --dump --at {t} --view {} --script \"{}\" --size {w}x{h}{}",
        scene.name,
        scene.script,
        if mono { " --mono" } else { "" }
    )
}

/// Write the whole capture set under `dir`. Returns the written paths.
pub fn write_capture_set(dir: &Path, seed: u64) -> io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    let mut index = String::new();
    index.push_str(&format!(
        "# Project Pulsar capture set — seed {seed}\n# every file is a deterministic function of (seed, script, size, capability)\n\n"
    ));
    let put = |rel: String, body: &str, note: String, written: &mut Vec<PathBuf>, index: &mut String| -> io::Result<()> {
        let path = dir.join(&rel);
        if let Some(p) = path.parent() {
            fs::create_dir_all(p)?;
        }
        fs::write(&path, body)?;
        index.push_str(&format!("{rel}\n    {note}\n"));
        written.push(path);
        Ok(())
    };

    // ---- the six representations at three sizes, colour and Mono ----
    for &(w, h) in &SIZES {
        for mono in [false, true] {
            for scene in &SCENES {
                let t = if scene.view == View::Dossier { 480.0 } else { 300.0 };
                let m = scene_model(seed, scene, t);
                let env = env_for(w, h, mono, SubcellGlyphMode::Braille2x4);
                let r = render_frame(&m, &env)?;
                let sub = format!("frames/{}{w}x{h}", if mono { "mono-" } else { "" });
                put(
                    format!("{sub}/{}.txt", scene.name),
                    &text_of(&r),
                    repro(seed, t, scene, w, h, mono),
                    &mut written,
                    &mut index,
                )?;
                if scene.name == "sky" || scene.name == "spectrum" {
                    put(
                        format!("{sub}/{}.ansi", scene.name),
                        &r.ansi,
                        "raw ANSI stream of the same frame (SGR attributes visible)".into(),
                        &mut written,
                        &mut index,
                    )?;
                }
            }
        }
    }

    // ---- temporal keyframes: the same scene as time advances ----
    for (scene_name, w, h, mono) in [
        ("sky", 120u16, 40u16, false),
        ("spectrum", 120, 40, false),
        ("fold", 120, 40, false),
        ("relation", 120, 40, false),
        ("stream", 120, 40, false),
        ("sky", 80, 24, false),
        ("sky", 42, 15, false),
        ("sky", 80, 24, true),
    ] {
        let scene = SCENES.iter().find(|s| s.name == scene_name).expect("scene");
        for &t in &KEYFRAME_TIMES {
            let m = scene_model(seed, scene, t);
            let env = env_for(w, h, mono, SubcellGlyphMode::Braille2x4);
            let r = render_frame(&m, &env)?;
            let sub = format!(
                "keyframes/{scene_name}-{}{w}x{h}",
                if mono { "mono-" } else { "" }
            );
            put(
                format!("{sub}/t{:03}.txt", t as u32),
                &text_of(&r),
                repro(seed, t, scene, w, h, mono),
                &mut written,
                &mut index,
            )?;
        }
    }

    // ---- compare: an earlier uncertain state beside the later interpretation ----
    for (view, name) in [(View::Sky, "sky"), (View::Spectrum, "spectrum"), (View::Fold, "fold")] {
        let m = model_with(
            seed,
            &[
                Cmd::Seek(secs_to_samples(300.0)),
                Cmd::SetView(view),
                Cmd::Select(SigId::Alpha),
                Cmd::PinAt(secs_to_samples(64.0)),
                Cmd::ToggleCompare,
            ],
        );
        for &(w, h) in &SIZES {
            let r = render_frame(&m, &env_for(w, h, false, SubcellGlyphMode::Braille2x4))?;
            put(
                format!("compare/{name}-{w}x{h}.txt"),
                &text_of(&r),
                format!("pinned t=64 s versus now t=300 s, {name} view"),
                &mut written,
                &mut index,
            )?;
        }
    }

    // ---- glyph-mode ladder (the capability axis the library documents) ----
    for (g, name) in [
        (SubcellGlyphMode::Braille2x4, "braille"),
        (SubcellGlyphMode::HalfBlock1x2, "halfblock"),
        (SubcellGlyphMode::Block, "block"),
        (SubcellGlyphMode::Ascii, "ascii"),
    ] {
        let m = scene_model(seed, &SCENES[1], 300.0);
        let r = render_frame(&m, &env_for(80, 24, false, g))?;
        put(
            format!("glyphs/spectrum-{name}.txt"),
            &text_of(&r),
            format!("spectrum at 80x24, --glyphs={name}"),
            &mut written,
            &mut index,
        )?;
    }

    fs::write(dir.join("INDEX.txt"), &index)?;
    written.push(dir.join("INDEX.txt"));
    Ok(written)
}

// ---------------------------------------------------------------------------
// the bounded demo
// ---------------------------------------------------------------------------

pub struct DemoStep {
    pub title: &'static str,
    pub script: &'static str,
}

/// The director's cue sheet. Pure data; each step's frame is a pure function of the
/// commands up to and including it.
pub const DEMO: &[DemoStep] = &[
    DemoStep { title: "00  ESTABLISH — only noise: the stream is buried", script: "at 12; view stream; select alpha" },
    DemoStep { title: "01  A first candidate carrier (BETA) appears; its fitted trace is tiny next to the noise", script: "at 40; select beta; focus" },
    DemoStep { title: "02  SPECTRUM — a bright line dominates; a false comb is forming", script: "at 72; view spectrum; focus; select alpha" },
    DemoStep { title: "03  The terrestrial line is flagged and masked; ALPHA's candidate is revised", script: "at 112" },
    DemoStep { title: "04  FOLD — ALPHA folds into a pulse; the 2P fold tests the period", script: "at 160; view fold; focus" },
    DemoStep { title: "05  RELATION — real sources climb slope 1/2; the ghost bends over", script: "at 280; view relation; focus" },
    DemoStep { title: "06  SKY, early — every identity is still a diffuse cloud", script: "at 40; view sky; select alpha" },
    DemoStep { title: "07  SKY — BETA locks and its cloud collapses first", script: "at 104" },
    DemoStep { title: "08  SKY — ALPHA joins; GAMMA is locked but unconstrained", script: "at 200" },
    DemoStep { title: "09  COMPARE — earlier uncertain state (64 s) beside the later interpretation (300 s)", script: "at 300; pin-at 64; compare" },
    DemoStep { title: "10  DOSSIER — the final interpretation", script: "compare; at 480; view dossier" },
    DemoStep { title: "11  SKY with the truth overlay — inference checked against what was injected", script: "view sky; truth" },
];

/// Run the whole demo into `out`, one frame per step. Non-interactive, bounded, and
/// byte-deterministic.
pub fn run_demo(seed: u64, env: &Env, out: &mut dyn Write) -> io::Result<usize> {
    let mut m = Model::new(seed);
    for (i, step) in DEMO.iter().enumerate() {
        for c in parse_script(step.script).map_err(io::Error::other)? {
            m.apply(c);
        }
        m.apply(Cmd::Frame(255));
        let r = render_frame(&m, env)?;
        writeln!(out, "=== {} ===", step.title)?;
        for l in &r.lines {
            writeln!(out, "{}", l.trim_end())?;
        }
        writeln!(out)?;
        let _ = i;
    }
    Ok(DEMO.len())
}
