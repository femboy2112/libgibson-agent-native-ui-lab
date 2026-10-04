//! Captures, keyframes, the demo and the Mono realisation.

mod support;
use pulsar::app::render_frame;
use pulsar::capture;
use pulsar::render::{sky, Env};
use pulsar::scenario::{SigId, DEFAULT_SEED};
use pulsar::session::{secs_to_samples, Cmd, Model, View};
use std::fs;
use std::path::{Path, PathBuf};
use support::*;

fn tmpdir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("pulsar-test-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn listing(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut v = Vec::new();
    walk(dir, &mut v);
    v.sort();
    v.into_iter()
        .map(|p| {
            let rel = p.strip_prefix(dir).unwrap().to_path_buf();
            (rel, fs::read(&p).unwrap())
        })
        .collect()
}

#[test]
fn the_capture_set_is_byte_deterministic() {
    let (a, b) = (tmpdir("a"), tmpdir("b"));
    let fa = capture::write_capture_set(&a, SEED).unwrap();
    capture::write_capture_set(&b, SEED).unwrap();
    assert!(fa.len() >= 130, "{} files", fa.len());
    assert_eq!(listing(&a), listing(&b));
    // the three required sizes, in colour and Mono, and the temporal keyframes exist
    for sub in ["frames/120x40", "frames/80x24", "frames/42x15", "frames/mono-120x40", "frames/mono-80x24", "frames/mono-42x15", "keyframes/sky-120x40", "keyframes/sky-80x24", "keyframes/sky-42x15"] {
        assert!(a.join(sub).is_dir(), "{sub} missing");
    }
    for &t in capture::KEYFRAME_TIMES.iter() {
        assert!(a.join(format!("keyframes/sky-120x40/t{:03}.txt", t as u32)).is_file());
    }
    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
}

/// Bitwise drift guard against the committed `captures/` tree. Floating-point
/// transcendental functions are platform-libm dependent, so this is opt-in:
/// `cargo test -- --ignored committed_captures`.
#[test]
#[ignore = "bitwise comparison with the committed captures; depends on the platform libm"]
fn committed_captures_match_regeneration() {
    let committed = Path::new(env!("CARGO_MANIFEST_DIR")).join("captures");
    assert!(committed.is_dir(), "no committed captures/ directory");
    let fresh = tmpdir("fresh");
    capture::write_capture_set(&fresh, DEFAULT_SEED).unwrap();
    assert_eq!(listing(&committed), listing(&fresh));
    let _ = fs::remove_dir_all(&fresh);
}

#[test]
fn keyframes_show_uncertainty_collapsing_into_interpretation() {
    let eng = pulsar::analysis::Engine::shared(SEED);
    // (1) the position cloud of a source contracts monotonically once it has a fix
    let rms = |id: SigId, t: f64| -> f64 {
        let cp = eng.checkpoint_at_sample(secs_to_samples(t));
        let p = cp.signal(id);
        let pts: Vec<(f64, f64)> = (0..sky::CLOUD_N).map(|i| sky::sample(SEED, id, p, i)).collect();
        let (mx, my) = (
            pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64,
            pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64,
        );
        (pts.iter().map(|p| (p.0 - mx).powi(2) + (p.1 - my).powi(2)).sum::<f64>() / pts.len() as f64).sqrt()
    };
    for id in [SigId::Alpha, SigId::Beta] {
        let series: Vec<f64> = capture::KEYFRAME_TIMES.iter().map(|&t| rms(id, t)).collect();
        // starts as the whole sky (uniform in a unit disc has rms radius ≈ 0.7)…
        assert!(series[0] > 0.6, "{} t=8: {series:?}", id.name());
        // …and ends tight
        assert!(*series.last().unwrap() < 0.15, "{} final: {series:?}", id.name());
        // and after the first lock it never widens again (beyond sampling noise)
        let lock_t = eng.checkpoint(60).signal(id).lock_cp.unwrap() as f64 * 8.0;
        let after: Vec<f64> = capture::KEYFRAME_TIMES.iter().filter(|&&t| t >= lock_t).map(|&t| rms(id, t)).collect();
        for w in after.windows(2) {
            assert!(w[1] <= w[0] * 1.15, "{} widened after lock: {after:?}", id.name());
        }
    }
    // (2) the rendered keyframes differ pairwise, start at SEARCH and end at RESOLVED
    let frames: Vec<String> = capture::KEYFRAME_TIMES
        .iter()
        .map(|&t| {
            let m = capture::scene_model(SEED, &capture::SCENES[4], t);
            capture::render_text(&m, &Env::new(120, 40)).unwrap()
        })
        .collect();
    for i in 0..frames.len() {
        for j in i + 1..frames.len() {
            assert_ne!(frames[i], frames[j], "keyframes {i} and {j} identical");
        }
    }
    assert!(frames[0].contains("SEARCH"));
    assert!(frames[frames.len() - 1].contains("RESOLVED"));
}

