//! Rendering across the matrix: sizes × colour depths × scenes (+ glyph realization fallback),
//! determinism of the byte stream, and agreement between LibGibson's own "what is on screen"
//! and an independent VT emulator.

mod common;
use common::*;
use gibson::{ColorDepth, SubcellGlyphMode};
use project_chronoscope::driver::*;

fn has_sgr(stream: &[u8], needle: &str) -> bool {
    String::from_utf8_lossy(stream).contains(needle)
}

#[test]
fn every_scene_renders_at_every_size_and_depth() {
    let mut combos = 0;
    for &(w, h) in &SIZES {
        for &d in &DEPTHS {
            for (name, script) in SCENES {
                let mut r = rig(w, h, d);
                run_script(&mut r, script).unwrap_or_else(|e| panic!("{name}: {e}"));
                r.settle().unwrap();
                let lines = r.lines();
                assert!(
                    lines.len() <= h as usize,
                    "{name} {w}x{h}: {} rows",
                    lines.len()
                );
                assert!(
                    lines.iter().all(|l| l.chars().count() <= w as usize),
                    "{name} {w}x{h}: a row exceeds the width"
                );
                let all = lines.join("\n");
                assert!(
                    all.contains("CHRONOSCOPE"),
                    "{name} {w}x{h} {d:?}: header missing\n{all}"
                );
                // the spatial view is not blank: braille/blocks/ascii ink exists below the header.
                // (A modal may legitimately cover a 42x15 screen; then the modal title is the proof.)
                let ink = lines
                    .iter()
                    .skip(1)
                    .flat_map(|l| l.chars())
                    .filter(|c| ('\u{2800}'..='\u{28FF}').contains(c) || "▀▄█▓▒░".contains(*c))
                    .count();
                if name.ends_with("modal") && (w, h) == (42, 15) {
                    assert!(
                        all.contains("FORK") || all.contains("INSPECT"),
                        "{name} {w}x{h} {d:?}: modal title missing\n{all}"
                    );
                } else {
                    assert!(
                        ink > 20,
                        "{name} {w}x{h} {d:?}: the spatial view is empty ({ink} ink glyphs)"
                    );
                }
                combos += 1;
            }
        }
    }
    assert_eq!(combos, 5 * 4 * 8);
}

#[test]
fn colour_depth_is_honoured_on_the_wire() {
    for &(w, h) in &[(80u16, 24u16), (120, 40)] {
        for (name, script) in [SCENES[1], SCENES[3], SCENES[4]] {
            let stream = |d: ColorDepth| {
                let mut r = rig(w, h, d);
                run_script(&mut r, script).unwrap();
                r.settle().unwrap();
                r.stream.take().unwrap()
            };
            let tc = stream(ColorDepth::TrueColor);
            assert!(
                has_sgr(&tc, "38;2;"),
                "{name}: truecolor uses 24-bit foregrounds"
            );
            let a256 = stream(ColorDepth::Ansi256);
            assert!(
                !has_sgr(&a256, "38;2;") && !has_sgr(&a256, "48;2;"),
                "{name}: ansi256 stream contains 24-bit colour"
            );
            assert!(
                has_sgr(&a256, "38;5;"),
                "{name}: ansi256 uses indexed colour"
            );
            let a16 = stream(ColorDepth::Ansi16);
            assert!(
                !has_sgr(&a16, "38;2;") && !has_sgr(&a16, "38;5;") && !has_sgr(&a16, "48;5;"),
                "{name}: ansi16 stream contains extended colour"
            );
            let mono = stream(ColorDepth::Mono);
            for pat in [
                "38;2;", "38;5;", "48;2;", "48;5;", "[31m", "[32m", "[36m", "[91m",
            ] {
                assert!(
                    !has_sgr(&mono, pat),
                    "{name}: mono stream contains colour sequence {pat}"
                );
            }
        }
    }
}

#[test]
fn mono_keeps_semantic_distinctions_without_colour() {
    // ghost vs lived and the epoch strip must still differ with colour off
    let mut r = rig(120, 40, ColorDepth::Mono);
    run_script(
        &mut r,
        "end; goto 50; fork replace2; branch 0; goto 120; settle",
    )
    .unwrap();
    let text = r.lines().join("\n");
    // epoch letters in the strip (s ? ^ D X r ! …) and the fork mark
    assert!(text.contains('◢'), "fork marker survives mono");
    assert!(
        text.contains('D') && text.contains('^'),
        "epoch letters stand in for epoch colour"
    );
}

fn in_family(mode: SubcellGlyphMode, c: char) -> bool {
    match mode {
        SubcellGlyphMode::Braille2x4 => ('\u{2800}'..='\u{28FF}').contains(&c),
        SubcellGlyphMode::HalfBlock1x2 => "▀▄█".contains(c),
        SubcellGlyphMode::Block => "░▒▓█".contains(c),
        _ => ".:-=+*#@".contains(c),
    }
}

