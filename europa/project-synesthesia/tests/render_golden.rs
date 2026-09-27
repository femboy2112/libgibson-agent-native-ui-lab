use std::process::Command;

fn render(color: &str, glyphs: &str) -> Vec<u8> {
    let output = Command::new(env!("CARGO_BIN_EXE_synesthesia"))
        .args([
            "--dump",
            "--profile",
            "--at-ms=3750",
            "--seed=2112",
            "--pattern=offbeat",
            "--width=120",
            "--height=32",
            color,
            &format!("--glyphs={glyphs}"),
        ])
        .output()
        .expect("run deterministic capture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn fixed_state_render_is_byte_stable_and_has_a_golden() {
    let first = render("--truecolor", "braille");
    let repeat = render("--truecolor", "braille");
    assert_eq!(first, repeat);
    assert!(first.windows(5).any(|window| window == b"SYNES"));
    assert!(first.windows(10).any(|window| window == b"EVT 000000"));
    assert!(first.windows(10).any(|window| window == b"KEY 000000"));
    assert!(first.windows(7).any(|window| window == b"096 BPM"));
    assert!(first.windows(6).any(|window| window == b"30 FPS"));
    assert_eq!(fnv1a(&first), 9_264_295_070_454_141_553);
}

#[test]
fn compact_performance_view_keeps_all_track_labels() {
    for width in ["80", "56"] {
        let output = Command::new(env!("CARGO_BIN_EXE_synesthesia"))
            .args([
                "--demo",
                "--silent",
                "--performance",
                "--at-ms=3750",
                &format!("--width={width}"),
                "--height=24",
                "--mono",
                "--glyphs=ascii",
                "--dump",
            ])
            .output()
            .expect("render compact performance view");
        assert!(output.status.success());
        let frame = String::from_utf8(output.stdout).expect("valid UTF-8 frame");
        for label in ["KICK", "SNAR", "HAT", "BASS", "02 / ROUTE", "OUT", "Q quit"] {
            assert!(frame.contains(label), "{width}-column frame lost {label}");
        }
    }
}

#[test]
fn short_view_preserves_tracks_route_output_and_exit() {
    let output = Command::new(env!("CARGO_BIN_EXE_synesthesia"))
        .args([
            "--demo",
            "--silent",
            "--at-ms=3750",
            "--width=60",
            "--height=20",
            "--mono",
            "--glyphs=ascii",
            "--dump",
        ])
        .output()
        .expect("render 60x20 constrained layout");
    assert!(output.status.success());
    let frame = String::from_utf8(output.stdout).expect("valid UTF-8 frame");
    for label in [
        "KICK",
        "SNAR",
        "HAT",
        "BASS",
        "02 / ROUTE",
        "03 / OUTPUT",
        "Q quit",
    ] {
        assert!(frame.contains(label), "short frame lost {label}");
    }
}

#[test]
fn too_small_view_keeps_an_exit_instruction() {
    let output = Command::new(env!("CARGO_BIN_EXE_synesthesia"))
        .args([
            "--demo",
            "--silent",
            "--performance",
            "--at-ms=3750",
            "--width=44",
            "--height=14",
            "--mono",
            "--glyphs=ascii",
            "--dump",
        ])
        .output()
        .expect("render 44x14 resize fallback");
    assert!(output.status.success());
    let frame = String::from_utf8(output.stdout).expect("valid UTF-8 frame");
    assert!(frame.contains("resize for signal field"));
    assert!(frame.contains("LIVE"));
    assert!(frame.contains("Q quit"));
}

#[test]
fn color_and_glyph_fallbacks_preserve_the_field() {
    let truecolor = render("--truecolor", "braille");
    let ansi256 = render("--ansi256", "halfblock");
    let ansi16 = render("--ansi16", "block");
    let mono = render("--mono", "ascii");
    assert!(truecolor.windows(5).any(|w| w == b"38;2;"));
    assert!(ansi256.windows(5).any(|w| w == b"38;5;"));
    assert!(!ansi16.windows(5).any(|w| w == b"38;2;"));
    assert!(!ansi16.windows(5).any(|w| w == b"38;5;"));
    assert!(contains_any(
        &ansi16,
        &["\x1b[30m", "\x1b[34m", "\x1b[91m", "\x1b[96m"]
    ));
    assert!(!mono.windows(5).any(|w| w == b"38;2;"));
    assert!(!mono.windows(5).any(|w| w == b"38;5;"));
    assert!(!contains_any(
        &mono,
        &["\x1b[30m", "\x1b[34m", "\x1b[91m", "\x1b[96m"]
    ));
    for frame in [&ansi256, &ansi16, &mono] {
        assert!(frame.windows(10).any(|w| w == b"SEQUENCER "));
        assert!(frame.len() > 1_000);
    }
    assert_ne!(truecolor, mono);
    assert_ne!(ansi256, ansi16);
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn contains_any(haystack: &[u8], needles: &[&str]) -> bool {
    needles.iter().any(|needle| {
        haystack
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    })
}