#[test]
fn the_demo_runs_to_completion_deterministically() {
    let env = Env::new(80, 24);
    let mut a = Vec::new();
    let mut b = Vec::new();
    let n = capture::run_demo(SEED, &env, &mut a).unwrap();
    capture::run_demo(SEED, &env, &mut b).unwrap();
    assert_eq!(a, b);
    assert_eq!(n, capture::DEMO.len());
    let text = String::from_utf8(a).unwrap();
    for i in 0..n {
        assert!(text.contains(&format!("=== {i:02}")), "step {i} missing");
    }
    // bounded: exactly n frames of 24 rows each plus headers
    assert_eq!(text.lines().filter(|l| l.starts_with("=== ")).count(), n);
}

#[test]
fn mono_realisation_carries_no_colour_and_still_distinguishes_identities() {
    let mut m = Model::new(SEED);
    m.apply(Cmd::Seek(secs_to_samples(300.0)));
    for view in View::ALL {
        m.apply(Cmd::SetView(view));
        m.apply(Cmd::Frame(255));
        let colour = render_frame(&m, &Env::new(80, 24)).unwrap();
        let mono = render_frame(&m, &Env::new(80, 24).mono()).unwrap();
        // the colour stream paints with RGB; the Mono stream has no colour parameters
        assert!(colour.ansi.contains("38;2;"), "{view:?}: colour frame has RGB foregrounds");
        for needle in ["38;2;", "48;2;", "38;5;", "48;5;"] {
            assert!(!mono.ansi.contains(needle), "{view:?}: Mono stream contains `{needle}`");
        }
        // Mono keeps structure: bold marks the selected identity, glyphs carry identity
        assert!(mono.ansi.contains("\u{1b}[1m") || mono.ansi.contains(";1m") || mono.ansi.contains("[1;"));
        let txt = mono.lines.join("\n");
        for id in SigId::ALL {
            assert!(txt.contains(id.glyph()), "{view:?}: {} glyph in Mono", id.name());
        }
        // identical text between colour and Mono: colour never carried meaning alone
        // (the only intended difference is realisation of colour, not of cells)
        assert_eq!(colour.lines, mono.lines, "{view:?}: Mono text differs from colour text");
    }
}

#[test]
fn glyph_modes_keep_the_scene_recognisable() {
    let m = model_at(SEED, 300.0, View::Spectrum);
    for g in [
        gibson::SubcellGlyphMode::Braille2x4,
        gibson::SubcellGlyphMode::HalfBlock1x2,
        gibson::SubcellGlyphMode::Block,
        gibson::SubcellGlyphMode::Ascii,
    ] {
        let mut env = Env::new(80, 24);
        env.glyphs = g;
        let r = render_frame(&m, &env).unwrap();
        let txt = r.lines.join("\n");
        assert!(txt.contains("SPECTRUM") && txt.contains("INTEGRATED POWER SPECTRUM"), "{g:?}:\n{txt}");
        if matches!(g, gibson::SubcellGlyphMode::Ascii) {
            // the plot geometry itself must be ASCII-only in the fallback ladder's last rung
            assert!(!txt.chars().any(|c| ('\u{2800}'..='\u{28FF}').contains(&c)), "braille leaked into ascii mode");
        }
    }
}