#[test]
fn glyph_realization_ladder_falls_back_without_losing_the_view() {
    for mode in [
        SubcellGlyphMode::Braille2x4,
        SubcellGlyphMode::HalfBlock1x2,
        SubcellGlyphMode::Block,
        SubcellGlyphMode::Ascii,
    ] {
        let mut r = Rig::headless(120, 40, ColorDepth::Mono, mode, opts());
        run_script(&mut r, "goto 200; settle").unwrap();
        let lines = r.lines();
        let body = lines[2..30].join("");
        let n = body.chars().filter(|c| in_family(mode, *c)).count();
        assert!(
            n > 30,
            "{mode:?}: only {n} glyphs of its own family in the viewport"
        );
        if mode != SubcellGlyphMode::Braille2x4 {
            assert!(
                !body.chars().any(|c| ('\u{2801}'..='\u{28FF}').contains(&c)),
                "{mode:?}: stray braille after fallback"
            );
        }
    }
}

#[test]
fn identical_program_seed_and_actions_give_identical_bytes() {
    for &(w, h) in &[(60u16, 20u16), (120, 40)] {
        for d in [ColorDepth::TrueColor, ColorDepth::Mono] {
            let run = || {
                let mut r = rig(w, h, d);
                run_script(&mut r, "end; goto 50; fork replace2; goto 100; frames 3; compare; frames 3; goto 215; settle").unwrap();
                r.stream.take().unwrap()
            };
            assert_eq!(run(), run(), "{w}x{h} {d:?}: the byte stream is a pure function of (program, seed, actions, clock)");
        }
    }
}

#[test]
fn a_settled_frame_costs_almost_nothing() {
    let mut r = rig(120, 40, ColorDepth::TrueColor);
    run_script(&mut r, "goto 150; settle").unwrap();
    r.render_only().unwrap();
    let again = r.render_only().unwrap();
    assert_eq!(
        again.exact_changed, 0,
        "re-rendering identical state changes no cells"
    );
    assert!(
        again.bytes <= 16,
        "re-render of an identical frame emitted {} bytes",
        again.bytes
    );
}

#[test]
fn libgibsons_visible_text_agrees_with_an_independent_vt_emulator() {
    for &(w, h) in &SIZES {
        let mut r = rig(w, h, ColorDepth::TrueColor);
        run_script(
            &mut r,
            "end; goto 50; fork replace2; goto 215; compare; settle",
        )
        .unwrap();
        let vt = screen_of(&r, w, h);
        let theirs: Vec<String> = vt
            .screen()
            .contents()
            .lines()
            .map(|l| l.trim_end().to_string())
            .collect();
        let ours: Vec<String> = r.lines().iter().map(|l| l.trim_end().to_string()).collect();
        for (i, (a, b)) in ours.iter().zip(theirs.iter()).enumerate() {
            assert_eq!(
                a, b,
                "{w}x{h} row {i}: Context::last_frame_lines disagrees with the VT emulator"
            );
        }
    }
}

#[test]
fn scrubbing_away_and_back_reproduces_the_same_screen() {
    // history-position idempotence: the screen is a function of (cursor, branch, mode)
    let shot = |wander: bool| {
        let mut r = rig(120, 40, ColorDepth::TrueColor);
        run_script(&mut r, "end; goto 50; fork replace2; branch 0").unwrap();
        if wander {
            run_script(&mut r, "goto 300; frames 4; step -30; frames 2; goto 10; frames 2; branch 1; frames 2; branch 0").unwrap();
        }
        // The camera's *facing* is view state that follows the last movement direction (a
        // deliberate hysteresis), so normalize it; every other input is history position.
        r.model.facing_back = false;
        run_script(&mut r, "goto 120; settle").unwrap();
        // let toasts expire so only history state remains
        for _ in 0..140 {
            r.frame().unwrap();
        }
        let mut fresh = rig(120, 40, ColorDepth::TrueColor);
        // reparse only the last full-screen state
        let vt = screen_of(&r, 120, 40);
        let _ = &mut fresh;
        vt.screen().contents()
    };
    assert_eq!(
        shot(false),
        shot(true),
        "the same history position must look the same however you got there"
    );
}

#[test]
fn resize_during_scrub_and_fork_never_breaks_the_frame() {
    let mut r = rig(120, 40, ColorDepth::TrueColor);
    run_script(&mut r, "end; goto 60; settle").unwrap();
    for (i, &(w, h)) in SIZES.iter().cycle().take(15).enumerate() {
        r.resize(w, h);
        match i % 3 {
            0 => run_script(&mut r, "step -7; frames 1").unwrap(),
            1 => run_script(&mut r, "step 13; frames 1").unwrap(),
            _ => run_script(&mut r, "fork insert2; frames 1; branch 0; goto 60").unwrap_or(()),
        }
        let lines = r.lines();
        assert!(lines.len() <= h as usize);
        assert!(
            lines.iter().all(|l| l.chars().count() <= w as usize),
            "{w}x{h} after resize"
        );
        assert!(lines.join("").contains("CHRONOSCOPE"));
    }
    assert!(r.model.hist.branches.len() > 1);
}
