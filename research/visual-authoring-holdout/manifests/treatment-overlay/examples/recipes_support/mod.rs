//! Shared harness for the composition recipes — NOT a composition lesson.
//!
//! This file exists so each `recipe_*.rs` can stay *pure composition*: it runs
//! the recipe interactively in a real terminal, or — when invoked with
//! `--capture WxH[:mono|:ansi16|:ansi256]` — prints exactly one deterministic
//! frame to stdout so a human (or CI) can inspect the rendered result.
//!
//! Read the recipe body, not this file. The only idea here worth noticing is
//! that visual acceptance is a *first-class run mode*, not an afterthought:
//! "it compiled" is not "it looks right".
#![allow(dead_code)]

use std::io;
use std::time::Duration;

use gibson::ui::prelude::*;
use gibson::{ColorDepth, Context, RenderMode};

/// A requested headless capture: terminal size + color capability.
pub struct Capture {
    pub width: u16,
    pub height: u16,
    pub depth: ColorDepth,
}

/// Parse `--capture WxH[:mono|:ansi16|:ansi256]` out of argv.
/// Returns `None` for an ordinary interactive run.
pub fn capture_request() -> Option<Capture> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--capture" {
            let spec = args.next().unwrap_or_else(|| "120x40".into());
            return Some(parse_spec(&spec));
        }
        if let Some(spec) = a.strip_prefix("--capture=") {
            return Some(parse_spec(spec));
        }
    }
    None
}

fn parse_spec(spec: &str) -> Capture {
    let (size, depth_tag) = spec.split_once(':').unwrap_or((spec, "truecolor"));
    let (w, h) = size.split_once('x').unwrap_or(("120", "40"));
    let depth = match depth_tag {
        "mono" => ColorDepth::Mono,
        "ansi16" => ColorDepth::Ansi16,
        "ansi256" => ColorDepth::Ansi256,
        _ => ColorDepth::TrueColor,
    };
    Capture {
        width: w.parse().unwrap_or(120),
        height: h.parse().unwrap_or(40),
        depth,
    }
}

/// Run `view` as a full-screen application, OR — under `--capture` — lower it
/// once at the requested size/capability and print the frame deterministically
/// (presentation time is pinned to zero, so captures are byte-stable).
pub fn present<M, A, U, V>(skin: Skin, initial: M, update: U, view: V) -> io::Result<()>
where
    M: Clone + 'static,
    A: Clone + 'static,
    U: Fn(&mut M, AppEvent<A>) -> Control + 'static,
    V: Fn(&M, &BuildCx) -> Element<A> + 'static,
{
    if let Some(cap) = capture_request() {
        let env = UiEnvironment {
            width: cap.width,
            height: cap.height,
            color_depth: cap.depth,
            motion: MotionPreference::None,
            ..UiEnvironment::default()
        };
        let mut runtime: UiRuntime<A> = UiRuntime::new(skin);
        let mut ctx = Context::headless(RenderMode::Fullscreen, cap.width, cap.height);
        let cx = runtime.build_cx(env, Duration::ZERO);
        let tree = view(&initial, &cx);
        let frame = runtime
            .frame(&tree, env, Duration::ZERO)
            .map_err(io::Error::other)?;
        ctx.set_root(frame.node);
        ctx.render_now()?;
        let lines = ctx.last_frame_lines();
        // Structural self-check: a composed frame must never overflow its box.
        for (i, line) in lines.iter().enumerate() {
            let cols = line.chars().count();
            assert!(
                cols <= cap.width as usize,
                "row {i} overflows {} cols (> width {})",
                cols,
                cap.width
            );
        }
        for line in &lines {
            println!("{}", line.trim_end());
        }
        return Ok(());
    }

    App::fullscreen().skin(skin).run(initial, update, view)?;
    Ok(())
}
