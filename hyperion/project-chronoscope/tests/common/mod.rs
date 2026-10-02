#![allow(dead_code)]
use gibson::{ColorDepth, SubcellGlyphMode};
use project_chronoscope::app::*;
use project_chronoscope::driver::*;

pub const SIZES: [(u16, u16); 5] = [(42, 15), (60, 20), (80, 24), (120, 40), (160, 50)];
pub const DEPTHS: [ColorDepth; 4] = [
    ColorDepth::TrueColor,
    ColorDepth::Ansi256,
    ColorDepth::Ansi16,
    ColorDepth::Mono,
];

pub fn opts() -> Options {
    Options {
        audio: AudioMode::Off,
        ..Options::default()
    }
}

pub fn rig(w: u16, h: u16, depth: ColorDepth) -> Rig {
    let mut r = Rig::headless(w, h, depth, SubcellGlyphMode::Braille2x4, opts());
    r.stream = Some(vec![]);
    r
}

/// Scenes every combination must survive: (name, script).
pub const SCENES: [(&str, &str); 8] = [
    ("start", "settle"),
    ("mid", "goto 200; settle"),
    ("catastrophe", "end; settle"),
    (
        "fork-ghost",
        "end; goto 50; fork replace2; branch 0; goto 120; settle",
    ),
    (
        "compare",
        "end; goto 50; fork replace2; goto 215; compare; settle",
    ),
    ("looking-back", "end; goto 300; step -8; settle"),
    ("fork-modal", "end; goto 50; openfork; settle"),
    ("inspect-modal", "goto 214; inspect; settle"),
];

/// Parse everything a rig emitted through a real VT emulator.
pub fn screen_of(rig: &Rig, w: u16, h: u16) -> vt100::Parser {
    let mut p = vt100::Parser::new(h, w, 0);
    p.process(rig.stream.as_deref().unwrap_or(&[]));
    p
}

pub fn contents(p: &vt100::Parser) -> String {
    p.screen().contents()
}
